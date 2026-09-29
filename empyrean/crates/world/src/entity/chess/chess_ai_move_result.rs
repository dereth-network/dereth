// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/ChessAiMoveResult.cs
//! Port of `Source/ACE.Server/Entity/Chess/ChessAiMoveResult.cs`.

use empyrean_entity::enums::ChessMoveResult;

use super::chess_piece_coord::ChessPieceCoord;

// ACE: ChessAiMoveResult
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChessAiMoveResult {
    pub result: ChessMoveResult,
    pub from: Option<ChessPieceCoord>,
    pub to: Option<ChessPieceCoord>,

    /// time taken in milliseconds to calculate ai move
    pub profiling_time: u32,
    /// minimax recursion count
    pub profiling_counter: u32,
}

impl ChessAiMoveResult {
    // ACE: ChessAiMoveResult.ChessAiMoveResult
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: ChessAiMoveResult.SetResult
    pub fn set_result(
        &mut self,
        result: ChessMoveResult,
        from: Option<ChessPieceCoord>,
        to: Option<ChessPieceCoord>,
    ) {
        self.result = result;
        self.from = from;
        self.to = to;
    }
}
