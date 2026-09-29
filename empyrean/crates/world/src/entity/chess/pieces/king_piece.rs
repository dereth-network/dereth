// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/Pieces/KingPiece.cs
//! Port of `Source/ACE.Server/Entity/Chess/Pieces/KingPiece.cs`.

use empyrean_entity::enums::{ChessColor, ChessPieceType};

use super::base_piece::BasePiece;
use crate::entity::chess::chess_piece_coord::ChessPieceCoord;

// ACE: KingPiece.KingPiece
#[must_use]
pub fn new(color: ChessColor, to: ChessPieceCoord) -> BasePiece {
    BasePiece::new(ChessPieceType::King, color, to)
}

// ACE: KingPiece.CanMove
#[must_use]
pub fn can_move(_piece: &BasePiece, dx: i32, dy: i32) -> bool {
    dx.abs() < 2 && dy.abs() < 2
}
