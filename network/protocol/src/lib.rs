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
//! [`address`] (`Name@world`), [`settings`] (whether a character is on the network, and a world's
//! federation flag), [`text`] (what every receiving world does to a remote line),
//! [`standin`] (the ids remote correspondents are shown under), [`limits`] (rate limits),
//! [`person`] (the person layer's words), [`wire`] (the link messages), [`directory`] (a listed
//! character's profile and the directory's own link messages), [`reach`] (the question the hub asks
//! a world before it carries a tell to an offline character's person elsewhere), [`say`] (the line
//! the hub asks a world to say as one of its characters, said by its person elsewhere), [`rooms`]
//! (which of a world's own rooms carries the Global room, and which the hub mirrors into Discord),
//! [`web`] (where a browser reaches a world) and [`challenge`] (the datagram the hub sends a
//! world's game port when its operator verifies it in game).

pub mod address;
pub mod challenge;
pub mod directory;
pub mod limits;
pub mod person;
pub mod reach;
pub mod rooms;
pub mod say;
pub mod settings;
pub mod standin;
pub mod text;
pub mod web;
pub mod wire;

pub use address::Address;
pub use directory::{DirectoryMessage, ListingOutcome, OffBy, Profile};
pub use person::{PersonFriend, PersonOp, PersonOutcome};
pub use reach::ReachMessage;
pub use rooms::RoomsMessage;
pub use say::{SayMessage, SayOutcome};
pub use settings::{CharacterSettings, Federation};
pub use wire::{ChannelOp, ChannelOutcome, Message, Outcome, WorldInfo};

/// How long a world has to answer a tell the hub hands it, in seconds; past that the hub answers
/// the sender's world `unavailable` itself.
pub const ANSWER_SECS: f64 = 30.0;
