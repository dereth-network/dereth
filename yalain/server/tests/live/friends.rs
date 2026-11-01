//! The executable's friends service over real sockets: two players of our client in the world,
//! each with a friends stream on TCP at the login port's number, and a stand-in hub on the link.
//! Each step waits for what it should bring, with a time limit only against a hang. Fixture: the
//! built executable, the install, the authored world and a key file in a scratch folder.

use std::io::{ErrorKind, Read as _, Write as _};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::path::Path;
use std::time::{Duration, Instant};

use auberean_client_sdk::{Action, Client};
use auberean_friends::{
    ClientConfig, ClientOut, ClientSession, Cx, Event, Hr, Mode, Request, Ticket,
};
use dereth_network_protocol::limits::Limits;
use dereth_network_protocol::wire::PROTOCOL;
use dereth_network_protocol::{Address, Message, Outcome, WorldInfo};
use tungstenite::WebSocket;
use yalain_testkit::session::{self as play, Drive};

use crate::network::{next, send, stand_in_hub, start_server, HANG};
use crate::session::{free_ports, must, scratch, Loopback, Running};

/// One player's friends stream: the stream's client half over a TCP socket, on the real clock.
struct Stream {
    session: ClientSession,
    at: SocketAddr,
    tcp: Option<TcpStream>,
    start: Instant,
    ticket: String,
    got: Vec<ClientOut>,
    next_cx: u64,
}

impl Stream {
    fn open(at: SocketAddr, account: &str) -> Self {
        let mut s = Self {
            session: ClientSession::new(ClientConfig {
                software: "yalain-live/1".into(),
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
    fn step(&mut self) {
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
                        tcp.set_read_timeout(Some(Duration::from_millis(2)))
                            .expect("a read timeout");
                        self.tcp = Some(tcp);
                        self.session.opened(now);
                    }
                    Err(_) => self.session.failed(now),
                },
                ClientOut::Write(bytes) => {
                    if let Some(tcp) = &mut self.tcp {
                        let _ = tcp.write_all(&bytes);
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

    fn request(&mut self, request: Request) -> Cx {
        self.next_cx += 1;
        let cx = Cx(self.next_cx);
        let now = self.now();
        self.session.request(now, cx, request);
        cx
    }

    /// The answer to a request, once it came.
    fn answered(&self, cx: Cx) -> Option<Hr> {
        self.got.iter().find_map(|o| match o {
            ClientOut::Answer { cx: c, hr, .. } if *c == cx => Some(*hr),
            _ => None,
        })
    }

    fn has(&self, event: &Event) -> bool {
        self.got
            .iter()
            .any(|o| matches!(o, ClientOut::Event(e) if e == event))
    }
}

/// Everyone in the test: the two players' clients and their streams.
struct Players {
    clients: Vec<Loopback>,
    streams: Vec<Stream>,
}

impl Players {
    fn step(&mut self) {
        for c in &mut self.clients {
            c.step();
        }
        for s in &mut self.streams {
            s.step();
        }
    }

    /// Steps everyone until `done` holds, within the limit against a hang.
    fn until(&mut self, what: &str, done: impl Fn(&Self) -> bool) -> Result<(), String> {
        let until = Instant::now() + HANG;
        while Instant::now() < until {
            self.step();
            if done(self) {
                return Ok(());
            }
        }
        Err(format!("{what}: it never came"))
    }

    /// A request on a player's stream, and its answer.
    fn ask(&mut self, who: usize, request: Request, what: &str) -> Result<Hr, String> {
        let cx = self.streams[who].request(request);
        self.until(what, |p| p.streams[who].answered(cx).is_some())?;
        Ok(self.streams[who].answered(cx).expect("answered"))
    }

    /// The next link message the world sends for which `pick` holds, stepping everyone between
    /// reads.
    fn sent_by_world<T>(
        &mut self,
        ws: &mut WebSocket<TcpStream>,
        what: &str,
        pick: impl Fn(&Message) -> Option<T>,
    ) -> Result<T, String> {
        let until = Instant::now() + HANG;
        while Instant::now() < until {
            self.step();
            if let Ok(m) = next(ws, what) {
                if let Some(t) = pick(&m) {
                    return Ok(t);
                }
            }
        }
        Err(format!("{what}: the world never sent it"))
    }
}

/// A player of our client, logged in as `account` and in the world as a new character `name`.
fn player(login: SocketAddr, account: &str, name: &str, dir: &Path) -> Loopback {
    let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("a client socket");
    socket.set_nonblocking(true).expect("nonblocking");
    let mut s = Loopback {
        socket,
        client: play::client(login, account),
        start: Instant::now(),
        buf: vec![0; 65_536],
    };
    must(play::character_list(&mut s), dir);
    let me = must(play::create(&mut s, name), dir);
    must(play::enter(&mut s, me), dir);
    s
}

fn say(c: &mut Client, text: &str) {
    c.act(Action::Say { text: text.into() });
}

/// The executable serves the friends stream at the login port's number: a ticket made from the
/// account admits a stream before the game login does; within the world a friend is added with
/// its state and a tell reaches the other player's stream from `Name@` and the world's name;
/// `@dn on` said in chat puts the character on the network, after which another world's friend is
/// watched over the link, a tell to it is answered as the hub answers, and a tell from it reaches
/// the stream.
/// Behaviour: server.friends.served-beside-the-login
#[test]
fn the_server_serves_friends_and_tells_within_the_world_and_across_the_link() {
    let dir = scratch("yalain-live-friends");
    let (hub, links) = stand_in_hub();
    let port = free_ports();
    let (child, _console) = start_server(&dir, port, hub);
    let _server = Running(child);
    let mut link = must(
        links
            .recv_timeout(HANG)
            .map_err(|e| format!("the link: {e}")),
        &dir,
    );
    link.ws
        .get_ref()
        .set_read_timeout(Some(Duration::from_millis(20)))
        .expect("a read timeout");
    must(next(&mut link.ws, "hello"), &dir);
    send(
        &mut link.ws,
        &Message::Welcome {
            proto: PROTOCOL,
            world: "shardfall".into(),
            hub: "stand-in".into(),
            worlds: vec![WorldInfo {
                slug: "coldeve".into(),
                name: "Coldeve".into(),
                online: true,
            }],
            rooms: Vec::new(),
            limits: Limits::DEFAULT,
        },
    );
    let at = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    // The streams come first, as the retail client's does, before the game login.
    let mut p = Players {
        clients: Vec::new(),
        streams: vec![Stream::open(at, "ann"), Stream::open(at, "cara")],
    };
    must(
        p.until("both tickets admitted", |p| {
            p.streams.iter().all(|s| {
                s.got
                    .iter()
                    .any(|o| matches!(o, ClientOut::Answer { hr, .. } if *hr == Hr::OK))
            })
        }),
        &dir,
    );
    p.clients.push(player(at, "ann", "Annwyn", &dir));
    p.clients.push(player(at, "cara", "Carawen", &dir));
    must(
        p.until("both friends lists", |p| {
            p.streams.iter().all(|s| {
                s.got
                    .iter()
                    .any(|o| matches!(o, ClientOut::Event(Event::FStart { .. })))
            })
        }),
        &dir,
    );

    // Within the world.
    let hr = must(
        p.ask(
            0,
            Request::FriendAdd {
                user: "Carawen".into(),
            },
            "the friend added",
        ),
        &dir,
    );
    assert_eq!(hr, Hr::OK);
    assert!(p.streams[0].has(&Event::FriendStat {
        user: "Carawen@Local World".into(),
        mode: Mode::ONLINE,
    }));
    let hr = must(
        p.ask(
            0,
            Request::SendToUserEx {
                user: "Carawen@Local World".into(),
                text: "hello".into(),
                extra: None,
            },
            "the tell answered",
        ),
        &dir,
    );
    assert_eq!(hr, Hr::OK);
    let told = Event::UTellEv {
        user: "Annwyn@Local World".into(),
        text: "hello".into(),
        extra: None,
    };
    must(p.until("the tell heard", |p| p.streams[1].has(&told)), &dir);

    // Across the link: on the network first.
    say(&mut p.clients[0].client, "@dn on");
    let until = Instant::now() + HANG;
    loop {
        let hr = must(
            p.ask(
                0,
                Request::FriendAdd {
                    user: "Bob@coldeve".into(),
                },
                "another world's friend",
            ),
            &dir,
        );
        if hr == Hr::OK {
            break;
        }
        assert!(Instant::now() < until, "never on the network: {hr}");
    }
    let a = |s: &str| Address::parse(s).expect("an address");
    must(
        p.sent_by_world(&mut link.ws, "the watch", |m| {
            (*m == Message::Watch {
                owner: a("Annwyn@shardfall"),
                target: a("Bob@coldeve"),
                on: true,
            })
            .then_some(())
        }),
        &dir,
    );
    let cx = p.streams[0].request(Request::SendToUserEx {
        user: "Bob@coldeve".into(),
        text: "hi bob".into(),
        extra: None,
    });
    let id = must(
        p.sent_by_world(&mut link.ws, "the tell", |m| match m {
            Message::Tell {
                id, from, to, text, ..
            } if *from == a("Annwyn@shardfall") && *to == a("Bob@coldeve") && text == "hi bob" => {
                Some(id.clone())
            }
            _ => None,
        }),
        &dir,
    );
    send(
        &mut link.ws,
        &Message::TellResult {
            id,
            outcome: Outcome::Delivered,
            message: None,
        },
    );
    must(
        p.until("the tell's answer", |p| p.streams[0].answered(cx).is_some()),
        &dir,
    );
    assert_eq!(p.streams[0].answered(cx), Some(Hr::OK));
    send(
        &mut link.ws,
        &Message::Tell {
            id: "h9".into(),
            from: a("Bob@coldeve"),
            to: a("Annwyn@shardfall"),
            text: "hey".into(),
            friend: true,
        },
    );
    let heard = Event::UTellEv {
        user: "Bob@coldeve".into(),
        text: "hey".into(),
        extra: None,
    };
    must(
        p.until("the remote tell heard", |p| p.streams[0].has(&heard)),
        &dir,
    );
    must(
        p.sent_by_world(&mut link.ws, "the remote tell's answer", |m| {
            (*m == Message::TellResult {
                id: "h9".into(),
                outcome: Outcome::Delivered,
                message: None,
            })
            .then_some(())
        }),
        &dir,
    );
}
