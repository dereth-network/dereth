//! The chess rules the client enforces.
//!
//! Chess is the one place the client runs a real rules engine of its own — it validates a move
//! locally before sending it, so a refusal is instant. The server still validates.
//!
//! A captured piece is moved to the sentinel coordinate [`HEAVEN`] rather than deleted. Nothing
//! reads the sentinel's exact value; it is carried opaquely.

/// `ChessPieceType`. `Castle` and `Rook` are the same value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PieceType {
    Empty = 0,
    Pawn = 1,
    /// Also spelled `Castle`.
    Rook = 2,
    Knight = 3,
    Bishop = 4,
    Queen = 5,
    King = 6,
}

/// The number of piece types, counting the empty square.
pub const NUM_PIECE_TYPES: u32 = 7;

/// Signed 32-bit board coordinates; each valid component is in 0..=7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Coord {
    pub x: i32,
    pub y: i32,
}

impl Coord {
    /// The coordinate validity test.
    #[must_use]
    pub fn is_valid(self) -> bool {
        (0..=7).contains(&self.x) && (0..=7).contains(&self.y)
    }
}

/// The off-board sentinel a captured piece is moved to rather than deleted.
///
/// The constant is never read back, so its exact value is not observable. `-1, -1` is the natural
/// off-board coordinate and is what this uses; nothing branches on it beyond "not on the board".
pub const HEAVEN: Coord = Coord { x: -1, y: -1 };

/// `ChessMoveResult`. The success values are positive and `OK_MOVE_MASK` extracts the base result;
/// the failures are negative.
pub mod move_result {
    pub const NO_MOVE_RESULT: i32 = 0;
    pub const OK_MOVE_TO_EMPTY_SQUARE: i32 = 1;
    pub const OK_MOVE_TO_OCCUPIED_SQUARE: i32 = 2;
    pub const OK_MOVE_EN_PASSANT: i32 = 3;
    /// Extracts the base result out of a success value.
    pub const OK_MOVE_MASK: i32 = 0x3FF;
    pub const OK_MOVE_CHECK: i32 = 0x400;
    pub const OK_MOVE_CHECKMATE: i32 = 0x800;
    pub const OK_MOVE_PROMOTION: i32 = 0x1000;

    pub const BAD_MOVE_INVALID_BOARD_STATE: i32 = -111;
    pub const BAD_MOVE_CANT_CASTLE_AFTER_MOVING: i32 = -110;
    pub const BAD_MOVE_CANT_CASTLE_THROUGH_CHECK: i32 = -109;
    pub const BAD_MOVE_CANT_CASTLE_OUT_OF_CHECK: i32 = -108;
    pub const BAD_MOVE_WOULD_COLLIDE: i32 = -107;
    pub const BAD_MOVE_SELF_CHECK: i32 = -106;
    pub const BAD_MOVE_WOULD_CLOBBER: i32 = -105;
    pub const BAD_MOVE_DESTINATION: i32 = -104;
    pub const BAD_MOVE_NOT_YOURS: i32 = -103;
    pub const BAD_MOVE_NO_PIECE: i32 = -102;
    pub const BAD_MOVE_DISTANCE: i32 = -101;
    pub const BAD_MOVE_DIRECTION: i32 = -100;
    pub const BAD_MOVE_NOT_YOUR_TURN: i32 = -3;
    pub const BAD_MOVE_NOT_PLAYING: i32 = -2;
    pub const BAD_MOVE_INVALID_COMMAND: i32 = -1;
}

/// Minigame UI state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum GameState {
    #[default]
    NotPlaying = 0,
    AttemptingToJoinGame = 1,
    WaitingForGameStart = 2,
    PlayingMyTurn = 3,
    PlayingTryingToMove = 4,
    PlayingNotMyTurn = 5,
}

/// A piece and its movement history (0x24 bytes in the original client's layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub piece_type: PieceType,
    /// 0 or 1.
    pub player: i32,
    pub cur_pos: Coord,
    pub last_known_good_pos: Coord,
    /// Castling and two-square-pawn eligibility.
    pub moved: bool,
}

/// The board and two piece arrays (0x134 bytes in the original client's layout).
#[derive(Debug, Clone)]
pub struct ChessLogic {
    /// The 8×8 board, indexed `[x][y]`, holding an index into [`ChessLogic::pieces`].
    board: [[Option<usize>; 8]; 8],
    pub pieces: Vec<Piece>,
    pub cur_player: i32,
    pub last_move_was_check: bool,
    pub en_passant_attack_site: Coord,
    pub en_passant_victim_pos: Coord,
}

impl Default for ChessLogic {
    fn default() -> Self {
        Self::new()
    }
}

impl ChessLogic {
    #[must_use]
    pub fn new() -> Self {
        Self {
            board: [[None; 8]; 8],
            pieces: Vec::new(),
            cur_player: 0,
            last_move_was_check: false,
            en_passant_attack_site: HEAVEN,
            en_passant_victim_pos: HEAVEN,
        }
    }

    /// Place a piece, as the board-state message does.
    pub fn place(&mut self, piece_type: PieceType, player: i32, at: Coord) -> usize {
        let i = self.pieces.len();
        self.pieces.push(Piece {
            piece_type,
            player,
            cur_pos: at,
            last_known_good_pos: at,
            moved: false,
        });
        if at.is_valid() {
            self.board[at.x as usize][at.y as usize] = Some(i);
        }
        i
    }

    #[must_use]
    pub fn at(&self, c: Coord) -> Option<&Piece> {
        if !c.is_valid() {
            return None;
        }
        self.board[c.x as usize][c.y as usize].map(|i| &self.pieces[i])
    }

    /// The board array itself — what the board's prepare-new-move copies out and
    /// its undo-moves copies back.
    #[must_use]
    pub fn board_snapshot(&self) -> [[Option<usize>; 8]; 8] {
        self.board
    }

    /// The other half of [`Self::board_snapshot`]. It writes **only** the board array, because that is
    /// all the undo writes — see `dereth_client_model::minigame::GameBoard::undo_moves` for why that is
    /// load-bearing rather than an omission.
    pub fn restore_board_snapshot(&mut self, board: [[Option<usize>; 8]; 8]) {
        self.board = board;
    }

    /// The client's piece swap — the pawn on `at` becomes a queen.
    ///
    /// The client deletes the pawn object and `new`s a queen with the same square, side
    /// and index; with a `PieceType` tag rather than a separate piece class the same thing is one
    /// store.
    /// Answers false when the square holds no piece, which is the promotion's own null guard.
    pub fn promote_to_queen(&mut self, at: Coord) -> bool {
        if !at.is_valid() {
            return false;
        }
        let Some(i) = self.board[at.x as usize][at.y as usize] else {
            return false;
        };
        self.pieces[i].piece_type = PieceType::Queen;
        true
    }

    /// Behavior: the board-state test every move starts with — each side has pieces and a king.
    ///
    /// The client keeps each side's king first in that side's piece list and tests only that; it
    /// does not compare the board with the pieces' positions (a refused move's undo leaves them
    /// apart, and play goes on).
    #[must_use]
    pub fn sanity_check_board(&self) -> bool {
        (0..2).all(|player| self.king_of(player).is_some())
    }

    /// The side's king, the first king in the piece list.
    fn king_of(&self, player: i32) -> Option<usize> {
        self.pieces
            .iter()
            .position(|p| p.piece_type == PieceType::King && p.player == player)
    }

    /// Behavior: move a piece and update the board and the piece's `moved` flag.
    ///
    /// The square the piece leaves is cleared only while it still holds that piece: a captured
    /// piece is sent to [`HEAVEN`] after the capturing piece has already taken its square.
    fn commit_piece_pos(&mut self, i: usize, to: Coord) {
        let from = self.pieces[i].cur_pos;
        if from.is_valid() && self.board[from.x as usize][from.y as usize] == Some(i) {
            self.board[from.x as usize][from.y as usize] = None;
        }
        self.pieces[i].cur_pos = to;
        self.pieces[i].moved = true;
        if to.is_valid() {
            self.board[to.x as usize][to.y as usize] = Some(i);
            self.pieces[i].last_known_good_pos = to;
        }
    }

    /// Whether a piece of `piece_type` may go `dx` across and `dy` forward (towards the opponent)
    /// onto an empty square.
    fn can_go_to(piece: &Piece, dx: i32, dy: i32) -> bool {
        match piece.piece_type {
            PieceType::Empty => false,
            PieceType::Pawn => dx == 0 && (dy == 1 || (dy == 2 && !piece.moved)),
            PieceType::Rook => (dx != 0) != (dy != 0),
            PieceType::Knight => {
                (dx.abs() == 1 && dy.abs() == 2) || (dx.abs() == 2 && dy.abs() == 1)
            }
            PieceType::Bishop => dx.abs() == dy.abs(),
            PieceType::Queen => dx == 0 || dy == 0 || dx.abs() == dy.abs(),
            PieceType::King => dx.abs() < 2 && dy.abs() < 2,
        }
    }

    /// Whether the piece may take a piece `dx` across and `dy` forward. Pawns take one square
    /// diagonally forward; every other piece takes as it moves.
    fn can_attack(piece: &Piece, dx: i32, dy: i32) -> bool {
        if piece.piece_type == PieceType::Pawn {
            dx.abs() == 1 && dy == 1
        } else {
            Self::can_go_to(piece, dx, dy)
        }
    }

    /// The straight, diagonal or knight step from `from` towards `to`, and how many steps it takes.
    fn move_vector(from: Coord, to: Coord) -> ((i32, i32), i32) {
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        let dist = if dx == 0 {
            dy.abs()
        } else if dy == 0 || dx == dy {
            dx.abs()
        } else {
            dx.abs().min(dy.abs())
        };
        if dist == 0 {
            return ((0, 0), 0);
        }
        ((dx / dist, dy / dist), dist)
    }

    /// The client's geometry half: the per-piece movement rule and the collision walk, without the
    /// self-check test. With `en_passant`, a pawn's diagonal step onto the en passant square takes
    /// the pawn standing on the victim square, as though it stood on the en passant square.
    ///
    /// A destination holding one of the mover's own pieces (the piece's own square included) is
    /// `BadMoveWouldClobber`; a move the piece cannot make, onto an empty square or onto an
    /// opponent's (a pawn straight into a piece), is `BadMoveDirection`, however far; a piece in
    /// the way is `BadMoveWouldCollide`.
    fn test_can_get_there(&self, i: usize, to: Coord, en_passant: bool) -> i32 {
        use move_result as m;
        if !to.is_valid() {
            return m::BAD_MOVE_DESTINATION;
        }
        let p = self.pieces[i];
        let from = p.cur_pos;
        let dx = to.x - from.x;
        // Forward is +y for player 0 and -y for player 1.
        let dy = if p.player == 0 {
            to.y - from.y
        } else {
            from.y - to.y
        };

        let mut taken_en_passant = false;
        let victim = if en_passant
            && to == self.en_passant_attack_site
            && p.piece_type == PieceType::Pawn
            && Self::can_attack(&p, dx, dy)
        {
            // Anything but a pawn on the victim square: the step is judged as onto an empty square.
            let v = self
                .at(self.en_passant_victim_pos)
                .copied()
                .filter(|v| v.piece_type == PieceType::Pawn);
            taken_en_passant = v.is_some();
            v
        } else {
            self.at(to).copied()
        };

        match victim {
            Some(v) => {
                if v.player == p.player {
                    return m::BAD_MOVE_WOULD_CLOBBER;
                }
                if !Self::can_attack(&p, dx, dy) {
                    return m::BAD_MOVE_DIRECTION;
                }
            }
            None => {
                if !Self::can_go_to(&p, dx, dy) {
                    return m::BAD_MOVE_DIRECTION;
                }
            }
        }

        // The collision walk, for any move longer than one square.
        if dx.abs() > 1 || (to.y - from.y).abs() > 1 {
            let (step, dist) = Self::move_vector(from, to);
            let mut c = from;
            for _ in 1..dist {
                c = Coord {
                    x: c.x + step.0,
                    y: c.y + step.1,
                };
                if self.at(c).is_some() {
                    return m::BAD_MOVE_WOULD_COLLIDE;
                }
            }
        }

        if victim.is_some() {
            if taken_en_passant {
                m::OK_MOVE_EN_PASSANT
            } else {
                m::OK_MOVE_TO_OCCUPIED_SQUARE
            }
        } else {
            m::OK_MOVE_TO_EMPTY_SQUARE
        }
    }

    /// The in-check test: whether any opponent piece on the board can reach the side's king.
    #[must_use]
    pub fn is_player_in_check(&self, player: i32) -> bool {
        let Some(king) = self.king_of(player) else {
            return false;
        };
        let king_pos = self.pieces[king].cur_pos;
        if !king_pos.is_valid() {
            return false;
        }
        (0..self.pieces.len()).any(|i| {
            self.pieces[i].player != player
                && self.pieces[i].cur_pos.is_valid()
                && self.test_can_get_there(i, king_pos, false) > 0
        })
    }

    /// The self-check test — try, test, undo: whether the piece's side is in check with the piece
    /// on `to` and whatever stood there taken; for an en passant capture (`result`
    /// `OK_MOVE_EN_PASSANT`), the pawn on the victim square is taken instead.
    fn does_move_self_check(&self, i: usize, to: Coord, result: i32) -> bool {
        let mut trial = self.clone();
        let victim = if result == move_result::OK_MOVE_EN_PASSANT {
            let v = self.en_passant_victim_pos;
            if v.is_valid() {
                trial.board[v.x as usize][v.y as usize]
            } else {
                None
            }
        } else {
            trial.board[to.x as usize][to.y as usize]
        };
        trial.commit_piece_pos(i, to);
        if let Some(v) = victim {
            trial.commit_piece_pos(v, HEAVEN);
        }
        trial.is_player_in_check(self.pieces[i].player)
    }

    /// The move-and-self-check test — the reachability test, then the self-check test.
    fn test_move_and_self_check(&self, i: usize, to: Coord) -> i32 {
        let r = self.test_can_get_there(i, to, true);
        if r > 0 && self.does_move_self_check(i, to, r) {
            return move_result::BAD_MOVE_SELF_CHECK;
        }
        r
    }

    /// The king's eight steps, in the order the client tries them.
    const KING_STEPS: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];

    /// Behavior: the checkmate test. The king may step out of check; failing that, while in check,
    /// the **first** opponent piece that attacks the king (in piece order) may be taken, or its line
    /// to the king blocked, by any of the side's pieces. Nothing else is tried: a check that only
    /// an en passant capture of the checking pawn answers is checkmate.
    #[must_use]
    pub fn is_player_in_check_mate(&self, player: i32) -> bool {
        let Some(king) = self.king_of(player) else {
            return false;
        };
        let king_pos = self.pieces[king].cur_pos;
        for (sx, sy) in Self::KING_STEPS {
            let to = Coord {
                x: king_pos.x + sx,
                y: king_pos.y + sy,
            };
            let r = self.test_can_get_there(king, to, true);
            if r > 0 && !self.does_move_self_check(king, to, r) {
                return false;
            }
        }
        if !self.is_player_in_check(player) {
            return false;
        }
        let attacker = (0..self.pieces.len()).find(|&a| {
            self.pieces[a].player != player
                && self.pieces[a].cur_pos.is_valid()
                && self.test_can_get_there(a, king_pos, false) > 0
        });
        let Some(attacker) = attacker else {
            return false;
        };
        // The attacker's square, then each square between it and the king.
        let from = self.pieces[attacker].cur_pos;
        let (step, dist) = Self::move_vector(from, king_pos);
        let mut sq = from;
        for _ in 0..dist {
            for j in 0..self.pieces.len() {
                if self.pieces[j].player != player || !self.pieces[j].cur_pos.is_valid() {
                    continue;
                }
                let r = self.test_can_get_there(j, sq, true);
                if r > 0 && !self.does_move_self_check(j, sq, r) {
                    return false;
                }
            }
            sq = Coord {
                x: sq.x + step.0,
                y: sq.y + step.1,
            };
        }
        true
    }

    /// The check-result computation: `OK_MOVE_CHECK` or `OK_MOVE_CHECKMATE` (never both) for the
    /// side, or 0. It also records whether that side is in check ([`Self::last_move_was_check`],
    /// set on checkmate too), which is what castling reads.
    pub fn compute_check_result(&mut self, player: i32) -> i32 {
        let r = if !self.is_player_in_check(player) {
            0
        } else if self.is_player_in_check_mate(player) {
            move_result::OK_MOVE_CHECKMATE
        } else {
            move_result::OK_MOVE_CHECK
        };
        self.last_move_was_check = r > move_result::OK_MOVE_MASK;
        r
    }

    /// What [`Self::do_move`] would answer for `from` â†’ `to`, without making the move: the same
    /// refusals in the same order, or the base result (`OK_MOVE_TO_EMPTY_SQUARE`,
    /// `OK_MOVE_TO_OCCUPIED_SQUARE` or `OK_MOVE_EN_PASSANT`) without the promotion, check and
    /// checkmate bits.
    #[must_use]
    pub fn test_move(&self, from: Coord, to: Coord) -> i32 {
        use move_result as m;
        if !from.is_valid() || !to.is_valid() {
            return m::BAD_MOVE_DESTINATION;
        }
        if !self.sanity_check_board() {
            return m::BAD_MOVE_INVALID_BOARD_STATE;
        }
        let Some(i) = self.board[from.x as usize][from.y as usize] else {
            return m::BAD_MOVE_NO_PIECE;
        };
        if self.pieces[i].player != self.cur_player {
            return m::BAD_MOVE_NOT_YOURS;
        }
        let p = self.pieces[i];
        if p.piece_type == PieceType::King && (to.x - from.x).abs() == 2 && to.y == from.y {
            // Castling moves the rook as it succeeds, so it is tried on a copy.
            self.clone().handle_castling(i, to)
        } else {
            self.test_move_and_self_check(i, to)
        }
    }

    /// The chess logic's `Move(from, to)`, transcribed step by step.
    pub fn do_move(&mut self, from: Coord, to: Coord) -> i32 {
        use move_result as m;
        if !from.is_valid() || !to.is_valid() {
            return m::BAD_MOVE_DESTINATION;
        }
        if !self.sanity_check_board() {
            return m::BAD_MOVE_INVALID_BOARD_STATE;
        }
        let Some(i) = self.board[from.x as usize][from.y as usize] else {
            return m::BAD_MOVE_NO_PIECE;
        };
        let mut victim = self.board[to.x as usize][to.y as usize];
        if self.pieces[i].player != self.cur_player {
            return m::BAD_MOVE_NOT_YOURS;
        }

        let p = self.pieces[i];
        let mut r =
            if p.piece_type == PieceType::King && (to.x - from.x).abs() == 2 && to.y == from.y {
                self.handle_castling(i, to)
            } else {
                let r = self.test_move_and_self_check(i, to);
                if r < 1 {
                    return r;
                }
                if r & m::OK_MOVE_MASK == m::OK_MOVE_EN_PASSANT {
                    let v = self.en_passant_victim_pos;
                    if v.is_valid() {
                        victim = self.board[v.x as usize][v.y as usize];
                    }
                }
                r
            };
        if r < 1 {
            return r;
        }

        self.commit_piece_pos(i, to);
        if let Some(v) = victim {
            self.commit_piece_pos(v, HEAVEN);
        }
        self.en_passant_attack_site = HEAVEN;
        if p.piece_type == PieceType::Pawn {
            if (to.y - from.y).abs() == 2 {
                self.en_passant_attack_site = Coord {
                    x: from.x,
                    y: (from.y + to.y) / 2,
                };
                self.en_passant_victim_pos = to;
            }
            if to.y == 0 || to.y == 7 {
                r |= m::OK_MOVE_PROMOTION;
            }
        }
        self.cur_player = i32::from(self.cur_player == 0);
        r |= self.compute_check_result(self.cur_player);
        r
    }

    /// Behavior: castling, refused in this order: the side was put in check by the last move
    /// ([`Self::last_move_was_check`], not a fresh test) — `BadMoveCantCastleOutOfCheck`; the king
    /// has moved — `BadMoveCantCastleAfterMoving`; then, walking out from the king towards the
    /// destination, the first piece met that is not the side's own rook — `BadMoveWouldCollide`
    /// (none at all: `BadMoveNoPiece`); that rook has moved — `BadMoveCantCastleAfterMoving`; the
    /// square the king passes is attacked — `BadMoveCantCastleThroughCheck`; the king's destination
    /// is attacked — `BadMoveSelfCheck`. The rook then stands on the square the king passed.
    fn handle_castling(&mut self, king: usize, to: Coord) -> i32 {
        use move_result as m;
        let k = self.pieces[king];
        if self.last_move_was_check {
            return m::BAD_MOVE_CANT_CASTLE_OUT_OF_CHECK;
        }
        if k.moved {
            return m::BAD_MOVE_CANT_CASTLE_AFTER_MOVING;
        }
        let step = if to.x < k.cur_pos.x { -1 } else { 1 };
        let through = Coord {
            x: k.cur_pos.x + step,
            y: k.cur_pos.y,
        };
        let dest = Coord {
            x: k.cur_pos.x + 2 * step,
            y: k.cur_pos.y,
        };
        let mut c = through;
        while c.is_valid() {
            if let Some(rook) = self.board[c.x as usize][c.y as usize] {
                let r = self.pieces[rook];
                if r.piece_type != PieceType::Rook || r.player != k.player {
                    return m::BAD_MOVE_WOULD_COLLIDE;
                }
                if r.moved {
                    return m::BAD_MOVE_CANT_CASTLE_AFTER_MOVING;
                }
                if self.does_move_self_check(king, through, 0) {
                    return m::BAD_MOVE_CANT_CASTLE_THROUGH_CHECK;
                }
                if self.does_move_self_check(king, dest, 0) {
                    return m::BAD_MOVE_SELF_CHECK;
                }
                self.commit_piece_pos(rook, through);
                self.commit_piece_pos(king, dest);
                return m::OK_MOVE_TO_EMPTY_SQUARE;
            }
            c.x += step;
        }
        m::BAD_MOVE_NO_PIECE
    }
}

#[cfg(test)]
mod tests {
    use super::move_result as m;
    use super::*;

    fn c(x: i32, y: i32) -> Coord {
        Coord { x, y }
    }

    /// Oracle: `18-emotes-and-chess.md` §7's two enum tables. The failure values are the signed
    /// forms of the unsigned encodings retail uses.
    #[test]
    fn the_move_result_enum_matches_the_documented_values() {
        assert_eq!(m::OK_MOVE_MASK, 0x3FF);
        assert_eq!(m::OK_MOVE_CHECK, 0x400);
        assert_eq!(m::OK_MOVE_CHECKMATE, 0x800);
        assert_eq!(m::OK_MOVE_PROMOTION, 0x1000);
        // The composites the document lists explicitly.
        assert_eq!(m::OK_MOVE_TO_EMPTY_SQUARE | m::OK_MOVE_CHECK, 0x401);
        assert_eq!(m::OK_MOVE_EN_PASSANT | m::OK_MOVE_CHECKMATE, 0x803);
        assert_eq!(m::OK_MOVE_PROMOTION | m::OK_MOVE_CHECK, 0x1400);
        assert_eq!(m::OK_MOVE_PROMOTION | m::OK_MOVE_CHECKMATE, 0x1800);
        // The unsigned encodings, round-tripped.
        #[allow(clippy::cast_sign_loss)] // the point is the two's-complement bit pattern
        {
            assert_eq!(m::BAD_MOVE_INVALID_BOARD_STATE as u32, 0xFFFF_FF91);
            assert_eq!(m::BAD_MOVE_DIRECTION as u32, 0xFFFF_FF9C);
            assert_eq!(m::BAD_MOVE_NOT_YOUR_TURN as u32, 0xFFFF_FFFD);
            assert_eq!(m::BAD_MOVE_INVALID_COMMAND as u32, 0xFFFF_FFFF);
        }
        assert_eq!(PieceType::King as u32, 6);
        assert_eq!(NUM_PIECE_TYPES, 7);
        assert_eq!(GameState::PlayingNotMyTurn as u32, 5);
    }

    /// Oracle: §8's, gate by gate and in order.
    #[test]
    fn move_rejects_in_the_documented_order() {
        let mut g = ChessLogic::new();
        g.place(PieceType::Pawn, 0, c(4, 1));
        g.place(PieceType::Pawn, 1, c(4, 6));
        // no kings yet: the board-state test refuses every move
        assert_eq!(g.do_move(c(4, 1), c(4, 2)), m::BAD_MOVE_INVALID_BOARD_STATE);
        g.place(PieceType::King, 0, c(4, 0));
        g.place(PieceType::King, 1, c(4, 7));

        assert_eq!(g.do_move(c(-1, 0), c(4, 2)), m::BAD_MOVE_DESTINATION);
        assert_eq!(g.do_move(c(0, 0), c(8, 0)), m::BAD_MOVE_DESTINATION);
        assert_eq!(g.do_move(c(0, 0), c(0, 1)), m::BAD_MOVE_NO_PIECE);
        assert_eq!(
            g.do_move(c(4, 6), c(4, 5)),
            m::BAD_MOVE_NOT_YOURS,
            "it is player 0's turn"
        );
    }

    /// Oracle: §8 — a two-square pawn advance arms en passant, and the capture takes the pawn from
    /// `en_passant_victim_pos`, not from the destination square.
    #[test]
    fn a_two_square_pawn_advance_arms_and_then_allows_en_passant() {
        let mut g = ChessLogic::new();
        let white = g.place(PieceType::Pawn, 0, c(4, 3));
        g.place(PieceType::Pawn, 1, c(3, 6));
        g.place(PieceType::King, 0, c(0, 0));
        g.place(PieceType::King, 1, c(7, 7));

        // White advances one square onto the fifth rank, so it is black's turn.
        assert!(g.do_move(c(4, 3), c(4, 4)) > 0);
        assert_eq!(g.pieces[white].cur_pos, c(4, 4));
        assert_eq!(g.cur_player, 1);

        // Black plays the two-square advance past the white pawn.
        let r = g.do_move(c(3, 6), c(3, 4));
        assert!(r > 0, "{r}");
        assert_eq!(
            g.en_passant_attack_site,
            c(3, 5),
            "the site is the square the pawn skipped"
        );
        assert_eq!(g.en_passant_victim_pos, c(3, 4));

        // White captures en passant: the destination is empty, the victim is one rank behind.
        let r = g.do_move(c(4, 4), c(3, 5));
        assert_eq!(r & m::OK_MOVE_MASK, m::OK_MOVE_EN_PASSANT, "{r:#X}");
        assert!(g.at(c(3, 4)).is_none(), "the captured pawn left the board");
        assert_eq!(g.pieces[white].cur_pos, c(3, 5));
        assert_eq!(
            g.en_passant_attack_site, HEAVEN,
            "the site is cleared after every move"
        );
    }

    /// Oracle: §8 — a pawn reaching rank 0 or 7 sets `OKMovePromotion`.
    #[test]
    fn a_pawn_reaching_the_last_rank_is_flagged_for_promotion() {
        let mut g = ChessLogic::new();
        g.place(PieceType::Pawn, 0, c(0, 6));
        g.place(PieceType::King, 0, c(4, 0));
        g.place(PieceType::King, 1, c(7, 4));
        let r = g.do_move(c(0, 6), c(0, 7));
        assert!(r & m::OK_MOVE_PROMOTION != 0, "{r:#X}");
        assert_eq!(r & m::OK_MOVE_MASK, m::OK_MOVE_TO_EMPTY_SQUARE);
    }

    /// Oracle: §8 — a move that leaves your own king in check is a self-check refusal.
    #[test]
    fn a_self_checking_move_is_refused() {
        let mut g = ChessLogic::new();
        g.place(PieceType::King, 0, c(4, 0));
        g.place(PieceType::King, 1, c(0, 7));
        // A white rook pinned on the file by a black rook.
        g.place(PieceType::Rook, 0, c(4, 3));
        g.place(PieceType::Rook, 1, c(4, 7));
        assert_eq!(g.do_move(c(4, 3), c(0, 3)), m::BAD_MOVE_SELF_CHECK);
        // Moving along the pin is fine.
        assert!(g.do_move(c(4, 3), c(4, 5)) > 0);
    }

    /// The client's castling refusals, each by its own code and in the client's order: out of
    /// check (the last move's check flag), the king has moved, the first piece met on the way is
    /// not the side's own rook, that rook has moved, the square passed is attacked, the
    /// destination is attacked.
    #[test]
    fn castling_refusals_come_in_the_clients_order() {
        let base = || {
            let mut g = ChessLogic::new();
            g.place(PieceType::King, 0, c(4, 0));
            g.place(PieceType::Rook, 0, c(7, 0));
            g.place(PieceType::King, 1, c(4, 7));
            g
        };
        // Clean castle: the rook stands on the square the king passed.
        let mut g = base();
        let r = g.do_move(c(4, 0), c(6, 0));
        assert!(r > 0, "{r}");
        assert_eq!(g.at(c(5, 0)).map(|p| p.piece_type), Some(PieceType::Rook));
        assert_eq!(g.at(c(6, 0)).map(|p| p.piece_type), Some(PieceType::King));

        // Out of check is the last move's flag, tested first, before the king's moved flag.
        let mut g = base();
        g.last_move_was_check = true;
        assert_eq!(
            g.do_move(c(4, 0), c(6, 0)),
            m::BAD_MOVE_CANT_CASTLE_OUT_OF_CHECK
        );
        g.pieces[0].moved = true;
        assert_eq!(
            g.do_move(c(4, 0), c(6, 0)),
            m::BAD_MOVE_CANT_CASTLE_OUT_OF_CHECK
        );
        // A king attacked without the flag (no move put it there) is not tested again: the castle
        // is judged by the squares it passes and reaches.
        let mut g = base();
        g.place(PieceType::Rook, 1, c(4, 5));
        assert!(g.do_move(c(4, 0), c(6, 0)) > 0);
        // A move that gives check sets the flag for the side to move.
        let mut g = base();
        g.place(PieceType::Rook, 1, c(3, 5));
        g.cur_player = 1;
        assert_eq!(
            g.do_move(c(3, 5), c(4, 5)),
            m::OK_MOVE_TO_EMPTY_SQUARE | m::OK_MOVE_CHECK
        );
        assert!(g.last_move_was_check);
        assert_eq!(
            g.do_move(c(4, 0), c(6, 0)),
            m::BAD_MOVE_CANT_CASTLE_OUT_OF_CHECK
        );

        // The king has already moved.
        let mut g = base();
        g.pieces[0].moved = true;
        assert_eq!(
            g.do_move(c(4, 0), c(6, 0)),
            m::BAD_MOVE_CANT_CASTLE_AFTER_MOVING
        );

        // The first piece on the way is not the side's own rook: a knight, or an opponent's rook.
        let mut g = base();
        g.place(PieceType::Knight, 0, c(6, 0));
        assert_eq!(g.do_move(c(4, 0), c(6, 0)), m::BAD_MOVE_WOULD_COLLIDE);
        let mut g = ChessLogic::new();
        g.place(PieceType::King, 0, c(4, 0));
        g.place(PieceType::Rook, 1, c(7, 0));
        g.place(PieceType::King, 1, c(4, 7));
        assert_eq!(g.do_move(c(4, 0), c(6, 0)), m::BAD_MOVE_WOULD_COLLIDE);
        // A piece in the way is found before the rook's moved flag is read.
        let mut g = base();
        g.pieces[1].moved = true;
        g.place(PieceType::Bishop, 0, c(5, 0));
        assert_eq!(g.do_move(c(4, 0), c(6, 0)), m::BAD_MOVE_WOULD_COLLIDE);
        // No piece at all before the edge.
        let mut g = base();
        assert_eq!(g.do_move(c(4, 0), c(2, 0)), m::BAD_MOVE_NO_PIECE);

        // The rook has moved; it is read before the squares are tested.
        let mut g = base();
        g.pieces[1].moved = true;
        g.place(PieceType::Rook, 1, c(5, 5));
        assert_eq!(
            g.do_move(c(4, 0), c(6, 0)),
            m::BAD_MOVE_CANT_CASTLE_AFTER_MOVING
        );

        // The square the king passes is attacked.
        let mut g = base();
        g.place(PieceType::Rook, 1, c(5, 5));
        assert_eq!(
            g.do_move(c(4, 0), c(6, 0)),
            m::BAD_MOVE_CANT_CASTLE_THROUGH_CHECK
        );

        // Only the destination is attacked: a self-check.
        let mut g = base();
        g.place(PieceType::Rook, 1, c(6, 5));
        assert_eq!(g.do_move(c(4, 0), c(6, 0)), m::BAD_MOVE_SELF_CHECK);
    }

    /// A move a piece cannot make is `BadMoveDirection` however it misses (never
    /// `BadMoveDistance`); a pawn straight into an opponent's piece cannot take it
    /// (`BadMoveDirection`); a piece moved onto its own square meets itself (`BadMoveWouldClobber`).
    #[test]
    fn the_clients_refusal_codes_for_a_move_a_piece_cannot_make() {
        let mut g = ChessLogic::new();
        g.place(PieceType::King, 0, c(4, 0));
        g.place(PieceType::King, 1, c(4, 7));
        g.place(PieceType::Rook, 0, c(0, 0));
        g.place(PieceType::Pawn, 0, c(2, 1));
        g.place(PieceType::Pawn, 1, c(2, 2));
        g.place(PieceType::Pawn, 0, c(6, 1));
        // a rook diagonally, a king two squares, a pawn three squares
        assert_eq!(g.test_move(c(0, 0), c(3, 3)), m::BAD_MOVE_DIRECTION);
        assert_eq!(g.test_move(c(4, 0), c(4, 2)), m::BAD_MOVE_DIRECTION);
        assert_eq!(g.test_move(c(6, 1), c(6, 4)), m::BAD_MOVE_DIRECTION);
        // a pawn straight into a piece
        assert_eq!(g.test_move(c(2, 1), c(2, 2)), m::BAD_MOVE_DIRECTION);
        assert_eq!(
            g.test_move(c(2, 1), c(2, 3)),
            m::BAD_MOVE_WOULD_COLLIDE,
            "the two-square advance is blocked on the way"
        );
        // onto its own square
        assert_eq!(g.test_move(c(0, 0), c(0, 0)), m::BAD_MOVE_WOULD_CLOBBER);
        // a sliding piece through a piece
        assert_eq!(g.test_move(c(0, 0), c(0, 5)), m::OK_MOVE_TO_EMPTY_SQUARE);
        g.place(PieceType::Knight, 1, c(0, 3));
        assert_eq!(g.test_move(c(0, 0), c(0, 5)), m::BAD_MOVE_WOULD_COLLIDE);
    }

    /// An en passant capture takes the pawn off its square for the self-check test: with the king,
    /// both pawns and an opponent's rook on one rank, the capture would open the rank, and is
    /// refused.
    #[test]
    fn an_en_passant_capture_that_opens_the_kings_rank_is_a_self_check() {
        let mut g = ChessLogic::new();
        g.place(PieceType::King, 0, c(0, 4));
        let white = g.place(PieceType::Pawn, 0, c(4, 4));
        g.place(PieceType::Pawn, 1, c(3, 6));
        g.place(PieceType::Rook, 1, c(7, 4));
        g.place(PieceType::King, 1, c(7, 7));
        g.cur_player = 1;
        assert!(
            g.do_move(c(3, 6), c(3, 4)) > 0,
            "the two-square advance past the white pawn"
        );
        assert_eq!(g.en_passant_attack_site, c(3, 5));
        assert_eq!(g.test_move(c(4, 4), c(3, 5)), m::BAD_MOVE_SELF_CHECK);
        assert_eq!(g.do_move(c(4, 4), c(3, 5)), m::BAD_MOVE_SELF_CHECK);
        assert_eq!(g.pieces[white].cur_pos, c(4, 4), "nothing moved");
        assert_eq!(g.at(c(3, 4)).map(|p| p.piece_type), Some(PieceType::Pawn));
        // without the rook the same capture is played
        let mut g = ChessLogic::new();
        g.place(PieceType::King, 0, c(0, 4));
        g.place(PieceType::Pawn, 0, c(4, 4));
        g.place(PieceType::Pawn, 1, c(3, 6));
        g.place(PieceType::King, 1, c(7, 7));
        g.cur_player = 1;
        assert!(g.do_move(c(3, 6), c(3, 4)) > 0);
        assert_eq!(
            g.do_move(c(4, 4), c(3, 5)) & m::OK_MOVE_MASK,
            m::OK_MOVE_EN_PASSANT
        );
        assert!(g.at(c(3, 4)).is_none());
    }

    /// The checkmate test answers a check by the king stepping away, or by taking or blocking the
    /// first attacker; an en passant capture of the checking pawn is not tried, so a check only
    /// that capture answers is checkmate.
    #[test]
    fn a_check_only_an_en_passant_capture_answers_is_checkmate() {
        let mut g = ChessLogic::new();
        // the white king on e4, walled in by its own pawns
        g.place(PieceType::King, 0, c(4, 3));
        for at in [
            c(3, 2),
            c(4, 2),
            c(5, 2),
            c(3, 3),
            c(5, 3),
            c(5, 4),
            c(4, 4),
        ] {
            g.place(PieceType::Pawn, 0, at);
        }
        // d7-d5 checks it; c6 guards d5
        g.place(PieceType::Pawn, 1, c(3, 6));
        g.place(PieceType::Pawn, 1, c(2, 5));
        g.place(PieceType::King, 1, c(7, 7));
        g.cur_player = 1;
        let r = g.do_move(c(3, 6), c(3, 4));
        assert_eq!(
            r,
            m::OK_MOVE_TO_EMPTY_SQUARE | m::OK_MOVE_CHECKMATE,
            "{r:#X}"
        );
        assert!(g.last_move_was_check, "set on checkmate too");
        // yet e5xd6 en passant is a legal answer
        assert_eq!(g.test_move(c(4, 4), c(3, 5)), m::OK_MOVE_EN_PASSANT);

        // the same check with d5 unguarded: the king takes the pawn
        let mut g = ChessLogic::new();
        g.place(PieceType::King, 0, c(4, 3));
        for at in [
            c(3, 2),
            c(4, 2),
            c(5, 2),
            c(3, 3),
            c(5, 3),
            c(5, 4),
            c(4, 4),
        ] {
            g.place(PieceType::Pawn, 0, at);
        }
        g.place(PieceType::Pawn, 1, c(3, 6));
        g.place(PieceType::King, 1, c(7, 7));
        g.cur_player = 1;
        assert_eq!(
            g.do_move(c(3, 6), c(3, 4)),
            m::OK_MOVE_TO_EMPTY_SQUARE | m::OK_MOVE_CHECK
        );
    }

    /// A check is answered by blocking the attacker's line or by taking the attacker; with two
    /// attackers only the king's own step answers it.
    #[test]
    fn a_check_is_answered_by_a_block_a_capture_or_a_step() {
        // the back-rank king boxed in by its pawns; a rook checks along the rank
        let boxed = || {
            let mut g = ChessLogic::new();
            g.place(PieceType::King, 1, c(7, 7));
            g.place(PieceType::Pawn, 1, c(6, 6));
            g.place(PieceType::Pawn, 1, c(7, 6));
            g.place(PieceType::King, 0, c(0, 0));
            g.place(PieceType::Rook, 0, c(0, 4));
            g
        };
        // a black bishop can block on e8
        let mut g = boxed();
        g.place(PieceType::Bishop, 1, c(2, 6));
        assert_eq!(
            g.do_move(c(0, 4), c(0, 7)),
            m::OK_MOVE_TO_EMPTY_SQUARE | m::OK_MOVE_CHECK
        );
        // a black knight can take the rook on a8
        let mut g = boxed();
        g.place(PieceType::Knight, 1, c(1, 5));
        assert_eq!(
            g.do_move(c(0, 4), c(0, 7)),
            m::OK_MOVE_TO_EMPTY_SQUARE | m::OK_MOVE_CHECK
        );
        // with a second attacker (a white bishop on the long diagonal, the g7 pawn gone and h7
        // covered), neither the block nor the capture helps
        let mut g = ChessLogic::new();
        g.place(PieceType::King, 1, c(7, 7));
        g.place(PieceType::Pawn, 1, c(7, 6));
        g.place(PieceType::Knight, 1, c(1, 5));
        g.place(PieceType::King, 0, c(0, 1));
        g.place(PieceType::Bishop, 0, c(1, 1));
        g.place(PieceType::Rook, 0, c(6, 0));
        g.place(PieceType::Rook, 0, c(0, 4));
        // the rook to a8 checks along the rank; the bishop on b2 already sees h8
        g.cur_player = 0;
        assert!(g.is_player_in_check(1));
        let r = g.do_move(c(0, 4), c(0, 7));
        assert_eq!(
            r,
            m::OK_MOVE_TO_EMPTY_SQUARE | m::OK_MOVE_CHECKMATE,
            "{r:#X}"
        );
    }

    /// Oracle: §8 — the check-result step adds the check and checkmate flags to the result, and
    /// it is computed **after** the turn has passed, so it describes the player about to move.
    #[test]
    fn check_and_checkmate_are_folded_into_the_result_for_the_next_player() {
        // A back-rank check: the black king on h8, one of its own pawns beside it, and a white rook
        // arriving on the eighth rank. The king can still drop to h7, so this is check and not mate
        // Note that g8 is *not* an escape, because it is on the rook's rank.
        let mut g = ChessLogic::new();
        g.place(PieceType::King, 1, c(7, 7));
        g.place(PieceType::Pawn, 1, c(6, 6));
        g.place(PieceType::King, 0, c(0, 0));
        g.place(PieceType::Rook, 0, c(0, 4));

        let r = g.do_move(c(0, 4), c(0, 7));
        assert_eq!(
            r,
            m::OK_MOVE_TO_EMPTY_SQUARE | m::OK_MOVE_CHECK,
            "{r:#X} — compute_check_result returns CHECK *or* CHECKMATE, never both"
        );
        assert!(g.is_player_in_check(1));
        assert!(!g.is_player_in_check_mate(1));

        // Box the last escape in — a second pawn on h7 — and the same rook move becomes mate. Note
        // that g8 must stay *empty*, or it would block the rook and there would be no check at all.
        let mut g = ChessLogic::new();
        g.place(PieceType::King, 1, c(7, 7));
        g.place(PieceType::Pawn, 1, c(6, 6));
        g.place(PieceType::Pawn, 1, c(7, 6));
        g.place(PieceType::King, 0, c(0, 0));
        g.place(PieceType::Rook, 0, c(0, 4));
        let r = g.do_move(c(0, 4), c(0, 7));
        assert_eq!(
            r,
            m::OK_MOVE_TO_EMPTY_SQUARE | m::OK_MOVE_CHECKMATE,
            "{r:#X} — 0x801 is the document's empty-square move with checkmate, with CHECK clear"
        );
        assert!(g.is_player_in_check_mate(1));
    }

    /// Oracle: §7 — a captured piece is moved to `HEAVEN` rather than deleted, so the piece array
    /// never shrinks and an index into it stays valid.
    #[test]
    fn a_captured_piece_goes_to_heaven_rather_than_being_deleted() {
        let mut g = ChessLogic::new();
        g.place(PieceType::Rook, 0, c(0, 0));
        let victim = g.place(PieceType::Pawn, 1, c(0, 5));
        g.place(PieceType::King, 0, c(4, 0));
        g.place(PieceType::King, 1, c(4, 7));
        let before = g.pieces.len();

        let r = g.do_move(c(0, 0), c(0, 5));
        assert_eq!(r & m::OK_MOVE_MASK, m::OK_MOVE_TO_OCCUPIED_SQUARE);
        assert_eq!(g.pieces.len(), before, "the array never shrinks");
        assert_eq!(g.pieces[victim].cur_pos, HEAVEN);
        assert!(!HEAVEN.is_valid());
        assert!(g.sanity_check_board());
    }

    /// Oracle: §8 — the capturing piece takes the victim's square; sending the victim to `HEAVEN`
    /// afterwards clears nothing it no longer holds, so the capturer stays on the board, blocks
    /// lines and can itself be captured.
    #[test]
    fn a_capturing_piece_stays_on_the_square_it_took() {
        let mut g = ChessLogic::new();
        let rook = g.place(PieceType::Rook, 0, c(0, 0));
        g.place(PieceType::Pawn, 1, c(0, 5));
        g.place(PieceType::King, 0, c(4, 0));
        g.place(PieceType::King, 1, c(4, 7));
        g.place(PieceType::Rook, 1, c(0, 7));

        assert!(g.do_move(c(0, 0), c(0, 5)) > 0);
        assert_eq!(g.at(c(0, 5)).map(|p| p.piece_type), Some(PieceType::Rook));
        assert_eq!(g.at(c(0, 5)).map(|p| p.player), Some(0));
        assert_eq!(g.pieces[rook].cur_pos, c(0, 5));
        // black's rook takes it back: the capture is onto an occupied square
        let r = g.do_move(c(0, 7), c(0, 5));
        assert_eq!(r & m::OK_MOVE_MASK, m::OK_MOVE_TO_OCCUPIED_SQUARE, "{r:#X}");
        assert_eq!(g.pieces[rook].cur_pos, HEAVEN);
    }

    /// `test_move` answers what `do_move` would, less the promotion, check and checkmate bits,
    /// and leaves the board as it was.
    #[test]
    fn a_trial_move_answers_like_the_real_move_and_leaves_the_board_unchanged() {
        let base = || {
            let mut g = ChessLogic::new();
            g.place(PieceType::King, 0, c(4, 0));
            g.place(PieceType::Rook, 0, c(7, 0));
            g.place(PieceType::Rook, 0, c(4, 3));
            g.place(PieceType::King, 1, c(4, 7));
            g.place(PieceType::Rook, 1, c(5, 5));
            g
        };
        let g = base();
        assert_eq!(
            g.test_move(c(4, 0), c(6, 0)),
            m::BAD_MOVE_CANT_CASTLE_THROUGH_CHECK
        );
        assert_eq!(g.test_move(c(4, 3), c(4, 7)), m::OK_MOVE_TO_OCCUPIED_SQUARE);
        assert_eq!(g.test_move(c(4, 3), c(0, 4)), m::BAD_MOVE_DIRECTION);
        assert_eq!(g.test_move(c(4, 7), c(4, 6)), m::BAD_MOVE_NOT_YOURS);
        assert_eq!(
            g.at(c(7, 0)).map(|p| p.piece_type),
            Some(PieceType::Rook),
            "nothing moved"
        );
        for (from, to) in [
            (c(4, 0), c(6, 0)),
            (c(4, 3), c(4, 6)),
            (c(4, 3), c(0, 4)),
            (c(7, 0), c(7, 7)),
        ] {
            let t = base().test_move(from, to);
            let d = base().do_move(from, to);
            let flags = m::OK_MOVE_CHECK | m::OK_MOVE_CHECKMATE | m::OK_MOVE_PROMOTION;
            assert_eq!(t, if d > 0 { d & !flags } else { d }, "{from:?} -> {to:?}");
        }
    }

    /// Oracle: §7's coordinate validity test.
    #[test]
    fn coordinates_are_valid_only_inside_the_eight_by_eight_board() {
        assert!(c(0, 0).is_valid());
        assert!(c(7, 7).is_valid());
        assert!(!c(-1, 0).is_valid());
        assert!(!c(8, 0).is_valid());
        assert!(!c(0, 8).is_valid());
    }
}
