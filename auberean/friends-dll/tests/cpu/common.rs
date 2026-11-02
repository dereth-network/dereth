//! What the library's tests share: a world with a script, run in memory against the engine on a
//! clock the tests turn, or on its own thread over loopback; and a wait that pumps until what is
//! wanted has come.

use core::time::Duration;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use auberean_friends_dll::engine::{Engine, Io, Message};
use stream::{
    Answer, Event, FriendStatus, Hr, Mode, Request, ServerConfig, ServerEvent, ServerOut,
    ServerStream, Ticket,
};

/// The world's name, as its login names it.
pub const WORLD_NAME: &str = "Dawnsong";

/// The world's slug.
pub const WORLD_SLUG: &str = "dawnsong";

/// The account whose ticket the world refuses.
pub const REFUSED: &str = "refused";

/// Seconds as a duration.
pub fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// A ticket for an account, as the launcher writes it.
pub fn ticket(account: &str) -> String {
    Ticket::new(account, "secret").encode().expect("a ticket")
}

/// The script's friends: one on this world online, one away, one on another world, one offline.
pub fn friends() -> Vec<FriendStatus> {
    vec![
        FriendStatus::new("Echo@Dawnsong", Mode::ONLINE),
        FriendStatus::new("Cara@Dawnsong", Mode::ONLINE.with(Mode::AWAY)),
        FriendStatus::new("Ann@coldeve", Mode::ONLINE),
        FriendStatus::new("Bob@Dawnsong", Mode::OFFLINE),
    ]
}

/// The script's ignore list.
pub fn ignored() -> Vec<String> {
    vec!["Mean@Dawnsong".to_owned()]
}

/// The script's world: it admits every ticket but the refused account's, sends the friends when
/// the character enters, answers the lists, adds and removes, echoes a tell to `Echo` back from
/// `Echo`, and answers any other tell as to one not online.
pub fn serve(world: &mut ServerStream, now: Duration, event: ServerEvent) {
    match event {
        ServerEvent::Ticket { cx, ticket } => {
            let account = Ticket::decode(&ticket)
                .map(|t| t.account)
                .unwrap_or_default();
            if account == REFUSED {
                world.refuse(now, cx, Hr::TICKET_REFUSED);
            } else {
                world.admit(now, cx);
                world
                    .enter(now, friends())
                    .expect("the friends fit a frame");
            }
        }
        ServerEvent::Request { cx, request } => {
            let (hr, answer) = match request {
                Request::GetFriendsList => (Hr::OK, Answer::GetFriendsList { friends: friends() }),
                Request::GetIgnoredList => (Hr::OK, Answer::GetIgnoredList { ignored: ignored() }),
                Request::FriendAdd { user } => {
                    let _ = world.event(
                        now,
                        Event::FriendStat {
                            user,
                            mode: Mode::ONLINE,
                        },
                    );
                    (Hr::OK, Answer::FriendAdd)
                }
                Request::SendToUserEx { user, text, extra } => {
                    if user.starts_with("Echo") {
                        let tell = Event::UTellEv {
                            user: format!("Echo@{WORLD_NAME}"),
                            text,
                            extra,
                        };
                        let _ = world.answer(now, cx, Hr::OK, Answer::SendToUserEx);
                        let _ = world.event(now, tell);
                        return;
                    }
                    (Hr::NOT_ONLINE, Answer::SendToUserEx)
                }
                other => (Hr::OK, Answer::empty(other.kind())),
            };
            let _ = world.answer(now, cx, hr, answer);
        }
        ServerEvent::Hello(_) | ServerEvent::Ended(_) => {}
    }
}

/// How the in-memory world behaves when the engine dials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// The dial succeeds and the world serves the script.
    Serves,
    /// The dial fails.
    Unreachable,
    /// The dial succeeds and the world says nothing.
    Silent,
    /// The dial never finishes.
    Hangs,
}

/// The engine and a world joined in memory, on a clock the tests turn.
#[derive(Debug)]
pub struct Rig {
    pub now: Duration,
    pub engine: Engine,
    pub reach: Reach,
    /// The open connection: its number and the world's half.
    pub world: Option<(u64, ServerStream)>,
    /// Every connection the engine dialed.
    pub dials: Vec<u64>,
    /// Every connection the engine closed.
    pub closes: Vec<u64>,
    /// What the engine has handed the client.
    pub messages: Vec<Message>,
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}

impl Rig {
    /// An engine that has not connected, and a world that serves the script.
    pub fn new() -> Self {
        Self {
            now: Duration::ZERO,
            engine: Engine::new("auberean-friends-dll/test"),
            reach: Reach::Serves,
            world: None,
            dials: Vec::new(),
            closes: Vec::new(),
            messages: Vec::new(),
        }
    }

    /// Run the engine's clock and the world's until neither has anything for the other, then take
    /// what the engine has for the client.
    pub fn settle(&mut self) {
        let now = self.now;
        self.engine.tick(now);
        for _ in 0..100 {
            let mut quiet = true;
            for io in self.engine.take_io() {
                quiet = false;
                match io {
                    Io::Dial { conn, .. } => {
                        self.dials.push(conn);
                        match self.reach {
                            Reach::Serves | Reach::Silent => {
                                self.world = Some((
                                    conn,
                                    ServerStream::new(
                                        now,
                                        ServerConfig::new(WORLD_SLUG, WORLD_NAME),
                                    ),
                                ));
                                self.engine.opened(now, conn);
                            }
                            Reach::Unreachable => self.engine.failed(now, conn),
                            Reach::Hangs => {}
                        }
                    }
                    Io::Write { conn, bytes } => {
                        if let Some((c, world)) = &mut self.world {
                            if *c == conn && self.reach == Reach::Serves {
                                world.on_bytes(now, &bytes);
                            }
                        }
                    }
                    Io::Close { conn } => {
                        self.closes.push(conn);
                        if let Some((c, world)) = &mut self.world {
                            if *c == conn {
                                world.closed(now);
                                self.world = None;
                            }
                        }
                    }
                }
            }
            let mut world_closed = None;
            if let Some((conn, world)) = &mut self.world {
                if self.reach == Reach::Serves {
                    let conn = *conn;
                    let mut writes = Vec::new();
                    let mut closed = false;
                    for out in world.poll(now) {
                        quiet = false;
                        match out {
                            ServerOut::Write(bytes) => writes.push(bytes),
                            ServerOut::Close => closed = true,
                            ServerOut::Event(e) => serve(world, now, e),
                        }
                    }
                    for bytes in writes {
                        self.engine.received(now, conn, &bytes);
                    }
                    if closed {
                        world_closed = Some(conn);
                    }
                }
            }
            if let Some(conn) = world_closed {
                self.world = None;
                self.engine.closed(now, conn);
            }
            if quiet {
                break;
            }
        }
        while let Some(m) = self.engine.next_message() {
            self.messages.push(m);
        }
    }

    /// Turn the clock by `by`, then settle.
    pub fn wait(&mut self, by: Duration) {
        self.now += by;
        self.settle();
    }

    /// The client connects, and the rig settles.
    pub fn connect(&mut self) {
        let now = self.now;
        self.engine.connect(now, "127.0.0.1", 9000);
        self.settle();
    }

    /// The client presents the script's ticket with `context`, and the rig settles.
    pub fn present(&mut self, context: u32) {
        let now = self.now;
        let t = ticket("ann");
        self.engine.present_ticket(now, context, t.as_bytes());
        self.settle();
    }

    /// The client asks, and the rig settles.
    pub fn ask(&mut self, context: u32, request: Request) {
        let now = self.now;
        self.engine.request(now, context, request);
        self.settle();
    }

    /// The world's end of the connection goes away with no goodbye.
    pub fn drop_world(&mut self) {
        if let Some((conn, _)) = self.world.take() {
            let now = self.now;
            self.engine.closed(now, conn);
            self.settle();
        }
    }

    /// The world says goodbye and closes the connection.
    pub fn world_says_goodbye(&mut self) {
        let now = self.now;
        if let Some((_, world)) = &mut self.world {
            world.close(now, stream::ByeReason::Shutdown);
        }
        self.settle();
    }

    /// The messages since the last take.
    pub fn take(&mut self) -> Vec<Message> {
        core::mem::take(&mut self.messages)
    }
}

/// Wait for what `f` finds, calling it again and again, yielding between calls: the libraries'
/// threads run on their own. The count only guards against a hang.
pub fn until<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    for _ in 0..200_000_000u64 {
        if let Some(t) = f() {
            return t;
        }
        thread::yield_now();
    }
    panic!("waited for {what} and it never came");
}

/// A world on loopback that serves the script, one connection at a time, on its own thread, with a
/// clock that moves a second for each read so that its bound on requests refills.
#[derive(Debug)]
pub struct LoopbackWorld {
    pub port: u16,
    /// The open connection, for the test to cut.
    current: Arc<Mutex<Option<TcpStream>>>,
    /// Each connection's greeting, when it comes.
    pub greetings: Receiver<u32>,
}

impl LoopbackWorld {
    /// Listen on a free port of loopback.
    pub fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a port on loopback");
        let port = listener.local_addr().expect("its address").port();
        let current = Arc::new(Mutex::new(None));
        let (tx, greetings) = mpsc::channel();
        let slot = Arc::clone(&current);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                *slot.lock().expect("the slot") = stream.try_clone().ok();
                serve_connection(stream, &tx);
            }
        });
        Self {
            port,
            current,
            greetings,
        }
    }

    /// Cut the open connection, with no goodbye.
    pub fn cut(&self) {
        if let Some(s) = self.current.lock().expect("the slot").take() {
            let _ = s.shutdown(Shutdown::Both);
        }
    }
}

fn serve_connection(mut stream: TcpStream, greetings: &Sender<u32>) {
    let mut now = Duration::ZERO;
    let mut world = ServerStream::new(now, ServerConfig::new(WORLD_SLUG, WORLD_NAME));
    let mut buf = [0u8; 4096];
    loop {
        let n = match stream.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        now += secs(1);
        world.on_bytes(now, &buf[..n]);
        loop {
            let outs = world.poll(now);
            if outs.is_empty() {
                break;
            }
            for out in outs {
                match out {
                    ServerOut::Write(bytes) => {
                        if stream.write_all(&bytes).is_err() {
                            return;
                        }
                    }
                    ServerOut::Close => {
                        let _ = stream.shutdown(Shutdown::Both);
                        return;
                    }
                    ServerOut::Event(ServerEvent::Hello(h)) => {
                        let _ = greetings.send(h.locale.unwrap_or(0));
                    }
                    ServerOut::Event(e) => serve(&mut world, now, e),
                }
            }
        }
    }
}
