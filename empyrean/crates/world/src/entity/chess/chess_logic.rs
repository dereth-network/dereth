// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/ChessLogic.cs
//! Port of `Source/ACE.Server/Entity/Chess/ChessLogic.cs` ("ported from Anahera's code" in ACE):
//! the board, move generation, check and checkmate, and the two AI searches.
//!
//! Not ACE's (retail, V311): the rules are the client's. ACE's board, move list,
//! move log and AI stay, but every rules decision is the client's own (`dereth_rules::chess`), asked
//! of a copy of this board ([`ChessLogic::client_board`]): whether a move is legal and, if not, the
//! client's refusal; check, checkmate (a real legal-move search), castling (through an attacked
//! square, after the king or that rook has moved), en passant and promotion (whose check is the
//! client's: once with the pawn on the last rank, once with the queen). The AI only plays moves the
//! client's rules allow and scores positions for the side it plays; undoing a move puts back the
//! side to move, the castling rights and a promoted pawn, so the AI's searches leave the board as
//! they found it. ACE instead tested checkmate against pseudo-legal moves (a side in check with any
//! move was never mated), let a move leave its own king in check (and so a king be captured), let
//! the king castle through an attacked empty square, and its searches corrupted the board.

use dereth_rules::chess as rules;
use empyrean_entity::enums::{ChessColor, ChessMoveFlag, ChessMoveResult, ChessPieceType};
use empyrean_entity::ObjectGuid;

use super::chess::{self, BOARD_SIZE};
use super::chess_ai_async_turn_key::ChessAiAsyncTurnKey;
use super::chess_move::{CastlingList, ChessMove};
use super::chess_piece_coord::ChessPieceCoord;
use super::pieces::base_piece::BasePiece;
use super::pieces::{bishop_piece, king_piece, knight_piece, pawn_piece, queen_piece, rook_piece};

/// `result.HasFlag(flag)` on the (non-`[Flags]`) `ChessMoveResult`.
#[must_use]
pub fn has_result_flag(result: ChessMoveResult, flag: ChessMoveResult) -> bool {
    result.0 & flag.0 == flag.0
}

const SQUARES: usize = 64;

/// A square as the client's rules name it (the same file and rank numbers).
#[must_use]
pub fn client_coord(c: ChessPieceCoord) -> rules::Coord {
    rules::Coord { x: c.x, y: c.y }
}

/// `Board[offset]` as an array index; C# throws `IndexOutOfRangeException` outside the board.
fn index(offset: i64) -> usize {
    usize::try_from(offset)
        .ok()
        .filter(|&i| i < SQUARES)
        .expect("ACE: ChessLogic.Board index out of range (IndexOutOfRangeException)")
}

/// `(int)color` as an index into the colour tables.
fn color_index(color: ChessColor) -> usize {
    usize::try_from(color.0).expect("ACE: negative ChessColor index (IndexOutOfRangeException)")
}

/// `(int)type` as an index into the piece tables; C# throws `IndexOutOfRangeException` outside them.
fn type_index(piece_type: ChessPieceType) -> usize {
    usize::try_from(piece_type.0)
        .ok()
        .filter(|&i| i < 7)
        .expect("ACE: ChessPieceType index out of range (IndexOutOfRangeException)")
}

// ACE: ChessLogic
#[derive(Debug, Clone)]
pub struct ChessLogic {
    pub turn: ChessColor,

    pub r#move: u32,
    pub half_move: u32,

    pub castling: CastlingList,

    pub en_passant_coord: Option<ChessPieceCoord>,

    pub board: Vec<Option<BasePiece>>,

    /// `Stack<ChessMove> History`: the top is the last element.
    pub history: Vec<ChessMove>,
}

impl Default for ChessLogic {
    fn default() -> Self {
        Self::new()
    }
}

impl ChessLogic {
    // ACE: ChessLogic.ChessLogic
    #[must_use]
    pub fn new() -> Self {
        let both = ChessMoveFlag::KingSideCastle | ChessMoveFlag::QueenSideCastle;
        let mut logic = Self {
            turn: ChessColor::White,
            r#move: 0,
            half_move: 0,
            castling: CastlingList::new(both, both),
            en_passant_coord: None,
            board: vec![None; SQUARES],
            history: Vec::new(),
        };

        // setup white
        logic.add_piece(ChessColor::White, ChessPieceType::Rook, 0, 0);
        logic.add_piece(ChessColor::White, ChessPieceType::Knight, 1, 0);
        logic.add_piece(ChessColor::White, ChessPieceType::Bishop, 2, 0);
        logic.add_piece(ChessColor::White, ChessPieceType::Queen, 3, 0);
        logic.add_piece(ChessColor::White, ChessPieceType::King, 4, 0);
        logic.add_piece(ChessColor::White, ChessPieceType::Bishop, 5, 0);
        logic.add_piece(ChessColor::White, ChessPieceType::Knight, 6, 0);
        logic.add_piece(ChessColor::White, ChessPieceType::Rook, 7, 0);

        for i in 0..8u32 {
            logic.add_piece(ChessColor::White, ChessPieceType::Pawn, i, 1);
        }

        // setup black
        logic.add_piece(ChessColor::Black, ChessPieceType::Rook, 0, 7);
        logic.add_piece(ChessColor::Black, ChessPieceType::Knight, 1, 7);
        logic.add_piece(ChessColor::Black, ChessPieceType::Bishop, 2, 7);
        logic.add_piece(ChessColor::Black, ChessPieceType::Queen, 3, 7);
        logic.add_piece(ChessColor::Black, ChessPieceType::King, 4, 7);
        logic.add_piece(ChessColor::Black, ChessPieceType::Bishop, 5, 7);
        logic.add_piece(ChessColor::Black, ChessPieceType::Knight, 6, 7);
        logic.add_piece(ChessColor::Black, ChessPieceType::Rook, 7, 7);

        for i in 0..8u32 {
            logic.add_piece(ChessColor::Black, ChessPieceType::Pawn, i, 6);
        }

        logic
    }

    // ACE: ChessLogic.AddPiece
    /// `AddPiece(color, type, uint x, uint y)`: a type without a piece class stores `null`
    /// (`Debug.Assert(false)` is compiled out of ACE's release build) and returns `None`.
    pub fn add_piece(
        &mut self,
        color: ChessColor,
        piece_type: ChessPieceType,
        x: u32,
        y: u32,
    ) -> Option<&mut BasePiece> {
        #[allow(clippy::cast_possible_wrap)]
        let to = ChessPieceCoord::new_xy(x as i32, y as i32);
        // Debug.Assert(to.IsValid()): compiled out of release builds

        let piece = match piece_type {
            ChessPieceType::Pawn => Some(pawn_piece::new(color, to)),
            ChessPieceType::Rook => Some(rook_piece::new(color, to)),
            ChessPieceType::Knight => Some(knight_piece::new(color, to)),
            ChessPieceType::Bishop => Some(bishop_piece::new(color, to)),
            ChessPieceType::Queen => Some(queen_piece::new(color, to)),
            ChessPieceType::King => Some(king_piece::new(color, to)),
            _ => None, // Debug.Assert(false)
        };

        // uint arithmetic: x + y * BoardSize
        let i = index(i64::from(x) + i64::from(y) * i64::from(BOARD_SIZE));
        self.board[i] = piece;
        self.board[i].as_mut()
    }

    /// `AddPiece(color, type, ChessPieceCoord to)`: `(uint)to.X`, `(uint)to.Y`.
    #[allow(clippy::cast_sign_loss)]
    pub fn add_piece_at(
        &mut self,
        color: ChessColor,
        piece_type: ChessPieceType,
        to: ChessPieceCoord,
    ) -> Option<&mut BasePiece> {
        self.add_piece(color, piece_type, to.x as u32, to.y as u32)
    }

    // ACE: ChessLogic.GetPiece
    /// `GetPiece(ChessPieceCoord from)`: `null` off the board.
    #[must_use]
    pub fn get_piece(&self, from: &ChessPieceCoord) -> Option<&BasePiece> {
        if !from.is_valid() {
            return None;
        }

        self.board[index(i64::from(from.x) + i64::from(from.y) * i64::from(BOARD_SIZE))].as_ref()
    }

    /// `GetPiece(ChessPieceCoord from)`, for writing.
    pub fn get_piece_mut(&mut self, from: &ChessPieceCoord) -> Option<&mut BasePiece> {
        if !from.is_valid() {
            return None;
        }

        self.board[index(i64::from(from.x) + i64::from(from.y) * i64::from(BOARD_SIZE))].as_mut()
    }

    /// `GetPiece(ChessColor color, ChessPieceType type)`: the first in board order.
    #[must_use]
    pub fn get_piece_by_type(
        &self,
        color: ChessColor,
        piece_type: ChessPieceType,
    ) -> Option<&BasePiece> {
        self.board
            .iter()
            .flatten()
            .find(|i| i.color == color && i.r#type == piece_type)
    }

    /// `GetPiece(ObjectGuid guid)`: the first in board order.
    #[must_use]
    pub fn get_piece_by_guid(&self, guid: ObjectGuid) -> Option<&BasePiece> {
        self.board.iter().flatten().find(|i| i.guid == guid)
    }

    /// `GetPiece(ObjectGuid guid)`, for writing.
    pub fn get_piece_by_guid_mut(&mut self, guid: ObjectGuid) -> Option<&mut BasePiece> {
        self.board.iter_mut().flatten().find(|i| i.guid == guid)
    }

    // ACE: ChessLogic.RemovePiece
    /// `RemovePiece(ChessPieceCoord target)`.
    pub fn remove_piece_at(&mut self, target: &ChessPieceCoord) {
        let coord = self.get_piece(target).map(|p| p.coord);
        if let Some(coord) = coord {
            self.remove_piece_coord_of(coord);
        }
    }

    /// `RemovePiece(BasePiece piece)`: clears the square the piece's own `Coord` names.
    pub fn remove_piece(&mut self, piece: &BasePiece) {
        self.remove_piece_coord_of(piece.coord);
    }

    fn remove_piece_coord_of(&mut self, coord: ChessPieceCoord) {
        self.board[index(i64::from(coord.offset()))] = None;
    }

    // ACE: ChessLogic.MovePiece
    pub fn move_piece(&mut self, from: &ChessPieceCoord, to: &ChessPieceCoord) {
        let from_piece = self
            .get_piece_mut(from)
            .expect("ACE: ChessLogic.MovePiece: fromPiece is null (NullReferenceException)");
        from_piece.coord = *to;

        self.remove_piece_at(to);

        let from_index = index(i64::from(from.offset()));
        let piece = self.board[from_index].clone();
        self.board[index(i64::from(to.offset()))] = piece;
        self.board[from_index] = None;
    }

    // ACE: ChessLogic.WalkPieces
    /// Every piece in board order (rank by rank, file by file).
    pub fn walk_pieces(&self, mut callback: impl FnMut(&BasePiece)) {
        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                if let Some(piece) = &self.board[index(i64::from(x + y * BOARD_SIZE))] {
                    callback(piece);
                }
            }
        }
    }

    /// The coordinates [`Self::walk_pieces`] visits, for callers whose callback reaches the world
    /// (it only ever changes a visited piece's guid, never the board layout).
    #[must_use]
    pub fn walk_piece_coords(&self) -> Vec<ChessPieceCoord> {
        let mut coords = Vec::new();
        self.walk_pieces(|p| coords.push(p.coord));
        coords
    }

    // ACE: ChessLogic.DoMove
    pub fn do_move(
        &mut self,
        color: ChessColor,
        from: &ChessPieceCoord,
        to: &ChessPieceCoord,
    ) -> ChessMoveResult {
        if !from.is_valid() {
            return ChessMoveResult::BadMoveDestination;
        }
        if !to.is_valid() {
            return ChessMoveResult::BadMoveDestination;
        }

        if self.turn != color {
            return ChessMoveResult::BadMoveNotYourTurn;
        }

        let Some(from_piece) = self.get_piece(from).cloned() else {
            return ChessMoveResult::BadMoveNoPiece;
        };

        if from_piece.color != color {
            return ChessMoveResult::BadMoveNotYours;
        }

        // Not ACE's (retail, V311): the client's rules decide, and a refused move
        // is answered with the client's own refusal (a move into check, castling through check,
        // a blocked path, ...). ACE accepted any pseudo-legal move and refused the rest with
        // BadMoveInvalidCommand.
        let verdict = self
            .client_board(color)
            .test_move(client_coord(*from), client_coord(*to));
        if verdict < 1 {
            return ChessMoveResult(verdict);
        }

        let mut storage = Vec::new();
        self.generate_moves_for(&from_piece, true, &mut storage);

        let found_move = storage
            .into_iter()
            .find(|m| m.from.equals(Some(from)) && m.to.equals(Some(to)));

        // if this fails the client and server failed to find a common valid move
        let Some(found_move) = found_move else {
            //return ChessMoveResult.BadMoveDestination;
            return ChessMoveResult::BadMoveInvalidCommand;
        };

        self.finalize_move(&found_move)
    }

    /// The client's rules over this board, `to_move` to play: every piece on its square, the en
    /// passant square the last move opened, whether each king and rook has moved (a king has
    /// once any move of it is in the history; a rook has unless it stands on its own corner and no
    /// move in the history left or reached that corner), and whether the last move put `to_move`
    /// in check (the flag the client's castling reads, V326). The client allows a two-square pawn
    /// advance only from the start rank, which a pawn never returns to.
    #[must_use]
    pub fn client_board(&self, to_move: ChessColor) -> rules::ChessLogic {
        let mut board = rules::ChessLogic::new();
        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                let Some(piece) = &self.board[index(i64::from(x + y * BOARD_SIZE))] else {
                    continue;
                };
                let piece_type = match piece.r#type {
                    ChessPieceType::Pawn => rules::PieceType::Pawn,
                    ChessPieceType::Rook => rules::PieceType::Rook,
                    ChessPieceType::Knight => rules::PieceType::Knight,
                    ChessPieceType::Bishop => rules::PieceType::Bishop,
                    ChessPieceType::Queen => rules::PieceType::Queen,
                    ChessPieceType::King => rules::PieceType::King,
                    _ => continue,
                };
                let home_rank = if piece.color == ChessColor::White {
                    0
                } else {
                    7
                };
                let at = ChessPieceCoord::new_xy(x, y);
                let moved = match piece.r#type {
                    ChessPieceType::Pawn => {
                        y != if piece.color == ChessColor::White {
                            1
                        } else {
                            6
                        }
                    }
                    ChessPieceType::King => self
                        .history
                        .iter()
                        .any(|m| m.color == piece.color && m.r#type == ChessPieceType::King),
                    ChessPieceType::Rook => {
                        !((x == 0 || x == 7) && y == home_rank)
                            || self.history.iter().any(|m| m.from == at || m.to == at)
                    }
                    _ => false,
                };
                let i = board.place(piece_type, piece.color.0, client_coord(at));
                board.pieces[i].moved = moved;
            }
        }
        board.cur_player = to_move.0;
        if let Some(site) = self.en_passant_coord {
            // the pawn that advanced two squares stands one square past the one it skipped
            let victim_y = if site.y < 4 { site.y + 1 } else { site.y - 1 };
            board.en_passant_attack_site = client_coord(site);
            board.en_passant_victim_pos = rules::Coord {
                x: site.x,
                y: victim_y,
            };
        }
        board.last_move_was_check = board.is_player_in_check(to_move.0);
        board
    }

    /// The moves `color` may make by the client's rules, in ACE's generated order: ACE's generated
    /// moves (which include every legal one) less those the client refuses, each once (ACE's full
    /// generation adds the castles again for every piece).
    #[must_use]
    pub fn legal_moves(&self, color: ChessColor) -> Vec<ChessMove> {
        let mut storage = Vec::new();
        self.generate_moves(color, &mut storage);
        let board = self.client_board(color);
        let mut seen = Vec::new();
        storage.retain(|m| {
            if seen.contains(&(m.from, m.to)) {
                return false;
            }
            seen.push((m.from, m.to));
            board.test_move(client_coord(m.from), client_coord(m.to)) > 0
        });
        storage
    }

    /// `color`'s score for the board: its material and position less its opponent's.
    #[must_use]
    pub fn evaluate_board_for(&self, color: ChessColor) -> f32 {
        let black = self.evaluate_board();
        if color == ChessColor::Black {
            black
        } else {
            -black
        }
    }

    // ACE: ChessLogic.AsyncCalculateAiSimpleMove
    /// Returns ACE's result and its two `ref` coordinates (`null` when no move is chosen).
    ///
    /// Not ACE's (retail, V311): the AI tries only the moves the client's rules
    /// allow and takes the one scoring best for its own side (the first of equals); with no move it
    /// answers `NoMoveResult`. ACE scored every position for Black whatever the AI played, started
    /// the best score at 0 (so a position scoring no better fell back to the first move that left
    /// it out of check), and tried pseudo-legal moves.
    pub fn async_calculate_ai_simple_move(
        &mut self,
        _key: &ChessAiAsyncTurnKey,
    ) -> (
        ChessMoveResult,
        Option<ChessPieceCoord>,
        Option<ChessPieceCoord>,
    ) {
        let color = self.turn;

        let mut best: Option<(f32, ChessMove)> = None;

        let storage = self.legal_moves(color);
        for generated_move in &storage {
            // no need to evaluate the board if the ai has checkmated the other player
            let result = self.finalize_move(generated_move);
            if has_result_flag(result, ChessMoveResult::OKMoveCheckmate) {
                return (result, Some(generated_move.from), Some(generated_move.to));
            }

            let board_score = self.evaluate_board_for(color);
            if best.as_ref().is_none_or(|(score, _)| board_score > *score) {
                best = Some((board_score, generated_move.clone()));
            }

            self.undo_move(1);
        }

        // checkmate / stalemate
        let Some((_, best_move)) = best else {
            return (ChessMoveResult::NoMoveResult, None, None);
        };

        let from = best_move.from;
        let to = best_move.to;

        (self.finalize_move(&best_move), Some(from), Some(to))
    }

    // ACE: ChessLogic.AsyncCalculateAiComplexMove
    /// Returns ACE's result and its two `ref` coordinates; `counter` is ACE's `ref uint counter`.
    ///
    /// Not ACE's (retail, V311): the search tries only the moves the client's
    /// rules allow, and with none it answers `NoMoveResult` (ACE threw).
    pub fn async_calculate_ai_complex_move(
        &mut self,
        _key: &ChessAiAsyncTurnKey,
        counter: &mut u32,
    ) -> (
        ChessMoveResult,
        Option<ChessPieceCoord>,
        Option<ChessPieceCoord>,
    ) {
        let depth: u32 = 3;
        let is_maximizing_power = true;

        let storage = self.legal_moves(self.turn);

        let mut best_board_score = -9999.0f32;
        let mut best_move: Option<ChessMove> = None;

        for generated_move in &storage {
            // no need to evaluate the board if the ai has checkmated the other player
            let result = self.finalize_move(generated_move);
            if has_result_flag(result, ChessMoveResult::OKMoveCheckmate) {
                return (result, Some(generated_move.from), Some(generated_move.to));
            }

            let board_score = self.minimax_alpha_beta(
                depth - 1,
                -10000.0,
                10000.0,
                !is_maximizing_power,
                counter,
            );
            self.undo_move(1);

            if board_score >= best_board_score {
                best_move = Some(generated_move.clone());
                best_board_score = board_score;
            }
        }

        // checkmate / stalemate
        let Some(best_move) = best_move else {
            return (ChessMoveResult::NoMoveResult, None, None);
        };
        let from = best_move.from;
        let to = best_move.to;

        (self.finalize_move(&best_move), Some(from), Some(to))
    }

    // ACE: ChessLogic.MinimaxAlphaBeta
    /// Not ACE's (retail, V311): the search follows the moves the client's rules
    /// allow, a leaf is scored for the maximising side (the side to move at a maximising node, its
    /// opponent at a minimising one; ACE scored every leaf for White), and the minimising branch
    /// takes the minimum (ACE took the maximum, so its score never dropped below 9999 and beta only
    /// grew).
    pub fn minimax_alpha_beta(
        &mut self,
        depth: u32,
        mut alpha: f32,
        mut beta: f32,
        is_maximizing_power: bool,
        counter: &mut u32,
    ) -> f32 {
        use empyrean_common::dotnet::math::{max_f32, min_f32};

        *counter = counter.wrapping_add(1);
        if depth == 0 {
            let maximizing = if is_maximizing_power {
                self.turn
            } else {
                chess::inverse_color(self.turn)
            };
            return self.evaluate_board_for(maximizing);
        }

        let storage = self.legal_moves(self.turn);

        if is_maximizing_power {
            let mut best_board_score = -9999.0f32;
            for m in &storage {
                self.finalize_move(m);
                best_board_score = max_f32(
                    best_board_score,
                    self.minimax_alpha_beta(depth - 1, alpha, beta, false, counter),
                );
                self.undo_move(1);

                alpha = max_f32(alpha, best_board_score);
                if beta <= alpha {
                    return best_board_score;
                }
            }
            best_board_score
        } else {
            let mut best_board_score = 9999.0f32;
            for m in &storage {
                self.finalize_move(m);
                best_board_score = min_f32(
                    best_board_score,
                    self.minimax_alpha_beta(depth - 1, alpha, beta, true, counter),
                );
                self.undo_move(1);

                beta = min_f32(beta, best_board_score);
                if beta <= alpha {
                    return best_board_score;
                }
            }
            best_board_score
        }
    }

    // ACE: ChessLogic.EvaluateBoard
    /// Black's material and position minus White's, summed in `float` in board order.
    #[must_use]
    pub fn evaluate_board(&self) -> f32 {
        let mut board_score = 0.0f32;
        self.walk_pieces(|piece| {
            // the knight and queen only have a single shared table
            let mut table_color = piece.color;
            if piece.r#type == ChessPieceType::Knight || piece.r#type == ChessPieceType::Queen {
                table_color = ChessColor::White;
            }

            let mut value = 0.0f32;
            value += chess::PIECE_SQUARE_TABLE[type_index(piece.r#type)][color_index(table_color)]
                [index(i64::from(piece.coord.offset()))];
            #[allow(clippy::cast_precision_loss)]
            {
                value += chess::PIECE_WORTH[type_index(piece.r#type)] as f32;
            }

            board_score += if piece.color == ChessColor::Black {
                value
            } else {
                -value
            };
        });

        board_score
    }

    // ACE: ChessLogic.GenerateMoves
    /// `GenerateMoves(BasePiece piece, bool single, List<ChessMove> storage)`.
    pub fn generate_moves_for(
        &self,
        piece: &BasePiece,
        single: bool,
        storage: &mut Vec<ChessMove>,
    ) {
        let color = piece.color;
        if piece.r#type == ChessPieceType::Pawn {
            let offsets = &chess::PAWN_OFFSETS[color_index(color)];

            // single
            let from = piece.coord;
            let mut to = from;
            to.move_offset_vec(offsets[0]);

            if self.get_piece(&to).is_none() {
                self.build_move(
                    storage,
                    ChessMoveFlag::Normal,
                    color,
                    piece.r#type,
                    from,
                    to,
                );

                // second
                to = from;
                to.move_offset_vec(offsets[1]);

                let start_rank = if color == ChessColor::White { 2 } else { 7 };
                if self.get_piece(&to).is_none() && from.rank() == start_rank {
                    self.build_move(
                        storage,
                        ChessMoveFlag::BigPawn,
                        color,
                        piece.r#type,
                        from,
                        to,
                    );
                }
            }

            // capture
            for offset in &offsets[2..4] {
                to = from;
                to.move_offset_vec(*offset);
                if !to.is_valid() {
                    continue;
                }

                let to_piece = self.get_piece(&to);
                if to_piece.is_some_and(|p| p.color != color) {
                    self.build_move(
                        storage,
                        ChessMoveFlag::Capture,
                        color,
                        piece.r#type,
                        from,
                        to,
                    );
                } else if let Some(en_passant) =
                    self.en_passant_coord.filter(|c| to.equals(Some(c)))
                {
                    self.build_move(
                        storage,
                        ChessMoveFlag::EnPassantCapture,
                        color,
                        piece.r#type,
                        from,
                        en_passant,
                    );
                }
            }
        } else {
            let range = chess::piece_offsets(piece.r#type);
            for offset in range {
                let from = piece.coord;
                let mut to = from;

                loop {
                    to.move_offset_vec(*offset);
                    if !to.is_valid() {
                        break;
                    }

                    if let Some(to_piece) = self.get_piece(&to) {
                        if to_piece.color != color {
                            self.build_move(
                                storage,
                                ChessMoveFlag::Capture,
                                color,
                                piece.r#type,
                                from,
                                to,
                            );
                        }
                        break;
                    }

                    self.build_move(
                        storage,
                        ChessMoveFlag::Normal,
                        color,
                        piece.r#type,
                        from,
                        to,
                    );

                    // Knights and Kings can't move more than once
                    if piece.r#type == ChessPieceType::Knight
                        || piece.r#type == ChessPieceType::King
                    {
                        break;
                    }
                }
            }
        }

        // only check for castling during full board generation or for a single king
        if !single || piece.r#type == ChessPieceType::King {
            // Not ACE's (a fix, V311): castling is looked at only while `color`
            // may still castle on either side. ACE's guard parsed as `(Castling[color] &
            // KingSideCastle) | QueenSideCastle`, never zero; the two flag checks below gated each
            // castle all the same.
            let castling = self.castling.get(color.0);
            if castling.intersects(ChessMoveFlag::KingSideCastle | ChessMoveFlag::QueenSideCastle) {
                let king = self
                    .get_piece_by_type(color, ChessPieceType::King)
                    .expect("ACE: ChessLogic.GenerateMoves: king is null (NullReferenceException)");
                let king_coord = king.coord;

                let op_color = chess::inverse_color(color);

                if self
                    .castling
                    .get(color.0)
                    .intersects(ChessMoveFlag::KingSideCastle)
                {
                    let mut castling_to_k = king_coord; // destination king
                    castling_to_k.move_offset(2, 0);
                    let mut castling_to_r = king_coord; // destination rook
                    castling_to_r.move_offset(1, 0);

                    if self.get_piece(&castling_to_r).is_none()
                        && self.get_piece(&castling_to_k).is_none()
                        && !self.can_attack(op_color, &king_coord)
                        && !self.can_attack(op_color, &castling_to_r)
                        && !self.can_attack(op_color, &castling_to_k)
                    {
                        self.build_move(
                            storage,
                            ChessMoveFlag::KingSideCastle,
                            color,
                            ChessPieceType::King,
                            king_coord,
                            castling_to_k,
                        );
                    }
                }

                if self
                    .castling
                    .get(color.0)
                    .intersects(ChessMoveFlag::QueenSideCastle)
                {
                    let mut castling_to_k = king_coord; // destination king
                    castling_to_k.move_offset(-2, 0);
                    let mut castling_to_r = king_coord; // destination rook
                    castling_to_r.move_offset(-1, 0);
                    let mut castling_to_i = king_coord; // intermediate
                    castling_to_i.move_offset(-3, 0);

                    if self.get_piece(&castling_to_r).is_none()
                        && self.get_piece(&castling_to_k).is_none()
                        && self.get_piece(&castling_to_i).is_none()
                        && !self.can_attack(op_color, &king_coord)
                        && !self.can_attack(op_color, &castling_to_r)
                        && !self.can_attack(op_color, &castling_to_k)
                    {
                        self.build_move(
                            storage,
                            ChessMoveFlag::QueenSideCastle,
                            color,
                            ChessPieceType::King,
                            king_coord,
                            castling_to_k,
                        );
                    }
                }
            }
        }
    }

    /// `GenerateMoves(ChessColor color, List<ChessMove> storage)`: every piece of `color` in board
    /// order, castling included.
    pub fn generate_moves(&self, color: ChessColor, storage: &mut Vec<ChessMove>) {
        self.walk_pieces(|piece| {
            if piece.color != color {
                return;
            }

            self.generate_moves_for(piece, false, storage);
        });
    }

    // ACE: ChessLogic.CanAttack
    /// Not ACE's (retail, V311): a sliding piece (and the king) attacks every
    /// square its ray reaches up to and including the first piece in the way, empty or not, so
    /// castling's "not through check" test sees rooks, bishops, queens and the king too. ACE counted
    /// only the ray's first occupied square, so an empty square was never attacked by them.
    #[must_use]
    pub fn can_attack(&self, attacker: ChessColor, victim: &ChessPieceCoord) -> bool {
        for x in 0..BOARD_SIZE {
            for y in 0..BOARD_SIZE {
                let Some(piece) = &self.board[index(i64::from(x + y * BOARD_SIZE))] else {
                    continue;
                };

                if piece.color != attacker {
                    continue;
                }

                if piece.can_attack_coord(victim) {
                    // the knight can jump over other pieces and the pawn can only attack a single space
                    if piece.r#type == ChessPieceType::Knight
                        || piece.r#type == ChessPieceType::Pawn
                    {
                        return true;
                    }

                    let range = chess::piece_offsets(piece.r#type);
                    for offset in range {
                        let from = piece.coord;
                        let mut to = from;

                        loop {
                            to.move_offset_vec(*offset);
                            if !to.is_valid() {
                                break;
                            }

                            if to.equals(Some(victim)) {
                                return true;
                            }
                            if self.get_piece(&to).is_some() {
                                break;
                            }
                        }
                    }
                }
            }
        }

        false
    }

    // ACE: ChessLogic.InCheck
    /// Not ACE's (retail, V311): the client's in-check test (a side with no king
    /// is not in check; ACE threw).
    #[must_use]
    pub fn in_check(&self, color: ChessColor) -> bool {
        self.client_board(color).is_player_in_check(color.0)
    }

    // ACE: ChessLogic.InCheckmate
    /// Not ACE's (retail, V311): the client's checkmate test, a search of every
    /// legal move, whatever `full_check` asks. ACE's quick form (`full_check` false, the one
    /// `FinalizeMove` used) counted pseudo-legal moves, so a side in check with any move at all was
    /// never mated.
    pub fn in_checkmate(&mut self, color: ChessColor, full_check: bool) -> bool {
        let _ = full_check;
        self.client_board(color).is_player_in_check_mate(color.0)
    }

    /// How the game stands for `color`, the side to move, by the client's rules: `Some(winner)` when
    /// it has no legal move (its opponent's colour when it is checkmated, `ChessWinnerStalemate`
    /// when it is not in check), `None` while it can play on.
    #[must_use]
    pub fn game_over(&self, color: ChessColor) -> Option<i32> {
        let board = self.client_board(color);
        if board.is_player_in_check_mate(color.0) {
            return Some(chess::inverse_color(color).0);
        }
        if self.legal_moves(color).is_empty() {
            return Some(chess::CHESS_WINNER_STALEMATE);
        }
        None
    }

    // ACE: ChessLogic.BuildMove
    pub fn build_move(
        &self,
        storage: &mut Vec<ChessMove>,
        mut result: ChessMoveFlag,
        color: ChessColor,
        piece_type: ChessPieceType,
        from: ChessPieceCoord,
        to: ChessPieceCoord,
    ) {
        let from_piece = self
            .get_piece(&from)
            .expect("ACE: ChessLogic.BuildMove: fromPiece is null (NullReferenceException)");
        let mut to_piece = self.get_piece(&to);

        // AC's Chess implementation doesn't support underpromotion
        let mut promotion = ChessPieceType::Empty;
        if from_piece.r#type == ChessPieceType::Pawn && (to.rank() == 1 || to.rank() == 8) {
            promotion = ChessPieceType::Queen;
            result |= ChessMoveFlag::Promotion;
        }

        let mut captured = ChessPieceType::Empty;
        if let Some(p) = to_piece {
            captured = p.r#type;
        } else if result.contains(ChessMoveFlag::EnPassantCapture) {
            captured = ChessPieceType::Pawn;

            let mut en_passant_coord = to;
            en_passant_coord.move_offset(0, if color == ChessColor::Black { 1 } else { -1 });

            to_piece = self.get_piece(&en_passant_coord);
        }

        let captured_guid = if captured == ChessPieceType::Empty {
            ObjectGuid::new(0)
        } else {
            to_piece
                .expect("ACE: ChessLogic.BuildMove: toPiece is null (NullReferenceException)")
                .guid
        };

        storage.push(ChessMove::new(
            result,
            color,
            piece_type,
            from,
            to,
            promotion,
            captured,
            self.r#move,
            self.half_move,
            self.castling,
            self.en_passant_coord,
            from_piece.guid,
            captured_guid,
        ));
    }

    // ACE: ChessLogic.FinalizeMove
    /// Not ACE's (retail, V311): the check and checkmate bits are the client's
    /// (`OKMoveCheck` or `OKMoveCheckmate`, a real legal-move search). For a promotion the client
    /// first computes them with the pawn still on the last rank, then again once it is a queen, and
    /// keeps the bits of both. ACE tested checkmate against pseudo-legal moves (a side in check with
    /// any move was never mated: the fool's mate answered `OKMoveCheck`), and its pseudo-legal
    /// moves could capture a king, after which the check test threw. Callers pass only moves the
    /// client's rules allow. An en passant capture stays a capture here (V306 reports it as such).
    pub fn finalize_move(&mut self, m: &ChessMove) -> ChessMoveResult {
        self.internal_move(m);

        let mut result = if m
            .flags
            .intersects(ChessMoveFlag::Capture | ChessMoveFlag::EnPassantCapture)
        {
            ChessMoveResult::OKMoveToOccupiedSquare
        } else {
            ChessMoveResult::OKMoveToEmptySquare
        };

        // win conditions
        let turn = self.turn;
        let mut board = self.client_board(turn);
        if m.flags.contains(ChessMoveFlag::Promotion) {
            result.0 |= ChessMoveResult::OKMovePromotion.0;

            let mut pawn_board = board.clone();
            if let Some(piece) = pawn_board
                .pieces
                .iter_mut()
                .find(|p| p.cur_pos == client_coord(m.to))
            {
                piece.piece_type = rules::PieceType::Pawn;
            }
            result.0 |= pawn_board.compute_check_result(turn.0);
        }
        result.0 |= board.compute_check_result(turn.0);

        result
    }

    // ACE: ChessLogic.InternalMove
    pub fn internal_move(&mut self, m: &ChessMove) {
        let flags = m.flags;
        let to = m.to;
        let from = m.from;
        let color = m.color;
        let op_color = chess::inverse_color(color);

        self.move_piece(&from, &to);

        if flags.contains(ChessMoveFlag::EnPassantCapture) {
            let mut en_passant_coord = to;
            en_passant_coord.move_offset(0, if color == ChessColor::Black { 1 } else { -1 });
            self.remove_piece_at(&en_passant_coord);
        }

        if flags.contains(ChessMoveFlag::Promotion) {
            let pawn_piece = self
                .get_piece(&to)
                .cloned()
                .expect("ACE: ChessLogic.InternalMove: pawnPiece is null (NullReferenceException)");
            let guid = pawn_piece.guid;

            self.remove_piece(&pawn_piece);

            #[allow(clippy::cast_sign_loss)]
            let queen_piece =
                self.add_piece(color, ChessPieceType::Queen, to.x as u32, to.y as u32);
            if let Some(queen_piece) = queen_piece {
                queen_piece.guid = guid;
            }
        }

        if m.r#type == ChessPieceType::King {
            // if we castled, move the rook next to our king
            if flags.intersects(ChessMoveFlag::KingSideCastle | ChessMoveFlag::QueenSideCastle) {
                let mut castling_to = m.to;
                let mut castling_from = castling_to;

                if flags.contains(ChessMoveFlag::KingSideCastle) {
                    castling_to.move_offset(-1, 0);
                    castling_from.move_offset(1, 0);
                }
                if flags.contains(ChessMoveFlag::QueenSideCastle) {
                    castling_to.move_offset(1, 0);
                    castling_from.move_offset(-2, 0);
                }

                self.move_piece(&castling_from, &castling_to);
            }

            // turn off castling, our king has moved
            self.castling.set(color.0, ChessMoveFlag::None);
        }

        // turn off castling if we have moved one of our rooks
        if self.castling.get(color.0) != ChessMoveFlag::None {
            self.do_castle_check(color, &from);
        }

        // turn off castling if we capture one of the opponent's rooks
        // Not ACE's (retail, V311): the square looked at is the one moved to,
        // where the captured rook stood, so taking a rook on its corner ends that castle. ACE looked
        // at the square moved from, leaving the opponent's right in place.
        if self.castling.get(op_color.0) != ChessMoveFlag::None {
            self.do_castle_check(op_color, &to);
        }

        if flags.contains(ChessMoveFlag::BigPawn) {
            let mut en_passant_coord = to;
            //enPassantCoord.MoveOffset(0, color == ChessColor.Black ? 2 : -2);
            en_passant_coord.move_offset(0, if color == ChessColor::Black { 1 } else { -1 });
            self.en_passant_coord = Some(en_passant_coord);
        } else {
            self.en_passant_coord = None;
        }

        self.history.push(m.clone());

        if color == ChessColor::Black {
            self.r#move = self.r#move.wrapping_add(1);
        }

        // reset 50 move rule counter if a pawn is moved or a piece is captured
        if m.r#type == ChessPieceType::Pawn
            || flags.intersects(ChessMoveFlag::Capture | ChessMoveFlag::EnPassantCapture)
        {
            self.half_move = 0;
        } else {
            self.half_move = self.half_move.wrapping_add(1);
        }

        self.turn = op_color;
    }

    // ACE: ChessLogic.UndoMove
    pub fn undo_move(&mut self, mut count: u32) {
        while count > 0 {
            let Some(m) = self.history.last().cloned() else {
                break;
            };

            // undo 'global' information
            // Not ACE's (a fix, V311): the side to move is the mover again, the
            // castling rights are the move's own copy of those before it, and a promotion is undone
            // by turning the queen, now back on `From`, into the mover's pawn. ACE set the side to
            // move to the mover's opponent, restored castling rights from the logic's own list (so
            // nothing), and put an opponent pawn on the promotion square while the queen stayed on
            // the pawn's square; its AI searches, run on the match's board, left these behind.
            let op_color = chess::inverse_color(m.color);
            self.turn = m.color;
            self.castling = m.castling;
            self.en_passant_coord = m.en_passant_coord;
            self.half_move = m.half_move;
            self.r#move = m.r#move;

            self.move_piece(&m.to, &m.from);

            let flags = m.flags;
            if flags.contains(ChessMoveFlag::Promotion) {
                if let Some(piece) = self.add_piece_at(m.color, ChessPieceType::Pawn, m.from) {
                    piece.guid = m.guid;
                }
            }

            if flags.contains(ChessMoveFlag::Capture) {
                let piece = self.add_piece_at(op_color, m.captured, m.to);
                piece
                    .expect("ACE: ChessLogic.UndoMove: piece is null (NullReferenceException)")
                    .guid = m.captured_guid;
            }

            if flags.contains(ChessMoveFlag::EnPassantCapture) {
                let mut en_passant_from = m.to;
                en_passant_from.move_offset(0, if m.color == ChessColor::Black { 1 } else { -1 });

                if let Some(piece) =
                    self.add_piece_at(op_color, ChessPieceType::Pawn, en_passant_from)
                {
                    piece.guid = m.captured_guid;
                }
            }

            if flags.intersects(ChessMoveFlag::KingSideCastle | ChessMoveFlag::QueenSideCastle) {
                let mut castling_to = m.to;
                let mut castling_from = castling_to;

                if flags.contains(ChessMoveFlag::KingSideCastle) {
                    castling_to.move_offset(1, 0);
                    castling_from.move_offset(-1, 0);
                }
                if flags.contains(ChessMoveFlag::QueenSideCastle) {
                    castling_to.move_offset(-2, 0);
                    castling_from.move_offset(1, 0);
                }

                // Not ACE's (a fix, V311): the rook goes back from beside the
                // king to its corner. ACE moved it the other way, from the empty corner, and threw
                // (NullReferenceException) whenever a castle was undone, so an AI that could castle
                // could not search.
                self.move_piece(&castling_from, &castling_to);
            }

            self.history.pop();
            count -= 1;
        }
    }

    // ACE: ChessLogic.DoCastleCheck
    pub fn do_castle_check(&mut self, color: ChessColor, from: &ChessPieceCoord) {
        let rook_flags = chess::rook_flags(color);
        for rook_flag in &rook_flags {
            if from.x == rook_flag.vector.0
                && from.y == rook_flag.vector.1
                && self.castling.get(color.0).intersects(rook_flag.flag)
            {
                let value = self.castling.get(color.0) & !rook_flag.flag;
                self.castling.set(color.0, value);
                break;
            }
        }
    }

    // ACE: ChessLogic.GetLastMove
    /// `History.Peek()`.
    ///
    /// # Panics
    /// On an empty history (`InvalidOperationException`).
    #[must_use]
    pub fn get_last_move(&self) -> &ChessMove {
        self.history
            .last()
            .expect("ACE: ChessLogic.GetLastMove: Stack empty (InvalidOperationException)")
    }

    // ACE: ChessLogic.DebugBoard
    /// The board as text, rank 8 first. ACE writes it to the console; here it is returned for the
    /// caller to log.
    #[must_use]
    pub fn debug_board(&self) -> String {
        let mut out = String::new();
        for y in (0..=7).rev() {
            for x in 0..=7 {
                let piece = self.get_piece(&ChessPieceCoord::new_xy(x, y));
                match piece.map(|p| p.class) {
                    None => out.push(' '),
                    Some(ChessPieceType::Pawn) => out.push('P'),
                    Some(ChessPieceType::King) => out.push('K'),
                    Some(ChessPieceType::Queen) => out.push('Q'),
                    Some(ChessPieceType::Bishop) => out.push('B'),
                    Some(ChessPieceType::Knight) => out.push('N'),
                    Some(ChessPieceType::Rook) => out.push('R'),
                    Some(_) => {}
                }
            }
            out.push('\n');
        }
        out
    }
}
