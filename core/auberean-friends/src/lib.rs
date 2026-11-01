//! The friends stream: the protocol between an Asheron's Call 2 client and its world's friends
//! service, as a codec and two sessions that do no I/O.
//!
//! **What it is.** The client's friends service (friends with presence, an ignore list, an away
//! state, and tells to `Name@world` on other worlds) runs over a stream of its own beside the game
//! connection. The stream carries the service's commands, answers and events as one XML element a
//! frame, in the vocabulary the client's friends service always used; the transport, the framing,
//! the greeting, keep-alive and the ticket are this network's own. The network's specification
//! has the whole protocol (`docs/network/08-friends-stream.md`); the vectors in `vectors/` pin it
//! for any implementation.
//!
//! - [`frame`]: the preamble and the frames;
//! - [`xml`]: the small XML the elements are written in;
//! - [`element`]: every element as a typed value ([`Element`]), read and written;
//! - [`ticket`]: the `a1.` ticket, written and read but never checked;
//! - [`client`]: the client's half ([`ClientSession`]): the greeting, keep-alive, and the waits
//!   between reconnects;
//! - [`server`]: the world's half of one stream ([`ServerStream`]): the greeting, the deadlines,
//!   the bounds, the ticket's place, and the answers that need no world.
//!
//! **How to use it.** A session takes bytes and the time, and gives back bytes to write and what
//! happened; its host owns the socket and the clock, and calls `poll` when `next_wake` comes:
//!
//! ```
//! use core::time::Duration;
//! use auberean_friends::{ClientConfig, ClientOut, ClientSession, ServerConfig, ServerStream};
//!
//! let now = Duration::ZERO;
//! let mut client = ClientSession::new(ClientConfig {
//!     software: "example/1.0".into(),
//!     locale: None,
//! });
//! client.connect(now);
//! assert_eq!(client.poll(now), [ClientOut::Dial]);
//! client.opened(now); // the host's connection is open
//! let mut world = ServerStream::new(now, ServerConfig::new("dawnsong", "Dawnsong"));
//! for out in client.poll(now) {
//!     if let ClientOut::Write(bytes) = out {
//!         world.on_bytes(now, &bytes); // over the host's connection
//!     }
//! }
//! ```
//!
//! **Depends on** nothing. **Used by** what speaks the stream: the world's friends service in the
//! server, beside its login (a [`ServerStream`] for each connection on the login port, with
//! [`Ticket::decode`] for the ticket's account and secret); the client's friends layer (a
//! [`ClientSession`] run by the client's network host, a thread with a socket on the desktop);
//! the friends library the retail client loads in place of its original (a [`ClientSession`] on
//! its own thread, with the client's call marks as [`Cx`]); and the launcher, which writes the
//! ticket ([`Ticket::encode`]) and asks a world whether it answers the stream (the preamble, a
//! `Hello` and its `Welcome`).
//!
//! **Must never** open a socket, a file or a thread, or read a clock: the host does, so the crate
//! builds for the browser and its tests run on a clock they turn themselves.

pub mod client;
pub mod element;
pub mod frame;
pub mod server;
pub mod stream;
pub mod ticket;
pub mod xml;

pub use client::{ClientConfig, ClientOut, ClientSession};
pub use element::{
    Answer, ByeReason, Cx, Element, ElementError, Event, Extra, FriendStatus, Hello, Hr, Kind,
    Mode, Request, Welcome, PROTO,
};
pub use frame::{FrameError, FrameReader, MAX_FRAME, PREAMBLE};
pub use server::{NotSent, ServerConfig, ServerEvent, ServerOut, ServerStream};
pub use stream::Ended;
pub use ticket::{Ticket, TicketError};
