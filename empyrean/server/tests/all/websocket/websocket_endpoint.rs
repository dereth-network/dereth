//! Divergence: the WebSocket endpoint is Empyrean's own; ACE speaks UDP only.
//! Behaviour: none (a transport this server adds; the claims are its contract, specified in
//! docs/networking/05-websocket-frame.md).
//! A client logs in over the endpoint with the same handshake as over UDP; a page origin not
//! listed is refused; a dropped socket's will logs its session off; the per-address limit and the
//! idle timeout each hold; wss:// serves the configured certificate, and a certificate without its key
//! is refused.
//! Fixture: loopback sockets, the shared client transport and a stand-in account table.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dereth_transport::web_frame;
use dereth_transport::wire::packet::ParsedPacket;
use empyrean_common::clock::SystemClock;
use empyrean_net::driver::udp::{Inbound, UdpDriver};
use empyrean_net::testing::{ClientStatus, TestAccounts, TestClient};
use empyrean_net::{ClockSnapshot, Event, NetConfig, Outgoing, PortKind, ServerNet};
use empyrean_server::websocket::{EndpointConfig, WebSocketEndpoint};
use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

const LOOPBACK: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

/// The nominal server a web client addresses: its frames name these ports whatever the server
/// really listens on.
const NOMINAL: SocketAddr = SocketAddr::new(LOOPBACK, 9000);

/// Either wire the server runs on in these tests.
enum Wire {
    Udp(UdpDriver),
    Ws(WebSocketEndpoint),
}

impl Wire {
    fn recv(&self, wait: Duration) -> Vec<Inbound> {
        let first = match self {
            Self::Udp(d) => d.recv_timeout(wait),
            Self::Ws(e) => e.recv_timeout(wait),
        };
        let rest = std::iter::from_fn(|| match self {
            Self::Udp(d) => d.try_recv(),
            Self::Ws(e) => e.try_recv(),
        });
        first.into_iter().chain(rest).collect()
    }

    fn transmit(&self, out: &Outgoing) {
        match self {
            Self::Udp(d) => {
                let _ = d.transmit(out);
            }
            Self::Ws(e) => {
                let _ = e.transmit(out);
            }
        }
    }
}

/// A server on `wire` that answers every login, reporting its events.
struct Server {
    stop: Arc<AtomicBool>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    fn start(wire: Wire, port: u16) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, events) = mpsc::channel();
        let stop_server = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let start = Instant::now();
            let system = SystemClock::new();
            let clock = || ClockSnapshot::take(&system, start.elapsed().as_secs_f64());
            let mut net = ServerNet::new(
                NetConfig {
                    port,
                    ..NetConfig::default()
                },
                empyrean_world::network::game_messages::game_message::transport_messages(),
            );
            let mut accounts = TestAccounts::default();
            while !stop_server.load(Ordering::SeqCst) {
                let received = wire.recv(Duration::from_millis(1));
                let now = clock();
                for d in received {
                    net.on_datagram(d.port_kind, d.from, &d.bytes, now);
                }
                let events: Vec<Event> = net.events().collect();
                for e in events {
                    if let Event::LoginRequest {
                        session, request, ..
                    } = &e
                    {
                        accounts.answer(&mut net, *session, request, clock());
                    }
                    let _ = tx.send(e);
                }
                net.do_session_work(clock());
                let outgoing: Vec<Outgoing> = net.drain_outgoing().collect();
                for out in &outgoing {
                    wire.transmit(out);
                }
            }
            if let Wire::Ws(e) = wire {
                e.shutdown();
            } else if let Wire::Udp(d) = wire {
                d.shutdown();
            }
        });
        Self {
            stop,
            events,
            thread: Some(thread),
        }
    }

    /// Waits up to `limit` for an event `pick` accepts.
    fn wait_for<T>(&self, limit: Duration, mut pick: impl FnMut(&Event) -> Option<T>) -> Option<T> {
        let end = Instant::now() + limit;
        while Instant::now() < end {
            if let Ok(e) = self.events.recv_timeout(Duration::from_millis(10)) {
                if let Some(t) = pick(&e) {
                    return Some(t);
                }
            }
        }
        None
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A header's flags, for comparing exchanges.
fn flags(bytes: &[u8], from_client: bool) -> u32 {
    let parsed = if from_client {
        ParsedPacket::parse_from_client(bytes)
    } else {
        ParsedPacket::parse(bytes)
    };
    parsed.map(|p| p.header.header.0).unwrap_or(u32::MAX)
}

/// One datagram of an exchange: its direction (true for client to server) and its header flags.
type Step = (bool, u32);

/// A read that found nothing before the socket's read timeout. Windows sometimes reports that
/// timeout on its overlapped sockets as `ERROR_IO_PENDING` (997) rather than as a timeout.
fn timed_out(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ) || (cfg!(windows) && e.raw_os_error() == Some(997))
}

/// A WebSocket client of the endpoint at `url` with page origin `origin`.
fn open(
    url: &str,
    origin: Option<&str>,
) -> tungstenite::Result<WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>> {
    let mut request = url.into_client_request()?;
    if let Some(origin) = origin {
        request
            .headers_mut()
            .insert("Origin", origin.parse().expect("an origin"));
    }
    let (ws, _) = tungstenite::connect(request)?;
    if let tungstenite::stream::MaybeTlsStream::Plain(s) = ws.get_ref() {
        s.set_read_timeout(Some(Duration::from_millis(1)))
            .expect("a read timeout");
    }
    Ok(ws)
}

fn endpoint(config: EndpointConfig) -> (WebSocketEndpoint, String) {
    let e = WebSocketEndpoint::bind(config).expect("the endpoint binds");
    let url = format!("ws://{}/", e.local_addr());
    (e, url)
}

/// Logs a test client in over a WebSocket, returning the exchange up to the connected state.
fn log_in_over_websocket<S: std::io::Read + std::io::Write>(
    ws: &mut WebSocket<S>,
    client: &mut TestClient,
) -> Vec<Step> {
    let start = Instant::now();
    let mut steps = Vec::new();
    while client.status() != ClientStatus::Connected && start.elapsed() < Duration::from_secs(5) {
        client.tick(start.elapsed().as_secs_f64());
        for (to, bytes) in client.take_outgoing() {
            steps.push((true, flags(&bytes, true)));
            ws.send(Message::Binary(web_frame::encode(to.port(), &bytes).into()))
                .expect("sent");
        }
        match ws.read() {
            Ok(Message::Binary(m)) => {
                let (port, datagram) = web_frame::decode(&m).expect("a frame");
                steps.push((false, flags(datagram, false)));
                client.handle_datagram(
                    SocketAddr::new(NOMINAL.ip(), port),
                    datagram,
                    start.elapsed().as_secs_f64(),
                );
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if timed_out(&e) => {}
            Err(e) => panic!("the connection failed: {e}"),
        }
    }
    steps
}

/// A client logs in over the WebSocket endpoint with the same handshake it makes over UDP: one
/// login request, the server's connect request, one connect response, from the same packets.
#[test]
fn a_websocket_login_makes_the_same_handshake_as_a_udp_one() {
    // Over UDP.
    let udp = UdpDriver::bind(LOOPBACK, 0).expect("bind P and P + 1");
    let server_addr = udp.local_addr(PortKind::C2S).expect("addr");
    let udp_server = Server::start(Wire::Udp(udp), server_addr.port());
    let socket = std::net::UdpSocket::bind(SocketAddr::new(LOOPBACK, 0)).expect("socket");
    socket
        .set_read_timeout(Some(Duration::from_millis(1)))
        .expect("timeout");
    let mut client = TestClient::new(socket.local_addr().unwrap(), server_addr, "over-udp", "pw");
    let start = Instant::now();
    let mut udp_steps: Vec<Step> = Vec::new();
    let mut buf = [0u8; 2048];
    while client.status() != ClientStatus::Connected && start.elapsed() < Duration::from_secs(5) {
        client.tick(start.elapsed().as_secs_f64());
        for (to, bytes) in client.take_outgoing() {
            udp_steps.push((true, flags(&bytes, true)));
            socket.send_to(&bytes, to).expect("send");
        }
        if let Ok((n, from)) = socket.recv_from(&mut buf) {
            udp_steps.push((false, flags(&buf[..n], false)));
            client.handle_datagram(from, &buf[..n], start.elapsed().as_secs_f64());
        }
    }
    assert_eq!(client.status(), ClientStatus::Connected, "over UDP");
    drop(udp_server);

    // Over the WebSocket endpoint, the client addressing its nominal ports.
    let (ws_endpoint, url) = endpoint(EndpointConfig::plain(SocketAddr::new(LOOPBACK, 0), &[]));
    let ws_server = Server::start(Wire::Ws(ws_endpoint), 9000);
    let mut ws = open(&url, None).expect("the endpoint accepts a client without an origin");
    let local = match ws.get_ref() {
        tungstenite::stream::MaybeTlsStream::Plain(s) => s.local_addr().unwrap(),
        _ => unreachable!("a plain connection"),
    };
    let mut ws_client = TestClient::new(local, NOMINAL, "over-websocket", "pw");
    let ws_steps = log_in_over_websocket(&mut ws, &mut ws_client);
    assert_eq!(
        ws_client.status(),
        ClientStatus::Connected,
        "over the WebSocket"
    );
    assert_eq!(
        (
            ws_client.login_requests_sent,
            ws_client.connect_responses_sent
        ),
        (client.login_requests_sent, client.connect_responses_sent),
        "one login request and one connect response each"
    );
    assert!(udp_steps.len() >= 3 && ws_steps.len() >= 3);
    assert_eq!(
        ws_steps[..3],
        udp_steps[..3],
        "the handshake's three datagrams carry the same headers"
    );
    assert!(
        ws_server
            .wait_for(Duration::from_secs(2), |e| matches!(
                e,
                Event::ConnectResponse { .. }
            )
            .then_some(()))
            .is_some(),
        "the world hears the connect response"
    );
}

/// A browser page may connect only from a listed origin; a page from any other origin is refused
/// at the handshake, and nothing reaches the sessions.
#[test]
fn a_page_from_an_origin_not_listed_is_refused() {
    let (e, url) = endpoint(EndpointConfig::plain(
        SocketAddr::new(LOOPBACK, 0),
        &["https://play.example.org"],
    ));
    match open(&url, Some("https://elsewhere.example.org")) {
        Err(tungstenite::Error::Http(r)) => assert_eq!(r.status(), 403),
        Err(other) => panic!("refused, but not with 403: {other}"),
        Ok(_) => panic!("a page from an origin not listed was admitted"),
    }
    assert!(open(&url, Some("https://play.example.org")).is_ok());
    // With no origins listed, no page at all.
    let (_none, closed) = endpoint(EndpointConfig::plain(SocketAddr::new(LOOPBACK, 0), &[]));
    assert!(open(&closed, Some("https://play.example.org")).is_err());
    assert!(e.try_recv().is_none(), "nothing reached the sessions");
}

/// A page that goes away without logging off still logs off: when its socket drops, the will it
/// handed over is delivered as if it had sent it, and the session ends at once.
#[test]
fn the_will_logs_the_session_off_when_the_socket_drops() {
    let (e, url) = endpoint(EndpointConfig::plain(SocketAddr::new(LOOPBACK, 0), &[]));
    let server = Server::start(Wire::Ws(e), 9000);
    let mut ws = open(&url, None).expect("open");
    let local = match ws.get_ref() {
        tungstenite::stream::MaybeTlsStream::Plain(s) => s.local_addr().unwrap(),
        _ => unreachable!("a plain connection"),
    };
    let mut client = TestClient::new(local, NOMINAL, "leaves-a-will", "pw");
    log_in_over_websocket(&mut ws, &mut client);
    assert_eq!(client.status(), ClientStatus::Connected);
    // The log-off, handed over as the will instead of being sent.
    client.log_off();
    let will: Vec<(u16, Vec<u8>)> = client
        .take_outgoing()
        .into_iter()
        .map(|(to, bytes)| (to.port(), bytes))
        .collect();
    assert!(!will.is_empty(), "the log-off has datagrams");
    ws.send(Message::Binary(web_frame::encode_will(&will).into()))
        .expect("the will sent");
    ws.flush().expect("flushed");
    // Gone without a close handshake, as a killed page's socket goes.
    drop(ws);
    let ended = server.wait_for(Duration::from_secs(3), |e| {
        matches!(e, Event::Disconnected { .. }).then_some(())
    });
    assert!(ended.is_some(), "the session ended when the socket dropped");
}

/// Whether `ws` is closed by the endpoint within `within`.
fn closed(
    ws: &mut WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
    within: Duration,
) -> bool {
    let end = Instant::now() + within;
    while Instant::now() < end {
        match ws.read() {
            Ok(Message::Close(_))
            | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                return true
            }
            Err(tungstenite::Error::Io(e)) if timed_out(&e) => {}
            Err(_) => return true,
            Ok(_) => {}
        }
    }
    false
}

/// No more connections are open at once from one address than the limit allows: a second one is
/// closed and the first stays open.
#[test]
fn a_connection_beyond_the_per_address_limit_is_closed() {
    let mut config = EndpointConfig::plain(SocketAddr::new(LOOPBACK, 0), &[]);
    config.max_per_ip = Some(1);
    // Far beyond the test's length, so only the limit can close anything.
    config.idle_timeout = Duration::from_secs(600);
    let (e, url) = endpoint(config);
    let mut first = open(&url, None).expect("the first connection");
    let mut second = open(&url, None).expect("the handshake completes");
    assert!(
        closed(&mut second, Duration::from_secs(5)),
        "a second connection from the address is closed"
    );
    assert!(
        !closed(&mut first, Duration::from_millis(300)),
        "the first stays open"
    );
    let end = Instant::now() + Duration::from_secs(2);
    while e.connections() > 1 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(e.connections(), 1, "only the first is open");
}

/// A connection that sends nothing for the idle timeout is closed, not before it, and nothing is
/// left open.
#[test]
fn an_idle_connection_is_closed_after_the_idle_timeout() {
    let idle = Duration::from_millis(300);
    let mut config = EndpointConfig::plain(SocketAddr::new(LOOPBACK, 0), &[]);
    config.idle_timeout = idle;
    let (e, url) = endpoint(config);
    let opened = Instant::now();
    let mut ws = open(&url, None).expect("the connection");
    assert!(
        closed(&mut ws, Duration::from_secs(5)),
        "an idle connection is closed"
    );
    assert!(
        opened.elapsed() >= idle,
        "closed after {:?}, before the idle timeout",
        opened.elapsed()
    );
    let end = Instant::now() + Duration::from_secs(2);
    while e.connections() > 0 && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(e.connections(), 0, "nothing is left open");
}

/// With a certificate and key the endpoint serves `wss://`: a client that trusts the certificate
/// logs in over TLS, and a certificate and key that do not belong together are refused.
#[test]
fn wss_serves_the_configured_certificate() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("websocket-tls");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    let made = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
        .expect("a self-signed certificate");
    let (cert, key) = (dir.join("cert.pem"), dir.join("key.pem"));
    std::fs::write(&cert, made.cert.pem()).expect("written");
    std::fs::write(&key, made.signing_key.serialize_pem()).expect("written");
    let other = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
        .expect("a second certificate");
    let other_key = dir.join("other-key.pem");
    std::fs::write(&other_key, other.signing_key.serialize_pem()).expect("written");
    assert!(
        empyrean_server::websocket::tls_config(&cert, &other_key).is_err(),
        "a key that is not the certificate's is refused"
    );

    let mut config = EndpointConfig::plain(SocketAddr::new(LOOPBACK, 0), &[]);
    config.tls = Some(Arc::new(
        empyrean_server::websocket::tls_config(&cert, &key).expect("the certificate and key"),
    ));
    let e = WebSocketEndpoint::bind(config).expect("the endpoint binds");
    assert!(e.tls());
    let port = e.local_addr().port();
    let _server = Server::start(Wire::Ws(e), 9000);

    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(made.cert.der().clone())
        .expect("the certificate as a root");
    let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .expect("protocols")
    .with_root_certificates(roots)
    .with_no_client_auth();
    let tcp = TcpStream::connect(SocketAddr::new(LOOPBACK, port)).expect("connect");
    let local = tcp.local_addr().unwrap();
    let tls = rustls::StreamOwned::new(
        rustls::ClientConnection::new(Arc::new(client_config), "localhost".try_into().unwrap())
            .expect("a TLS client"),
        tcp,
    );
    let (mut ws, _) = tungstenite::client(format!("wss://localhost:{port}/"), tls)
        .expect("the TLS WebSocket handshake");
    ws.get_ref()
        .sock
        .set_read_timeout(Some(Duration::from_millis(1)))
        .expect("a read timeout");
    let mut client = TestClient::new(local, NOMINAL, "over-wss", "pw");
    log_in_over_websocket(&mut ws, &mut client);
    assert_eq!(
        client.status(),
        ClientStatus::Connected,
        "logged in over wss://"
    );
}
