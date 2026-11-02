//! The client's session, sans-IO: the game login, the login connection held all session, world
//! connections reached by referral, the receive queues and their drain order, messages in and out.
//!
//! **Depends on** `auberean-protocol` (the messages), `auberean-friends` (the friends stream),
//! `auberean-primitives` (the string scramble)
//! and the shared connection manager and transport (`dereth-client-net`, `dereth-transport`, with
//! `dereth-primitives` for their times). **Used by** the client runtime
//! (`auberean-client-runtime`).
//!
//! A session is a **login connection**, opened by the game login and held for the whole session,
//! and the **world connections** the login and world servers refer the client to: one is current
//! at a time, and a referral to another moves the player there. The host owns the sockets and the
//! clock: it hands [`Session`] each datagram with the time, sends what [`Session::poll_transmit`]
//! gives, and reads what happened from [`Session::poll_event`].
//!
//! - [`friends`]: the friends service's connection, as the client's friends layer sees it.
//! - [`logon`]: the game login's body: the version, the ticket, the account, the session key.
//! - [`queues`]: the queues the client receives on, the order it reads them in, and the queue each
//!   of its messages goes out on.
//! - [`session`]: the session.
//!
//! **Must never** do I/O or read a clock: datagrams and times come in, datagrams and events go out.

pub mod friends;
pub mod logon;
pub mod queues;
pub mod session;

pub use dereth_primitives::LocalTime;
/// The errors a server's connection sections carry.
pub use dereth_transport::conn::NetErrorCode;
pub use session::{Event, Incoming, Received, Session, Settings};
