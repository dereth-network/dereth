//! Behaviour: none (a transport the servers add; each server's tests hold it to its contract, and
//! these the part neither sees alone: the run of ports a connection numbers).
//! Fixture: loopback sockets, and clients of this process.

use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use dereth_server_websocket::endpoint::{gone_idle, Endpoint, EndpointConfig, Inbound};
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
