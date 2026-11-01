//! Every Asheron's Call 2 message both ways: typed, decoded with exact consumption and encoded back
//! to the same bytes.
//!
//! **Depends on** `auberean-primitives` (the string scramble, vectors) and the script machine
//! (`auberean-script`), whose codec reads script-call arguments. **Used by** the client's session
//! (`auberean-client-net`) and the server (`yalain-world`).
//!
//! A message is the payload of one reassembled transport blob: a type word (`0x0001_00xx`), then
//! its fields, little-endian, with strings and some fields padded to four bytes from the message
//! start ([`wire`]). [`ServerMessage`] and [`ClientMessage`] hold one variant per type;
//! [`ClientBlob`] adds the client's encrypted queues, which no server can read.
//!
//! - [`text`]: the scrambled strings and string-table references.
//! - [`table`]: keyed tables, with the order their senders lay them out in.
//! - [`types`]: the shared structures (looks, placement, behaviors, movement orders, qualities,
//!   character identities).
//! - [`login`], [`physics`], [`quality`], [`script`]: the message bodies by family.
//!
//! Script-call arguments stay bytes here; the script machine reads them through
//! [`script::ArgumentCodec`].
//!
//! **Must never** do I/O or keep session state: a message in, a value out, and back.

#![no_std]

extern crate alloc;

pub mod login;
pub mod message;
pub mod physics;
pub mod quality;
pub mod script;
pub mod table;
pub mod text;
pub mod types;
pub mod wire;

pub use message::{message_type, ClientBlob, ClientMessage, ServerMessage, SECURE_QUEUES};
pub use wire::DecodeError;

/// The queues messages travel on.
pub mod queue {
    /// Server events: the world, the character list.
    pub const EVENT: u16 = 1;
    /// Transport control.
    pub const CONTROL: u16 = 2;
    /// The client's game events and positions.
    pub const WEENIE: u16 = 3;
    /// Login requests and chat.
    pub const LOGON: u16 = 4;
    /// The data download.
    pub const DATABASE: u16 = 5;
}
