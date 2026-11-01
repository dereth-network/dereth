//! The friends service's connection, as the client's friends layer sees it: connect, send a
//! request with the layer's own mark, and take what came back when the layer pumps.
//!
//! The friends service (friends with presence, an ignore list, the player's away state, tells to
//! `Name@world`) is reached over a stream of its own beside the game connection: the friends
//! stream, whose elements are the service's commands, answers and events ([`stream`]). The
//! client's friends layer calls a [`FriendsConnection`]; what comes back reaches it only when it
//! asks ([`FriendsConnection::dispatch`]), so the layer and the scripts it calls are never entered
//! from anywhere else.
//!
//! - **Connecting.** [`FriendsConnection::connect`] starts an attempt and returns at once; its
//!   outcome comes back as one [`Delivery::Connect`]. A connection that later goes down comes back
//!   as one more, and the layer decides whether to connect again.
//! - **Requests.** Each carries the layer's [`stream::Cx`]; exactly one answer comes back with it:
//!   the world's, or the connection's own [`stream::Hr::NOT_CONNECTED`] when the stream was down or
//!   went down before the world answered.
//! - **The host's side.** A connection that rides a socket says what its host must do through
//!   [`FriendsConnection::io`]: dial, write, close, and report the socket's news back.
//!
//! [`FriendsLink`] is the one connection: the friends stream's client session, its host's socket
//! under it.
//!
//! **Must never** do I/O or read a clock: the host owns the socket and passes the time in.

use std::collections::VecDeque;

use core::time::Duration;

pub use auberean_friends as stream;
pub use auberean_friends::{
    Answer, ByeReason, ClientConfig, Cx, Ended, Event, Extra, FriendStatus, Hr, Mode, Request,
    Ticket,
};

use auberean_friends::{ClientOut, ClientSession};

/// How an attempt to connect, or a connection, ended up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connect {
    /// The stream is up: the world greeted the client.
    Connected,
    /// The world closed the stream, saying why.
    Closed,
    /// The attempt failed, or the stream was lost without a word.
    Failed,
}

/// One thing that came back over the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Connect(Connect),
    /// A request's answer, with the mark it was sent with.
    Answer {
        cx: Cx,
        hr: Hr,
        answer: Answer,
    },
    /// An event the world sent unasked.
    Event(Event),
}

/// What the host must do for a connection that rides its socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Io {
    /// Open a connection to this host and port, then report [`FriendsIo::opened`] or
    /// [`FriendsIo::failed`].
    Dial { host: String, port: u16 },
    /// Write these bytes to the open connection.
    Write(Vec<u8>),
    /// Close the connection, after writing everything before this.
    Close,
}

/// The host's side of a connection that rides a socket.
pub trait FriendsIo {
    /// The next thing the host must do, if any.
    fn poll_io(&mut self, now: Duration) -> Option<Io>;
    /// The host's connection opened.
    fn opened(&mut self, now: Duration);
    /// The host's attempt to connect failed.
    fn failed(&mut self, now: Duration);
    /// The host's connection ended.
    fn closed(&mut self, now: Duration);
    /// Bytes read from the host's connection.
    fn on_bytes(&mut self, now: Duration, bytes: &[u8]);
    /// When the connection next needs the time passed in, if nothing arrives first.
    fn next_wake(&self) -> Option<Duration>;
}

/// The friends service's connection, as the client's friends layer calls it.
pub trait FriendsConnection: core::fmt::Debug + Send {
    /// Start connecting to the service at `host` and `port`; one [`Delivery::Connect`] comes back
    /// for each call. Asking again while connected changes nothing.
    fn connect(&mut self, now: Duration, host: &str, port: u16);
    /// Send a request with the caller's mark; its one answer comes back with that mark.
    fn request(&mut self, now: Duration, cx: Cx, request: Request);
    /// Everything that came back since the last call, in order.
    fn dispatch(&mut self, now: Duration) -> Vec<Delivery>;
    /// Close the connection and connect no more.
    fn disconnect(&mut self, now: Duration);
    /// The host's side, for a connection that rides the host's socket.
    fn io(&mut self) -> Option<&mut dyn FriendsIo> {
        None
    }
}

/// The friends stream's client session as the friends layer's connection.
///
/// The session dials again on its own after a stream ends, waiting longer each time; the link
/// lets it only while the layer wants it: after telling the layer the stream is down, it dials no
/// more unless the layer asks to connect again before the link next runs.
#[derive(Debug)]
pub struct FriendsLink {
    session: ClientSession,
    host: String,
    port: u16,
    io: VecDeque<Io>,
    delivered: Vec<Delivery>,
    /// The layer asked to connect, and has not been told the stream is down since.
    armed: bool,
    /// The layer was told the stream is down: unless it asks again, dial no more.
    told_down: bool,
}

impl FriendsLink {
    /// A link that is not connected, greeting worlds as `software` (1 to 64 printable ASCII
    /// characters) with the client's Windows locale number, if it has one.
    #[must_use]
    pub fn new(software: &str, locale: Option<u32>) -> Self {
        Self {
            session: ClientSession::new(ClientConfig {
                software: software.to_owned(),
                locale,
            }),
            host: String::new(),
            port: 0,
            io: VecDeque::new(),
            delivered: Vec::new(),
            armed: false,
            told_down: false,
        }
    }

    /// Run the session's clock and sort what it has: the host's work, the layer's news.
    fn pump(&mut self, now: Duration) {
        if core::mem::take(&mut self.told_down) && !self.armed {
            self.session.disconnect(now);
        }
        for out in self.session.poll(now) {
            match out {
                ClientOut::Dial => self.io.push_back(Io::Dial {
                    host: self.host.clone(),
                    port: self.port,
                }),
                ClientOut::Write(bytes) => self.io.push_back(Io::Write(bytes)),
                ClientOut::Close => self.io.push_back(Io::Close),
                ClientOut::Up(_) => self.delivered.push(Delivery::Connect(Connect::Connected)),
                ClientOut::Down { ended, retry_in } => {
                    if retry_in.is_none() && ended == Ended::OwnBye(ByeReason::Shutdown) {
                        continue;
                    }
                    self.armed = false;
                    let how = if matches!(ended, Ended::PeerBye(_)) {
                        Connect::Closed
                    } else {
                        Connect::Failed
                    };
                    self.delivered.push(Delivery::Connect(how));
                }
                ClientOut::Answer { cx, hr, answer } => {
                    self.delivered.push(Delivery::Answer { cx, hr, answer });
                }
                ClientOut::Event(e) => self.delivered.push(Delivery::Event(e)),
            }
        }
    }
}

impl FriendsConnection for FriendsLink {
    fn connect(&mut self, now: Duration, host: &str, port: u16) {
        host.clone_into(&mut self.host);
        self.port = port;
        self.armed = true;
        self.session.connect(now);
    }

    fn request(&mut self, now: Duration, cx: Cx, request: Request) {
        self.session.request(now, cx, request);
    }

    fn dispatch(&mut self, now: Duration) -> Vec<Delivery> {
        self.pump(now);
        let out = core::mem::take(&mut self.delivered);
        if out
            .iter()
            .any(|d| matches!(d, Delivery::Connect(Connect::Closed | Connect::Failed)))
        {
            self.told_down = true;
        }
        out
    }

    fn disconnect(&mut self, now: Duration) {
        self.armed = false;
        self.session.disconnect(now);
    }

    fn io(&mut self) -> Option<&mut dyn FriendsIo> {
        Some(self)
    }
}

impl FriendsIo for FriendsLink {
    fn poll_io(&mut self, now: Duration) -> Option<Io> {
        self.pump(now);
        self.io.pop_front()
    }

    fn opened(&mut self, now: Duration) {
        self.session.opened(now);
    }

    fn failed(&mut self, now: Duration) {
        self.session.failed(now);
    }

    fn closed(&mut self, now: Duration) {
        self.session.closed(now);
    }

    fn on_bytes(&mut self, now: Duration, bytes: &[u8]) {
        self.session.on_bytes(now, bytes);
    }

    fn next_wake(&self) -> Option<Duration> {
        self.session.next_wake()
    }
}
