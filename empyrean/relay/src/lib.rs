//! A relay that speaks the later protocol to Dereth and the November 1999 protocol to an ac1999
//! server, so Dereth's own tree carries one wire format. It translates message by message and keeps
//! the state the 1999 wire never carried.
//!
//! **Depends on** `empyrean-net` (the later protocol's server sessions) and `empyrean-common` (its
//! clock), and the shared `dereth-primitives`, `dereth-transport`, `dereth-protocol` and `dereth-dat`.
//! **Used by** nothing: `cargo xtask seams` keeps it that way for `core/` and `dereth/`.
//!
//! **Must never** listen beyond the loopback address, or forward an action the 1999 client could not
//! send.
//!
//! **Where it listens:** towards Dereth, on the loopback address only; towards the server, nowhere
//! (see [`serve`]). It sees the player's password, so the player runs it, on the player's machine.
//!
//! The 1999 transport and codecs are here because this is the only place they are used:
//! [`wire`] (packets and the data connection), [`codec`] (the application messages, normalized to
//! the later types) and [`handshake`] (the login). [`down`] and [`up`] translate each way, [`dats`]
//! holds and serves the server's dat push, [`bridge`] is one session and [`relay`] all of them,
//! with no socket; [`serve`] puts sockets under it.

pub mod bridge;
pub mod codec;
pub mod dats;
pub mod down;
pub mod handshake;
pub mod relay;
pub mod report;
pub mod serve;
pub mod up;
pub mod wire;

#[cfg(test)]
mod exchange_tests;
