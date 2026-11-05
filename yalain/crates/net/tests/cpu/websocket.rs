//! The world's WebSocket endpoint, on loopback: what a connection numbers, who it lets in, how long
//! it stays, and a client on one connection reaching both services of a real `ServerNet`. Fixture:
//! the endpoint on loopback, and WebSocket clients of this process.

use std::io;
use std::net::{IpAddr, Ipv4Addr, TcpStream};
use std::time::Duration;

use dereth_server_websocket::endpoint::{Endpoint, Inbound};
use dereth_server_websocket::settings::WebSocketSettings;
use dereth_transport::conn::{ConnectRequest, Referral};
use dereth_transport::web_frame;
use dereth_transport::wire::{PacketFlags, ParsedPacket};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::StatusCode;
use tungstenite::{Message, WebSocket};
use yalain_net::endpoint::standalone;
use yalain_net::login::build_login;
use yalain_net::websocket::{index_of, port_of, start, world_port, FRIENDS, PORTS, WORLD};
use yalain_net::{fixed_stream, LocalTime, NetConfig, NetEvent, Port, ServerNet};

/// A guard against a hang, not the time a step is expected to take.
const HANG: Duration = Duration::from_secs(120);

/// A plain endpoint on loopback with `more` of its settings.
fn endpoint(more: WebSocketSettings) -> Endpoint {
    let settings = WebSocketSettings {
        enabled: true,
        listen: "127.0.0.1:0".to_owned(),
        ..more
    };
    start(&settings).expect("starts").expect("listening")
}

/// A client of `e`, naming a page's `origin` and the address a proxy says it is forwarding.
fn open(
    e: &Endpoint,
    origin: Option<&str>,
    forwarded: Option<&str>,
) -> Result<WebSocket<TcpStream>, tungstenite::Error> {
    let stream = TcpStream::connect(e.local_addr()).expect("connects");
    let mut request = format!("ws://{}/", e.local_addr()).into_client_request()?;
    let headers = request.headers_mut();
    if let Some(origin) = origin {
        headers.insert("Origin", origin.parse().expect("an origin"));
    }
    if let Some(forwarded) = forwarded {
        headers.insert("X-Forwarded-For", forwarded.parse().expect("an address"));
    }
    match tungstenite::client(request, stream) {
        Ok((ws, _)) => Ok(ws),
        Err(tungstenite::HandshakeError::Failure(e)) => Err(e),
        Err(tungstenite::HandshakeError::Interrupted(_)) => {
            panic!("the stream blocks, so the handshake does not stop half way")
        }
    }
}

fn send(ws: &mut WebSocket<TcpStream>, port: u16, bytes: &[u8]) {
    ws.send(Message::Binary(web_frame::encode(port, bytes).into()))
        .expect("sent");
}

/// The next datagram the endpoint took.
fn next(e: &Endpoint) -> Inbound {
    e.recv_timeout(HANG).expect("a datagram")
}

fn refused_status(got: Result<WebSocket<TcpStream>, tungstenite::Error>) -> StatusCode {
    match got {
        Err(tungstenite::Error::Http(answer)) => answer.status(),
        Err(e) => panic!("another failure: {e}"),
        Ok(_) => panic!("it was let in"),
    }
}

/// The four ports are the service's pairs in a row, each known by its own index.
/// Behaviour: server.operator.websocket-reaches-both-services
#[test]
fn a_connection_numbers_the_login_pair_and_then_the_world_pair() {
    assert_eq!(
        Port::ALL.map(index_of),
        [0, 1, 2, 3],
        "login's even and odd, then the world's"
    );
    for index in 0..FRIENDS {
        assert_eq!(port_of(index).map(index_of), Some(index));
    }
    assert_eq!(
        port_of(FRIENDS),
        None,
        "the friends stream carries no datagrams"
    );
    assert_eq!(FRIENDS + 1, PORTS, "and is the last the connection numbers");
}

/// Only a page from a listed origin is let in; none listed refuses every page, and a client
/// that names no origin is no page and is let in.
/// Behaviour: server.operator.websocket-allowed-origins
#[test]
fn only_a_page_from_a_listed_origin_is_let_in() {
    let e = endpoint(WebSocketSettings {
        allowed_origins: vec!["https://play.example.org".to_owned()],
        ..WebSocketSettings::default()
    });
    assert!(open(&e, Some("https://play.example.org"), None).is_ok());
    assert!(open(&e, None, None).is_ok());
    assert_eq!(
        refused_status(open(&e, Some("https://elsewhere.example.org"), None)),
        StatusCode::FORBIDDEN
    );
    let none_listed = endpoint(WebSocketSettings::default());
    assert_eq!(
        refused_status(open(&none_listed, Some("https://play.example.org"), None)),
        StatusCode::FORBIDDEN
    );
    assert!(open(&none_listed, None, None).is_ok());
}

/// Reads until the connection is closed; whether it was told why.
fn closed(ws: &mut WebSocket<TcpStream>) -> bool {
    ws.get_ref()
        .set_read_timeout(Some(HANG))
        .expect("a read timeout");
    loop {
        match ws.read() {
            Ok(Message::Close(_)) => return true,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                return false
            }
            Err(_) => return true,
        }
    }
}

/// One client address holds as many connections as the setting says: the next is closed, and
/// another address is let in. A closed connection gives its place back.
/// Behaviour: server.operator.websocket-limit-per-address
#[test]
fn a_connection_past_the_limit_for_its_address_is_closed() {
    let e = endpoint(WebSocketSettings {
        maximum_connections_per_ip_address: 1,
        idle_timeout: 86_400,
        trusted_proxies: vec!["127.0.0.1".to_owned()],
        ..WebSocketSettings::default()
    });
    let ann = Some("203.0.113.1");
    let mut first = open(&e, None, ann).expect("the first");
    send(&mut first, 7000, &[1]);
    assert_eq!(next(&e).bytes, [1], "the first is carried");
    let mut second = open(&e, None, ann).expect("its handshake");
    assert!(closed(&mut second), "the second is closed");
    let mut other = open(&e, None, Some("203.0.113.2")).expect("another address");
    send(&mut other, 7000, &[2]);
    assert_eq!(next(&e).bytes, [2], "another address's is carried");
    first.close(None).expect("closes");
    let _ = closed(&mut first);
    // The endpoint takes a moment to see it closed: an attempt too soon is closed in its turn.
    for tries in 0.. {
        let mut again = open(&e, None, ann).expect("its handshake");
        send(&mut again, 7000, &[3]);
        if e.recv_timeout(Duration::from_millis(50)).is_some() {
            break;
        }
        assert!(tries < 2_400, "the place was never given back");
    }
}

/// A connection that sends nothing for the idle timeout is closed, and its will is delivered as
/// if the client had sent it, each datagram to the service its port names.
/// Behaviour: server.operator.websocket-idle-timeout
#[test]
fn an_idle_connection_is_closed_and_its_will_delivered() {
    let e = endpoint(WebSocketSettings {
        idle_timeout: 1,
        ..WebSocketSettings::default()
    });
    let mut ws = open(&e, None, None).expect("let in");
    send(&mut ws, 7000, &[1]);
    assert_eq!(next(&e).port, 0);
    let will = web_frame::encode_will(&[(7003, vec![8]), (7001, vec![9])]);
    ws.send(Message::Binary(will.into())).expect("sent");
    assert!(closed(&mut ws), "closed for going quiet");
    let (a, b) = (next(&e), next(&e));
    assert_eq!((port_of(a.port), a.bytes), (Some(Port::WORLD_ODD), vec![8]));
    assert_eq!((port_of(b.port), b.bytes), (Some(Port::LOGIN_ODD), vec![9]));
}

/// The sessions see a proxied client at the address its trusted proxy names; a client whose
/// proxy is not trusted is the proxy, whatever it says.
/// Behaviour: server.operator.websocket-proxied-client-address
#[test]
fn only_a_trusted_proxy_names_the_client_to_the_sessions() {
    let trusted = endpoint(WebSocketSettings {
        trusted_proxies: vec!["127.0.0.1".to_owned()],
        ..WebSocketSettings::default()
    });
    let client: IpAddr = "203.0.113.9".parse().expect("an address");
    let mut ws = open(&trusted, None, Some("198.51.100.1, 203.0.113.9")).expect("let in");
    send(&mut ws, 7000, &[1]);
    assert_eq!(
        next(&trusted).from.ip(),
        client,
        "the last address it names"
    );
    let stranger = endpoint(WebSocketSettings::default());
    let mut ws = open(&stranger, None, Some("203.0.113.9")).expect("let in");
    send(&mut ws, 7000, &[1]);
    assert_eq!(next(&stranger).from.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
}

/// The login's answer goes back on the connection from the login's own port number, and a
/// connection's client is told the world service by its own numbering, not the service's real
/// port; a client on UDP keeps the real one. A client behind a trusted proxy reaches the world
/// from its login's address, which the same-address rule asks of it.
/// Behaviour: server.operator.websocket-reaches-both-services
#[test]
fn a_proxied_connection_logs_in_and_reaches_the_world_on_the_same_socket() {
    let now = LocalTime(10.0);
    let e = endpoint(WebSocketSettings {
        trusted_proxies: vec!["127.0.0.1".to_owned()],
        ..WebSocketSettings::default()
    });
    let world = "198.51.100.7:9102".parse().expect("an address");
    let mut net = ServerNet::new(NetConfig::retail_ids(world), fixed_stream(1), 0.0, now);
    let numbering = e.numbering();
    net.refer_carried(move |client| world_port(&numbering, client));
    let mut ws = open(&e, None, Some("203.0.113.9")).expect("let in");
    ws.get_ref()
        .set_read_timeout(Some(HANG))
        .expect("a read timeout");

    // Over the wire between client and server, both ways, one step at a time.
    let carried = |net: &mut ServerNet| {
        let d = next(&e);
        net.on_datagram(port_of(d.port).expect("a port"), d.from, &d.bytes, now);
        for out in net.take_datagrams() {
            e.transmit(out.to, index_of(out.from), &out.bytes)
                .expect("the connection's")
                .expect("queued");
        }
        d.from
    };
    let answer = |ws: &mut WebSocket<TcpStream>| loop {
        if let Message::Binary(m) = ws.read().expect("a message") {
            let (port, bytes) = web_frame::decode(&m).expect("a frame");
            return (port, ParsedPacket::parse(bytes).expect("a packet"));
        }
    };

    let login = standalone(
        0,
        0,
        PacketFlags::LOGIN_REQUEST,
        build_login("tester", b"a ticket", 1),
    );
    send(&mut ws, 7000, &login);
    let from = carried(&mut net);
    assert_eq!(from.ip().to_string(), "203.0.113.9");
    let (port, p) = answer(&mut ws);
    assert_eq!(port, 7000, "the handshake comes from the login's even port");
    let cookie = ConnectRequest::from_bytes(&p.optional[&PacketFlags::CONNECT_REQUEST])
        .expect("the handshake")
        .cookie;
    let response = standalone(
        0,
        0,
        PacketFlags::CONNECT_RESPONSE,
        cookie.to_le_bytes().to_vec(),
    );
    send(&mut ws, 7001, &response);
    carried(&mut net);
    let Some(NetEvent::LoggedIn { conn, .. }) = net.poll_event() else {
        panic!("the login finished");
    };

    assert_eq!(
        e.numbering().port(from, WORLD),
        Some(7002),
        "the connection has numbered its ports by the time its datagrams arrive"
    );
    let cookie = net.refer(conn, now).expect("referred");
    net.tick(LocalTime(now.0 + 1.0));
    for out in net.take_datagrams() {
        e.transmit(out.to, index_of(out.from), &out.bytes)
            .expect("the connection's")
            .expect("queued");
    }
    // The clock's packet may come first.
    let (port, p) = loop {
        let (port, p) = answer(&mut ws);
        if p.optional.contains_key(&PacketFlags::REFERRAL) {
            break (port, p);
        }
    };
    assert_eq!(port, 7001, "data comes from the login's odd port");
    let referred = Referral::from_bytes(&p.optional[&PacketFlags::REFERRAL]).expect("referred");
    assert_eq!(referred.addr.port(), 7002, "the connection's numbering");

    let world_login = standalone(
        0,
        0,
        PacketFlags::WORLD_LOGIN_REQUEST,
        cookie.to_le_bytes().to_vec(),
    );
    send(&mut ws, 7002, &world_login);
    assert_eq!(carried(&mut net), from, "the same address");
    let (port, p) = answer(&mut ws);
    assert_eq!(port, 7002, "the world's handshake, on the same socket");
    let cookie = ConnectRequest::from_bytes(&p.optional[&PacketFlags::CONNECT_REQUEST])
        .expect("the world's handshake")
        .cookie;
    let response = standalone(
        0,
        0,
        PacketFlags::CONNECT_RESPONSE,
        cookie.to_le_bytes().to_vec(),
    );
    send(&mut ws, 7003, &response);
    carried(&mut net);
    let joined = std::iter::from_fn(|| net.poll_event())
        .any(|ev| matches!(ev, NetEvent::WorldJoined { login, .. } if login == conn));
    assert!(joined, "the client reached the world");
}

/// A bad certificate file stops the start, in the words of the file it could not read, and so
/// does a port another program holds.
/// Behaviour: server.operator.websocket-settings-refused-as-empyreans
#[test]
fn a_certificate_that_will_not_read_or_a_port_taken_stops_the_start() {
    let unreadable = start(&WebSocketSettings {
        enabled: true,
        listen: "127.0.0.1:0".to_owned(),
        tls_certificate: "no-such-cert.pem".to_owned(),
        tls_private_key: "no-such-key.pem".to_owned(),
        ..WebSocketSettings::default()
    })
    .expect_err("refused");
    assert!(unreadable.contains("no-such-cert.pem"), "{unreadable}");
    let held = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a port");
    let taken = start(&WebSocketSettings {
        enabled: true,
        listen: held.local_addr().expect("its address").to_string(),
        ..WebSocketSettings::default()
    })
    .expect_err("refused");
    assert!(taken.starts_with("WebSocket 127.0.0.1:"), "{taken}");
    assert!(start(&WebSocketSettings::default()).expect("off").is_none());
}
