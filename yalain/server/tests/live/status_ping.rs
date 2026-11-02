//! The status ping on the server's login port, read as a Dereth Network hub reads it when it
//! tries a world from outside for its host: its hello, the token back, its ask, and the world's
//! details. Fixture: the authored world on loopback, the hub's own datagrams and reader.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use dereth_network_protocol::probe;

use crate::session::{scratch, serving, start_server, LIFE};

/// How long the test waits for the server, in seconds, before it calls it a hang: shorter than
/// the session tests' guard (a ping is answered at once or never), and still far past what a
/// loaded machine takes. It guards a hang, never a timing.
const HANG: f64 = 120.0;

/// What came back to `socket` for `datagram`, sent again every half second until it comes.
fn asked(socket: &UdpSocket, to: SocketAddr, datagram: &[u8], log: &dyn Fn() -> String) -> Vec<u8> {
    let started = Instant::now();
    let mut buf = [0u8; 2048];
    loop {
        socket.send_to(datagram, to).expect("sent");
        if let Ok((n, from)) = socket.recv_from(&mut buf) {
            assert_eq!(from, to, "answered from the port it was sent to");
            return buf[..n].to_vec();
        }
        assert!(
            started.elapsed().as_secs_f64() < HANG,
            "no answer\n{}",
            log()
        );
    }
}

/// The hub's hello to the login port draws a token, and its ask with that token the world's
/// details as the hub reads them: open, no one in it, Yalain and its version, and the world's name.
/// Behaviour: server.operator.status-ping-on-the-login-port
#[test]
fn the_hubs_status_ping_reads_the_worlds_details_from_the_login_port() {
    let dir = scratch("yalain-live-status-ping");
    let (port, _server, ()) = serving(&dir, |port| (start_server(&dir, port, LIFE), ()));
    let log = || std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
    let started = Instant::now();
    while !log().contains("[listening]") {
        assert!(
            started.elapsed().as_secs_f64() < HANG,
            "never listening\n{}",
            log()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a socket");
    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .expect("a wait");
    let login = SocketAddr::from((Ipv4Addr::LOCALHOST, port));

    let reply = asked(&socket, login, &probe::status_hello(), &log);
    let token = probe::status_token(&reply).expect("a token");
    assert!(reply.len() < probe::REQUEST_LEN, "smaller than the hello");
    let reply = asked(&socket, login, &probe::status_ask(&token), &log);
    let status = probe::Status::parse(&reply).expect("the world's details");
    assert_eq!(status.state, 1, "open");
    assert_eq!(status.players, 0);
    assert_eq!(status.software, "Yalain");
    assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
    assert!(!status.name.is_empty(), "the world's name");
    assert!(log().contains("status ping on the login port"), "{}", log());
}
