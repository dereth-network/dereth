// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/Pieces/BasePiece.cs
//! Port of `Source/ACE.Server/Entity/Chess/Pieces/BasePiece.cs`.
//!
//! ACE's piece classes (`PawnPiece`, `RookPiece`, ...) derive from `BasePiece` and override
//! `CanMove`/`CanAttack`. Here one struct carries the runtime class in [`BasePiece::class`], and the
//! two virtuals match on it. The class is kept apart from `Type` because ACE can change a piece's
//! `Type` without changing its class (`ChessMatch.UpgradeWeeniePiece`).

use empyrean_entity::enums::{ChessColor, ChessPieceType};
use empyrean_entity::ObjectGuid;

use super::{bishop_piece, king_piece, knight_piece, pawn_piece, queen_piece, rook_piece};
use crate::entity::chess::chess_piece_coord::ChessPieceCoord;

// ACE: BasePiece
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasePiece {
    pub guid: ObjectGuid,

    pub r#type: ChessPieceType,
    pub color: ChessColor,
    pub coord: ChessPieceCoord,

    /// The C# class this piece was constructed as (`piece is PawnPiece`, ...).
    pub class: ChessPieceType,
}

impl BasePiece {
    // ACE: BasePiece.BasePiece
    /// `base(type, color, to)`: the class is the type it is built with.
    #[must_use]
    pub fn new(piece_type: ChessPieceType, color: ChessColor, to: ChessPieceCoord) -> Self {
        Self {
            guid: ObjectGuid::default(),
            r#type: piece_type,
            color,
            coord: to,
            class: piece_type,
        }
    }

    // ACE: BasePiece.CanAttack
    /// `CanAttack(ChessPieceCoord target)`.
    #[must_use]
    pub fn can_attack_coord(&self, target: &ChessPieceCoord) -> bool {
        let dx = target.x - self.coord.x;
        let dy = target.y - self.coord.y;

        self.can_attack(dx, dy)
    }

    /// `virtual CanAttack(int dx, int dy)`: `CanMove(dx, dy)` unless the class overrides it.
    #[must_use]
    pub fn can_attack(&self, dx: i32, dy: i32) -> bool {
        match self.class {
            ChessPieceType::Pawn => pawn_piece::can_attack(self, dx, dy),
            _ => self.can_move(dx, dy),
        }
    }

    // ACE: BasePiece.CanMove
    /// `virtual CanMove(int dx, int dy)`: false for the base class.
    #[must_use]
    pub fn can_move(&self, dx: i32, dy: i32) -> bool {
        match self.class {
            ChessPieceType::Pawn => pawn_piece::can_move(self, dx, dy),
            ChessPieceType::Rook => rook_piece::can_move(self, dx, dy),
            ChessPieceType::Knight => knight_piece::can_move(self, dx, dy),
            ChessPieceType::Bishop => bishop_piece::can_move(self, dx, dy),
            ChessPieceType::Queen => queen_piece::can_move(self, dx, dy),
            ChessPieceType::King => king_piece::can_move(self, dx, dy),
            _ => false,
        }
    }
}
