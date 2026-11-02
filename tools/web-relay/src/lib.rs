//! The proxy that lets a browser play on a server that speaks only UDP: one game datagram per
//! WebSocket message, on the player's own machine or beside the server for many players.
//!
//! **Depends on** `dereth-transport` for the frame (`dereth_transport::web_frame`) and the login
//! request, `dereth-network-protocol` for the rule a world's WebSocket address keeps,
//! `tungstenite` for the WebSocket, `rustls` for its own TLS and `ureq` to announce itself to the
//! hub. **Used by** no crate: the web client's worker opens one WebSocket to it per session,
//! exactly as it opens one to a server that accepts WebSocket connections itself, so the client
//! has one path.
//!
//! It is for servers that do not accept WebSocket connections (ACE, GDLE, or an Empyrean with its
//! WebSocket endpoint off); an Empyrean with the endpoint on is reached directly. It runs one of
//! two ways ([`config`]):
//!
//! - **On the player's machine**, for that player: it listens on loopback only and admits pages
//!   served from that machine and the one origin the player names.
//! - **Beside the server, for many players** (`--public`): run by the world's operator, behind a
//!   TLS proxy or with its own certificate, it admits only the page origins named (the Dereth web
//!   client's and the operator's), counts sessions by client address and in total, limits what
//!   each sends and how long it may be silent, and may announce its `wss://` address to the hub,
//!   whose list of worlds then names it ([`announce`]).
//!
//! **Must never** send to any host but the one server it was started for, or to any port of it
//! but the logon port and the one above it, or admit a page it was not told to: it opens a hole
//! through the browser's sandbox, and those rules keep it the size of a game session.
//!
//! Each WebSocket gets its own UDP socket, so the server sees one client per browser session, as
//! it would see one per game client ([`session`]). [`relay`] admits connections and runs them.

pub mod announce;
pub mod client;
pub mod config;
pub mod limits;
pub mod log;
pub mod origin;
pub mod relay;
pub mod session;

#[cfg(test)]
mod tests;
