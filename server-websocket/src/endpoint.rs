//! The WebSocket endpoint, one more kind of client endpoint beside a server's UDP listeners.
//!
//! A browser cannot send UDP, so a browser client sends each datagram as one binary WebSocket
//! message behind a two-byte port (`dereth_transport::web_frame`, specified in
//! `docs/networking/05-websocket-frame.md`). Each connection is one client address to the
//! sessions: its datagrams reach the server's sessions exactly as a UDP client's do, each with the
//! index of the port the frame names in the connection's run of them (`web_frame::PortRun`), and
//! what the sessions send to that address goes back over the same connection, framed with the port
//! it came from. Everything above the transport (the packets, the checksums and the key stream,
//! the sessions, the world) is the same. The server says how many ports a connection numbers, and
//! what each index is to it.
//!
//! One thread accepts, and one thread per connection carries its frames both ways, polling its
//! socket every [`POLL`] and the datagrams queued for it in between, as the web relay does. What a
//! connection received goes into one channel the server's thread drains ([`Endpoint::try_recv`]),
//! beside its UDP listeners.
//!
//! The rules it keeps:
//!
//! - **TLS by default.** With a certificate and key it serves `wss://` (rustls). Plain `ws://` only
//!   where the settings allow it ([`WebSocketSettings::validate`]): on loopback, or behind a
//!   reverse proxy that terminates TLS.
//! - **Origins.** A browser page may connect only from an origin the settings list; none listed
//!   refuses every page. A connection that sends no `Origin` is not a browser page, and is
//!   accepted.
//! - **The client's address** is the connection's peer, or, from a trusted proxy, the last address
//!   in its `X-Forwarded-For` header; with the peer's port it is the session's address.
//! - **Limits.** Connections open at once from one client address, and an idle timeout, after
//!   which the connection is closed.
//! - **The will.** When a connection closes, however it closes, its will's datagrams are delivered
//!   as if the client had sent them, so a page that went away logs off at once.
//!
//! A connection's address stays reserved for a while after it closes ([`RETIRED_FOR`]), so what the
//! sessions still send to it is dropped here rather than sent as a UDP datagram to the same
//! address. A UDP client and a WebSocket connection could in principle share an address (the same
//! IP, and a UDP port equal to a TCP port); the WebSocket connection's traffic then wins.

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dereth_transport::web_frame::{self, PortRun};
use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::{Message, WebSocket};

use crate::settings::WebSocketSettings;

/// How long a connection's read waits before it looks at what is queued for it.
pub const POLL: Duration = Duration::from_millis(2);

/// How long the TLS and WebSocket handshakes may take.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a closed connection's address stays reserved.
pub const RETIRED_FOR: Duration = Duration::from_secs(600);

/// A datagram a connection received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inbound {
    /// The port the frame named, as its index in the connection's run of ports.
    pub port: u16,
    /// The connection's session address.
    pub from: SocketAddr,
    pub bytes: Vec<u8>,
}

/// The endpoint's settings, checked.
#[derive(Debug, Clone)]
pub struct EndpointConfig {
    pub listen: SocketAddr,
    /// `None` serves plain `ws://`.
    pub tls: Option<Arc<rustls::ServerConfig>>,
    pub allowed_origins: Vec<String>,
    pub trusted_proxies: Vec<IpAddr>,
    /// `None` is unlimited.
    pub max_per_ip: Option<u32>,
    pub idle_timeout: Duration,
}

impl EndpointConfig {
    /// The endpoint's settings from `[server.websocket]`, the certificate and key read. `base` is
    /// the directory relative paths are resolved against.
    ///
    /// # Errors
    /// A setting that cannot be served, or a certificate or key that cannot be read.
    pub fn from_settings(
        s: &WebSocketSettings,
        resolve: impl Fn(&str) -> std::path::PathBuf,
    ) -> Result<Self, String> {
        let (listen, tls) = s.validate().map_err(|e| e.to_string())?;
        let tls = if tls {
            Some(Arc::new(tls_config(
                &resolve(&s.tls_certificate),
                &resolve(&s.tls_private_key),
            )?))
        } else {
            None
        };
        Ok(Self {
            listen,
            tls,
            allowed_origins: s.allowed_origins.clone(),
            trusted_proxies: s.trusted_proxy_addresses().map_err(|e| e.to_string())?,
            max_per_ip: u32::try_from(s.maximum_connections_per_ip_address).ok(),
            idle_timeout: Duration::from_secs(u64::from(s.idle_timeout.max(1))),
        })
    }

    /// A plain loopback endpoint on `listen` that admits `origins`, for tests.
    #[must_use]
    pub fn plain(listen: SocketAddr, origins: &[&str]) -> Self {
        Self {
            listen,
            tls: None,
            allowed_origins: origins.iter().map(|o| (*o).to_owned()).collect(),
            trusted_proxies: Vec::new(),
            max_per_ip: None,
            idle_timeout: Duration::from_secs(60),
        }
    }
}

/// The server's TLS settings: the PEM certificate chain and private key.
///
/// # Errors
/// A file that cannot be read or parsed, or a key that does not suit the certificate.
pub fn tls_config(
    certificate: &std::path::Path,
    key: &std::path::Path,
) -> Result<rustls::ServerConfig, String> {
    use rustls::pki_types::pem::PemObject;
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};
    let chain = CertificateDer::pem_file_iter(certificate)
        .and_then(|it| it.collect::<Result<Vec<_>, _>>())
        .map_err(|e| format!("{}: {e}", certificate.display()))?;
    if chain.is_empty() {
        return Err(format!("{}: no certificate", certificate.display()));
    }
    let key = PrivateKeyDer::from_pem_file(key).map_err(|e| format!("{}: {e}", key.display()))?;
    rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .map_err(|e| format!("the certificate and key: {e}"))
}

/// Whether a page from `origin` may connect: no `Origin` (not a browser page), or one listed,
/// compared without case and without a trailing slash.
#[must_use]
pub fn origin_allowed(origin: Option<&str>, allowed: &[String]) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    let origin = origin.trim_end_matches('/');
    allowed
        .iter()
        .any(|a| a.trim_end_matches('/').eq_ignore_ascii_case(origin))
}

/// The client's address: from a trusted proxy, the last address its `X-Forwarded-For` names;
/// otherwise the peer's.
#[must_use]
pub fn client_ip(peer: IpAddr, forwarded_for: Option<&str>, trusted: &[IpAddr]) -> IpAddr {
    if !trusted.contains(&peer) {
        return peer;
    }
    forwarded_for
        .and_then(|v| v.rsplit(',').next())
        .and_then(|a| a.trim().parse().ok())
        .unwrap_or(peer)
}

/// Whether a connection last heard from at `heard` has gone quiet for longer than `timeout` by `now`.
#[must_use]
pub fn gone_idle(heard: Instant, now: Instant, timeout: Duration) -> bool {
    now.saturating_duration_since(heard) > timeout
}

/// One datagram for a connection: the index of the server port it comes from, and its bytes.
type Queued = (u16, Vec<u8>);

/// An open connection.
#[derive(Debug)]
struct Open {
    queue: Sender<Queued>,
    /// The port of its first datagram, once it has sent one.
    first_port: Option<u16>,
}

/// What the endpoint shares between its threads.
#[derive(Debug, Default)]
struct Shared {
    /// Each open connection's queue, by session address.
    open: HashMap<SocketAddr, Open>,
    /// Addresses of closed connections, and when they closed.
    retired: HashMap<SocketAddr, Instant>,
    /// Open connections per client address.
    per_ip: HashMap<IpAddr, u32>,
}

/// Who can ask the endpoint how an open connection numbers its ports, from any thread.
#[derive(Debug, Clone)]
pub struct Numbering {
    shared: Arc<Mutex<Shared>>,
}

impl Numbering {
    /// The port the open connection at `address` numbers the port of `index` as; `None` when
    /// `address` is no open connection's, or it has sent no datagram yet.
    #[must_use]
    pub fn port(&self, address: SocketAddr, index: u16) -> Option<u16> {
        let first = lock(&self.shared).open.get(&address)?.first_port?;
        first.checked_add(index)
    }
}

/// The WebSocket endpoint.
#[derive(Debug)]
pub struct Endpoint {
    local: SocketAddr,
    tls: bool,
    rx: Receiver<Inbound>,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl Endpoint {
    /// Binds `config.listen` and starts accepting, for a server of `ports` ports.
    ///
    /// # Errors
    /// The bind error.
    pub fn bind(config: EndpointConfig, ports: u16) -> io::Result<Self> {
        let listener = TcpListener::bind(config.listen)?;
        let local = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        let (tx, rx) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let tls = config.tls.is_some();
        let accept = {
            let (shared, stop) = (Arc::clone(&shared), Arc::clone(&stop));
            let config = Arc::new(config);
            std::thread::Builder::new()
                .name("websocket".to_owned())
                .spawn(move || accept_loop(&listener, &config, ports, &tx, &shared, &stop))?
        };
        Ok(Self {
            local,
            tls,
            rx,
            shared,
            stop,
            accept: Some(accept),
        })
    }

    /// The address it listens on.
    #[must_use]
    pub const fn local_addr(&self) -> SocketAddr {
        self.local
    }

    /// Whether it serves `wss://`.
    #[must_use]
    pub const fn tls(&self) -> bool {
        self.tls
    }

    /// A datagram a connection received, if one is waiting.
    #[must_use]
    pub fn try_recv(&self) -> Option<Inbound> {
        self.rx.try_recv().ok()
    }

    /// Waits up to `timeout` for a datagram.
    #[must_use]
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Inbound> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Sends `bytes`, from the port of `index`, if `to` is a connection's address: `None` when it
    /// is not this endpoint's (it is a UDP client's), `Some(Ok)` when it was queued or dropped for
    /// a closed connection, and `Some(Err)` when the connection's thread has gone.
    #[must_use]
    pub fn transmit(&self, to: SocketAddr, index: u16, bytes: &[u8]) -> Option<io::Result<()>> {
        let mut shared = lock(&self.shared);
        if let Some(open) = shared.open.get(&to) {
            if open.queue.send((index, bytes.to_vec())).is_ok() {
                return Some(Ok(()));
            }
            shared.open.remove(&to);
            shared.retired.insert(to, Instant::now());
            return Some(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
        }
        shared.retired.contains_key(&to).then_some(Ok(()))
    }

    /// How open connections number their ports.
    #[must_use]
    pub fn numbering(&self) -> Numbering {
        Numbering {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Open connections.
    #[must_use]
    pub fn connections(&self) -> usize {
        lock(&self.shared).open.len()
    }

    /// Stops accepting and waits for the accept thread. Open connections close as their threads
    /// see the stop flag, delivering their wills to a server that no longer reads them.
    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.accept.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn accept_loop(
    listener: &TcpListener,
    config: &Arc<EndpointConfig>,
    ports: u16,
    tx: &Sender<Inbound>,
    shared: &Arc<Mutex<Shared>>,
    stop: &Arc<AtomicBool>,
) {
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, peer)) => {
                let (config, tx, shared, stop) = (
                    Arc::clone(config),
                    tx.clone(),
                    Arc::clone(shared),
                    Arc::clone(stop),
                );
                let spawned = std::thread::Builder::new()
                    .name(format!("websocket-{peer}"))
                    .spawn(move || {
                        if let Err(e) =
                            connection(stream, peer, &config, ports, &tx, &shared, &stop)
                        {
                            tracing::debug!("WebSocket {peer}: {e}");
                        }
                    });
                if let Err(e) = spawned {
                    tracing::warn!("WebSocket {peer}: no thread for the connection: {e}");
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
                let mut shared = lock(shared);
                shared.retired.retain(|_, at| at.elapsed() < RETIRED_FOR);
            }
            Err(e) => {
                tracing::warn!("WebSocket accept: {e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// A connection's stream: plain, or TLS over it.
enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ServerConnection, TcpStream>>),
}

impl std::fmt::Debug for Stream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Plain(_) => "Stream::Plain",
            Self::Tls(_) => "Stream::Tls",
        })
    }
}

impl Stream {
    fn tcp(&self) -> &TcpStream {
        match self {
            Self::Plain(s) => s,
            Self::Tls(s) => &s.sock,
        }
    }
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

/// Why a connection ended, for the log.
#[derive(Debug)]
enum Ended {
    Closed,
    Idle,
    Stopped,
}

// The handshake callback's error type is the library's own response.
#[allow(clippy::result_large_err)]
fn connection(
    stream: TcpStream,
    peer: SocketAddr,
    config: &EndpointConfig,
    ports: u16,
    tx: &Sender<Inbound>,
    shared: &Arc<Mutex<Shared>>,
    stop: &AtomicBool,
) -> Result<(), String> {
    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(HANDSHAKE_TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    let stream = match &config.tls {
        Some(tls) => Stream::Tls(Box::new(rustls::StreamOwned::new(
            rustls::ServerConnection::new(Arc::clone(tls)).map_err(|e| e.to_string())?,
            stream,
        ))),
        None => Stream::Plain(stream),
    };

    let mut forwarded: Option<String> = None;
    let check = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let origin = req.headers().get("origin").and_then(|v| v.to_str().ok());
        forwarded = req
            .headers()
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if origin_allowed(origin, &config.allowed_origins) {
            Ok(resp)
        } else {
            tracing::info!(
                "WebSocket {peer}: refused the page origin {}",
                origin.unwrap_or("")
            );
            let mut refusal = ErrorResponse::new(Some("origin not allowed".into()));
            *refusal.status_mut() = tungstenite::http::StatusCode::FORBIDDEN;
            Err(refusal)
        }
    };
    let mut ws: WebSocket<Stream> =
        tungstenite::accept_hdr(stream, check).map_err(|e| format!("handshake: {e}"))?;
    ws.get_ref()
        .tcp()
        .set_read_timeout(Some(POLL))
        .map_err(|e| e.to_string())?;

    let ip = client_ip(peer.ip(), forwarded.as_deref(), &config.trusted_proxies);
    let address = SocketAddr::new(ip, peer.port());
    let (out_tx, out_rx) = mpsc::channel::<Queued>();
    {
        let mut s = lock(shared);
        let count = s.per_ip.get(&ip).copied().unwrap_or(0);
        if config.max_per_ip.is_some_and(|max| count >= max) || s.open.contains_key(&address) {
            drop(s);
            tracing::info!(
                "WebSocket {address}: refused, {count} connection(s) already open from {ip}"
            );
            let _ = ws.close(Some(tungstenite::protocol::CloseFrame {
                code: tungstenite::protocol::frame::coding::CloseCode::Policy,
                reason: "too many connections".into(),
            }));
            let _ = ws.flush();
            return Ok(());
        }
        s.per_ip.insert(ip, count + 1);
        s.retired.remove(&address);
        s.open.insert(
            address,
            Open {
                queue: out_tx,
                first_port: None,
            },
        );
    }
    tracing::info!(
        "WebSocket {address}: open{}",
        if peer.ip() == ip {
            String::new()
        } else {
            format!(" (through {})", peer.ip())
        }
    );

    let mut run = PortRun::new(ports);
    let mut will: Vec<Queued> = Vec::new();
    let result = carry(
        &mut ws,
        address,
        config.idle_timeout,
        tx,
        &out_rx,
        &mut run,
        &mut will,
        shared,
        stop,
    );
    for (port, bytes) in will.drain(..) {
        let _ = tx.send(Inbound {
            port,
            from: address,
            bytes,
        });
    }
    {
        let mut s = lock(shared);
        s.open.remove(&address);
        s.retired.insert(address, Instant::now());
        if let Some(n) = s.per_ip.get_mut(&ip) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                s.per_ip.remove(&ip);
            }
        }
    }
    match result {
        Ok(ended) => tracing::info!("WebSocket {address}: closed ({ended:?})"),
        Err(e) => tracing::info!("WebSocket {address}: closed: {e}"),
    }
    Ok(())
}

fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if matches!(io.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut))
}

/// Lets the server ask how the connection numbers its ports, once it has said (before the datagram
/// that said it reaches the server).
fn note_numbering(run: &PortRun, numbered: &mut bool, shared: &Mutex<Shared>, address: SocketAddr) {
    if *numbered {
        return;
    }
    if let Some(first) = run.port(0) {
        if let Some(open) = lock(shared).open.get_mut(&address) {
            open.first_port = Some(first);
        }
        *numbered = true;
    }
}

/// Carries frames both ways until the connection closes, goes idle or the endpoint stops, keeping
/// the connection's latest will.
#[allow(clippy::too_many_arguments)]
fn carry(
    ws: &mut WebSocket<Stream>,
    address: SocketAddr,
    idle_timeout: Duration,
    tx: &Sender<Inbound>,
    out_rx: &Receiver<Queued>,
    run: &mut PortRun,
    will: &mut Vec<Queued>,
    shared: &Mutex<Shared>,
    stop: &AtomicBool,
) -> Result<Ended, String> {
    let mut last_heard = Instant::now();
    let mut numbered = false;
    loop {
        if stop.load(Ordering::SeqCst) {
            let _ = ws.close(None);
            let _ = ws.flush();
            return Ok(Ended::Stopped);
        }
        match ws.read() {
            Ok(Message::Binary(message)) => {
                last_heard = Instant::now();
                if let Some(datagrams) = web_frame::decode_will(&message) {
                    *will = datagrams
                        .into_iter()
                        .filter_map(|(port, d)| Some((run.index(port)?, d)))
                        .collect();
                    note_numbering(run, &mut numbered, shared, address);
                } else if let Some((port, datagram)) = web_frame::decode(&message) {
                    if let Some(index) = run.index(port) {
                        note_numbering(run, &mut numbered, shared, address);
                        if tx
                            .send(Inbound {
                                port: index,
                                from: address,
                                bytes: datagram.to_vec(),
                            })
                            .is_err()
                        {
                            return Ok(Ended::Stopped);
                        }
                    }
                }
            }
            Ok(Message::Close(_)) => return Ok(Ended::Closed),
            Ok(_) => last_heard = Instant::now(),
            Err(e) if would_block(&e) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(Ended::Closed)
            }
            Err(e) => return Err(e.to_string()),
        }
        if gone_idle(last_heard, Instant::now(), idle_timeout) {
            let _ = ws.close(None);
            let _ = ws.flush();
            return Ok(Ended::Idle);
        }
        while let Ok((index, bytes)) = out_rx.try_recv() {
            let Some(port) = run.port(index) else {
                continue;
            };
            let framed = web_frame::encode(port, &bytes);
            match ws.write(Message::Binary(framed.into())) {
                Ok(()) => {}
                Err(e) if would_block(&e) => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        match ws.flush() {
            Ok(()) => {}
            Err(e) if would_block(&e) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return Ok(Ended::Closed)
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}
