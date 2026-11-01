//! The engine on the real clock with real sockets: what the client's calls reach.
//!
//! Every call does its part at once and returns: a request becomes bytes for the connection's
//! writer, a `connect` a dial on a thread. [`Host::intake`] runs at the start of each pump: what the
//! sockets reported goes into the engine, the clock runs, and the sockets get their work; then the
//! pump takes the client's messages one at a time with [`Host::next_message`], so that what the
//! client calls while it handles one (a `connect`, a ticket, a request) lands behind it.

use core::time::Duration;
use std::time::Instant;

use stream::Request;

use crate::engine::{Context, Engine, Io, Message};
use crate::net::{Net, NetEvent};

/// The library's name and version, as its greeting says them.
pub const SOFTWARE: &str = concat!("auberean-friends-dll/", env!("CARGO_PKG_VERSION"));

/// The engine, its sockets and its clock.
#[derive(Debug)]
pub struct Host {
    engine: Engine,
    net: Net,
    start: Instant,
}

impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}

impl Host {
    /// A library that is not connected.
    #[must_use]
    pub fn new() -> Self {
        Self {
            engine: Engine::new(SOFTWARE),
            net: Net::new(),
            start: Instant::now(),
        }
    }

    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    /// The client sets the library up with its locale.
    pub fn init(&mut self, locale: u32) {
        self.engine.init(locale);
    }

    /// The client asks to be connected.
    pub fn connect(&mut self, host: &str, port: u16) {
        tracing::info!(host, port, "connect");
        let now = self.now();
        self.engine.connect(now, host, port);
        self.run_io();
    }

    /// The client is leaving.
    pub fn disconnect(&mut self) {
        tracing::info!("disconnect");
        let now = self.now();
        self.engine.disconnect(now);
        self.run_io();
    }

    /// A request with the client's context.
    pub fn request(&mut self, context: Context, request: Request) {
        tracing::debug!(kind = ?request.kind(), "request");
        let now = self.now();
        self.engine.request(now, context, request);
        self.run_io();
    }

    /// The ticket the client was started with.
    pub fn set_ticket(&mut self, bytes: &[u8]) {
        self.engine.set_ticket(bytes);
    }

    /// The ticket, for the client to present.
    #[must_use]
    pub fn ticket(&self) -> Option<&[u8]> {
        self.engine.ticket()
    }

    /// The client presents its ticket.
    pub fn present_ticket(&mut self, context: Context, bytes: &[u8]) {
        tracing::info!("ticket presented");
        let now = self.now();
        self.engine.present_ticket(now, context, bytes);
        self.run_io();
    }

    /// The client is done with its ticket.
    pub fn release_ticket(&mut self, context: Context) {
        self.engine.release_ticket(context);
    }

    /// What the sockets reported, into the engine; the clock; the sockets' work.
    pub fn intake(&mut self) {
        for event in self.net.events() {
            let now = self.now();
            match event {
                NetEvent::Opened(conn) => self.engine.opened(now, conn),
                NetEvent::Failed(conn) => {
                    tracing::info!(conn, "dial failed");
                    self.engine.failed(now, conn);
                }
                NetEvent::Received(conn, bytes) => self.engine.received(now, conn, &bytes),
                NetEvent::Closed(conn) => self.engine.closed(now, conn),
            }
            self.run_io();
        }
        let now = self.now();
        self.engine.tick(now);
        self.run_io();
    }

    /// The next thing to hand the client.
    pub fn next_message(&mut self) -> Option<Message> {
        let message = self.engine.next_message();
        if let Some(m) = &message {
            log_message(m);
        }
        message
    }

    fn run_io(&mut self) {
        for io in self.engine.take_io() {
            match io {
                Io::Dial { conn, host, port } => {
                    tracing::info!(conn, host, port, "dial");
                    self.net.dial(conn, &host, port);
                }
                Io::Write { conn, bytes } => self.net.write(conn, bytes),
                Io::Close { conn } => self.net.close(conn),
            }
        }
    }
}

/// What the log says of a message: never a tell's text.
fn log_message(message: &Message) {
    match message {
        Message::Connected => tracing::info!("connected"),
        Message::Disconnected => tracing::info!("disconnected by the world"),
        Message::Lost => tracing::info!("connection lost"),
        Message::Result { hr, reply, .. } => {
            tracing::debug!(hr = %hr, answer = reply_kind(reply), "result");
        }
        Message::Event(stream::Event::FStart { friends }) => {
            tracing::debug!(friends = friends.len(), "friends");
        }
        Message::Event(stream::Event::FriendStat { user, mode }) => {
            tracing::debug!(user, mode = mode.0, "friend state");
        }
        Message::Event(stream::Event::UTellEv { user, .. }) => tracing::debug!(user, "tell"),
    }
}

fn reply_kind(reply: &crate::engine::Reply) -> &'static str {
    match reply {
        crate::engine::Reply::Answer(a) => a.kind().answer_name(),
        crate::engine::Reply::TicketReleased => "ticket released",
    }
}
