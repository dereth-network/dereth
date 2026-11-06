//! A player of this game: our own client on a UDP socket, and beside it a friends stream, the
//! stream's client half over TCP, presenting a ticket made from the game login's account.

use std::io::{ErrorKind, Read as _, Write as _};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::time::{Duration, Instant};

use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::Client;
use auberean_friends::{
    ClientConfig, ClientOut, ClientSession, Cx, Event, Hr, Mode, Request, Ticket,
};
use auberean_script::native_object::{TextInfo, VariableValue};
use yalain_testkit::session as play;

/// The game's client on a loopback socket, on the real clock.
pub struct Game {
    socket: UdpSocket,
    pub client: Client,
    start: Instant,
    buf: Vec<u8>,
}

impl Game {
    fn new(login: SocketAddr, account: &str) -> Self {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
        socket.set_nonblocking(true).expect("nonblocking");
        Self {
            socket,
            client: play::client(login, account),
            start: Instant::now(),
            buf: vec![0; 65_536],
        }
    }

    fn time(&self) -> LocalTime {
        LocalTime(1.0 + self.start.elapsed().as_secs_f64())
    }

    pub fn now(&self) -> f64 {
        self.time().0
    }

    /// What came, the client's frame, and what it sends.
    pub fn step(&mut self) {
        // Until nothing more came, or a reset from a port not answering yet: the client resends.
        while let Ok((n, from)) = self.socket.recv_from(&mut self.buf) {
            let now = self.time();
            self.client.handle_datagram(from, &self.buf[..n], now);
        }
        let now = self.time();
        self.client.frame(now);
        while let Some((to, bytes)) = self.client.poll_transmit() {
            let _ = self.socket.send_to(&bytes, to);
        }
    }
}

/// The lines the server's text put in the player's chat windows, in order.
pub fn shown(c: &Client) -> Vec<String> {
    c.shown
        .iter()
        .filter_map(|s| s.text.literal.as_deref())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Whether a line of the client's own strings holds `needle`, in a literal of it or of one of its
/// variables.
pub fn names(text: &TextInfo, needle: &str) -> bool {
    let holds = |l: &Option<Vec<u16>>| {
        l.as_deref()
            .is_some_and(|l| String::from_utf16_lossy(l).contains(needle))
    };
    holds(&text.literal)
        || text.variables.iter().any(|v| match &v.value {
            VariableValue::Nested(t) => names(t, needle),
            _ => false,
        })
}

/// A friends stream: the stream's client half over a TCP socket, on the real clock.
pub struct Stream {
    session: ClientSession,
    at: SocketAddr,
    tcp: Option<TcpStream>,
    start: Instant,
    ticket: String,
    pub got: Vec<ClientOut>,
    next_cx: u64,
}

impl Stream {
    fn open(at: SocketAddr, account: &str) -> Self {
        let mut s = Self {
            session: ClientSession::new(ClientConfig {
                software: "yalain-network/1".into(),
                locale: None,
            }),
            at,
            tcp: None,
            start: Instant::now(),
            // The game login's own secret, as a launcher gives both: the world checks the two alike.
            ticket: Ticket::new(account, "ticket").encode().expect("a ticket"),
            got: Vec::new(),
            next_cx: 0x10,
        };
        let now = s.now();
        s.session.connect(now);
        s
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    /// Reads what came, and does what the session asks.
    pub fn step(&mut self) {
        let now = self.now();
        if let Some(tcp) = &mut self.tcp {
            let mut buf = [0u8; 8192];
            match tcp.read(&mut buf) {
                Ok(0) => {
                    self.tcp = None;
                    self.session.closed(now);
                }
                Ok(n) => self.session.on_bytes(now, &buf[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => {
                    self.tcp = None;
                    self.session.closed(now);
                }
            }
        }
        for o in self.session.poll(now) {
            match o {
                ClientOut::Dial => match TcpStream::connect(self.at) {
                    Ok(tcp) => {
                        tcp.set_nonblocking(true).expect("nonblocking");
                        self.tcp = Some(tcp);
                        self.session.opened(now);
                    }
                    Err(_) => self.session.failed(now),
                },
                ClientOut::Write(bytes) => {
                    if let Some(tcp) = &mut self.tcp {
                        let _ = tcp.set_nonblocking(false);
                        let _ = tcp.write_all(&bytes);
                        let _ = tcp.set_nonblocking(true);
                    }
                }
                ClientOut::Close => self.tcp = None,
                ClientOut::Up(_) => {
                    let ticket = self.ticket.clone();
                    self.request(Request::PresentTicket {
                        email: String::new(),
                        ticket,
                    });
                }
                other => self.got.push(other),
            }
        }
    }

    /// Sends a request; its mark.
    pub fn request(&mut self, request: Request) -> Cx {
        self.next_cx += 1;
        let cx = Cx(self.next_cx);
        let now = self.now();
        self.session.request(now, cx, request);
        cx
    }

    /// The answer to a request, once it came.
    pub fn answered(&self, cx: Cx) -> Option<Hr> {
        self.got.iter().find_map(|o| match o {
            ClientOut::Answer { cx: c, hr, .. } if *c == cx => Some(*hr),
            _ => None,
        })
    }

    /// Whether the ticket was taken.
    pub fn admitted(&self) -> bool {
        self.got
            .iter()
            .any(|o| matches!(o, ClientOut::Answer { hr, .. } if *hr == Hr::OK))
    }

    /// Whether the world sent the friends list for the character in it.
    pub fn started(&self) -> bool {
        self.got
            .iter()
            .any(|o| matches!(o, ClientOut::Event(Event::FStart { .. })))
    }

    /// The world's events since `from`.
    pub fn events_since(&self, from: usize) -> impl Iterator<Item = &Event> {
        self.got[from.min(self.got.len())..]
            .iter()
            .filter_map(|o| match o {
                ClientOut::Event(e) => Some(e),
                _ => None,
            })
    }

    /// The last state the world gave `user`, since `from`.
    pub fn mode_since(&self, from: usize, user: &str) -> Option<Mode> {
        self.events_since(from)
            .filter_map(|e| match e {
                Event::FriendStat { user: u, mode } if u == user => Some(*mode),
                _ => None,
            })
            .last()
    }

    /// Whether a tell from `user` saying `text` came since `from`.
    pub fn told_since(&self, from: usize, user: &str, text: &str) -> bool {
        self.events_since(from)
            .any(|e| matches!(e, Event::UTellEv { user: u, text: t, .. } if u == user && t == text))
    }
}

/// A player of this game: its account, its character's name, the game's client and the stream.
pub struct Player {
    pub name: &'static str,
    pub game: Game,
    pub stream: Stream,
}

impl Player {
    /// A player of the world whose login is on loopback at `port`; its stream opens at once, as
    /// the retail client's does, before the game login.
    pub fn new(name: &'static str, port: u16) -> Self {
        let at = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let account = name.to_lowercase();
        Self {
            name,
            game: Game::new(at, &account),
            stream: Stream::open(at, &account),
        }
    }

    pub fn step(&mut self) {
        self.game.step();
        self.stream.step();
    }
}
