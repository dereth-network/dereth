// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/Pieces/BishopPiece.cs
//! Port of `Source/ACE.Server/Entity/Chess/Pieces/BishopPiece.cs`.

use empyrean_entity::enums::{ChessColor, ChessPieceType};

use super::base_piece::BasePiece;
use crate::entity::chess::chess_piece_coord::ChessPieceCoord;

// ACE: BishopPiece.BishopPiece
#[must_use]
pub fn new(color: ChessColor, to: ChessPieceCoord) -> BasePiece {
    BasePiece::new(ChessPieceType::Bishop, color, to)
}

// ACE: BishopPiece.CanMove
#[must_use]
pub fn can_move(_piece: &BasePiece, dx: i32, dy: i32) -> bool {
    dx.abs() == dy.abs()
}
