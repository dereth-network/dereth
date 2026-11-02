//! The relay run on loopback against a stand-in UDP server, as a browser would reach it: what it
//! carries, what it refuses, and what it logs. The relay's clock is the test's, turned by hand;
//! the tests wait for what the relay does (a datagram, a log line, a closed socket), never for a
//! while.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

use dereth_transport::conn::{build_login_request, ConnectionAuthenticator};
use dereth_transport::web_frame;
use dereth_transport::{OutPacket, PacketFlags, ProtoHeader};
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use crate::client::Proxies;
use crate::config::{Config, Mode, Public};
use crate::limits::{Clock, Limits, Rate, SendRate};
use crate::log::Log;
use crate::origin::WEB_CLIENT;
use crate::relay::Relay;

/// Only guards against a hang: every wait below ends on an event.
const HANG: Duration = Duration::from_secs(20);

/// A login request datagram, as the game client's first.
pub fn login_datagram(account: &str, password: &str) -> Vec<u8> {
    let auth = ConnectionAuthenticator::account_password(account, password);
    let mut p = OutPacket::new(ProtoHeader::default());
    p.add_optional_header(PacketFlags::LOGIN_REQUEST, build_login_request(&auth))
        .expect("a login request");
    p.serialize(None).expect("plaintext")
}

/// The relay's clock, turned by the test.
#[derive(Clone)]
struct TestClock(Arc<AtomicU64>);

impl TestClock {
    fn new() -> Self {
        Self(Arc::new(AtomicU64::new(0)))
    }

    fn clock(&self) -> Clock {
        let at = Arc::clone(&self.0);
        Arc::new(move || Duration::from_millis(at.load(Ordering::SeqCst)))
    }

    fn advance(&self, by: Duration) {
        let ms = u64::try_from(by.as_millis()).expect("a short while");
        self.0.fetch_add(ms, Ordering::SeqCst);
    }
}

/// The relay's log lines, as they come.
struct Lines(Receiver<String>, Vec<String>);

impl Lines {
    /// Waits for a line `wanted` accepts, and answers it.
    fn wait(&mut self, what: &str, wanted: impl Fn(&str) -> bool) -> String {
        loop {
            match self.0.recv_timeout(HANG) {
                Ok(line) => {
                    self.1.push(line.clone());
                    if wanted(&line) {
                        return line;
                    }
                }
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {
                    panic!("no log line {what}: {:#?}", self.1)
                }
            }
        }
    }

    /// Every line so far, waiting for none.
    fn all(&mut self) -> Vec<String> {
        while let Ok(line) = self.0.try_recv() {
            self.1.push(line);
        }
        self.1.clone()
    }
}

/// A stand-in game server on loopback.
fn game() -> (UdpSocket, SocketAddr) {
    let game = UdpSocket::bind("127.0.0.1:0").expect("a game port");
    game.set_read_timeout(Some(HANG)).expect("a hang guard");
    let addr = game.local_addr().expect("its address");
    (game, addr)
}

/// A relay beside `server` on a loopback port, with `change` made to its settings.
fn public(server: SocketAddr, change: impl FnOnce(&mut Public)) -> Config {
    let mut p = Public {
        origins: vec![WEB_CLIENT.to_owned()],
        tls: None,
        proxies: Proxies::default(),
        limits: Limits::PUBLIC,
        announce: None,
    };
    change(&mut p);
    Config {
        listen: "127.0.0.1:0".parse().expect("an address"),
        server,
        mode: Mode::Public(Box::new(p)),
    }
}

/// A player's relay for `server` on a loopback port.
fn local(server: SocketAddr, allow_origin: Option<&str>) -> Config {
    Config {
        listen: "127.0.0.1:0".parse().expect("an address"),
        server,
        mode: Mode::Local {
            allow_origin: allow_origin.map(str::to_owned),
        },
    }
}

/// Starts `config`'s relay on `clock`: its address and its log.
fn start(config: Config, clock: &TestClock) -> (SocketAddr, Lines) {
    let (tx, rx) = std::sync::mpsc::channel();
    let log = Log::to(move |l| {
        let _ = tx.send(l.to_owned());
    });
    let relay = Relay::bind(config, clock.clock(), log).expect("the relay binds");
    (relay.spawn().expect("it serves"), Lines(rx, Vec::new()))
}

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

/// Opens a WebSocket to the relay with these request headers.
fn open(relay: SocketAddr, headers: &[(&str, &str)]) -> Result<Ws, tungstenite::Error> {
    let mut request = format!("ws://{relay}/")
        .into_client_request()
        .expect("a request");
    for (name, value) in headers {
        request.headers_mut().insert(
            tungstenite::http::HeaderName::from_bytes(name.as_bytes()).expect("a name"),
            value.parse().expect("a value"),
        );
    }
    tungstenite::connect(request).map(|(ws, _)| ws)
}

/// The HTTP status a refused WebSocket request was answered with.
fn refused(r: Result<Ws, tungstenite::Error>) -> u16 {
    match r {
        Err(tungstenite::Error::Http(response)) => response.status().as_u16(),
        Err(e) => panic!("refused otherwise: {e}"),
        Ok(_) => panic!("admitted"),
    }
}

const PAGE: (&str, &str) = ("Origin", WEB_CLIENT);

fn send(ws: &mut Ws, message: Vec<u8>) {
    ws.send(Message::Binary(message.into())).expect("sent");
}

fn receive(game: &UdpSocket) -> (Vec<u8>, SocketAddr) {
    let mut buf = [0u8; 2048];
    let (n, from) = game.recv_from(&mut buf).expect("a datagram");
    (buf[..n].to_vec(), from)
}

/// A whole round trip: a WebSocket client's message reaches a UDP "server", and its answer
/// comes back framed with the port it was sent from, in the client's numbering.
#[test]
fn a_datagram_crosses_both_ways() {
    let (game, server) = game();
    let (relay, _) = start(local(server, None), &TestClock::new());
    let mut ws = open(relay, &[]).expect("admitted");
    send(&mut ws, web_frame::encode(9000, b"hello"));
    let (d, session) = receive(&game);
    assert_eq!(d, b"hello");
    game.send_to(b"welcome", session).expect("answered");
    let back = loop {
        if let Message::Binary(b) = ws.read().expect("a message") {
            break b;
        }
    };
    assert_eq!(&back[..], web_frame::encode(9000, b"welcome").as_slice());
}

/// A message to a port outside the session's pair never leaves the relay; the next one to the
/// pair does.
#[test]
fn a_datagram_to_a_port_outside_the_pair_is_dropped() {
    let (game, server) = game();
    let (relay, _) = start(public(server, |_| {}), &TestClock::new());
    let mut ws = open(relay, &[PAGE]).expect("admitted");
    send(&mut ws, web_frame::encode(9000, b"login"));
    assert_eq!(receive(&game).0, b"login");
    for port in [9002, 8999, 22, 53] {
        send(&mut ws, web_frame::encode(port, b"elsewhere"));
    }
    send(&mut ws, web_frame::encode(9000, b"pair"));
    assert_eq!(receive(&game).0, b"pair", "the four before it were dropped");
}

/// A page that goes away without closing its WebSocket still logs off: the relay sends the
/// session's will from the session's own socket.
#[test]
fn a_session_that_drops_its_socket_leaves_its_will() {
    let (game, server) = game();
    let (relay, _) = start(local(server, None), &TestClock::new());
    let mut ws = open(relay, &[]).expect("admitted");
    send(&mut ws, web_frame::encode(9000, b"hello"));
    let (_, session) = receive(&game);
    send(
        &mut ws,
        web_frame::encode_will(&[(9000, b"goodbye".to_vec())]),
    );
    ws.flush().expect("flushed");
    // Gone without a close handshake, as a killed page's socket goes.
    drop(ws);
    let (d, from) = receive(&game);
    assert_eq!(d, b"goodbye");
    assert_eq!(from, session, "from the session's own socket");
}

/// On the player's machine, a page served from elsewhere may open it only when named.
#[test]
fn a_page_served_elsewhere_opens_a_players_relay_only_when_named() {
    let (_game, server) = game();
    let clock = TestClock::new();
    let (named, _) = start(local(server, Some("https://play.example.org")), &clock);
    assert!(open(named, &[("Origin", "https://play.example.org")]).is_ok());
    let (unnamed, _) = start(local(server, None), &clock);
    assert_eq!(
        refused(open(unnamed, &[("Origin", "https://play.example.org")])),
        403
    );
    assert!(open(unnamed, &[("Origin", "http://127.0.0.1:8080")]).is_ok());
}

/// Beside a server, only the origins named are admitted: not another page, not a local one, and
/// not a request with no origin at all.
#[test]
fn beside_a_server_an_origin_not_named_is_refused() {
    let (_game, server) = game();
    let (relay, mut lines) = start(
        public(server, |p| {
            p.origins.push("https://play.example.org".into())
        }),
        &TestClock::new(),
    );
    assert!(open(relay, &[PAGE]).is_ok());
    assert!(open(relay, &[("Origin", "https://play.example.org")]).is_ok());
    assert_eq!(
        refused(open(relay, &[("Origin", "https://evil.example")])),
        403
    );
    assert_eq!(
        refused(open(relay, &[("Origin", "http://127.0.0.1:8080")])),
        403
    );
    assert_eq!(refused(open(relay, &[])), 403, "no origin");
    lines.wait("for the request with no origin", |l| {
        l.contains("refused the page origin (none)")
    });
}

/// Sessions are counted by client address and in total: the one past either limit is refused
/// (429, 503), and a place is given back when a session closes.
#[test]
fn sessions_are_refused_at_the_per_address_and_total_limits() {
    let (_game, server) = game();
    let (relay, mut lines) = start(
        public(server, |p| {
            p.limits.per_ip = Some(2);
            p.limits.sessions = Some(3);
            p.proxies.trusted = vec!["127.0.0.1".parse().expect("a range")];
        }),
        &TestClock::new(),
    );
    let from = |ip: &'static str| [PAGE, ("X-Forwarded-For", ip)];
    let a1 = open(relay, &from("203.0.113.1")).expect("first");
    let _a2 = open(relay, &from("203.0.113.1")).expect("second");
    assert_eq!(refused(open(relay, &from("203.0.113.1"))), 429);
    let _b = open(relay, &from("203.0.113.2")).expect("another address");
    assert_eq!(refused(open(relay, &from("203.0.113.3"))), 503, "full");
    drop(a1);
    lines.wait("for the closed session", |l| {
        l.starts_with("203.0.113.1 ") && l.contains(": closed")
    });
    assert!(
        open(relay, &from("203.0.113.3")).is_ok(),
        "a place given back"
    );
}

/// A forwarded header from a peer that is not the trusted proxy is not believed: the sessions
/// are counted under the peer's own address, so a client cannot pose as many.
#[test]
fn a_spoofed_forwarded_header_from_an_untrusted_peer_is_ignored() {
    let (_game, server) = game();
    let (relay, mut lines) = start(
        public(server, |p| {
            p.limits.per_ip = Some(1);
            p.proxies.trusted = vec!["10.0.0.1".parse().expect("a range")];
        }),
        &TestClock::new(),
    );
    let _first = open(relay, &[PAGE, ("X-Forwarded-For", "203.0.113.5")]).expect("first");
    assert_eq!(
        refused(open(relay, &[PAGE, ("X-Forwarded-For", "203.0.113.6")])),
        429,
        "counted as the peer, not as the address it wrote"
    );
    let line = lines.wait("for the session", |l| l.contains(": session on udp"));
    assert!(line.starts_with("127.0.0.1:"), "{line}");
    assert!(!lines.all().iter().any(|l| l.contains("203.0.113")));
}

/// Handshakes are counted (a peer's own and in all) and timed: a connection that has not finished
/// its handshake by the deadline, on the relay's clock, is cut off.
#[test]
fn handshakes_are_limited_in_count_and_time() {
    let (_game, server) = game();
    let clock = TestClock::new();
    let (relay, mut lines) = start(
        public(server, |p| p.limits.handshakes_per_peer = Some(2)),
        &clock,
    );
    let mut slow = TcpStream::connect(relay).expect("connect");
    slow.write_all(b"GET / HTTP/1.1\r\nHost: x\r\n")
        .expect("half a request");
    let _idle = TcpStream::connect(relay).expect("connect");
    let _third = TcpStream::connect(relay).expect("connect");
    lines.wait("for the third connection's refusal", |l| {
        l.contains("refused before its handshake: 2 already open from that address")
    });
    clock.advance(Limits::PUBLIC.handshake_time + Duration::from_secs(1));
    lines.wait("for the slow handshake", |l| {
        l.contains("handshake took too long")
    });
    slow.set_read_timeout(Some(HANG)).expect("a hang guard");
    let mut rest = Vec::new();
    let _ = slow.read_to_end(&mut rest);
    assert!(rest.is_empty(), "closed with no answer");
}

/// A session silent for the idle limit, on the relay's clock, is closed, and its will sent.
#[test]
fn an_idle_session_is_closed_and_leaves_its_will() {
    let (game, server) = game();
    let clock = TestClock::new();
    let (relay, mut lines) = start(public(server, |_| {}), &clock);
    let mut ws = open(relay, &[PAGE]).expect("admitted");
    send(&mut ws, web_frame::encode(9000, b"hello"));
    let (_, session) = receive(&game);
    send(
        &mut ws,
        web_frame::encode_will(&[(9000, b"goodbye".to_vec())]),
    );
    ws.flush().expect("flushed");
    // The will is in once the relay has read it: a datagram after it shows that.
    send(&mut ws, web_frame::encode(9000, b"after the will"));
    assert_eq!(receive(&game).0, b"after the will");
    clock.advance(Duration::from_secs(61));
    let (d, from) = receive(&game);
    assert_eq!((d.as_slice(), from), (&b"goodbye"[..], session));
    lines.wait("for the idle close", |l| l.contains("closed, idle"));
}

/// A session may send its rate's burst at once; what it sends past that is dropped, and counted
/// in its closing line.
#[test]
fn a_session_over_its_rate_has_the_rest_dropped() {
    let (game, server) = game();
    let (relay, mut lines) = start(
        public(server, |p| {
            p.limits.send = Some(SendRate {
                datagrams: Rate {
                    per_second: 1,
                    burst: 5,
                },
                bytes: Rate {
                    per_second: 1_000_000,
                    burst: 1_000_000,
                },
            });
        }),
        &TestClock::new(),
    );
    let mut ws = open(relay, &[PAGE]).expect("admitted");
    send(
        &mut ws,
        web_frame::encode_will(&[(9000, b"goodbye".to_vec())]),
    );
    for n in 0..8u8 {
        send(&mut ws, web_frame::encode(9000, &[n]));
    }
    ws.close(None).expect("closed");
    let mut carried = Vec::new();
    loop {
        let (d, _) = receive(&game);
        if d == b"goodbye" {
            break;
        }
        carried.push(d[0]);
    }
    assert_eq!(carried, [0, 1, 2, 3, 4]);
    lines.wait("for the closing line", |l| {
        l.contains("3 dropped over its rate")
    });
}

/// The log names each session's client (behind a trusted proxy, the address it names), the UDP
/// address the server sees it from, and the account its login names; never its password.
#[test]
fn the_log_names_the_client_and_its_account_never_the_password() {
    let (game, server) = game();
    let (relay, mut lines) = start(
        public(server, |p| {
            p.proxies.trusted = vec!["127.0.0.1".parse().expect("a range")];
        }),
        &TestClock::new(),
    );
    let mut ws = open(relay, &[PAGE, ("X-Forwarded-For", "203.0.113.5")]).expect("admitted");
    send(
        &mut ws,
        web_frame::encode(9000, &login_datagram("Ironbeard", "hunter2")),
    );
    let (_, session) = receive(&game);
    let line = lines.wait("for the login", |l| l.contains("logging in as"));
    assert!(
        line.starts_with("203.0.113.5 (through 127.0.0.1:"),
        "{line}"
    );
    assert!(line.contains("\"ironbeard\""), "{line}");
    assert!(line.ends_with(&format!("from udp {session}")), "{line}");
    drop(ws);
    lines.wait("for the close", |l| l.contains(": closed"));
    assert!(!lines.all().iter().any(|l| l.contains("hunter2")));
}

/// With a certificate and key of its own the relay serves `wss://`.
#[test]
fn it_serves_wss_with_its_own_certificate() {
    let (game, server) = game();
    let dir = std::env::temp_dir().join(format!("dereth-web-relay-tls-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a folder");
    let made =
        rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).expect("a certificate");
    let (cert, key) = (dir.join("cert.pem"), dir.join("key.pem"));
    std::fs::write(&cert, made.cert.pem()).expect("written");
    std::fs::write(&key, made.signing_key.serialize_pem()).expect("written");
    let (relay, _) = start(
        public(server, |p| p.tls = Some((cert.clone(), key.clone()))),
        &TestClock::new(),
    );
    std::fs::remove_dir_all(&dir).expect("removed");

    let mut roots = rustls::RootCertStore::empty();
    roots.add(made.cert.der().clone()).expect("a root");
    let client = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("protocols")
    .with_root_certificates(roots)
    .with_no_client_auth();
    let tls = rustls::StreamOwned::new(
        rustls::ClientConnection::new(Arc::new(client), "localhost".try_into().expect("a name"))
            .expect("a TLS client"),
        TcpStream::connect(relay).expect("connect"),
    );
    let mut request = format!("wss://localhost:{}/", relay.port())
        .into_client_request()
        .expect("a request");
    request
        .headers_mut()
        .insert("Origin", WEB_CLIENT.parse().expect("a value"));
    let (mut ws, _) = tungstenite::client(request, tls).expect("the wss handshake");
    ws.send(Message::Binary(web_frame::encode(9000, b"over tls").into()))
        .expect("sent");
    assert_eq!(receive(&game).0, b"over tls");
}
