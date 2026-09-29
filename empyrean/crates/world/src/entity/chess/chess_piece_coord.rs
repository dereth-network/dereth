// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/ChessPieceCoord.cs
//! Port of `Source/ACE.Server/Entity/Chess/ChessPieceCoord.cs`.
//!
//! ACE's `ChessPieceCoord` is a class; every holder that can be `null` is an
//! `Option<ChessPieceCoord>` here. ACE never shares one instance between two holders that then
//! mutate it (every mutation happens on a fresh copy), so a `Copy` value keeps its behaviour.

use std::fmt;

use empyrean_common::dotnet::binary_reader::{BinaryReader, ReadError};

use super::chess::BOARD_SIZE;
use crate::network::game_messages::game_message::BinaryWriter;

// ACE: ChessPieceCoord
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChessPieceCoord {
    pub x: i32,
    pub y: i32,
}

impl Default for ChessPieceCoord {
    fn default() -> Self {
        Self::new()
    }
}

impl ChessPieceCoord {
    // ACE: ChessPieceCoord.Offset
    #[must_use]
    pub const fn offset(&self) -> i32 {
        self.x + self.y * BOARD_SIZE
    }

    // ACE: ChessPieceCoord.Rank
    #[must_use]
    pub const fn rank(&self) -> i32 {
        self.y + 1
    }

    // ACE: ChessPieceCoord.ChessPieceCoord
    /// `new ChessPieceCoord()`: (-1, -1).
    #[must_use]
    pub const fn new() -> Self {
        Self { x: -1, y: -1 }
    }

    /// `new ChessPieceCoord(int x, int y)`.
    #[must_use]
    pub const fn new_xy(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    /// `new ChessPieceCoord(int offset)`.
    #[must_use]
    pub const fn from_offset(offset: i32) -> Self {
        Self {
            x: offset % BOARD_SIZE,
            y: offset / BOARD_SIZE,
        }
    }

    /// `new ChessPieceCoord(BinaryReader reader)`: X then Y.
    ///
    /// # Errors
    /// A short read (ACE's `EndOfStreamException`).
    pub fn read(reader: &mut BinaryReader<'_>) -> Result<Self, ReadError> {
        let x = reader.read_i32()?;
        let y = reader.read_i32()?;
        Ok(Self { x, y })
    }

    // ACE: ChessPieceCoord.Equals
    /// `Equals(ChessPieceCoord coord)`: false for a null `coord`.
    #[must_use]
    pub fn equals(&self, coord: Option<&ChessPieceCoord>) -> bool {
        coord.is_some_and(|c| self.x == c.x && self.y == c.y)
    }

    // ACE: ChessPieceCoord.IsValid
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        if self.x < 0 || self.x >= BOARD_SIZE {
            return false;
        }
        if self.y < 0 || self.y >= BOARD_SIZE {
            return false;
        }
        true
    }

    // ACE: ChessPieceCoord.Move
    pub fn r#move(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    // ACE: ChessPieceCoord.MoveOffset
    /// `MoveOffset(int x, int y)`; the `Vector2` overload casts each component with `(int)`,
    /// which the whole-number offsets in [`super::chess`] already are.
    pub fn move_offset(&mut self, x: i32, y: i32) {
        self.x += x;
        self.y += y;
    }

    /// `MoveOffset(Vector2 offset)`.
    pub fn move_offset_vec(&mut self, offset: (i32, i32)) {
        self.move_offset(offset.0, offset.1);
    }
}

// ACE: ChessPieceCoord.ToString
/// `$"{(char)('A' + X)}{Y + 1}"`.
impl fmt::Display for ChessPieceCoord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // (char)('A' + X) is a UTF-16 code unit; off the board it is still one char
        let c = u32::try_from(i32::from(b'A') + self.x)
            .ok()
            .and_then(char::from_u32)
            .unwrap_or('\u{FFFD}');
        write!(f, "{c}{}", self.y + 1)
    }
}

// ACE: ChessPieceCoordExtensions.ReadChessPieceCoord
/// `reader.ReadChessPieceCoord()`.
///
/// # Errors
/// A short read.
pub fn read_chess_piece_coord(reader: &mut BinaryReader<'_>) -> Result<ChessPieceCoord, ReadError> {
    ChessPieceCoord::read(reader)
}

// ACE: ChessPieceCoordExtensions.Write
/// `writer.Write(ChessPieceCoord coord)`: X then Y; a null coord throws in ACE.
pub fn write(writer: &mut Vec<u8>, coord: Option<&ChessPieceCoord>) {
    let coord = coord.expect("ACE: ChessPieceCoord is null (NullReferenceException)");
    writer.write_i32(coord.x);
    writer.write_i32(coord.y);
}
