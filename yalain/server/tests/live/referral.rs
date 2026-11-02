//! A world that refers players from elsewhere to another address, played from this machine: the
//! executable started with `--advertise` naming an address nothing answers at, our client logging
//! in over loopback. Each step waits for the client's model or what it sends, with a time limit
//! only against a hang. Fixture: the built executable and the install.

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::{Action, Client};
use yalain_testkit::session::{self as play, Drive};

use crate::session::{must, scratch, serving, start_server_with, LIFE};

/// An address no client reaches anything at (a documentation address).
const ELSEWHERE: &str = "203.0.113.10";

/// Our client on a loopback socket, on the real clock, noting where it sends.
struct Watched {
    socket: UdpSocket,
    client: Client,
    start: Instant,
    buf: Vec<u8>,
    sent_to: Vec<SocketAddr>,
}

impl Watched {
    fn time(&self) -> LocalTime {
        LocalTime(1.0 + self.start.elapsed().as_secs_f64())
    }
}

impl Drive for Watched {
    fn client(&mut self) -> &mut Client {
        &mut self.client
    }

    fn client_ref(&self) -> &Client {
        &self.client
    }

    fn step(&mut self) {
        loop {
            match self.socket.recv_from(&mut self.buf) {
                Ok((n, from)) => {
                    let now = self.time();
                    self.client.handle_datagram(from, &self.buf[..n], now);
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        let now = self.time();
        self.client.frame(now);
        while let Some((to, bytes)) = self.client.poll_transmit() {
            if !self.sent_to.contains(&to) {
                self.sent_to.push(to);
            }
            let _ = self.socket.send_to(&bytes, to);
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    fn now(&self) -> f64 {
        self.time().0
    }
}

/// A world that refers players from elsewhere to an address nothing answers at still takes a
/// player on this machine into the world: its client, logged in over loopback, is sent to the
/// world service over loopback, makes a character and enters it.
/// Behaviour: server.operator.player-on-the-host-enters-a-world-advertised-elsewhere
#[test]
fn a_player_on_the_host_enters_a_world_that_refers_others_elsewhere() {
    let dir = scratch("yalain-live-referral");
    let (port, _server, ()) = serving(&dir, |port| {
        let more = ["--bind", "127.0.0.1", "--advertise", ELSEWHERE];
        (start_server_with(&dir, port, LIFE, &more), ())
    });
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    let login = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let mut s = Watched {
        socket,
        client: play::client(login, "host"),
        start: Instant::now(),
        buf: vec![0; 65_536],
        sent_to: Vec::new(),
    };
    must(play::character_list(&mut s), &dir);
    let me = must(play::create(&mut s, "Hearthkeeper"), &dir);
    s.client().act(Action::PlayCharacter { character: me });
    let until = Instant::now() + Duration::from_secs_f64(play::HANG);
    let world = loop {
        if let Some(to) = s.sent_to.iter().find(|a| a.port() == port + 2) {
            break *to;
        }
        assert!(Instant::now() < until, "no world login was ever sent");
        s.step();
    };
    assert_eq!(
        world,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port + 2),
        "the world login goes where the login was reached"
    );
    must(
        play::until(&mut s, play::HANG, "the player in the world", |c| {
            c.model.player == Some(me) && c.model.player_object().is_some() && c.body.placed()
        }),
        &dir,
    );
    must(play::log_off(&mut s), &dir);
}
