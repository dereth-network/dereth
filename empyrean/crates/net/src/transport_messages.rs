//! The two game messages the transport itself sends: ACE's `GameMessageCharacterError` (login
//! rejects, handshake failures, shutdown) and `GameMessageBootAccount` (wrong client version).
//!
//! Their builders are empyrean-world's ports (`game_message_character_error`,
//! `game_message_boot_account`). empyrean-net cannot depend on empyrean-world, so whoever owns the
//! [`ServerNet`](crate::ServerNet) hands them in when it is built: two plain function pointers,
//! copied into every [`NetIo`](crate::session::NetIo). No builder code lives here.

use std::fmt;

use crate::enums::CharacterError;
use crate::OutboundMessage;

/// The builders of the game messages the transport sends on its own.
#[derive(Clone, Copy)]
pub struct TransportMessages {
    /// `new GameMessageCharacterError(error)`.
    pub character_error: fn(CharacterError) -> OutboundMessage,
    /// `new GameMessageBootAccount(reason)`.
    pub boot_account: fn(Option<&str>) -> OutboundMessage,
}

impl fmt::Debug for TransportMessages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TransportMessages")
    }
}
