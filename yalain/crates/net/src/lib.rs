//! Server sessions over the shared transport: the login and world services' handshakes, each
//! connection's reliable transport, the referral that takes a client from the login service to
//! the world, message sending under blob ids, and teardown.
//!
//! **Depends on** the shared transport (`dereth-transport`, unchanged, with `dereth-primitives`),
//! `auberean-primitives`, `hmac` with `sha2` for the status ping's tokens, the Dereth Network's
//! protocol (`dereth-network-protocol`) for the hub's challenge to the game port, and the game
//! servers' WebSocket endpoint (`dereth-server-websocket`, with the `websocket` feature).
//! **Used by** the game server (`yalain-world`), the server executable
//! (`yalain`), its test kit (`yalain-testkit`) and the client's test kit's server double
//! (`auberean-testkit`).
//!
//! - [`server::ServerNet`]: the connections, sans-IO.
//! - [`endpoint::Endpoint`]: one connection's transport, either end (the test kit's clients use
//!   the client end).
//! - [`handshakes`]: the logins answered whose clients have not answered back.
//! - [`login`]: the game login body.
//! - [`referral`]: which address a referral sends a client to.
//! - [`pace`]: the pace a server sends its large messages at.
//! - [`memory::MemoryNet`]: an in-memory network for tests; [`udp`] (feature `udp`): sockets.
//! - `websocket` (feature `websocket`): the WebSocket endpoint beside the sockets.
//! - [`status_ping`]: the world's live status for a launcher or a hub, on the login port.
//!
//! **Must never** read what a message means: it moves message bytes and queue numbers.

pub mod blob;
pub mod endpoint;
pub mod handshakes;
pub mod login;
pub mod memory;
pub mod pace;
pub mod referral;
pub mod server;
pub mod status_ping;
#[cfg(feature = "udp")]
pub mod udp;
#[cfg(feature = "websocket")]
pub mod websocket;

pub use blob::BlobKind;
pub use dereth_primitives::LocalTime;
pub use server::{
    fixed_stream, CloseReason, ConnId, Datagram, NetConfig, NetEvent, Port, ServerNet, Service,
    ServiceId,
};
