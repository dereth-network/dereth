//! The Dereth Network's world-to-hub protocol: addresses, settings, the text rules, stand-in ids,
//! rate limits and the link messages, with no I/O.
//!
//! **Depends on** no workspace crate. **Used by** the hub (`dereth-network-hub`), the conformance
//! suite (`dereth-network-conformance`) and Empyrean's network module (`empyrean-world`,
//! `empyrean-server`).
//!
//! **Must never** read a clock, open a socket or touch a file: every function takes the time it
//! needs and returns what to send, so the same code runs in a hub, a world, a client in a browser
//! and a test on virtual time. It builds for `wasm32-unknown-unknown`.
//!
//! **Specified in** `docs/network/`. The modules follow the specification's pages:
//! [`address`] (`Name@world`), [`settings`] (a character's Visible, Reachable and Listed, and a
//! world's federation flag), [`text`] (what every receiving world does to a remote line),
//! [`standin`] (the ids remote correspondents are shown under), [`limits`] (rate limits),
//! [`person`] (the person layer's words), [`wire`] (the link messages) and [`challenge`] (the
//! datagram the hub sends a world's game port when its operator verifies it in game).

pub mod address;
pub mod challenge;
pub mod limits;
pub mod person;
pub mod settings;
pub mod standin;
pub mod text;
pub mod wire;

pub use address::Address;
pub use person::{Alts, PersonFriend, PersonOp, PersonOutcome};
pub use settings::{CharacterSettings, Federation, Listed, Reachable, Visible};
pub use wire::{ChannelOp, ChannelOutcome, Message, Outcome, WorldInfo};

/// How long after a character tells someone that someone may answer it, whatever the character's
/// Reachable setting says, in seconds: a conversation a character starts is never one-sided.
pub const REPLY_WINDOW_SECS: f64 = 30.0 * 60.0;

/// How long a world has to answer a tell the hub hands it, in seconds; past that the hub answers
/// the sender's world `unavailable` itself.
pub const ANSWER_SECS: f64 = 30.0;
