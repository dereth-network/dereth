//! A world that serves only the friends stream, for trying the friends library with the retail
//! client beside a server that has no friends service of its own.
//!
//! ```text
//! cargo run -p auberean-friends-dll --example friends-world -- [--bind 127.0.0.1:9000] [--name Dawnsong] [--slug dawnsong]
//! ```
//!
//! It admits any ticket of the `a1.` form, and gives each stream four friends: `Echo` (online),
//! `Cara` (away) and `Bob` (offline) on this world, and `Ann` on another (online). A tell to `Echo`
//! comes back from `Echo` with the same text; the text `away` or `busy` also sets `Echo`'s state,
//! `bye` takes `Echo` offline, and any other text brings it back online. A tell to `Cara` or `Ann`
//! is delivered (and answered by no one), to `Bob` is answered as to one not online, and to anyone
//! else as to no such user. Adding a friend shows it online; the ignore list starts empty. Every
//! element is printed, the ticket's secret never.

use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use stream::{
    Answer, Event, FriendStatus, Hr, Mode, Request, ServerConfig, ServerEvent, ServerOut,
    ServerStream, Ticket,
};

#[derive(Debug, Clone)]
struct Options {
    bind: String,
    name: String,
    slug: String,
}

fn options() -> Options {
    let mut o = Options {
        bind: "127.0.0.1:9000".into(),
        name: "Dawnsong".into(),
        slug: String::new(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().unwrap_or_default();
        match a.as_str() {
            "--bind" => o.bind = value(),
            "--name" => o.name = value(),
            "--slug" => o.slug = value(),
            other => {
                eprintln!("friends-world: unknown option {other}");
                std::process::exit(2);
            }
        }
    }
    o
}

fn main() {
    let o = options();
    let listener = match TcpListener::bind(&o.bind) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("friends-world: cannot listen on {}: {e}", o.bind);
            std::process::exit(1);
        }
    };
    let start = Instant::now();
    println!("friends-world: {} on {}", o.name, o.bind);
    for (n, stream) in listener.incoming().enumerate() {
        let Ok(stream) = stream else { continue };
        let o = o.clone();
        thread::spawn(move || Connection::new(n + 1, stream, &o, start).run());
    }
}

/// One stream and its world's half, with the script's friends.
struct Connection {
    n: usize,
    stream: TcpStream,
    world: ServerStream,
    start: Instant,
    name: String,
    friends: Vec<FriendStatus>,
    ignored: Vec<String>,
}

impl Connection {
    fn new(n: usize, stream: TcpStream, o: &Options, start: Instant) -> Self {
        let now = start.elapsed();
        let local = |who: &str| format!("{who}@{}", o.name);
        Self {
            n,
            stream,
            world: ServerStream::new(now, ServerConfig::new(&o.slug, &o.name)),
            start,
            name: o.name.clone(),
            friends: vec![
                FriendStatus::new(&local("Echo"), Mode::ONLINE),
                FriendStatus::new(&local("Cara"), Mode::ONLINE.with(Mode::AWAY)),
                FriendStatus::new(&local("Bob"), Mode::OFFLINE),
                FriendStatus::new("Ann@coldeve", Mode::ONLINE),
            ],
            ignored: Vec::new(),
        }
    }

    fn say(&self, what: &str) {
        let t = self.start.elapsed();
        println!("[{:>7.3}] #{} {what}", t.as_secs_f64(), self.n);
    }

    fn run(mut self) {
        self.say(&format!("open from {:?}", self.stream.peer_addr().ok()));
        let _ = self.stream.set_read_timeout(Some(Duration::from_secs(1)));
        let mut buf = [0u8; 8192];
        loop {
            let now = self.start.elapsed();
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    self.world.closed(now);
                    self.flush();
                    self.say("closed by the client");
                    return;
                }
                Ok(n) => self.world.on_bytes(now, &buf[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) => {
                    self.world.closed(now);
                    self.flush();
                    self.say(&format!("lost: {e}"));
                    return;
                }
            }
            if !self.flush() {
                return;
            }
        }
    }

    /// Everything the stream has: written, or served. False once it is closed.
    fn flush(&mut self) -> bool {
        loop {
            let now = self.start.elapsed();
            let outs = self.world.poll(now);
            if outs.is_empty() {
                return true;
            }
            for out in outs {
                match out {
                    ServerOut::Write(bytes) => {
                        self.show_written(&bytes);
                        if self.stream.write_all(&bytes).is_err() {
                            return false;
                        }
                    }
                    ServerOut::Close => {
                        let _ = self.stream.shutdown(Shutdown::Both);
                        self.say("closed by the world");
                        return false;
                    }
                    ServerOut::Event(e) => self.serve(now, e),
                }
            }
        }
    }

    fn show_written(&self, bytes: &[u8]) {
        let mut rest = bytes;
        while rest.len() >= 4 {
            let n = u32::from_le_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
            let Some(text) = rest.get(4..4 + n) else {
                break;
            };
            self.say(&format!("-> {}", String::from_utf8_lossy(text)));
            rest = &rest[4 + n..];
        }
    }

    fn serve(&mut self, now: Duration, event: ServerEvent) {
        match event {
            ServerEvent::Hello(h) => self.say(&format!(
                "<- Hello from {} (locale {:?})",
                h.software, h.locale
            )),
            ServerEvent::Ticket { cx, ticket } => match Ticket::decode(&ticket) {
                Ok(t) => {
                    self.say(&format!("<- ticket for account {:?}: admitted", t.account));
                    self.world.admit(now, cx);
                    let _ = self.world.enter(now, self.friends.clone());
                }
                Err(e) => {
                    self.say(&format!("<- a ticket that is not one ({e}): refused"));
                    self.world.refuse(now, cx, Hr::TICKET_REFUSED);
                }
            },
            ServerEvent::Request { cx, request } => {
                self.say(&format!("<- {request:?}"));
                let mut after = Vec::new();
                let (hr, answer) = self.answer(now, request, &mut after);
                let _ = self.world.answer(now, cx, hr, answer);
                for e in after {
                    let _ = self.world.event(now, e);
                }
            }
            ServerEvent::Ended(e) => self.say(&format!("ended: {e:?}")),
        }
    }

    fn mode_of(&self, who: &str) -> Option<Mode> {
        self.friends
            .iter()
            .find(|f| f.user.eq_ignore_ascii_case(who))
            .map(|f| f.mode)
    }

    /// The local form of a name: `Name@<this world>` for a name with no world.
    fn full(&self, user: &str) -> String {
        if user.contains('@') {
            user.to_owned()
        } else {
            format!("{user}@{}", self.name)
        }
    }

    /// The answer to a request; what follows the answer goes in `after`.
    fn answer(&mut self, now: Duration, request: Request, after: &mut Vec<Event>) -> (Hr, Answer) {
        match request {
            Request::GetFriendsList => (
                Hr::OK,
                Answer::GetFriendsList {
                    friends: self.friends.clone(),
                },
            ),
            Request::GetIgnoredList => (
                Hr::OK,
                Answer::GetIgnoredList {
                    ignored: self.ignored.clone(),
                },
            ),
            Request::FriendAdd { user } => {
                let user = self.full(&user);
                if self.mode_of(&user).is_some() {
                    return (Hr(0xC112_1009), Answer::FriendAdd);
                }
                self.friends.push(FriendStatus::new(&user, Mode::ONLINE));
                let _ = self.world.event(
                    now,
                    Event::FriendStat {
                        user,
                        mode: Mode::ONLINE,
                    },
                );
                (Hr::OK, Answer::FriendAdd)
            }
            Request::FriendDelete { user } => {
                let user = self.full(&user);
                let before = self.friends.len();
                self.friends.retain(|f| !f.user.eq_ignore_ascii_case(&user));
                let hr = if self.friends.len() < before {
                    Hr::OK
                } else {
                    Hr(0xC112_1401)
                };
                (hr, Answer::FriendDelete)
            }
            Request::IgnoredAdd { user } => {
                self.ignored.push(self.full(&user));
                (Hr::OK, Answer::IgnoredAdd)
            }
            Request::IgnoredDelete { user } => {
                let user = self.full(&user);
                self.ignored.retain(|u| !u.eq_ignore_ascii_case(&user));
                (Hr::OK, Answer::IgnoredDelete)
            }
            Request::SendToUserEx { user, text, extra } => {
                (self.tell(&user, text, extra, after), Answer::SendToUserEx)
            }
            other => (Hr::OK, Answer::empty(other.kind())),
        }
    }

    /// A tell's code; a tell to `Echo` comes back after the answer, with any change of its state.
    fn tell(
        &mut self,
        user: &str,
        text: String,
        extra: Option<stream::Extra>,
        after: &mut Vec<Event>,
    ) -> Hr {
        let user = self.full(user);
        let echo = format!("Echo@{}", self.name);
        if !user.eq_ignore_ascii_case(&echo) {
            return match self.mode_of(&user) {
                Some(m) if m.is_online() => Hr::OK,
                Some(_) => Hr::NOT_ONLINE,
                None => Hr::NO_SUCH_USER,
            };
        }
        let mode = match text.trim().to_ascii_lowercase().as_str() {
            "away" => Mode::ONLINE.with(Mode::AWAY),
            "busy" => Mode::ONLINE.with(Mode::NOT_DISTURBED),
            "bye" => Mode::OFFLINE,
            _ => Mode::ONLINE,
        };
        after.push(Event::UTellEv {
            user: echo.clone(),
            text,
            extra,
        });
        if self.mode_of(&echo) != Some(mode) {
            if let Some(f) = self.friends.iter_mut().find(|f| f.user == echo) {
                f.mode = mode;
            }
            after.push(Event::FriendStat { user: echo, mode });
        }
        Hr::OK
    }
}
