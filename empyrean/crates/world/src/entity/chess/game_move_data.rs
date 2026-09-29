// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/GameMoveData.cs
//! Port of `Source/ACE.Server/Entity/Chess/GameMoveData.cs`.

use empyrean_entity::enums::{ChessColor, ChessMoveType};

use super::chess_piece_coord::ChessPieceCoord;

// ACE: GameMoveData
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GameMoveData {
    pub move_type: ChessMoveType,
    pub color: ChessColor,
    pub from: Option<ChessPieceCoord>,
    pub to: Option<ChessPieceCoord>,
}

impl GameMoveData {
    // ACE: GameMoveData.GameMoveData
    #[must_use]
    pub fn new(
        move_type: ChessMoveType,
        color: ChessColor,
        from: Option<ChessPieceCoord>,
        to: Option<ChessPieceCoord>,
    ) -> Self {
        Self {
            move_type,
            color,
            from,
            to,
        }
    }
}
