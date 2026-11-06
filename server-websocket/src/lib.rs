//! A game server's WebSocket endpoint: its settings, and with the endpoint feature the listener
//! that carries game datagrams to and from browser clients.
//!
//! **Depends on** `dereth-transport` for the frame (with the `endpoint` feature), with `serde`
//! for the settings and `tungstenite` and `rustls` for the listener. **Used by** the game servers:
//! `empyrean-common` and `empyrean-server`, and `yalain-net` and `yalain`.
//!
//! **Must never** know one server's sessions or ports: a server says how many ports a connection
//! numbers and takes each datagram by that port's index.
//!
//! [`settings`] is the `[server.websocket]` section, its defaults and its refusals, read alike by
//! every server so that the two cannot drift. `endpoint` is the listener.

#[cfg(feature = "endpoint")]
pub mod endpoint;
pub mod settings;
