// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/Pieces/PawnPiece.cs
//! Port of `Source/ACE.Server/Entity/Chess/Pieces/PawnPiece.cs`.

use empyrean_entity::enums::{ChessColor, ChessPieceType};

use super::base_piece::BasePiece;
use crate::entity::chess::chess_piece_coord::ChessPieceCoord;

// ACE: PawnPiece.PawnPiece
#[must_use]
pub fn new(color: ChessColor, to: ChessPieceCoord) -> BasePiece {
    BasePiece::new(ChessPieceType::Pawn, color, to)
}

// ACE: PawnPiece.CanMove
// ACE-BUG: the start rank (2 or 7, a 1-based rank) is compared with the 0-based `Coord.Y`, so a pawn
// on its start square counts as moved. Nothing reaches it: `CanMove` is only called through the
// default `CanAttack`, which PawnPiece overrides.
#[must_use]
pub fn can_move(piece: &BasePiece, dx: i32, dy: i32) -> bool {
    let start_rank = if piece.color == ChessColor::White {
        2
    } else {
        7
    };
    let has_moved = start_rank != piece.coord.y;
    let ady = dy.abs();
    dx == 0 && (ady == 1 || ady == 2 && !has_moved)
}

// ACE: PawnPiece.CanAttack
#[must_use]
pub fn can_attack(piece: &BasePiece, dx: i32, dy: i32) -> bool {
    let y = if piece.color == ChessColor::White {
        1
    } else {
        -1
    };
    dx.abs() == 1 && dy == y
}
