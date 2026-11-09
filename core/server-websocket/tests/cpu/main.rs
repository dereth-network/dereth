//! Behaviour: none (a transport the servers add; each server's tests hold it to its contract, and
//! these the part neither sees alone: the run of ports a connection numbers, and what holds a
//! connection before and after it is one).
//! Fixture: loopback sockets, and clients of this process.

use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use dereth_server_websocket::endpoint::{gone_idle, holder_of, Endpoint, EndpointConfig, Inbound};
use dereth_transport::web_frame;
use tungstenite::{Message, WebSocket};

/// A guard against a hang, not the time a step is expected to take.
const HANG: Duration = Duration::from_secs(120);

fn endpoint(ports: u16) -> Endpoint {
    let at = SocketAddr::from((Ipv4Addr::LOCALHOST, 0));
    Endpoint::bind(EndpointConfig::plain(at, &[]), ports).expect("binds")
}

fn client(e: &Endpoint) -> WebSocket<TcpStream> {
    let stream = TcpStream::connect(e.local_addr()).expect("connects");
    let url = format!("ws://{}/", e.local_addr());
    let (ws, _) = tungstenite::client(url, stream).expect("the handshake");
    ws
}

fn send(ws: &mut WebSocket<TcpStream>, port: u16, bytes: &[u8]) {
    ws.send(Message::Binary(web_frame::encode(port, bytes).into()))
        .expect("sent");
}

fn next(e: &Endpoint) -> Inbound {
    e.recv_timeout(HANG).expect("a datagram")
}

/// The next frame the client gets: its port and its bytes.
fn frame(ws: &mut WebSocket<TcpStream>) -> (u16, Vec<u8>) {
    loop {
        if let Message::Binary(m) = ws.read().expect("a message") {
            let (port, bytes) = web_frame::decode(&m).expect("a frame");
            return (port, bytes.to_vec());
        }
    }
}

/// A connection numbers a run of the server's ports from its first datagram's: each frame reaches
/// the server with its port's index, a port above the run reaches nothing, and what the server
/// sends from an index comes back framed with that port.
#[test]
fn a_connection_numbers_every_port_of_a_server_from_its_first_datagrams_port() {
    let e = endpoint(4);
    let mut ws = client(&e);
    for (port, index) in [(7000, 0), (7001, 1), (7002, 2), (7003, 3)] {
        send(&mut ws, port, &[1, 2]);
        let d = next(&e);
        assert_eq!((d.port, d.bytes.as_slice()), (index, &[1u8, 2][..]));
    }
    send(&mut ws, 7004, &[9]);
    send(&mut ws, 6999, &[9]);
    send(&mut ws, 7000, &[3]);
    assert_eq!(
        next(&e).bytes,
        [3],
        "the ports outside the run reached nothing"
    );
    let from = next_address(&e, &mut ws);
    for (index, port) in [(0, 7000), (3, 7003)] {
        e.transmit(from, index, &[4])
            .expect("the connection's")
            .expect("queued");
        assert_eq!(frame(&mut ws), (port, vec![4]));
    }
}

/// The address of the connection `ws`, from a datagram it sends.
fn next_address(e: &Endpoint, ws: &mut WebSocket<TcpStream>) -> SocketAddr {
    send(ws, 7000, &[0]);
    next(e).from
}

/// A connection's numbering can be asked from outside it: nothing before its first datagram, and
/// the port of any index after it; nothing once it is closed.
#[test]
fn how_a_connection_numbers_its_ports_can_be_asked_while_it_is_open() {
    let e = endpoint(4);
    let numbering = e.numbering();
    let mut ws = client(&e);
    send(&mut ws, 7100, &[1]);
    let from = next(&e).from;
    assert_eq!(
        numbering.port(from, 2),
        Some(7102),
        "said before it arrives"
    );
    let until = Instant::now() + HANG;
    assert_eq!(
        numbering.port(SocketAddr::from(([192, 0, 2, 1], 1)), 2),
        None
    );
    ws.close(None).expect("closes");
    while numbering.port(from, 2).is_some() {
        assert!(Instant::now() < until, "still numbered");
        let _ = ws.read();
        std::thread::yield_now();
    }
}

/// A connection is idle only once longer than its timeout has passed since it was last heard from.
#[test]
fn a_connection_goes_idle_only_past_its_timeout() {
    let heard = Instant::now();
    let timeout = Duration::from_secs(60);
    assert!(!gone_idle(heard, heard, timeout));
    assert!(!gone_idle(heard, heard + timeout, timeout));
    assert!(gone_idle(
        heard,
        heard + timeout + Duration::from_secs(1),
        timeout
    ));
    assert!(
        !gone_idle(heard + timeout, heard, timeout),
        "a clock behind the last message is not idle"
    );
}

/// Reads until the far side closes: true if it does before the guard.
fn closed_by_server(s: &mut TcpStream) -> bool {
    use std::io::Read as _;
    s.set_read_timeout(Some(HANG)).expect("set");
    let mut buf = [0u8; 64];
    loop {
        match s.read(&mut buf) {
            Ok(0) | Err(_) => return true,
            Ok(_) => {}
        }
    }
}

fn config() -> EndpointConfig {
    EndpointConfig::plain(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)), &[])
}

/// Past as many handshakes at once as its address may hold connections, the next connection from it
/// is closed unanswered. A handshake that ends gives its turn back.
#[test]
fn an_address_holds_no_more_handshakes_at_once_than_connections_it_may_hold() {
    let mut cfg = config();
    cfg.max_per_ip = Some(2);
    cfg.handshake_timeout = HANG;
    let e = Endpoint::bind(cfg, 1).expect("binds");
    // Turns are taken as connections are accepted, in order.
    let mut first = TcpStream::connect(e.local_addr()).expect("connects");
    let second = TcpStream::connect(e.local_addr()).expect("connects");
    let mut third = TcpStream::connect(e.local_addr()).expect("connects");
    assert!(closed_by_server(&mut third), "past the limit");
    // Ending a handshake gives its turn back; another connection may then wait for it.
    use std::io::Write as _;
    first.write_all(b"not a handshake\r\n\r\n").expect("sent");
    assert!(closed_by_server(&mut first), "its handshake failed");
    let until = Instant::now() + HANG;
    let free = loop {
        assert!(Instant::now() < until, "the turn was never given back");
        let mut s = TcpStream::connect(e.local_addr()).expect("connects");
        s.set_read_timeout(Some(Duration::from_millis(100)))
            .expect("set");
        let mut b = [0u8; 8];
        if matches!(std::io::Read::read(&mut s, &mut b), Err(ref e) if e.kind() != std::io::ErrorKind::ConnectionReset)
        {
            break s;
        }
    };
    drop((second, free));
}

/// However many a trusted proxy brings at once, only the limit on all handshakes holds it: its
/// clients are not known yet. Past that limit a connection is closed unanswered.
#[test]
fn handshakes_in_all_are_limited_and_a_trusted_proxy_only_by_that() {
    let mut cfg = config();
    cfg.max_per_ip = Some(1);
    cfg.max_handshakes = 3;
    cfg.handshake_timeout = HANG;
    cfg.trusted_proxies = vec![Ipv4Addr::LOCALHOST.into()];
    let e = Endpoint::bind(cfg, 1).expect("binds");
    let held: Vec<TcpStream> = (0..3)
        .map(|_| TcpStream::connect(e.local_addr()).expect("connects"))
        .collect();
    let mut past = TcpStream::connect(e.local_addr()).expect("connects");
    assert!(closed_by_server(&mut past), "past the limit in all");
    for mut s in held {
        s.set_read_timeout(Some(Duration::from_millis(20)))
            .expect("set");
        let mut b = [0u8; 8];
        let waiting = std::io::Read::read(&mut s, &mut b);
        assert!(
            waiting.is_err_and(|e| e.kind() != std::io::ErrorKind::ConnectionReset),
            "each of the first still has its turn"
        );
    }
}

/// A handshake has its time in all: bytes coming slowly don't lengthen it.
#[test]
fn a_handshake_sent_a_byte_at_a_time_still_ends() {
    use std::io::Write as _;
    let mut cfg = config();
    cfg.handshake_timeout = Duration::from_millis(100);
    let e = Endpoint::bind(cfg, 1).expect("binds");
    let mut s = TcpStream::connect(e.local_addr()).expect("connects");
    s.write_all(b"GET / HTTP/1.1\r\nX-Slow: ").expect("sent");
    let until = Instant::now() + HANG;
    let mut wrote = 0u64;
    let mut read = s.try_clone().expect("clones");
    read.set_nonblocking(true).expect("set");
    loop {
        assert!(Instant::now() < until, "still waiting after {wrote} pieces");
        // Far more often than its time, so that each read's wait alone would never end, and in pieces
        // large enough that the handshake's reader, which gives up at last by itself, doesn't first.
        assert!(wrote < 300, "still waiting after {wrote} pieces");
        if s.write_all(&[b'a'; 200]).is_err() {
            return;
        }
        wrote += 1;
        let mut b = [0u8; 8];
        match std::io::Read::read(&mut read, &mut b) {
            Ok(0) => return,
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return,
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// A client that takes nothing of what it is sent is closed, sending or not, and what waits for it
/// goes with it.
#[test]
fn a_client_that_stops_reading_is_closed() {
    let mut cfg = config();
    cfg.write_stall = Duration::from_millis(200);
    // Far beyond the test's length, so only what waits for the client can close it.
    cfg.idle_timeout = Duration::from_secs(3600);
    let e = Endpoint::bind(cfg, 2).expect("binds");
    let mut ws = client(&e);
    send(&mut ws, 7000, &[1]);
    let from = next(&e).from;
    let chunk = vec![0u8; 60_000];
    let until = Instant::now() + HANG;
    while e.connections() > 0 {
        assert!(Instant::now() < until, "still open");
        let _ = e.transmit(from, 0, &chunk);
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(ws);
}

/// An address counts for a limit as itself; an IPv6 one by its /64, whatever the rest says; one
/// carried in an IPv6 socket's address as the IPv4 address it is.
#[test]
fn a_limit_counts_an_ipv6_address_by_its_64() {
    let a: std::net::IpAddr = "2001:db8:1:2:aaaa::1".parse().expect("parses");
    let b: std::net::IpAddr = "2001:db8:1:2:bbbb::2".parse().expect("parses");
    let c: std::net::IpAddr = "2001:db8:1:3::1".parse().expect("parses");
    assert_eq!(holder_of(a), holder_of(b));
    assert_ne!(holder_of(a), holder_of(c));
    let v4: std::net::IpAddr = "203.0.113.7".parse().expect("parses");
    let mapped: std::net::IpAddr = "::ffff:203.0.113.7".parse().expect("parses");
    assert_eq!(holder_of(v4), v4);
    assert_eq!(holder_of(mapped), v4);
    let other: std::net::IpAddr = "::ffff:203.0.113.8".parse().expect("parses");
    assert_ne!(holder_of(mapped), holder_of(other));
}
