//! A world that refers players from elsewhere to another address, played from this machine: the
//! executable started with `--advertise` naming an address nothing answers at, our client logging
//! in over loopback; and a world's referral over a WebSocket, our client's datagrams carried as the
//! frame's messages over its one connection. Each step waits for the client's model or what it
//! sends, with a time limit only against a hang. Fixture: the built executable and the install.

use std::io::ErrorKind;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::Path;
use std::time::{Duration, Instant};

use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::{Action, Client};
use dereth_transport::web_frame;
use tungstenite::client::IntoClientRequest;
use tungstenite::WebSocket;
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

/// The login's port as the client numbers it. The server's real ports are others.
const NOMINAL: u16 = 7000;

/// What the log says before the endpoint's address.
const ENDPOINT_AT: &str = "WebSocket endpoint on ";

/// The address the endpoint took, from the log.
fn endpoint(dir: &Path) -> SocketAddr {
    let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
    log.lines()
        .find_map(|l| {
            let at = l.find(ENDPOINT_AT)? + ENDPOINT_AT.len();
            l[at..].split(' ').next()?.parse().ok()
        })
        .unwrap_or_else(|| panic!("the endpoint's address\n{log}"))
}

/// A client of the endpoint, with the headers a proxy in front of it would send.
fn connect(at: SocketAddr, forwarded: Option<&str>) -> WebSocket<TcpStream> {
    let stream = TcpStream::connect(at).expect("connects");
    let mut request = format!("ws://{at}/")
        .into_client_request()
        .expect("a request");
    if let Some(forwarded) = forwarded {
        request
            .headers_mut()
            .insert("X-Forwarded-For", forwarded.parse().expect("an address"));
    }
    let (ws, _) = tungstenite::client(request, stream).expect("the handshake");
    ws.get_ref()
        .set_read_timeout(Some(Duration::from_millis(1)))
        .expect("a read timeout");
    ws
}

/// Our client over a WebSocket, on the real clock, noting which ports it frames its datagrams to.
struct OverWebSocket {
    ws: WebSocket<TcpStream>,
    client: Client,
    start: Instant,
    /// The address the client last sent to, which it hears each answer from.
    server: IpAddr,
    framed_to: Vec<u16>,
}

impl OverWebSocket {
    fn new(ws: WebSocket<TcpStream>, account: &str) -> Self {
        let server = IpAddr::V4(Ipv4Addr::LOCALHOST);
        Self {
            ws,
            client: play::client(SocketAddr::new(server, NOMINAL), account),
            start: Instant::now(),
            server,
            framed_to: Vec::new(),
        }
    }

    fn time(&self) -> LocalTime {
        LocalTime(1.0 + self.start.elapsed().as_secs_f64())
    }
}

impl Drive for OverWebSocket {
    fn client(&mut self) -> &mut Client {
        &mut self.client
    }

    fn client_ref(&self) -> &Client {
        &self.client
    }

    fn step(&mut self) {
        loop {
            match self.ws.read() {
                Ok(tungstenite::Message::Binary(m)) => {
                    let (port, datagram) = web_frame::decode(&m).expect("a frame");
                    let now = self.time();
                    let from = SocketAddr::new(self.server, port);
                    self.client.handle_datagram(from, datagram, now);
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break
                }
                Err(e) => panic!("the connection failed: {e}"),
            }
        }
        let now = self.time();
        self.client.frame(now);
        while let Some((to, bytes)) = self.client.poll_transmit() {
            self.server = to.ip();
            if !self.framed_to.contains(&to.port()) {
                self.framed_to.push(to.port());
            }
            let framed = web_frame::encode(to.port(), &bytes);
            self.ws
                .send(tungstenite::Message::Binary(framed.into()))
                .expect("sent");
        }
    }

    fn now(&self) -> f64 {
        self.time().0
    }
}

/// A client on one WebSocket logs in, makes a character, is sent to the world by the port its own
/// numbering gives it (not the world service's real one) and plays there, all over that one
/// connection, and the same again through a trusted proxy: the sessions take it at the address the
/// proxy names, and its world login is let in from its login's address.
/// Behaviour: server.operator.websocket-reaches-both-services
/// Behaviour: server.operator.websocket-proxied-client-address
#[test]
fn a_client_on_one_websocket_logs_in_and_reaches_the_world() {
    let dir = scratch("yalain-live-websocket");
    let (port, _server, ()) = serving(&dir, |port| {
        let more = [
            "--bind",
            "127.0.0.1",
            "--websocket",
            "--websocket-listen",
            "127.0.0.1:0",
            "--websocket-trusted-proxy",
            "127.0.0.1",
        ];
        (start_server_with(&dir, port, LIFE, &more), ())
    });
    let at = endpoint(&dir);
    assert_ne!(port, NOMINAL, "the client's numbers are its own");

    let plays = |ws: WebSocket<TcpStream>, account: &str, name: &str| {
        let mut s = OverWebSocket::new(ws, account);
        must(play::character_list(&mut s), &dir);
        let me = must(play::create(&mut s, name), &dir);
        s.client().act(Action::PlayCharacter { character: me });
        must(
            play::until(&mut s, play::HANG, "the player in the world", |c| {
                c.model.player == Some(me) && c.model.player_object().is_some() && c.body.placed()
            }),
            &dir,
        );
        assert!(
            s.framed_to.contains(&(NOMINAL + 2)),
            "the world by the connection's numbers: {:?}",
            s.framed_to
        );
        assert!(
            s.framed_to
                .iter()
                .all(|p| (NOMINAL..NOMINAL + 4).contains(p)),
            "no port of the server's own: {:?}",
            s.framed_to
        );
        must(play::log_off(&mut s), &dir);
    };

    plays(connect(at, None), "websocket", "Wirewalker");
    plays(connect(at, Some("203.0.113.9")), "proxied", "Roundabout");
    let log = std::fs::read_to_string(dir.join("server.log")).unwrap_or_default();
    assert!(
        log.contains("WebSocket 203.0.113.9:") && log.contains("(through 127.0.0.1)"),
        "{log}"
    );
}
