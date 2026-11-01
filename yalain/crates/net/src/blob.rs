//! The 64-bit id every message travels under: how the client may order it, which server sent
//! it, its ordering stamp and its sequence number.
//!
//! From the top: three flag bits (`0x20` out of the world, `0x40` cell, `0x80` ephemeral) and the
//! ordering kind (`1` private, `2` visual, `3` unordered) share the top byte; the next byte is the
//! sending server's id; then a 16-bit ordering stamp (counted per queue and kind) and a 32-bit
//! sequence number (counted per connection). The January 2017 login server sent its character list
//! messages as `0x23`, out of the world and unordered.
//!
//! How the client takes each kind: a **private** message waits until the private message stamped
//! before it has been handled, so private stamps run 1, 2, 3... on a connection; a **visual** one is
//! ordered by the visual-ordering stamp inside the message, against its object; an **unordered**
//! one is handled as it arrives. An **ephemeral** message belongs to a stream (its low word is the
//! stream's number, not a sequence number) and supersedes the older ones of its stream: the world
//! servers sent positions this way, one stream per object.

/// The top byte of a blob id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlobKind(pub u8);

impl BlobKind {
    /// Out of the world, unordered: the login connection's messages.
    pub const LOGIN: Self = Self(0x23);
    /// Unordered: leaving the world, objects appearing.
    pub const UNORDERED: Self = Self(0x03);
    /// Ordered among the player's own messages.
    pub const PRIVATE: Self = Self(0x01);
    /// Ordered among what others see of an object.
    pub const VISUAL: Self = Self(0x02);
    /// Superseded by a newer one of its stream (positions).
    pub const EPHEMERAL: Self = Self(0x83);
}

/// A blob id from its parts.
#[must_use]
pub const fn blob_id(kind: BlobKind, server_id: u8, stamp: u16, sequence: u32) -> u64 {
    (kind.0 as u64) << 56 | (server_id as u64) << 48 | (stamp as u64) << 32 | sequence as u64
}
