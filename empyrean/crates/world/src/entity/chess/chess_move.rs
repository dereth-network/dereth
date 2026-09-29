// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/ChessMove.cs
//! Port of `Source/ACE.Server/Entity/Chess/ChessMove.cs`.

use empyrean_entity::enums::{ChessColor, ChessMoveFlag, ChessPieceType};
use empyrean_entity::ObjectGuid;

use super::chess_piece_coord::ChessPieceCoord;

/// ACE's `List<ChessMoveFlag> Castling`, one entry per colour.
///
/// Not ACE's (a fix, V311): a value, so each move keeps the castling rights
/// from before it and undoing the move puts them back. ACE's moves held the logic's own list, so an
/// undone move restored nothing and rights cleared while the AI searched stayed cleared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CastlingList([ChessMoveFlag; 2]);

impl CastlingList {
    /// `new List<ChessMoveFlag> { a, b }`.
    #[must_use]
    pub fn new(white: ChessMoveFlag, black: ChessMoveFlag) -> Self {
        Self([white, black])
    }

    /// `Castling[index]`.
    ///
    /// # Panics
    /// Outside 0..2 (C#'s `ArgumentOutOfRangeException`).
    #[must_use]
    pub fn get(&self, index: i32) -> ChessMoveFlag {
        self.0[Self::slot(index)]
    }

    /// `Castling[index] = value`.
    pub fn set(&mut self, index: i32, value: ChessMoveFlag) {
        self.0[Self::slot(index)] = value;
    }

    fn slot(index: i32) -> usize {
        usize::try_from(index)
            .ok()
            .filter(|&i| i < 2)
            .expect("ACE: Castling[index] out of range (ArgumentOutOfRangeException)")
    }
}

// ACE: ChessMove
/// Holds all of the information that can change during a half turn.
#[derive(Debug, Clone)]
pub struct ChessMove {
    pub flags: ChessMoveFlag,
    pub color: ChessColor,
    pub r#type: ChessPieceType,
    pub from: ChessPieceCoord,
    pub to: ChessPieceCoord,
    pub promotion: ChessPieceType,
    pub captured: ChessPieceType,
    pub r#move: u32,
    pub half_move: u32,
    pub castling: CastlingList,
    pub en_passant_coord: Option<ChessPieceCoord>,
    pub guid: ObjectGuid,
    pub captured_guid: ObjectGuid,
}

impl ChessMove {
    // ACE: ChessMove.ChessMove
    /// `From` and `To` are copied (`new ChessPieceCoord(from)`), and so are the castling rights.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        flags: ChessMoveFlag,
        color: ChessColor,
        piece_type: ChessPieceType,
        from: ChessPieceCoord,
        to: ChessPieceCoord,
        promotion: ChessPieceType,
        captured: ChessPieceType,
        r#move: u32,
        half_move: u32,
        castling: CastlingList,
        en_passant_coord: Option<ChessPieceCoord>,
        guid: ObjectGuid,
        captured_guid: ObjectGuid,
    ) -> Self {
        Self {
            flags,
            color,
            r#type: piece_type,
            from,
            to,
            promotion,
            captured,
            r#move,
            half_move,
            castling,
            en_passant_coord,
            guid,
            captured_guid,
        }
    }
}
