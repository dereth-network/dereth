// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/ChessDelayedAction.cs
//! Port of `Source/ACE.Server/Entity/Chess/ChessDelayedAction.cs`.

use empyrean_entity::enums::{ChessColor, ChessDelayedActionType};

use super::chess_piece_coord::ChessPieceCoord;

// ACE: ChessDelayedAction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChessDelayedAction {
    pub action: ChessDelayedActionType,
    /// C#'s default for an unset enum field is 0 (`White`).
    pub color: ChessColor,
    pub from: Option<ChessPieceCoord>,
    pub to: Option<ChessPieceCoord>,
    pub stalemate: bool,
}

impl ChessDelayedAction {
    // ACE: ChessDelayedAction.ChessDelayedAction
    /// `new ChessDelayedAction(action)`.
    #[must_use]
    pub fn new(action: ChessDelayedActionType) -> Self {
        Self {
            action,
            color: ChessColor(0),
            from: None,
            to: None,
            stalemate: false,
        }
    }

    /// `new ChessDelayedAction(action, color, stalemate = false)`.
    #[must_use]
    pub fn with_color(action: ChessDelayedActionType, color: ChessColor, stalemate: bool) -> Self {
        Self {
            action,
            color,
            from: None,
            to: None,
            stalemate,
        }
    }

    /// `new ChessDelayedAction(action, color, from, to, stalemate = false)`.
    #[must_use]
    pub fn with_move(
        action: ChessDelayedActionType,
        color: ChessColor,
        from: Option<ChessPieceCoord>,
        to: Option<ChessPieceCoord>,
        stalemate: bool,
    ) -> Self {
        Self {
            action,
            color,
            from,
            to,
            stalemate,
        }
    }
}
