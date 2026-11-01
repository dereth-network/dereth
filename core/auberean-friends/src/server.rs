//! The world's half of one stream, with no I/O: its host accepts the connection, writes and reads,
//! and passes the time in.
//!
//! The stream checks the preamble, answers the greeting, holds the deadlines, the keep-alive and
//! the bound on requests, and answers on its own what needs no world: a request before the ticket
//! (not ticketed), a second ticket (already ticketed), any request while the account has no
//! character in the world (not connected), `Flags` (echoed), and the reserved `CallCreate` and
//! `SendToRoomEx` (not implemented). The rest it hands to its host as [`ServerEvent`]s: the ticket
//! to check ([`ServerStream::admit`] or [`ServerStream::refuse`]), and each request to answer
//! ([`ServerStream::answer`]).
//!
//! The host binds the stream to the account's character: [`ServerStream::enter`] when it enters the
//! world, or at once on admitting a stream whose account's character is already in it (a client
//! that reconnects), sends the friends list as `FStart`; [`ServerStream::leave`] when it leaves sends
//! nothing.

use core::fmt;
use core::time::Duration;

use crate::element::{
    Answer, ByeReason, Cx, Element, Event, FriendStatus, Hello, Hr, Request, Welcome, PROTO,
};
use crate::frame::{FrameError, FrameReader, PREAMBLE, WEBSOCKET_OPENING};
use crate::stream::{secs, Ended, BURST, HELLO_DEADLINE, PING, RATE, SILENCE, TICKET_DEADLINE};

/// What the world says of itself in its greeting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    /// The world's network slug; empty for a world on no network.
    pub world: String,
    /// The world's name as its login shows it, which the client recognizes its own world by.
    pub name: String,
    /// The keep-alive interval, in seconds.
    pub ping: u32,
}

impl ServerConfig {
    /// A world's greeting, with the keep-alive interval of [`PING`].
    #[must_use]
    pub fn new(world: &str, name: &str) -> Self {
        Self {
            world: world.to_owned(),
            name: name.to_owned(),
            ping: u32::try_from(PING.as_secs()).unwrap_or(20),
        }
    }
}

/// What the stream asks of its host, and tells it, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerOut {
    /// Write these bytes to the connection.
    Write(Vec<u8>),
    /// Close the connection, after writing everything before this.
    Close,
    Event(ServerEvent),
}

/// What the host must know or do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// The client greeted the world, and was welcomed.
    Hello(Hello),
    /// Check this ticket as the game login checks an account and its secret, then
    /// [`ServerStream::admit`] or [`ServerStream::refuse`] it.
    Ticket { cx: Cx, ticket: String },
    /// Answer this request with [`ServerStream::answer`].
    Request { cx: Cx, request: Request },
    /// The stream is over; the host forgets it.
    Ended(Ended),
}

/// Why the stream did not send what its host gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotSent {
    /// The stream is not admitted, or is over.
    NotAdmitted,
    /// The account has no character in the world: events wait for one.
    NotInWorld,
    /// The element does not fit a frame.
    TooLong,
}

impl fmt::Display for NotSent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotAdmitted => "the stream is not admitted",
            Self::NotInWorld => "the account has no character in the world",
            Self::TooLong => "the element does not fit a frame",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Preamble,
    Hello,
    Ticket {
        welcomed: Duration,
        checking: Option<Cx>,
    },
    Admitted {
        in_world: bool,
    },
    Ended,
}

/// One elements' worth of the bound on requests, in thousandths.
const TOKEN: u64 = 1_000;

/// The world's half of one friends stream.
#[derive(Debug)]
pub struct ServerStream {
    config: ServerConfig,
    state: State,
    opened: Duration,
    opening: Vec<u8>,
    reader: FrameReader,
    last_heard: Duration,
    last_sent: Duration,
    next_seq: u32,
    /// Thousandths of an element the stream may still send at once.
    allowance: u64,
    allowance_at: Duration,
    out: Vec<ServerOut>,
}

impl ServerStream {
    /// A stream the host has just accepted.
    #[must_use]
    pub fn new(now: Duration, config: ServerConfig) -> Self {
        Self {
            config,
            state: State::Preamble,
            opened: now,
            opening: Vec::with_capacity(4),
            reader: FrameReader::new(),
            last_heard: now,
            last_sent: now,
            next_seq: 1,
            allowance: u64::from(BURST) * TOKEN,
            allowance_at: now,
            out: Vec::new(),
        }
    }

    /// Bytes read from the connection.
    pub fn on_bytes(&mut self, now: Duration, bytes: &[u8]) {
        if self.state == State::Ended {
            return;
        }
        self.last_heard = now;
        let mut rest = bytes;
        if self.state == State::Preamble {
            let take = (4 - self.opening.len()).min(rest.len());
            self.opening.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            let websocket = WEBSOCKET_OPENING.starts_with(&self.opening);
            if !PREAMBLE.starts_with(&self.opening) && !websocket {
                return self.finish(Ended::NotAStream { websocket: false });
            }
            if self.opening.len() < 4 {
                return;
            }
            if websocket {
                return self.finish(Ended::NotAStream { websocket: true });
            }
            self.state = State::Hello;
        }
        self.reader.push(rest);
        while self.state != State::Ended {
            match self.reader.next_frame() {
                Ok(None) => return,
                Ok(Some(text)) => {
                    if !self.spend(now) {
                        return self.bye(now, ByeReason::Flood);
                    }
                    match Element::parse(&text) {
                        Ok(element) => self.handle(now, element),
                        Err(_) => return self.bye(now, ByeReason::Protocol),
                    }
                }
                Err(_) => return self.bye(now, ByeReason::Protocol),
            }
        }
    }

    /// Take one element from the allowance, refilled at [`RATE`] a second up to [`BURST`].
    fn spend(&mut self, now: Duration) -> bool {
        let ms =
            u64::try_from(now.saturating_sub(self.allowance_at).as_millis()).unwrap_or(u64::MAX);
        self.allowance = self
            .allowance
            .saturating_add(ms.saturating_mul(u64::from(RATE)))
            .min(u64::from(BURST) * TOKEN);
        self.allowance_at = now;
        if self.allowance < TOKEN {
            return false;
        }
        self.allowance -= TOKEN;
        true
    }

    fn handle(&mut self, now: Duration, element: Element) {
        if self.state == State::Hello {
            return match element {
                Element::Hello(h) if h.proto == PROTO => {
                    self.send(
                        now,
                        &Element::Welcome(Welcome {
                            proto: PROTO,
                            world: self.config.world.clone(),
                            name: self.config.name.clone(),
                            ping: self.config.ping,
                        }),
                    );
                    self.state = State::Ticket {
                        welcomed: now,
                        checking: None,
                    };
                    self.out.push(ServerOut::Event(ServerEvent::Hello(h)));
                }
                _ => self.bye(now, ByeReason::Protocol),
            };
        }
        match element {
            Element::Ping { seq } => self.send(now, &Element::Pong { seq }),
            Element::Bye { reason } => self.finish(Ended::PeerBye(reason)),
            Element::Request { cx, request } => self.request(now, cx, request),
            // A pong, an element that goes the other way, or one this version does not know.
            _ => {}
        }
    }

    fn request(&mut self, now: Duration, cx: Cx, request: Request) {
        let kind = request.kind();
        let refuse = |s: &mut Self, hr| s.send_answer(now, cx, hr, Answer::empty(kind));
        match (self.state, request) {
            (
                State::Ticket {
                    welcomed,
                    checking: None,
                },
                Request::PresentTicket { ticket, .. },
            ) => {
                self.state = State::Ticket {
                    welcomed,
                    checking: Some(cx),
                };
                self.out
                    .push(ServerOut::Event(ServerEvent::Ticket { cx, ticket }));
            }
            (_, Request::PresentTicket { .. }) => refuse(self, Hr::ALREADY_TICKETED),
            (State::Ticket { .. }, _) => refuse(self, Hr::NOT_TICKETED),
            (State::Admitted { in_world: false }, _) => refuse(self, Hr::NOT_CONNECTED),
            (_, Request::Flags { word, flags, .. }) => self.send_answer(
                now,
                cx,
                Hr::OK,
                Answer::Flags {
                    which_flag: word,
                    flags,
                },
            ),
            (_, Request::CallCreate | Request::SendToRoomEx) => {
                refuse(self, Hr::NOT_IMPLEMENTED);
            }
            (_, request) => self
                .out
                .push(ServerOut::Event(ServerEvent::Request { cx, request })),
        }
    }

    /// Admit the ticket the stream presented with `cx`: the stream is bound to its account from
    /// now on. False when no ticket with that mark waits.
    pub fn admit(&mut self, now: Duration, cx: Cx) -> bool {
        if self.checking() != Some(cx) {
            return false;
        }
        self.send_answer(now, cx, Hr::OK, Answer::PresentTicket { status: 0 });
        self.state = State::Admitted { in_world: false };
        true
    }

    /// Refuse the ticket the stream presented with `cx`, with [`Hr::TICKET_REFUSED`] or
    /// [`Hr::BANNED`]. The stream may present another before its deadline. False when no ticket
    /// with that mark waits.
    pub fn refuse(&mut self, now: Duration, cx: Cx, hr: Hr) -> bool {
        if self.checking() != Some(cx) {
            return false;
        }
        self.send_answer(now, cx, hr, Answer::PresentTicket { status: 0 });
        if let State::Ticket { checking, .. } = &mut self.state {
            *checking = None;
        }
        true
    }

    /// The mark of the ticket waiting for the host's check.
    fn checking(&self) -> Option<Cx> {
        match self.state {
            State::Ticket { checking, .. } => checking,
            _ => None,
        }
    }

    /// The account's character is in the world: send its friends, each with its state, as
    /// `FStart`, and serve its requests from now on.
    ///
    /// # Errors
    /// [`NotSent::NotAdmitted`] before the ticket is admitted; [`NotSent::TooLong`] for a list
    /// that does not fit a frame.
    pub fn enter(&mut self, now: Duration, friends: Vec<FriendStatus>) -> Result<(), NotSent> {
        if !matches!(self.state, State::Admitted { .. }) {
            return Err(NotSent::NotAdmitted);
        }
        self.try_send(now, &Element::Event(Event::FStart { friends }))?;
        self.state = State::Admitted { in_world: true };
        Ok(())
    }

    /// The account's character left the world. Nothing is sent; requests are answered not
    /// connected until the next [`ServerStream::enter`].
    pub fn leave(&mut self) {
        if let State::Admitted { in_world } = &mut self.state {
            *in_world = false;
        }
    }

    /// Answer a request the stream handed over.
    ///
    /// # Errors
    /// [`NotSent::NotAdmitted`] on a stream that is not admitted; [`NotSent::TooLong`] for an answer
    /// that does not fit a frame.
    pub fn answer(&mut self, now: Duration, cx: Cx, hr: Hr, answer: Answer) -> Result<(), NotSent> {
        if !matches!(self.state, State::Admitted { .. }) {
            return Err(NotSent::NotAdmitted);
        }
        self.try_send(now, &Element::Answer { cx, hr, answer })
    }

    /// Send an event: a friend's state ([`Event::FriendStat`]) or a tell ([`Event::UTellEv`]).
    ///
    /// # Errors
    /// [`NotSent::NotAdmitted`] on a stream that is not admitted, [`NotSent::NotInWorld`] while the
    /// account has no character in the world, [`NotSent::TooLong`] for one that does not fit a
    /// frame.
    pub fn event(&mut self, now: Duration, event: Event) -> Result<(), NotSent> {
        match self.state {
            State::Admitted { in_world: true } => self.try_send(now, &Element::Event(event)),
            State::Admitted { in_world: false } => Err(NotSent::NotInWorld),
            _ => Err(NotSent::NotAdmitted),
        }
    }

    /// A newer stream for the same account took this one's place: say so and close.
    pub fn replace(&mut self, now: Duration) {
        self.close(now, ByeReason::Replaced);
    }

    /// Say goodbye for `reason` and close.
    pub fn close(&mut self, now: Duration, reason: ByeReason) {
        if self.state != State::Ended {
            self.bye(now, reason);
        }
    }

    /// The host's connection ended.
    pub fn closed(&mut self, _now: Duration) {
        if self.state != State::Ended {
            self.finish(Ended::Lost);
        }
    }

    /// Run the clock: the deadlines, the silence and the keep-alive. Then everything the stream has
    /// for its host, in order.
    pub fn poll(&mut self, now: Duration) -> Vec<ServerOut> {
        let since = |t: Duration| now.saturating_sub(t);
        match self.state {
            State::Ended => {}
            State::Preamble if since(self.opened) >= HELLO_DEADLINE => {
                self.finish(Ended::NotAStream { websocket: false });
            }
            State::Hello if since(self.opened) >= HELLO_DEADLINE => self.bye(now, ByeReason::Idle),
            State::Ticket { welcomed, .. } if since(welcomed) >= TICKET_DEADLINE => {
                self.bye(now, ByeReason::Idle);
            }
            _ if since(self.last_heard) >= SILENCE => self.bye(now, ByeReason::Idle),
            State::Ticket { .. } | State::Admitted { .. }
                if since(self.last_sent) >= secs(self.config.ping) =>
            {
                let seq = self.next_seq;
                self.next_seq = self.next_seq.wrapping_add(1);
                self.send(now, &Element::Ping { seq });
            }
            _ => {}
        }
        core::mem::take(&mut self.out)
    }

    /// When the stream next needs [`ServerStream::poll`] if nothing arrives first.
    #[must_use]
    pub fn next_wake(&self) -> Option<Duration> {
        let silent = self.last_heard + SILENCE;
        let ping = self.last_sent + secs(self.config.ping);
        match self.state {
            State::Ended => None,
            State::Preamble | State::Hello => Some(silent.min(self.opened + HELLO_DEADLINE)),
            State::Ticket { welcomed, .. } => {
                Some(silent.min(ping).min(welcomed + TICKET_DEADLINE))
            }
            State::Admitted { .. } => Some(silent.min(ping)),
        }
    }

    /// Whether the stream's ticket is admitted and the stream not over.
    #[must_use]
    pub fn is_admitted(&self) -> bool {
        matches!(self.state, State::Admitted { .. })
    }

    /// Whether the stream acts for a character in the world.
    #[must_use]
    pub fn is_in_world(&self) -> bool {
        self.state == State::Admitted { in_world: true }
    }

    /// Whether the stream is over.
    #[must_use]
    pub fn is_ended(&self) -> bool {
        self.state == State::Ended
    }

    fn try_send(&mut self, now: Duration, element: &Element) -> Result<(), NotSent> {
        let mut bytes = Vec::new();
        element
            .write_frame(&mut bytes)
            .map_err(|_: FrameError| NotSent::TooLong)?;
        self.last_sent = now;
        match self.out.last_mut() {
            Some(ServerOut::Write(buf)) => buf.extend_from_slice(&bytes),
            _ => self.out.push(ServerOut::Write(bytes)),
        }
        Ok(())
    }

    /// Send one of the stream's own small elements, which always fit a frame.
    fn send(&mut self, now: Duration, element: &Element) {
        let _fits = self.try_send(now, element);
    }

    fn send_answer(&mut self, now: Duration, cx: Cx, hr: Hr, answer: Answer) {
        self.send(now, &Element::Answer { cx, hr, answer });
    }

    fn bye(&mut self, now: Duration, reason: ByeReason) {
        self.send(now, &Element::Bye { reason });
        self.finish(Ended::OwnBye(reason));
    }

    fn finish(&mut self, ended: Ended) {
        self.state = State::Ended;
        self.out.push(ServerOut::Close);
        self.out.push(ServerOut::Event(ServerEvent::Ended(ended)));
    }
}
