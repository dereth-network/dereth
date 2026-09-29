// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/Pieces/RookPiece.cs
//! Port of `Source/ACE.Server/Entity/Chess/Pieces/RookPiece.cs`.

use empyrean_entity::enums::{ChessColor, ChessPieceType};

use super::base_piece::BasePiece;
use crate::entity::chess::chess_piece_coord::ChessPieceCoord;

// ACE: RookPiece.RookPiece
#[must_use]
pub fn new(color: ChessColor, to: ChessPieceCoord) -> BasePiece {
    BasePiece::new(ChessPieceType::Rook, color, to)
}

// ACE: RookPiece.CanMove
#[must_use]
pub fn can_move(_piece: &BasePiece, dx: i32, dy: i32) -> bool {
    (dx != 0) ^ (dy != 0)
}
