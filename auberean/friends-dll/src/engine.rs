//! The library's state, with no I/O: the friends stream's client session, the client's requests
//! and their contexts, the ticket, and the connect events the client waits for.
//!
//! The client calls in ([`Engine::connect`], [`Engine::request`] and the rest), its host reports
//! what its sockets did ([`Engine::opened`], [`Engine::received`] and the rest), and the engine
//! answers with two queues: [`Io`] for the host's sockets, now, and [`Message`]s for the client,
//! which the host hands over only inside the client's pump.
//!
//! **The connect events.** The client calls `connect` at login and waits, with no time limit, for
//! one connect event; after a connection it had, it calls `connect` again the moment it hears the
//! connection is gone. So:
//! - every `connect` is answered later, by exactly one event;
//! - the first connection of a login ends in [`Message::Connected`] when the world welcomes the
//!   client, or in one [`Message::Lost`] when it cannot be reached or does not welcome the client
//!   within [`FIRST_WAIT`], after which the engine stays down until the next login's `connect`;
//! - a connection that was up and ends is one [`Message::Disconnected`] when the world said
//!   goodbye, else one [`Message::Lost`]; the session dials again on its own waits (1, 2, 5, 10,
//!   then every 30 seconds), and the client's prompt `connect` does not cut a wait short; when the
//!   world welcomes it again, [`Message::Connected`] once more, and the client presents its ticket
//!   again;
//! - a `connect` while connected is a new login: the stream is closed and dialed again, with one
//!   event for the new attempt.
//!
//! **Contexts.** Each request carries the client's context, an opaque pointer of its own that must
//! come back in exactly one result. The engine gives each request its own mark on the stream and
//! keeps the context beside it; an answer from the world finds its context by the mark, and one
//! that matches no request waiting is dropped. The session answers every request exactly once
//! (the world's answer, or its own when the stream is down), so every context comes back once.
//! A request is never refused on the spot: one made while the stream is down is answered at the
//! next pump with the not-connected code, since the client drops a tell that fails at once
//! without a word.

use core::time::Duration;
use std::collections::{BTreeMap, VecDeque};

use stream::{
    Answer, ClientConfig, ClientOut, ClientSession, Cx, Ended, Event, Extra, Hr, Request,
};

/// How long the first connection of a login may take before the engine gives it up with
/// [`Message::Lost`]: the client waits for its connect event with no limit of its own.
pub const FIRST_WAIT: Duration = Duration::from_secs(15);

/// The client's context for a request: its own pointer, opaque, as 32 bits.
pub type Context = u32;

/// What the host's sockets are to do, now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Io {
    /// Open connection `conn` to the world, then report [`Engine::opened`] or [`Engine::failed`].
    Dial { conn: u64, host: String, port: u16 },
    /// Write these bytes to connection `conn`.
    Write { conn: u64, bytes: Vec<u8> },
    /// Close connection `conn` after writing what was given before.
    Close { conn: u64 },
}

/// What a result carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    /// The answer to a request on the stream (or the session's own, for one it could not send).
    Answer(Answer),
    /// The client is done with its ticket: answered by the library, since the stream has no such
    /// request.
    TicketReleased,
}

/// What the library hands the client, in order, inside its pump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// The world welcomed the client.
    Connected,
    /// The world said goodbye and closed the stream.
    Disconnected,
    /// The world could not be reached, or the stream was lost.
    Lost,
    /// A request's result: the client's context, the code and what it carries.
    Result {
        context: Context,
        hr: Hr,
        reply: Reply,
    },
    /// An event from the world.
    Event(Event),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Not connected, and not trying: before the first `connect`, or after a first connection
    /// failed or the client disconnected.
    Off,
    /// A login's first connection, asked for at `since`, has had no event yet.
    First { since: Duration },
    /// Connected, and the client was told.
    Up,
    /// A connection the client had is gone, and the client was told; the session dials again on
    /// its waits.
    Lost,
}

/// The library's state.
#[derive(Debug)]
pub struct Engine {
    software: String,
    session: ClientSession,
    phase: Phase,
    target: Option<(String, u16)>,
    /// The latest connection the session dialed.
    conn: u64,
    /// Requests sent and not yet answered: the stream's mark, and the client's context.
    calls: BTreeMap<u64, Context>,
    next_mark: u64,
    ticket: Option<Vec<u8>>,
    messages: VecDeque<Message>,
    io: Vec<Io>,
}

impl Engine {
    /// A library that is not connected; `software` names it in the stream's greeting.
    #[must_use]
    pub fn new(software: &str) -> Self {
        Self {
            software: software.to_owned(),
            session: ClientSession::new(ClientConfig {
                software: software.to_owned(),
                locale: None,
            }),
            phase: Phase::Off,
            target: None,
            conn: 0,
            calls: BTreeMap::new(),
            next_mark: 1,
            ticket: None,
            messages: VecDeque::new(),
            io: Vec::new(),
        }
    }

    /// The client sets the library up, at every login, with its Windows locale. A connection that
    /// is not up takes the locale for its greeting.
    pub fn init(&mut self, locale: u32) {
        if self.phase == Phase::Off && !self.session.is_up() {
            self.session = ClientSession::new(ClientConfig {
                software: self.software.clone(),
                locale: Some(locale),
            });
        }
    }

    /// The client asks to be connected to `host` (as the player typed it) and `port`. The answer
    /// is always a connect event, later.
    pub fn connect(&mut self, now: Duration, host: &str, port: u16) {
        match self.phase {
            Phase::Off => {
                self.target = Some((host.to_owned(), port));
                self.phase = Phase::First { since: now };
                self.session.connect(now);
            }
            Phase::Up => {
                // A new login while connected: close the stream, then dial again for it.
                self.target = Some((host.to_owned(), port));
                self.phase = Phase::Off;
                self.session.disconnect(now);
                self.drain(now);
                self.phase = Phase::First { since: now };
                self.session.connect(now);
            }
            // The event this call waits for is still to come: the first connection's, or the one
            // the session's next dial brings.
            Phase::First { .. } | Phase::Lost => {}
        }
        self.drain(now);
    }

    /// The client is leaving: say goodbye, close the stream and dial no more.
    pub fn disconnect(&mut self, now: Duration) {
        self.phase = Phase::Off;
        self.session.disconnect(now);
        self.drain(now);
    }

    /// A request with the client's context. Its result comes out of the pump: the world's answer,
    /// or the not-connected code when the stream is down.
    pub fn request(&mut self, now: Duration, context: Context, request: Request) {
        let mark = self.next_mark;
        self.next_mark += 1;
        self.calls.insert(mark, context);
        self.session.request(now, Cx(mark), request);
        self.drain(now);
    }

    /// The ticket the client was started with, as it hands it over: the text, then a NUL. The
    /// library keeps the text.
    pub fn set_ticket(&mut self, bytes: &[u8]) {
        let text = trim_nul(bytes);
        self.ticket = (!text.is_empty()).then(|| text.to_vec());
    }

    /// The ticket, for the client to present: `None` when it was started without one.
    #[must_use]
    pub fn ticket(&self) -> Option<&[u8]> {
        self.ticket.as_deref()
    }

    /// The client presents a ticket (the one [`Engine::ticket`] gave it) to the world.
    pub fn present_ticket(&mut self, now: Duration, context: Context, bytes: &[u8]) {
        let ticket = String::from_utf8_lossy(trim_nul(bytes)).into_owned();
        self.request(
            now,
            context,
            Request::PresentTicket {
                email: String::new(),
                ticket,
            },
        );
    }

    /// The client is done with its ticket: answered at once, at the next pump.
    pub fn release_ticket(&mut self, context: Context) {
        self.messages.push_back(Message::Result {
            context,
            hr: Hr::OK,
            reply: Reply::TicketReleased,
        });
    }

    /// Connection `conn` is open.
    pub fn opened(&mut self, now: Duration, conn: u64) {
        if conn == self.conn {
            self.session.opened(now);
            self.drain(now);
        }
    }

    /// Connection `conn` could not be opened.
    pub fn failed(&mut self, now: Duration, conn: u64) {
        if conn == self.conn {
            self.session.failed(now);
            self.drain(now);
        }
    }

    /// Bytes read from connection `conn`.
    pub fn received(&mut self, now: Duration, conn: u64, bytes: &[u8]) {
        if conn == self.conn {
            self.session.on_bytes(now, bytes);
            self.drain(now);
        }
    }

    /// Connection `conn` ended.
    pub fn closed(&mut self, now: Duration, conn: u64) {
        if conn == self.conn {
            self.session.closed(now);
            self.drain(now);
        }
    }

    /// Run the clock: the session's keep-alive, deadlines and waits, and the limit on a first
    /// connection.
    pub fn tick(&mut self, now: Duration) {
        if let Phase::First { since } = self.phase {
            if now.saturating_sub(since) >= FIRST_WAIT {
                self.session.disconnect(now);
                self.drain(now);
                if matches!(self.phase, Phase::First { .. }) {
                    self.messages.push_back(Message::Lost);
                    self.phase = Phase::Off;
                }
            }
        }
        self.drain(now);
    }

    /// What the host's sockets are to do, in order.
    pub fn take_io(&mut self) -> Vec<Io> {
        core::mem::take(&mut self.io)
    }

    /// The next thing to hand the client.
    pub fn next_message(&mut self) -> Option<Message> {
        self.messages.pop_front()
    }

    /// How many requests wait for an answer.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.calls.len()
    }

    /// Whether the stream is up.
    #[must_use]
    pub fn is_up(&self) -> bool {
        self.session.is_up()
    }

    /// Everything the session has, sorted into the sockets' work and the client's messages.
    fn drain(&mut self, now: Duration) {
        loop {
            let outs = self.session.poll(now);
            if outs.is_empty() {
                return;
            }
            for out in outs {
                self.handle(now, out);
            }
        }
    }

    fn handle(&mut self, now: Duration, out: ClientOut) {
        match out {
            ClientOut::Dial => {
                self.conn += 1;
                match &self.target {
                    Some((host, port)) => self.io.push(Io::Dial {
                        conn: self.conn,
                        host: host.clone(),
                        port: *port,
                    }),
                    None => self.session.failed(now),
                }
            }
            ClientOut::Write(bytes) => self.io.push(Io::Write {
                conn: self.conn,
                bytes,
            }),
            ClientOut::Close => self.io.push(Io::Close { conn: self.conn }),
            ClientOut::Up(_) => {
                if matches!(self.phase, Phase::First { .. } | Phase::Lost) {
                    self.messages.push_back(Message::Connected);
                    self.phase = Phase::Up;
                }
            }
            ClientOut::Down { ended, .. } => match self.phase {
                Phase::First { .. } => {
                    self.messages.push_back(Message::Lost);
                    self.phase = Phase::Off;
                    self.session.disconnect(now);
                }
                Phase::Up => {
                    self.messages.push_back(match ended {
                        Ended::PeerBye(_) => Message::Disconnected,
                        _ => Message::Lost,
                    });
                    self.phase = Phase::Lost;
                }
                Phase::Lost | Phase::Off => {}
            },
            ClientOut::Answer { cx, hr, answer } => {
                if let Some(context) = self.calls.remove(&cx.0) {
                    self.messages.push_back(Message::Result {
                        context,
                        hr,
                        reply: Reply::Answer(answer),
                    });
                }
            }
            ClientOut::Event(event) => self.messages.push_back(Message::Event(event)),
        }
    }
}

/// The extra data the client attaches to a tell, as the stream carries it: 16 bytes. The client's
/// is a version, its chat flags and the teller's marks, 12 bytes when the teller has none; data
/// shorter than 16 bytes is carried with zeros after it, which the reader of the data never
/// reaches. None for none, or for data longer than 16 bytes, which the tell goes without.
#[must_use]
pub fn extra(bytes: &[u8]) -> Option<Extra> {
    if bytes.is_empty() || bytes.len() > 16 {
        return None;
    }
    let mut out = [0u8; 16];
    out[..bytes.len()].copy_from_slice(bytes);
    Some(Extra(out))
}

/// The bytes before the first NUL.
fn trim_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |n| &bytes[..n])
}
