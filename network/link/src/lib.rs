//! A world's end of the Dereth Network's link: the WebSocket to the hub with the world's key, its
//! keep-alive and reconnects, carried on a thread of its own.
//!
//! **Depends on** `dereth-network-protocol` (the frame bound, and the verification's lines). **Used
//! by** the world servers that link to a hub: Empyrean (`empyrean-server`) and Yalain (`yalain`).
//!
//! **Must never** read or write a link message itself: frames go both ways as text, and what they
//! say is the world's. It knows no game, keeps no records and installs no logger; what happens to
//! the link reaches the world as [`Event`]s, for it to log.
//!
//! **Specified in** `docs/network/02-link-protocol.md` section 1. [`LinkClient::start`] connects to
//! `<hub>/v1/servers/<slug>/link` with the world's key as a bearer token, over `wss://` with the
//! web's root certificates, or plain `ws://` to a hub on this machine only ([`HubUrl`]). It then:
//!
//! - hands the world each frame the hub sends ([`Event::Frame`]), and sends the hub each frame the
//!   world gives it ([`LinkClient::send`]); a frame the world gave for a connection that has since
//!   gone is dropped rather than sent first on the next one, where only `hello` may come first;
//! - answers the hub's pings, and gives the link up when the hub falls silent for [`SILENT_FOR`],
//!   stops reading for [`WRITE_TIMEOUT`], sends a frame longer than the protocol allows, or sends
//!   faster than the world takes its frames, so that [`MAX_WAITING_FRAMES`] wait;
//! - connects again after a lost link or a failed attempt, waiting as [`Reconnect`] says: 1, 2, 5,
//!   10, then every 30 seconds, and a minute after the hub refuses the key or the world (`401`,
//!   `403`).
//!
//! Nothing here logs, and nothing here holds the key anywhere but the request it sends. [`verify`]
//! is a world's other calls to the hub, the two of its operator's verification.

pub mod verify;

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The waits between reconnection attempts; the last repeats.
pub const BACKOFF_SECS: [u64; 5] = [1, 2, 5, 10, 30];

/// The wait after the hub refuses the world's key (`401`) or the world itself (`403`): the
/// protocol asks for no retry faster than once a minute.
pub const REFUSED_WAIT: Duration = Duration::from_secs(60);

/// How long a link may stay silent (the hub pings every 20 seconds) before it is given up.
pub const SILENT_FOR: Duration = Duration::from_secs(70);

/// How long one write to the hub may wait before the link is given up.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a connection, or the handshake's reads, may wait for the hub.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// The most frames from the hub that may wait for the world. A hub sending faster than the world
/// takes them loses the link here (it is made again, as after any lost link), so it never holds
/// the world's memory.
pub const MAX_WAITING_FRAMES: usize = 20_000;

/// How long the link thread waits on a read before it looks for frames to send.
const POLL: Duration = Duration::from_millis(20);

/// The hub's address, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubUrl {
    pub tls: bool,
    pub host: String,
    pub port: u16,
    /// The path prefix, without a trailing slash (usually empty).
    pub prefix: String,
}

impl HubUrl {
    /// Reads `ws://host[:port][/prefix]` or `wss://...`. A plain `ws://` hub must be on loopback.
    ///
    /// # Errors
    /// A URL of another shape, or `ws://` to a host that is not loopback.
    pub fn parse(url: &str) -> Result<Self, String> {
        let (tls, rest) = if let Some(r) = url.strip_prefix("wss://") {
            (true, r)
        } else if let Some(r) = url.strip_prefix("ws://") {
            (false, r)
        } else {
            return Err(format!(
                "{url}: the hub's address is wss://host or ws://127.0.0.1:port"
            ));
        };
        let (authority, path) = rest.split_once('/').map_or((rest, ""), |(a, p)| (a, p));
        let default_port = if tls { 443 } else { 80 };
        let bad_port = || format!("{url}: a bad port");
        let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
            let (h, after) = v6
                .split_once(']')
                .ok_or_else(|| format!("{url}: a bad address"))?;
            let port = match after.strip_prefix(':') {
                Some(p) => p.parse::<u16>().map_err(|_| bad_port())?,
                None if after.is_empty() => default_port,
                None => return Err(format!("{url}: a bad address")),
            };
            (h.to_owned(), port)
        } else {
            match authority.rsplit_once(':') {
                Some((h, p)) => (h.to_owned(), p.parse::<u16>().map_err(|_| bad_port())?),
                None => (authority.to_owned(), default_port),
            }
        };
        if host.is_empty() || authority.contains('@') {
            return Err(format!("{url}: no host"));
        }
        if !tls && !is_loopback(&host) {
            return Err(format!(
                "{url}: plain ws:// is for a hub on this machine only; use wss://"
            ));
        }
        Ok(Self {
            tls,
            host,
            port,
            prefix: path.trim_end_matches('/').to_owned(),
        })
    }

    /// The verification route's URL for `slug`: the same host over `https://` (`http://` for a
    /// loopback hub on `ws://`).
    #[must_use]
    pub fn verify(&self, slug: &str) -> String {
        let link = self.link(slug);
        let link = link.strip_suffix("/link").unwrap_or(&link);
        let rest = link
            .strip_prefix("wss://")
            .map(|r| format!("https://{r}"))
            .or_else(|| link.strip_prefix("ws://").map(|r| format!("http://{r}")))
            .unwrap_or_default();
        format!("{rest}/verify")
    }

    /// The route the world posts its answer to the hub's challenge to, beside the verification
    /// route.
    #[must_use]
    pub fn answer(&self, slug: &str) -> String {
        format!("{}/answer", self.verify(slug))
    }

    /// The link's URL for `slug`.
    #[must_use]
    pub fn link(&self, slug: &str) -> String {
        let scheme = if self.tls { "wss" } else { "ws" };
        let prefix = if self.prefix.is_empty() {
            String::new()
        } else {
            format!("/{}", self.prefix)
        };
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!(
            "{scheme}://{host}:{}{prefix}/v1/servers/{slug}/link",
            self.port
        )
    }
}

fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// A world's key: `drth_` and 43 characters of base64url.
#[must_use]
pub fn is_key(key: &str) -> bool {
    key.len() == 48
        && key.starts_with("drth_")
        && key[5..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// What happened to the link, for the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A connection is up: the world says `hello` first on it.
    Up,
    /// A frame from the hub, as its text.
    Frame(String),
    /// The connection that was up is gone, and why.
    Down(String),
    /// An attempt to connect failed; the next is made after `retry_in`.
    Unreachable { why: String, retry_in: Duration },
    /// The hub refused the world's key (`401`) or the world itself (`403`, its federation flag is
    /// not on); the next attempt is made after `retry_in`.
    Refused { status: u16, retry_in: Duration },
}

/// The waits between attempts to connect: 1, 2, 5, 10, then every 30 seconds while attempts
/// fail; a minute after a refusal; and back to the first after a link that came up.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconnect {
    failed: usize,
}

impl Reconnect {
    /// The wait after an attempt that could not reach the hub, or whose handshake failed.
    pub fn after_failure(&mut self) -> Duration {
        let secs = BACKOFF_SECS[self.failed.min(BACKOFF_SECS.len() - 1)];
        self.failed += 1;
        Duration::from_secs(secs)
    }

    /// The wait after the hub refused the key or the world.
    #[must_use]
    pub fn after_refusal(&self) -> Duration {
        REFUSED_WAIT
    }

    /// The wait after a link that came up and was lost: the first step, and the steps start
    /// again.
    pub fn after_link(&mut self) -> Duration {
        self.failed = 0;
        Duration::from_secs(BACKOFF_SECS[0])
    }
}

/// The link thread's time: what it reads the time from, and how it waits between attempts. The
/// wall's, except in tests.
trait Clock: Send + 'static {
    fn now(&self) -> Instant;
    /// Waits `d`, or less once `stop` is set.
    fn pause(&self, d: Duration, stop: &AtomicBool);
}

/// The real clock.
struct Wall;

impl Clock for Wall {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn pause(&self, d: Duration, stop: &AtomicBool) {
        let until = Instant::now() + d;
        while Instant::now() < until && !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50).min(until - Instant::now()));
        }
    }
}

/// An event, with the connection it belongs to (0 for none).
type Tagged = (u64, Event);

/// A world's running link to its hub.
pub struct LinkClient {
    events: Receiver<Tagged>,
    /// How many frames wait in `events` (see [`MAX_WAITING_FRAMES`]).
    waiting: Arc<AtomicUsize>,
    out: Sender<(u64, String)>,
    /// The connection the world last heard come up: what it sends is for that one.
    epoch: u64,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for LinkClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkClient")
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

impl LinkClient {
    /// Starts the link thread, which connects to `hub`'s link for `slug` with `key` and keeps it
    /// up until [`LinkClient::stop`].
    ///
    /// # Errors
    /// When the thread cannot be started.
    pub fn start(hub: &HubUrl, slug: &str, key: &str) -> io::Result<Self> {
        Self::start_with(hub, slug, key, Wall)
    }

    fn start_with(hub: &HubUrl, slug: &str, key: &str, clock: impl Clock) -> io::Result<Self> {
        let (event_tx, events) = mpsc::channel();
        let (out, out_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let waiting = Arc::new(AtomicUsize::new(0));
        let thread = {
            let (target, url, key) = (hub.clone(), hub.link(slug), key.to_owned());
            let (stop, waiting) = (Arc::clone(&stop), Arc::clone(&waiting));
            std::thread::Builder::new()
                .name("network-link".into())
                .spawn(move || {
                    let side = Side {
                        events: event_tx,
                        waiting,
                        out: out_rx,
                        stop,
                    };
                    run(&target, &url, &key, &side, &clock);
                })?
        };
        Ok(Self {
            events,
            waiting,
            out,
            epoch: 0,
            stop,
            thread: Some(thread),
        })
    }

    /// The next thing that happened to the link, if anything has. Never waits.
    pub fn try_event(&mut self) -> Option<Event> {
        let (epoch, e) = self.events.try_recv().ok()?;
        match &e {
            Event::Up => self.epoch = epoch,
            Event::Frame(_) => {
                let _ = self
                    .waiting
                    .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                        Some(n.saturating_sub(1))
                    });
            }
            _ => {}
        }
        Some(e)
    }

    /// Sends `frame` on the connection the world last heard come up ([`Event::Up`]). A frame for a
    /// connection that has gone is dropped.
    pub fn send(&self, frame: String) {
        let _ = self.out.send((self.epoch, frame));
    }

    /// Closes the link and waits for its thread.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    /// A link with no thread and no hub, for the tests of a world's host: the other half hands it
    /// events and reads what it sent.
    #[must_use]
    pub fn detached() -> (Self, Detached) {
        let (event_tx, events) = mpsc::channel();
        let (out, out_rx) = mpsc::channel();
        let waiting = Arc::new(AtomicUsize::new(0));
        (
            Self {
                events,
                waiting: Arc::clone(&waiting),
                out,
                epoch: 0,
                stop: Arc::new(AtomicBool::new(false)),
                thread: None,
            },
            Detached {
                events: event_tx,
                out: out_rx,
                waiting,
                epoch: 0,
            },
        )
    }
}

impl Drop for LinkClient {
    fn drop(&mut self) {
        self.halt();
    }
}

/// The far half of a [`LinkClient::detached`] link: it stands in for the link thread.
#[derive(Debug)]
pub struct Detached {
    events: Sender<Tagged>,
    out: Receiver<(u64, String)>,
    waiting: Arc<AtomicUsize>,
    /// The connection the last [`Event::Up`] handed began.
    epoch: u64,
}

impl Detached {
    /// Hands the world `e`, as the link thread would.
    pub fn hand(&mut self, e: Event) {
        match &e {
            Event::Up => self.epoch += 1,
            Event::Frame(_) => {
                self.waiting.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
        let _ = self.events.send((self.epoch, e));
    }

    /// The frames the world sent for the connection now up, oldest first.
    pub fn sent(&self) -> Vec<String> {
        self.out
            .try_iter()
            .filter(|(epoch, _)| *epoch == self.epoch)
            .map(|(_, f)| f)
            .collect()
    }

    /// How many frames handed the world it has not taken.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.waiting.load(Ordering::SeqCst)
    }
}

/// The link thread's half of the channels.
struct Side {
    events: Sender<Tagged>,
    waiting: Arc<AtomicUsize>,
    out: Receiver<(u64, String)>,
    stop: Arc<AtomicBool>,
}

impl Side {
    fn stopping(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    /// Tells the world; false once it has gone.
    fn tell(&self, epoch: u64, e: Event) -> bool {
        self.events.send((epoch, e)).is_ok()
    }
}

/// A plain or TLS stream to the hub.
enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(buf),
            Self::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls(s) => s.flush(),
        }
    }
}

impl Stream {
    fn tcp(&self) -> &TcpStream {
        match self {
            Self::Plain(s) => s,
            Self::Tls(s) => s.get_ref(),
        }
    }
}

/// Why a connection attempt failed.
enum Failed {
    /// The hub refused the key (401) or the world (403).
    Refused(u16),
    Other(String),
}

fn connect(
    target: &HubUrl,
    url: &str,
    key: &str,
) -> Result<tungstenite::WebSocket<Stream>, Failed> {
    use tungstenite::client::IntoClientRequest as _;
    let addr = (target.host.as_str(), target.port)
        .to_socket_addrs()
        .map_err(|e| Failed::Other(format!("{}: {e}", target.host)))?
        .next()
        .ok_or_else(|| Failed::Other(format!("{}: no address", target.host)))?;
    let tcp = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| Failed::Other(e.to_string()))?;
    tcp.set_read_timeout(Some(CONNECT_TIMEOUT))
        .map_err(|e| Failed::Other(e.to_string()))?;
    // A hub that stops reading must not hold this thread in a write: past this, the link is lost.
    tcp.set_write_timeout(Some(WRITE_TIMEOUT))
        .map_err(|e| Failed::Other(e.to_string()))?;
    let _ = tcp.set_nodelay(true);
    let stream = if target.tls {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|e| Failed::Other(e.to_string()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(target.host.clone())
            .map_err(|e| Failed::Other(e.to_string()))?;
        let conn = rustls::ClientConnection::new(Arc::new(config), name)
            .map_err(|e| Failed::Other(e.to_string()))?;
        Stream::Tls(Box::new(rustls::StreamOwned::new(conn, tcp)))
    } else {
        Stream::Plain(tcp)
    };
    let mut request = url
        .into_client_request()
        .map_err(|e| Failed::Other(e.to_string()))?;
    let auth = format!("Bearer {key}")
        .parse()
        .map_err(|_| Failed::Other("the key is not a header value".into()))?;
    request.headers_mut().insert("Authorization", auth);
    let config = tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(dereth_network_protocol::wire::MAX_MESSAGE_BYTES))
        .max_frame_size(Some(dereth_network_protocol::wire::MAX_MESSAGE_BYTES));
    match tungstenite::client::client_with_config(request, stream, Some(config)) {
        Ok((ws, _)) => Ok(ws),
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(r))) => {
            let code = r.status().as_u16();
            if code == 401 || code == 403 {
                Err(Failed::Refused(code))
            } else {
                Err(Failed::Other(format!("the hub answered {code}")))
            }
        }
        Err(e) => Err(Failed::Other(e.to_string())),
    }
}

/// The link thread: connects, carries, and connects again, until told to stop.
fn run(target: &HubUrl, url: &str, key: &str, side: &Side, clock: &impl Clock) {
    let mut reconnect = Reconnect::default();
    let mut epoch = 0u64;
    while !side.stopping() {
        let mut ws = match connect(target, url, key) {
            Ok(ws) => ws,
            Err(Failed::Refused(status)) => {
                let retry_in = reconnect.after_refusal();
                if !side.tell(0, Event::Refused { status, retry_in }) {
                    return;
                }
                clock.pause(retry_in, &side.stop);
                continue;
            }
            Err(Failed::Other(why)) => {
                let retry_in = reconnect.after_failure();
                if !side.tell(0, Event::Unreachable { why, retry_in }) {
                    return;
                }
                clock.pause(retry_in, &side.stop);
                continue;
            }
        };
        let _ = ws.get_mut().tcp().set_read_timeout(Some(POLL));
        let _ = ws.get_mut().tcp().set_write_timeout(Some(WRITE_TIMEOUT));
        epoch += 1;
        if !side.tell(epoch, Event::Up) {
            return;
        }
        let why = carry(&mut ws, epoch, side, clock);
        let _ = side.tell(epoch, Event::Down(why));
        let retry_in = reconnect.after_link();
        if !side.stopping() {
            clock.pause(retry_in, &side.stop);
        }
    }
}

/// Carries frames both ways on one connection until it is lost: why it was.
fn carry(
    ws: &mut tungstenite::WebSocket<Stream>,
    epoch: u64,
    side: &Side,
    clock: &impl Clock,
) -> String {
    let mut heard = clock.now();
    let mut stalled: Option<Instant> = None;
    loop {
        if side.stopping() {
            let _ = ws.close(None);
            let _ = ws.flush();
            return "stopping".into();
        }
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                heard = clock.now();
                if side.waiting.fetch_add(1, Ordering::SeqCst) >= MAX_WAITING_FRAMES {
                    side.waiting.fetch_sub(1, Ordering::SeqCst);
                    let _ = ws.close(None);
                    return "the hub sends faster than this world takes its frames".into();
                }
                if !side.tell(epoch, Event::Frame(t.to_string())) {
                    return "the world has gone".into();
                }
            }
            Ok(tungstenite::Message::Close(_)) => return "closed by the hub".into(),
            Ok(_) => heard = clock.now(),
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return e.to_string(),
        }
        while let Ok((for_epoch, frame)) = side.out.try_recv() {
            if side.stopping() {
                return "stopping".into();
            }
            if for_epoch != epoch {
                continue;
            }
            if let Err(e) = ws.write(tungstenite::Message::Text(frame.into())) {
                return e.to_string();
            }
        }
        match ws.flush() {
            Ok(()) => stalled = None,
            Err(tungstenite::Error::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {
                // A hub that takes nothing for longer than a write may wait is gone.
                let since = *stalled.get_or_insert_with(|| clock.now());
                if clock.now().saturating_duration_since(since) > WRITE_TIMEOUT {
                    return "the hub stopped reading".into();
                }
            }
            Err(e) => return e.to_string(),
        }
        if clock.now().saturating_duration_since(heard) > SILENT_FOR {
            let _ = ws.close(None);
            return "the hub fell silent".into();
        }
    }
}

#[cfg(test)]
mod tests;
