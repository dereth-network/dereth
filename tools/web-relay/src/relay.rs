//! The relay: it accepts browser connections, admits them by origin and by count, and runs each
//! session on a thread of its own.
//!
//! A connection is admitted in steps, each refusing what it must:
//!
//! 1. **Accepted:** counted among the handshakes in progress (by the peer's address too, unless
//!    the peer is a trusted proxy), and given [`Limits::handshake_time`] for its TLS and
//!    WebSocket handshakes together.
//! 2. **The request:** its page origin must be one the relay admits ([`crate::origin`]); its client
//!    address is read ([`crate::client`]); and the client must have a place among the sessions,
//!    by its address and in total. A refusal is an HTTP answer (403, 400, 429 or 503) and a log
//!    line naming the client.
//! 3. **The session:** its own UDP socket, carried by [`crate::session::carry`] until it closes or
//!    goes idle; then its will is sent from that socket, and its place given back.
//!
//! The log names each session's client address, the UDP address it reaches the server from (the
//! address the server's own log names it by) and the account its login names, so an operator can
//! tell which player a character is.

use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tungstenite::handshake::HandshakeError;
use tungstenite::http::StatusCode;
use tungstenite::protocol::WebSocketConfig;

use crate::client::{counted_as, Proxies};
use crate::config::{Config, Mode};
use crate::limits::{Admission, Clock, Limits, Refusal, Ticket};
use crate::log::Log;
use crate::session::{carry, Carry, Counts, Ended, POLL};

/// How long one read during the handshakes waits before the deadline is looked at again.
const HANDSHAKE_SLICE: Duration = Duration::from_millis(100);

/// How long one write may wait on a page that is not reading.
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// The most a session's frames may hold: a datagram is far shorter, a will a few of them.
const MAX_MESSAGE: usize = 128 * 1024;

/// The most a session's unsent frames may hold before the page is taken to have stopped reading.
const MAX_UNSENT: usize = 1024 * 1024;

/// The relay's TLS settings from the PEM certificate chain and private key.
///
/// # Errors
/// A file that cannot be read or parsed, or a key that does not suit the certificate.
pub fn tls_config(certificate: &Path, key: &Path) -> Result<rustls::ServerConfig, String> {
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

/// A connection's stream, plain or TLS over it, with the handshakes' deadline while they last.
struct Stream {
    inner: Inner,
    clock: Clock,
    /// While the handshakes last: when they must be done, on the relay's clock.
    deadline: Option<Duration>,
}

enum Inner {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ServerConnection, TcpStream>>),
}

impl Stream {
    fn tcp(&self) -> &TcpStream {
        match &self.inner {
            Inner::Plain(s) => s,
            Inner::Tls(s) => &s.sock,
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.deadline.is_some_and(|d| (self.clock)() >= d) {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the handshake took too long",
            ));
        }
        let read = match &mut self.inner {
            Inner::Plain(s) => s.read(buf),
            Inner::Tls(s) => s.read(buf),
        };
        match read {
            // A slice of the handshakes' time is up: the handshake goes on until the deadline.
            Err(e) if self.deadline.is_some() && e.kind() == io::ErrorKind::TimedOut => {
                Err(io::ErrorKind::WouldBlock.into())
            }
            r => r,
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match &mut self.inner {
            Inner::Plain(s) => s.write(buf),
            Inner::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.inner {
            Inner::Plain(s) => s.flush(),
            Inner::Tls(s) => s.flush(),
        }
    }
}

/// What every connection's thread shares.
struct Shared {
    config: Config,
    tls: Option<Arc<rustls::ServerConfig>>,
    limits: Limits,
    proxies: Proxies,
    sessions: Arc<Admission>,
    handshakes: Arc<Admission>,
    clock: Clock,
    log: Log,
}

/// A relay bound to its address, not yet serving.
pub struct Relay {
    listener: TcpListener,
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Relay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Relay")
            .field("listen", &self.listener.local_addr().ok())
            .field("server", &self.shared.config.server)
            .finish_non_exhaustive()
    }
}

impl Relay {
    /// Reads the TLS files, if any, and binds the listening address.
    ///
    /// # Errors
    /// A TLS file that does not read, or an address that cannot be bound.
    pub fn bind(config: Config, clock: Clock, log: Log) -> Result<Self, String> {
        let tls = match &config.mode {
            Mode::Public(p) => match &p.tls {
                Some((cert, key)) => Some(Arc::new(tls_config(cert, key)?)),
                None => None,
            },
            Mode::Local { .. } => None,
        };
        let listener =
            TcpListener::bind(config.listen).map_err(|e| format!("{}: {e}", config.listen))?;
        let limits = config.limits();
        let shared = Shared {
            proxies: config.proxies(),
            sessions: Admission::new(limits.per_ip, limits.sessions),
            handshakes: Admission::new(limits.handshakes_per_peer, limits.handshakes),
            limits,
            config,
            tls,
            clock,
            log,
        };
        Ok(Self {
            listener,
            shared: Arc::new(shared),
        })
    }

    /// The address it listens on.
    ///
    /// # Errors
    /// The socket cannot say.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// What it does, in one line, for its start.
    #[must_use]
    pub fn describe(&self) -> String {
        let s = &self.shared;
        let listen = self.listener.local_addr().unwrap_or(s.config.listen);
        let scheme = if s.tls.is_some() { "wss" } else { "ws" };
        let to = format!(
            "relaying {scheme}://{listen} to udp {} and port {}",
            s.config.server,
            s.config.server.port().saturating_add(1)
        );
        match &s.config.mode {
            Mode::Local { allow_origin } => format!(
                "{to}{}",
                allow_origin
                    .as_deref()
                    .map_or_else(String::new, |o| format!(", for local pages and {o}"))
            ),
            Mode::Public(p) => {
                let count =
                    |n: Option<u32>| n.map_or("any number of".to_owned(), |n| n.to_string());
                format!(
                    "{to}, for pages from {}; at most {} sessions from one address and {} in all{}",
                    p.origins.join(", "),
                    count(p.limits.per_ip),
                    count(p.limits.sessions),
                    if p.proxies.trusted.is_empty() {
                        String::new()
                    } else {
                        format!(
                            ", the client named by {} from a trusted proxy",
                            p.proxies.header.name()
                        )
                    }
                )
            }
        }
    }

    /// Serves until the process ends.
    pub fn serve(self) {
        for stream in self.listener.incoming() {
            match stream {
                Ok(stream) => accept(&self.shared, stream),
                Err(e) => self.shared.log.line(&format!("accept: {e}")),
            }
        }
    }

    /// Serves on a thread of its own; answers the address it listens on.
    ///
    /// # Errors
    /// The socket cannot say its address.
    pub fn spawn(self) -> io::Result<SocketAddr> {
        let addr = self.local_addr()?;
        std::thread::Builder::new()
            .name("dereth-web-relay".into())
            .spawn(move || self.serve())?;
        Ok(addr)
    }
}

/// Counts a new connection among the handshakes and starts its thread, or turns it away.
fn accept(shared: &Arc<Shared>, stream: TcpStream) {
    let Ok(peer) = stream.peer_addr() else {
        return;
    };
    // A trusted proxy brings every client's connection: it is counted in the total only.
    let by = (!shared.proxies.trusts(peer.ip())).then(|| counted_as(peer.ip()));
    let ticket = match shared.handshakes.admit(by) {
        Ok(t) => t,
        Err(why) => {
            shared.log.line(&format!(
                "{peer}: refused before its handshake: {why} handshaking"
            ));
            return;
        }
    };
    // The handshakes' time runs from here, not from when the thread gets going.
    let deadline = (shared.clock)() + shared.limits.handshake_time;
    let shared = Arc::clone(shared);
    let spawned = std::thread::Builder::new()
        .name(format!("dereth-web-relay {peer}"))
        .spawn(move || connection(&shared, stream, peer, ticket, deadline));
    if let Err(e) = spawned {
        eprintln!("{peer}: no thread for the connection: {e}");
    }
}

/// The answer to a refused WebSocket request.
fn refusal(status: StatusCode, why: &str) -> ErrorResponse {
    let mut r = ErrorResponse::new(Some(why.to_owned()));
    *r.status_mut() = status;
    r
}

/// Who a session is, for the log: its client address, and the peer it came through when that is
/// another.
fn who(client: IpAddr, peer: SocketAddr) -> String {
    if client == crate::client::canonical(peer.ip()) {
        peer.to_string()
    } else {
        format!("{client} (through {peer})")
    }
}

// The handshake callback's error type is the library's own response.
#[allow(clippy::result_large_err)]
fn connection(
    shared: &Shared,
    tcp: TcpStream,
    peer: SocketAddr,
    handshaking: Ticket,
    deadline: Duration,
) {
    let log = &shared.log;
    let setup = tcp
        .set_nodelay(true)
        .and_then(|()| tcp.set_read_timeout(Some(HANDSHAKE_SLICE)))
        .and_then(|()| tcp.set_write_timeout(Some(WRITE_TIMEOUT)));
    if let Err(e) = setup {
        log.line(&format!("{peer}: {e}"));
        return;
    }
    let inner = match &shared.tls {
        Some(tls) => match rustls::ServerConnection::new(Arc::clone(tls)) {
            Ok(c) => Inner::Tls(Box::new(rustls::StreamOwned::new(c, tcp))),
            Err(e) => {
                log.line(&format!("{peer}: {e}"));
                return;
            }
        },
        None => Inner::Plain(tcp),
    };
    let stream = Stream {
        inner,
        deadline: Some(deadline),
        clock: Arc::clone(&shared.clock),
    };

    let mut admitted: Option<(IpAddr, Ticket)> = None;
    let check = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok());
        let origin = header("origin");
        let client = match shared
            .proxies
            .client(peer.ip(), header(shared.proxies.header.name()))
        {
            Ok(ip) => ip,
            Err(e) => {
                log.line(&format!("{peer}: refused: {e}"));
                return Err(refusal(
                    StatusCode::BAD_REQUEST,
                    "unreadable client address",
                ));
            }
        };
        let page_allowed = match &shared.config.mode {
            Mode::Local { allow_origin } => {
                crate::origin::local_allows(origin, allow_origin.as_deref())
            }
            Mode::Public(p) => crate::origin::public_allows(origin, &p.origins),
        };
        if !page_allowed {
            log.line(&format!(
                "{}: refused the page origin {}",
                who(client, peer),
                origin.unwrap_or("(none)")
            ));
            return Err(refusal(StatusCode::FORBIDDEN, "origin not allowed"));
        }
        match shared.sessions.admit(Some(counted_as(client))) {
            Ok(ticket) => {
                admitted = Some((client, ticket));
                Ok(resp)
            }
            Err(why) => {
                log.line(&format!("{}: refused: {why}", who(client, peer)));
                Err(match why {
                    Refusal::PerAddress(_) => refusal(
                        StatusCode::TOO_MANY_REQUESTS,
                        "too many sessions from this address",
                    ),
                    Refusal::Total(_) => {
                        refusal(StatusCode::SERVICE_UNAVAILABLE, "the relay is full")
                    }
                })
            }
        }
    };
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE))
        .max_write_buffer_size(MAX_UNSENT);
    let mut handshake = tungstenite::accept_hdr_with_config(stream, check, Some(config));
    let mut ws = loop {
        match handshake {
            Ok(ws) => break ws,
            Err(HandshakeError::Interrupted(mid)) => handshake = mid.handshake(),
            Err(HandshakeError::Failure(e)) => {
                // A refusal has its own line already.
                if !matches!(e, tungstenite::Error::Http(_)) {
                    log.line(&format!("{peer}: handshake: {e}"));
                }
                return;
            }
        }
    };
    drop(handshaking);
    let Some((client, _session)) = admitted else {
        return;
    };
    let who = who(client, peer);
    ws.get_mut().deadline = None;
    if let Err(e) = ws.get_ref().tcp().set_read_timeout(Some(POLL)) {
        log.line(&format!("{who}: {e}"));
        return;
    }

    let server = shared.config.server;
    let local: IpAddr = if server.ip().is_loopback() {
        Ipv4Addr::LOCALHOST.into()
    } else {
        Ipv4Addr::UNSPECIFIED.into()
    };
    let udp = match UdpSocket::bind((local, 0)).and_then(|u| u.set_nonblocking(true).map(|()| u)) {
        Ok(u) => u,
        Err(e) => {
            log.line(&format!("{who}: no UDP socket: {e}"));
            return;
        }
    };
    let udp_addr = udp
        .local_addr()
        .map_or_else(|_| "?".to_owned(), |a| a.to_string());
    log.line(&format!(
        "{who}: session on udp {udp_addr} ({} open)",
        shared.sessions.open()
    ));

    let on_login = |account: &str| {
        log.line(&format!(
            "{who}: logging in as {account:?} from udp {udp_addr}"
        ));
    };
    let mut counts = Counts::default();
    let mut last_will = Vec::new();
    let result = carry(
        &mut ws,
        &udp,
        &Carry {
            server,
            clock: &shared.clock,
            idle: shared.limits.idle,
            send: shared.limits.send,
            on_login: &on_login,
        },
        &mut counts,
        &mut last_will,
    );
    for (to, datagram) in &last_will {
        let _ = udp.send_to(datagram, *to);
    }
    let ended = match &result {
        Ok(Ended::Closed) => "closed".to_owned(),
        Ok(Ended::Idle) => "closed, idle".to_owned(),
        Err(e) => format!("closed: {e}"),
    };
    log.line(&format!(
        "{who}: {ended}, after {} datagrams out and {} back{}{}",
        counts.out,
        counts.back,
        if counts.over_rate == 0 {
            String::new()
        } else {
            format!(", {} dropped over its rate", counts.over_rate)
        },
        if last_will.is_empty() {
            String::new()
        } else {
            format!(", and its will of {}", last_will.len())
        }
    ));
}
