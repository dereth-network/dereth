//! Which game a session carries: Asheron's Call or Asheron's Call 2.
//!
//! The two games share the transport (ports, handshake, fragments, reassembly and checksums), so
//! the transport cannot tell them apart; the application layer can. Every reassembled message
//! begins with a type dword:
//!
//! * an **AC1** message's type is a value of the master opcode table (every one below `0x10000`:
//!   game events and actions `0x00xx`–`0x02xx`, top-level messages `0xF6xx`–`0xF7xx`, wrapped ones
//!   under `0xF7B0`/`0xF7B1`);
//! * an **AC2** message's type is `[u16 type][u16 0x0001]`, a dword of the form `0x0001_xxxx`,
//!   which no AC1 type has.
//!
//! Each message is sorted by its first dword into one of those two forms or neither
//! ([`Form`]); a session's game is the form of a clear majority (more than two thirds) of its
//! messages ([`Evidence::verdict`]), and `unknown` otherwise, including a session with no
//! messages. No server address is consulted.

use dereth_protocol::Opcode;

/// A session's game.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Game {
    /// Asheron's Call: its messages go through the `dereth-protocol` decoder.
    Ac1,
    /// Asheron's Call 2: its messages are stored whole, with their type
    /// ([`crate::ac2`]).
    Ac2,
    /// No clear majority, or no messages. Decoded as AC1, the corpus's own game.
    #[default]
    Unknown,
}

impl Game {
    /// The spelling the index stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ac1 => "ac1",
            Self::Ac2 => "ac2",
            Self::Unknown => "unknown",
        }
    }

    /// The decoder a session of this game goes through: AC2's for AC2, AC1's otherwise.
    #[must_use]
    pub const fn decoder(self) -> Self {
        match self {
            Self::Ac2 => Self::Ac2,
            Self::Ac1 | Self::Unknown => Self::Ac1,
        }
    }
}

/// The AC1 ordered-event and ordered-action headers: not types of the master table, but the
/// dword AC1 game events and actions begin with.
const ORDERED_EVENT: u32 = 0xF7B0;
const ORDERED_ACTION: u32 = 0xF7B1;

/// What one message's first dword looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    /// A type of the AC1 master opcode table.
    Ac1,
    /// `0x0001_xxxx`: an AC2 type.
    Ac2,
    /// Shorter than a dword, or a value neither game uses as a type.
    Other,
}

/// The form of one reassembled message.
#[must_use]
pub fn form(blob: &[u8]) -> Form {
    let Some(b) = blob.get(..4) else {
        return Form::Other;
    };
    let first = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    if first >> 16 == 0x0001 {
        Form::Ac2
    } else if first == ORDERED_EVENT || first == ORDERED_ACTION || Opcode(first).info().is_some() {
        Form::Ac1
    } else {
        Form::Other
    }
}

/// A session's messages counted by [`Form`]: the evidence for its game, stored on the session
/// row (`game_ac1`, `game_ac2`, `game_other`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Evidence {
    pub ac1: u64,
    pub ac2: u64,
    pub other: u64,
}

impl Evidence {
    /// Count one message.
    pub fn add(&mut self, f: Form) {
        match f {
            Form::Ac1 => self.ac1 += 1,
            Form::Ac2 => self.ac2 += 1,
            Form::Other => self.other += 1,
        }
    }

    /// Messages counted.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.ac1 + self.ac2 + self.other
    }

    /// The game of more than two thirds of the messages; `unknown` when neither form has that
    /// majority or there are no messages.
    #[must_use]
    pub fn verdict(&self) -> Game {
        let n = self.total();
        if self.ac2 * 3 > n * 2 {
            Game::Ac2
        } else if self.ac1 * 3 > n * 2 {
            Game::Ac1
        } else {
            Game::Unknown
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(blobs: &[&[u8]]) -> Evidence {
        let mut e = Evidence::default();
        for b in blobs {
            e.add(form(b));
        }
        e
    }

    /// AC1 traffic: bare top-level messages, a wrapped event and a wrapped action, plus one
    /// retail stray whose first dword is an object id.
    #[test]
    fn an_ac1_session_is_ac1() {
        let e = session(&[
            &[0xE1, 0xF7, 0, 0, 1, 0, 0, 0],
            &[0xB0, 0xF7, 0, 0, 1, 0, 0, 0x50, 7, 0, 0, 0, 0x24, 0, 0, 0],
            &[0xB1, 0xF7, 0, 0, 3, 0, 0, 0, 0xC8, 0, 0, 0],
            &[0x48, 0xF7, 0, 0],
            &[0x94, 0xF1, 0x36, 0x80, 0, 0xD2, 1, 0, 0],
        ]);
        assert_eq!((e.ac1, e.ac2, e.other), (4, 0, 1));
        assert_eq!(e.verdict(), Game::Ac1);
    }

    /// AC2 traffic: every type dword is `[u16 type][u16 0x0001]`.
    #[test]
    fn an_ac2_session_is_ac2() {
        let e = session(&[
            &[0x86, 0x00, 0x01, 0x00, 0x21, 0x30, 0, 0],
            &[0x8E, 0x00, 0x01, 0x00],
            &[0xC9, 0x00, 0x01, 0x00, 0, 0, 0, 0],
            &[0xD3, 0x00, 0x01, 0x00],
            &[0x01],
        ]);
        assert_eq!((e.ac1, e.ac2, e.other), (0, 4, 1));
        assert_eq!(e.verdict(), Game::Ac2);
    }

    /// Half of each, or nothing at all: no clear majority, so no game is claimed.
    #[test]
    fn a_mixed_or_empty_session_is_unknown() {
        let mixed = session(&[
            &[0xE1, 0xF7, 0, 0],
            &[0x48, 0xF7, 0, 0],
            &[0x86, 0x00, 0x01, 0x00],
            &[0x8E, 0x00, 0x01, 0x00],
        ]);
        assert_eq!((mixed.ac1, mixed.ac2), (2, 2));
        assert_eq!(mixed.verdict(), Game::Unknown);
        // Two thirds exactly is not more than two thirds.
        let two_thirds = session(&[&[0x86, 0, 1, 0], &[0x8E, 0, 1, 0], &[0xE1, 0xF7, 0, 0]]);
        assert_eq!(two_thirds.verdict(), Game::Unknown);
        // Messages of neither form.
        let junk = session(&[&[0x78, 0x56, 0x34, 0x12], &[1, 2]]);
        assert_eq!(junk.verdict(), Game::Unknown);
        assert_eq!(Evidence::default().verdict(), Game::Unknown);
    }

    #[test]
    fn no_ac1_type_has_the_ac2_form() {
        for i in dereth_protocol::OPCODES {
            assert_ne!(i.opcode.0 >> 16, 1, "{:#X} {}", i.opcode.0, i.name);
        }
    }
}
