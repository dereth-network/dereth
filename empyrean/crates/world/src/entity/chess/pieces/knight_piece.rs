// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/Pieces/KnightPiece.cs
//! Port of `Source/ACE.Server/Entity/Chess/Pieces/KnightPiece.cs`.

use empyrean_entity::enums::{ChessColor, ChessPieceType};

use super::base_piece::BasePiece;
use crate::entity::chess::chess_piece_coord::ChessPieceCoord;

// ACE: KnightPiece.KnightPiece
#[must_use]
pub fn new(color: ChessColor, to: ChessPieceCoord) -> BasePiece {
    BasePiece::new(ChessPieceType::Knight, color, to)
}

// ACE: KnightPiece.CanMove
#[must_use]
pub fn can_move(_piece: &BasePiece, dx: i32, dy: i32) -> bool {
    let adx = dx.abs();
    let ady = dy.abs();

    adx == 1 && ady == 2 || adx == 2 && ady == 1
}
