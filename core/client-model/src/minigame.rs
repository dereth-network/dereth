//! The chess window and board grid — their state machine and the eleven
//! messages it is the only speaker of.
//!
//! This file is the production caller of [`crate::chess::ChessLogic`], the board's rules engine.
//!
//! # What raises the board
//!
//! A window like this can be raised from the wrong message or from nowhere at all, so the chain
//! is spelled out, every hop checked:
//!
//! ```text
//! double-click a chessboard in the world
//!   use the selected item
//!     r = determine its use result
//!           the item has TYPE_GAMEPIECE 0x80000000 (bit 31)
//!             -> 7, and only for an object NOT owned by you
//!     1 < r < 8  ->  mark the player as using the item and **return**
//!   the use-result dispatch, case 7:
//!       dispatch the minigame begin-game notice for `id`
//!   the minigame window handles that notice:
//!       make the window visible
//!       attempt to join the game
//!   the join attempt's free-game arm:
//!               current game = gameId
//!               state = AttemptingToJoinGame
//!       send join event (gameId, -1)     ; 0x0269 on the Weenie queue
//!       enable the resign button
//!       append "Attempting to join game, please wait...\n" to the information scroll
//! ```
//!
//! **No network round trip raises this window.** Unlike the slumlord (`0x0036` out, `0x021D`
//! back) the board is raised locally by the use itself, and the *first* thing the raised window
//! does is send `0x0269`. So "what makes retail raise the chess board" and "what the client must
//! send for a game to start" are one answer, and `0x0269 Game_Join { game_id = the board's guid,
//! which_team = -1 }` is it. ACE's chess-join action likewise reads
//! a board guid followed by a signed colour value that is expected to be `-1`, then
//! handles the join for that board.
//!
//! **The use side is in this workspace.** `dereth_client_model::inventory::use_object`
//! returns [`crate::inventory::use_object::UseResult::BeginGame`] and emits
//! [`crate::Notice::BeginGame`]. This module is that notice's receiver; a mere counter of it
//! would leave the board unraised at the very last hop.
//!
//! # Information text uses the chat scroll, not a label
//!
//! The info-text operation makes three calls: it obtains the UI system, converts the narrow string
//! to a wide string, and adds it visibly to scroll type 0. There is **no
//! info-text element in the panel**: initialization binds three buttons and the game-board grid
//! constructor binds one list box. That is the whole window. Every line this file composes
//! therefore goes to [`crate::scroll`], which
//! is why the round trip is observable without drawing anything.
//! The move-notification path uses the same narrow-string scroll path.
//!
//! # A measured retail quirk: undo restores only the board array
//!
//! Undoing a move copies the previous board back over the current board array and redraws it.
//! It restores **neither** each piece's current position and moved flag **nor** the current player; the move
//! logic has already changed all three. So a move
//! the *client* accepted and the *server* rejected leaves the local logic desynchronised from the
//! drawn board. That is retail's behaviour, it is transcribed here unchanged, and
//! `GameBoard::undo_moves` says so at its definition rather than quietly improving on it.
//!
//! # Shard safety
//!
//! **No datagram leaves this process.** Every sender here puts a typed [`crate::Request`] in a
//! [`crate::RequestSink`]; framing and the wire belong to the session.

use crate::chess::{move_result as mr, ChessLogic, Coord, GameState, PieceType};
use crate::{Request, RequestSink};
use dereth_primitives::ObjectId;

/// The join event's second argument, pushed as `-1` (`0xFFFFFFFF`).
///
/// ACE's chess-join action reads it and ignores it as an expected `-1` value;
/// the client has no other value for it, because joining is the event's only caller.
pub const JOIN_ANY_TEAM: u32 = 0xFFFF_FFFF;

/// `Game_GameOver`'s winning-team value when the game ended without a result to announce.
///
/// The game-over handler takes a separate arm for it: it raises the end-game notice
/// (which hides the window) and composes **no** verdict line, leaving only the default sentence.
pub const GAME_OVER_ABORTED: i32 = -2;

/// `Game_GameOver`'s winning-team value for a draw, and the join response's team for a refusal.
pub const REFUSED_OR_STALEMATE: i32 = -1;

// ---------------------------------------------------------------------------------------------
// The sixteen user-visible literals, as the client spells them
// ---------------------------------------------------------------------------------------------

/// Shared game-over and try-to-quit text.
pub const DEFAULT_STATE: &str = "To join a game, select and use the game board.\n";
/// Try-to-join text.
pub const ATTEMPTING_TO_JOIN: &str = "Attempting to join game, please wait...\n";
/// The join-game response's `team == -1` arm.
pub const COULD_NOT_JOIN: &str = "You could not join that game.\n";
/// The join-game response's accepted arm.
pub const JOINED: &str = "You have joined the game, waiting for all players to be ready.\n";
/// The start-game notice when the named team equals the player's team.
pub const BEGUN_YOUR_TURN: &str = "The game has begun, it is your turn to move.\n";
/// Start-game text used by the remaining case.
pub const BEGUN_THEIR_TURN: &str = "The game has begun, waiting for your opponent to move.\n";
/// The local-move refusal tail and opponent-move result tail.
pub const YOUR_TURN: &str = "It is your turn to move.\n";
/// The move response's accepted arm.
pub const WAITING_FOR_OPPONENT: &str = "Waiting for your opponent to move.\n";
/// Opponent-offers-stalemate text when `on == 0`.
pub const STALEMATE_RETRACTED: &str = "Your opponent has retracted their offer of stalemate.\n";
/// Opponent-offers-stalemate text when `on != 0`.
pub const STALEMATE_OFFERED: &str =
    "Your opponent has declared a stalemate.  To agree, press your stalemate button.\n";
/// The join attempt when another game is current. Channel `0x1A`, not the
/// scroll's default: this one goes out as a `StringInfo` display notice.
pub const ALREADY_ANOTHER_GAME: &str = "You are already playing another game.";
/// The join attempt when this game is already current. Channel `0x1A`.
pub const ALREADY_THIS_GAME: &str = "You are already playing this game.";
/// The move-result line between the refusal and [`YOUR_TURN`].
pub const TRY_AGAIN: &str = ", try again.  ";
/// The move-result `-3` arm, which returns early and appends nothing.
pub const NOT_YOUR_TURN: &str = "Its not your turn, please wait for your opponents move.\n";
/// Local move-result text selected by `result & 0x400`.
pub const OPPONENT_IN_CHECK: &str = "  Your opponent is in Check.\n";
/// Accepted local-move prefix.
pub const MOVE_IN_PROGRESS: &str = "Move in progress.";
/// Local move-result text selected by `result & 0x800`.
pub const YOU_CHECKMATED: &str = "You have checkmated your opponent!\n";
/// Opponent move-result text selected by `result & 0x400`.
pub const YOU_ARE_IN_CHECK: &str = "You are in check!  ";
/// Opponent move-result text selected by `result & 0x800`.
pub const YOU_WERE_CHECKMATED: &str = "You have been checkmated!\n";
/// The game-over notice when the winner is not your team.
pub const DEFEATED: &str = "You have been defeated!  ";
/// The game-over notice when the winning team is the player's team.
pub const VICTORIOUS: &str = "You are victorious!  ";
/// Game-over text used when the winning-team value is `-1`.
pub const ENDED_IN_STALEMATE: &str = "The game has ended in a stalemate.  ";
/// The resign dialog text. It is wide text and belongs to channel `0x1A`'s dialog.
pub const RESIGN_PROMPT: &str = "If a game is in progress, resigning will be recorded as your \
    loss. Are you sure you want to resign?";
/// Both the Pass and Stalemate button arms when there is no game.
/// Channel `0x1A`.
pub const NOT_PLAYING: &str = "You are not currently playing a game.";

/// Display string information on channel `0x1A`, the channel the join flow's two refusals
/// and the buttons' *"not currently playing"* response use.
pub const NOTICE_CHANNEL: u32 = 0x1A;

/// The client's refusal table — the `switch`.
///
/// `-3` (`BAD_MOVE_NOT_YOUR_TURN`) is **not** here: it takes an early return with
/// [`NOT_YOUR_TURN`] and no suffix. `-1`, `-2` and `-111` fall through to the `default`, which is
/// *"That move is invalid"* — so the table below is the eleven the switch names plus that default,
/// and it is a total function over `i32`.
#[must_use]
pub fn move_refusal(result: i32) -> &'static str {
    match result {
        mr::BAD_MOVE_CANT_CASTLE_AFTER_MOVING => "You cannot castle after moving the King or Rook",
        mr::BAD_MOVE_CANT_CASTLE_THROUGH_CHECK => "You cannot castle through check",
        mr::BAD_MOVE_CANT_CASTLE_OUT_OF_CHECK => "You cannot castle out of check",
        mr::BAD_MOVE_WOULD_COLLIDE => "You can only move through empty squares",
        mr::BAD_MOVE_SELF_CHECK => "That move would put you in check",
        mr::BAD_MOVE_WOULD_CLOBBER => "You cannot attack your own pieces",
        mr::BAD_MOVE_DESTINATION => "You cannot move off the board",
        mr::BAD_MOVE_NOT_YOURS => "The selected piece is not yours",
        mr::BAD_MOVE_NO_PIECE => "You tried to move an empty square",
        mr::BAD_MOVE_DISTANCE => "The selected piece cannot move that far",
        mr::BAD_MOVE_DIRECTION => "The selected piece cannot move that direction",
        _ => "That move is invalid",
    }
}

/// Behavior: composed rather than printed.
///
/// ```text
/// result < 1 :  -3 -> NOT_YOUR_TURN, and nothing else
///               else  move_refusal(result) + ", try again.  " + "It is your turn to move.\n"
/// result > 0 :  & 0x800 -> "You have checkmated your opponent!\n"
///               else "Move in progress." + (& 0x400 ? OPPONENT_IN_CHECK : "\n")
/// ```
#[must_use]
pub fn show_move_result(result: i32) -> String {
    if result < 1 {
        if result == mr::BAD_MOVE_NOT_YOUR_TURN {
            return NOT_YOUR_TURN.to_owned();
        }
        return format!("{}{TRY_AGAIN}{YOUR_TURN}", move_refusal(result));
    }
    if result & mr::OK_MOVE_CHECKMATE != 0 {
        return YOU_CHECKMATED.to_owned();
    }
    let tail = if result & mr::OK_MOVE_CHECK != 0 {
        OPPONENT_IN_CHECK
    } else {
        "\n"
    };
    format!("{MOVE_IN_PROGRESS}{tail}")
}

/// Format the opponent's move result for the minigame UI.
///
/// Note the asymmetry with [`show_move_result`]: checkmate returns early here too, but the
/// non-mate arm has **no** *"Move in progress."* prefix and always ends in [`YOUR_TURN`].
#[must_use]
pub fn show_opponent_move_result(result: i32) -> String {
    if result & mr::OK_MOVE_CHECKMATE != 0 {
        return YOU_WERE_CHECKMATED.to_owned();
    }
    let head = if result & mr::OK_MOVE_CHECK != 0 {
        YOU_ARE_IN_CHECK
    } else {
        ""
    };
    format!("{head}{YOUR_TURN}")
}

// ---------------------------------------------------------------------------------------------
// Board-grid behavior — the half that is not drawing
// ---------------------------------------------------------------------------------------------

/// The client's 32-entry table, `(x, y, side, type)`, in its own order.
///
/// The client fills it with 128 integer stores; the slots are indexed, and
/// the loop walks them four at a time. The back rank
/// is `R N B Q K B N R`, which with [`PieceType`]'s `Rook = 2 … King = 6` is the `2 3 4 5 6 4 3 2`
#[allow(clippy::cast_possible_truncation)] // every index is below 8
/// the table literally holds.
pub const RESET_LAYOUT: [(i32, i32, i32, PieceType); 32] = {
    use PieceType as P;
    const BACK: [P; 8] = [
        P::Rook,
        P::Knight,
        P::Bishop,
        P::Queen,
        P::King,
        P::Bishop,
        P::Knight,
        P::Rook,
    ];
    let mut out = [(0, 0, 0, P::Empty); 32];
    let mut i = 0;
    while i < 8 {
        out[i] = (i as i32, 0, 0, BACK[i]);
        out[8 + i] = (i as i32, 1, 0, P::Pawn);
        out[16 + i] = (i as i32, 6, 1, P::Pawn);
        out[24 + i] = (i as i32, 7, 1, BACK[i]);
        i += 1;
    }
    out
};

/// The board grid (type `0x274`) minus the twelve `DataId`s and list box — the logic half.
///
/// The drawing half is not here: it needs the game's texture array, which this workspace's
/// `dereth-ui` does not have, and the panel renderer, which reads layout
/// property `0x10000021 UI_MiniGame_PieceIconArray` off the list box.
#[derive(Debug, Clone, Default)]
pub struct GameBoard {
    /// The board's [`crate::chess::ChessLogic`].
    pub logic: ChessLogic,
    /// The 8×8 previous-board snapshot, retained so that
    /// a server refusal can put the board back.
    previous: Option<[[Option<usize>; 8]; 8]>,
    /// The selected coordinate. `(-1, -1)` is "nothing selected" on the wire.
    pub selected: Option<Coord>,
    /// How many redraws would have run. The panel redraws from this count.
    pub draws: u32,
}

impl GameBoard {
    /// Behavior: clear all pieces, create the 32 starting pieces, then
    /// redraw.
    pub fn reset(&mut self) {
        self.logic = ChessLogic::new();
        self.previous = None;
        self.selected = None;
        for (x, y, side, ty) in RESET_LAYOUT {
            self.logic.place(ty, side, Coord { x, y });
        }
        self.draws += 1;
    }

    /// Prepare a new board move by copying all 64 current-board pointers to the previous board.
    pub fn prepare_new_move(&mut self) {
        self.previous = Some(self.logic.board_snapshot());
    }

    /// Behavior: restore the previous board array, then redraw.
    ///
    /// **It restores the board array and nothing else.** Each piece's current position and
    /// moved flag, and the logic's current-player field keep the values the accepted move gave them.
    /// That is measured, not assumed: the whole body is one 8-iteration copy loop and a redraw
    /// call, with no other store. Transcribed as-is — see this module's header.
    pub fn undo_moves(&mut self) {
        if let Some(p) = self.previous {
            self.logic.restore_board_snapshot(p);
        }
        self.draws += 1;
    }

    /// Behavior: free whatever the move captured, clear the
    /// snapshot, drop the selection, and redraw.
    ///
    /// The client's loop deletes the previous-board entry when the *current* board holds a
    /// **different** non-null piece on that square; with indices rather than pointers the same
    /// test is "the square changed occupant", and the captured piece has already been moved to
    /// [`crate::chess::HEAVEN`] by the move. Counting it is what makes the capture observable.
    pub fn commit_moves(&mut self) -> u32 {
        let mut captured = 0;
        if let Some(prev) = self.previous.take() {
            let now = self.logic.board_snapshot();
            for x in 0..8 {
                for y in 0..8 {
                    match (prev[x][y], now[x][y]) {
                        (Some(a), Some(b)) if a != b => captured += 1,
                        _ => {}
                    }
                }
            }
        }
        self.selected = None;
        self.draws += 1;
        captured
    }

    /// Behavior: the pawn that reached the far rank becomes a
    /// queen, and the check result for the promoting side's opponent is computed again over the
    /// new board and OR-ed into the move's result.
    ///
    /// The client deletes the pawn, creates a queen in its place and recomputes
    /// the check result; here the piece is mutated in place, which is the same board. The move's
    /// own check bits (computed with the pawn on the last rank) are kept, so a pawn that uncovers
    /// a check and becomes a queen that mates answers both `OK_MOVE_CHECK` and `OK_MOVE_CHECKMATE`.
    pub fn do_promotion(&mut self, at: Coord, result: i32) -> i32 {
        let Some(owner) = self.logic.at(at).map(|p| p.player) else {
            return result;
        };
        self.logic.promote_to_queen(at);
        result | self.logic.compute_check_result(i32::from(owner == 0))
    }

    /// The board square a list-box cell index stands for, given whose side you are on.
    ///
    /// Convert with `x = num & 7`, `y = num >> 3` (both with
    /// the sign fix-ups a signed index needs), then **team 0 flips `y`** and any other
    /// team flips `x`. The reverse conversion uses the same map inverted, which is why a board looks
    /// right way up to both players.
    #[must_use]
    pub fn coord_of_cell(cell: usize, team: i32) -> Option<Coord> {
        if cell >= 64 {
            return None;
        }
        #[allow(clippy::cast_possible_truncation)] // cell < 64
        let (mut x, mut y) = ((cell % 8) as i32, (cell / 8) as i32);
        if team == 0 {
            y = 7 - y;
        } else {
            x = 7 - x;
        }
        let c = Coord { x, y };
        c.is_valid().then_some(c)
    }

    /// [`Self::coord_of_cell`] inverted — the client's index arithmetic.
    #[must_use]
    pub fn cell_of_coord(c: Coord, team: i32) -> Option<usize> {
        if !c.is_valid() {
            return None;
        }
        let cell = if team == 0 {
            (7 - c.y) * 8 + c.x
        } else {
            c.y * 8 + (7 - c.x)
        };
        usize::try_from(cell).ok().filter(|n| *n < 64)
    }
}

// ---------------------------------------------------------------------------------------------
// Minigame window behavior
// ---------------------------------------------------------------------------------------------

/// The client's three button ids: `0x10000175`, `0x10000176` and `0x10000177`, tested in
/// that order.
pub mod button {
    /// Resign — raises the confirmation dialog, whose Yes sends `0x026A Game_Quit`.
    pub const RESIGN: u32 = 0x1000_0175;
    /// Pass — `0x026D Game_MovePass`, with no confirmation.
    pub const PASS: u32 = 0x1000_0176;
    /// Stalemate — toggles the local offer flag and sends `0x026E Game_Stalemate(on)`.
    pub const STALEMATE: u32 = 0x1000_0177;
}

/// The state setter's argument for the button's normal state: the same `1` the salvage panel uses
/// for its enabled Salvage button.
pub const BUTTON_STATE_NORMAL: u32 = 1;

/// What one gesture or one message did, so a caller can assert on it without a UI tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MiniGameEffect {
    /// The visibility value applied by this action, if any.
    pub set_visible: Option<bool>,
    /// Whether this action redrew the board.
    pub redrew: bool,
}

/// The minigame window (type `0x620`) — nine notice handlers, three buttons and the board.
#[derive(Debug, Clone, Default)]
pub struct MiniGame {
    /// The game board.
    pub board: GameBoard,
    /// The player's team. `-1` until `Game_JoinGameResponse` names one.
    pub team: i32,
    /// The current game's **chessboard-object guid**, not an index.
    pub current_game: ObjectId,
    /// The current game state.
    pub state: GameState,
    /// Whether this player is offering stalemate.
    pub stalemate: bool,
    /// The resign-dialog context. Non-zero means a resign dialog is already up and the
    /// Resign button is a no-op.
    pub resign_dialog: u32,
    /// The last visibility value. The panel mirrors it.
    pub visible: bool,
    /// The last state written to the Resign button.
    pub resign_button_state: u32,
    /// The last state written to the Stalemate button.
    pub stalemate_button_state: u32,
    /// What the info-text and local-feedback paths have composed
    /// and not yet been drained of, as `(chat type, text)` in the order they were written.
    ///
    /// A queue rather than a `NoticeSink` argument because the six inbound handlers are called
    /// from `Hud::ui_event`, which holds `&mut World` and has no sink.
    /// The world drain path moves them into [`crate::scroll::Scroll`], matching retail's
    /// scroll append.
    pub lines: Vec<(u32, String)>,
    /// A Resign click just created a dialog context, and the host owes this window
    /// a callback dialog with [`RESIGN_PROMPT`]. Cleared by the answer.
    pub resign_prompt_pending: bool,

    // ---- counters, three-state on purpose: "never driven" must not read as "drove and did
    // ---- nothing". ---------------------------------------------------------------------------
    /// Begin-game notice arrivals.
    pub begin_games: u32,
    /// Join attempts that sent the join event.
    pub joins_sent: u32,
    /// Join attempts refused with one of the two local strings.
    pub joins_refused: u32,
    /// `Game_JoinGameResponse` arrivals this window accepted.
    pub join_responses: u32,
    /// `Game_StartGame` arrivals this window accepted.
    pub starts: u32,
    /// Sent moves — one `0x026B` each.
    pub moves_sent: u32,
    /// Local refusals: the chess move returned `< 1` and nothing went out.
    pub moves_refused_locally: u32,
    /// `Game_MoveResponse` arrivals that committed.
    pub moves_committed: u32,
    /// `Game_MoveResponse` arrivals that undid.
    pub moves_undone: u32,
    /// `Game_OpponentTurn` arrivals this window replayed.
    pub opponent_moves: u32,
    /// Pieces freed while committing moves, summed.
    pub captures: u32,
    /// Promotions either side made.
    pub promotions: u32,
    /// `Game_OpponentStalemateState` arrivals.
    pub stalemate_offers: u32,
    /// `Game_GameOver` arrivals this window accepted.
    pub game_overs: u32,
    /// Messages refused because `game_id` was not the current game, or the state was wrong.
    /// Non-zero here with a zero sibling counter is the guard doing its job, not a defect.
    pub rejected_by_guard: u32,
}

impl MiniGame {
    /// The client's three initial visibility settings.
    ///
    /// Resign and Stalemate start visible; **Pass starts hidden**. All three go
    /// through the visibility setter, not the button-state setter.
    pub const PASS_BUTTON_STARTS_VISIBLE: bool = false;

    #[must_use]
    pub fn new() -> Self {
        Self {
            team: -1,
            ..Self::default()
        }
    }

    /// Restore the default state in its original store order.
    ///
    /// ```text
    ///           current game = 0
    ///           state = NotPlaying
    ///           team = -1
    ///           stalemate offer = false
    ///           append information text
    ///           stalemate button state = normal
    ///           board: current player = 0, last-move-was-check = false,
    ///           both en-passant coordinates and selection = (-1,-1)
    /// ```
    ///
    /// Note what it does **not** do: it does not clear the pieces. The board keeps whatever
    /// position the game ended in until the next join response resets it.
    pub fn goto_default_state(&mut self, text: &str) {
        self.current_game = ObjectId(0);
        self.state = GameState::NotPlaying;
        self.team = -1;
        self.stalemate = false;
        self.set_info_text(text);
        self.stalemate_button_state = BUTTON_STATE_NORMAL;
        self.board.logic.cur_player = 0;
        self.board.logic.last_move_was_check = false;
        self.board.logic.en_passant_attack_site = crate::chess::HEAVEN;
        self.board.logic.en_passant_victim_pos = crate::chess::HEAVEN;
        self.board.selected = None;
    }

    /// Behavior: append `text` to the visible scroll with chat type 0.
    fn set_info_text(&mut self, text: &str) {
        self.lines
            .push((crate::chat::text_type::DEFAULT, text.to_owned()));
    }

    /// Display `text` on channel `0x1A`, the *other* stream this window uses.
    fn display_string_info(&mut self, text: &str) {
        self.lines.push((NOTICE_CHANNEL, text.to_owned()));
    }

    /// Behavior: make the window visible, then attempt to join.
    pub fn on_begin_game(&mut self, game: ObjectId, req: &mut dyn RequestSink) -> MiniGameEffect {
        self.begin_games += 1;
        self.visible = true;
        self.try_to_join_game(game, req);
        MiniGameEffect {
            set_visible: Some(true),
            redrew: false,
        }
    }

    /// Behavior: hide the window, and do nothing else.
    pub fn on_end_game(&mut self) -> MiniGameEffect {
        self.visible = false;
        MiniGameEffect {
            set_visible: Some(false),
            redrew: false,
        }
    }

    /// Attempt to join the selected minigame.
    ///
    /// Three arms, and only the middle one sends: already in *this* game, already in *another*
    /// game, or free. The two refusals go out on channel `0x1A` as a `StringInfo`, which is a
    /// different stream from [`Self::set_info_text`]'s scroll line — both end in the chat window,
    /// and the distinction is preserved because retail preserves it.
    pub fn try_to_join_game(&mut self, game: ObjectId, req: &mut dyn RequestSink) -> bool {
        if self.current_game == game {
            self.joins_refused += 1;
            self.display_string_info(ALREADY_THIS_GAME);
            return false;
        }
        if self.current_game != ObjectId(0) {
            self.joins_refused += 1;
            self.display_string_info(ALREADY_ANOTHER_GAME);
            return false;
        }
        self.current_game = game;
        self.state = GameState::AttemptingToJoinGame;
        req.send(Request::GameJoin(dereth_protocol::trade::GameJoin {
            game_id: game.0,
            which_team: JOIN_ANY_TEAM,
        }));
        self.joins_sent += 1;
        self.resign_button_state = BUTTON_STATE_NORMAL;
        self.set_info_text(ATTEMPTING_TO_JOIN);
        true
    }

    /// Receive a minigame join response.
    ///
    /// Guarded on `game == current_game && state == AttemptingToJoinGame`, so a response to a
    /// join this window did not make is dropped on the floor with no text at all.
    pub fn recv_join_game_response(&mut self, game: ObjectId, team: i32) -> bool {
        if game != self.current_game || self.state != GameState::AttemptingToJoinGame {
            self.rejected_by_guard += 1;
            return false;
        }
        self.join_responses += 1;
        if team == REFUSED_OR_STALEMATE {
            self.set_info_text(COULD_NOT_JOIN);
            self.state = GameState::NotPlaying;
            self.current_game = ObjectId(0);
            return true;
        }
        self.set_info_text(JOINED);
        self.state = GameState::WaitingForGameStart;
        self.team = team;
        self.board.reset();
        self.stalemate = false;
        true
    }

    /// Receive the minigame start notice.
    pub fn recv_start_game(&mut self, game: ObjectId, team: i32) -> bool {
        if game != self.current_game || self.state != GameState::WaitingForGameStart {
            self.rejected_by_guard += 1;
            return false;
        }
        self.starts += 1;
        if team == self.team {
            self.state = GameState::PlayingMyTurn;
            self.set_info_text(BEGUN_YOUR_TURN);
        } else {
            self.state = GameState::PlayingNotMyTurn;
            self.set_info_text(BEGUN_THEIR_TURN);
        }
        true
    }

    /// Receive the local move response.
    ///
    /// The server's code is a `ChessMoveResult` and, as `dereth_protocol`'s own note says, the shard's
    /// codes only ever reject — a success is announced by `Game_OpponentTurn` to the other player,
    /// not by a positive `0x0283`. The positive arm is still transcribed because retail has it.
    pub fn recv_move_response(&mut self, game: ObjectId, result: i32) -> bool {
        if game != self.current_game || self.state != GameState::PlayingTryingToMove {
            self.rejected_by_guard += 1;
            return false;
        }
        if result > 0 {
            self.captures += self.board.commit_moves();
            self.moves_committed += 1;
            self.state = GameState::PlayingNotMyTurn;
            self.set_info_text(WAITING_FOR_OPPONENT);
        } else {
            self.board.undo_moves();
            self.moves_undone += 1;
            self.state = GameState::PlayingMyTurn;
            self.set_info_text(&show_move_result(result));
        }
        true
    }

    /// Receive the opponent-turn notice and apply the opponent's board move.
    ///
    /// An opponent move snapshots the board, applies the move and **commits unconditionally** — the
    /// opponent's move is not speculative, so there is nothing to undo — and only then forks on
    /// the result. A move the local rules engine refuses therefore leaves the board *committed to
    /// whatever the move did* and prints nothing, which is retail's behaviour and is why
    /// [`Self::opponent_moves`] counts arrivals and not successes.
    pub fn recv_opponent_turn(
        &mut self,
        game: ObjectId,
        move_data: &dereth_protocol::trade::GameMoveData,
    ) -> bool {
        if game != self.current_game || self.state != GameState::PlayingNotMyTurn {
            self.rejected_by_guard += 1;
            return false;
        }
        self.opponent_moves += 1;
        let (from, to) = match (move_data.from, move_data.to) {
            (Some(f), Some(t)) => (coord(f), coord(t)),
            // An absent coordinate pair leaves the destination coordinates at their constructor
            // values for a `Pass`, `Resign` or `Stalemate` move type, and the opponent-move handler reads
            // them regardless. The signed coordinate-validity check in `Move` is what refuses it.
            _ => (crate::chess::HEAVEN, crate::chess::HEAVEN),
        };
        self.board.prepare_new_move();
        let mut r = self.board.logic.do_move(from, to);
        self.captures += self.board.commit_moves();
        if r > 0 {
            if r & mr::OK_MOVE_PROMOTION != 0 {
                r = self.board.do_promotion(to, r);
                self.promotions += 1;
            }
            self.set_info_text(&show_opponent_move_result(r));
            self.board.draws += 1;
        }
        self.state = GameState::PlayingMyTurn;
        true
    }

    /// Receive the opponent's stalemate offer.
    ///
    /// Guarded on the game id **only** — there is no state test — and it writes nothing but a
    /// scroll line. In particular it does **not** touch the local stalemate flag: your own offer is
    /// yours, and agreeing means pressing your own button.
    pub fn recv_opponent_stalemate(&mut self, game: ObjectId, on: bool) -> bool {
        if game != self.current_game {
            self.rejected_by_guard += 1;
            return false;
        }
        self.stalemate_offers += 1;
        self.set_info_text(if on {
            STALEMATE_OFFERED
        } else {
            STALEMATE_RETRACTED
        });
        true
    }

    /// Receive the game-over notice.
    ///
    /// Guarded on the game id only. A winning team of `-2` raises `EndGame` (which hides the
    /// window) and composes **no** verdict; every other value composes one and then appends
    /// [`DEFAULT_STATE`], and all of them end in [`Self::goto_default_state`].
    pub fn recv_game_over(&mut self, game: ObjectId, team_winner: i32) -> bool {
        if game != self.current_game {
            self.rejected_by_guard += 1;
            return false;
        }
        self.game_overs += 1;
        let verdict = if team_winner == GAME_OVER_ABORTED {
            self.on_end_game();
            ""
        } else if team_winner == REFUSED_OR_STALEMATE {
            ENDED_IN_STALEMATE
        } else if team_winner == self.team {
            VICTORIOUS
        } else {
            DEFEATED
        };
        self.goto_default_state(&format!("{verdict}{DEFAULT_STATE}"));
        true
    }

    /// Behavior: the resign dialog's callback.
    ///
    /// The dialog reports its answer through a try-to-quit notice, which this handler consumes.
    /// **The quit event goes out on the bare `yes`**, before the nonzero-current-game test, so
    /// resigning with no game in
    /// progress still sends `0x026A` and simply skips the reset.
    pub fn recv_try_to_quit_game(&mut self, confirmed: bool, req: &mut dyn RequestSink) {
        self.resign_dialog = 0;
        self.resign_prompt_pending = false;
        if !confirmed {
            return;
        }
        req.send(Request::GameQuit(dereth_protocol::trade::GameQuit));
        if self.current_game != ObjectId(0) {
            self.goto_default_state(DEFAULT_STATE);
            self.on_end_game();
        }
    }

    /// Forget a resign question whose current UI was destroyed before it produced an answer.
    ///
    /// Retail gets this reset by destroying the minigame window together with its framework; this
    /// rebuild keeps the game model across UI generations, so the dialog host mirrors the field
    /// reset explicitly. No try-to-quit notice is raised and no packet is sent.
    pub fn cancel_resign_dialog(&mut self) {
        self.resign_dialog = 0;
        self.resign_prompt_pending = false;
    }

    /// Handle UI element message 1, a button click.
    ///
    /// Returns whether the id was one of this window's three.
    pub fn on_button(&mut self, id: u32, req: &mut dyn RequestSink) -> bool {
        match id {
            button::RESIGN => {
                // a dialog already up swallows the click.
                if self.resign_dialog != 0 {
                    return true;
                }
                // Create the current-UI callback dialog for quitting the minigame.
                // The dialog factory returns the context id. `1` reserves the slot synchronously;
                // the current-UI
                // dialog host replaces it with the factory's actual context before drawing.
                self.resign_dialog = 1;
                self.resign_prompt_pending = true;
                true
            }
            button::PASS => {
                if self.current_game == ObjectId(0) {
                    self.display_string_info(NOT_PLAYING);
                    return true;
                }
                req.send(Request::GameMovePass(dereth_protocol::trade::GameMovePass));
                true
            }
            button::STALEMATE => {
                if self.current_game == ObjectId(0) {
                    self.display_string_info(NOT_PLAYING);
                    return true;
                }
                // Toggle the local stalemate flag, then send the **new** value as the
                // argument. The button is a toggle and the message carries the toggled state.
                self.stalemate = !self.stalemate;
                req.send(Request::GameStalemate(
                    dereth_protocol::trade::GameStalemate {
                        on: i32::from(self.stalemate),
                    },
                ));
                true
            }
            _ => false,
        }
    }

    /// Handle a board-cell press; `cell` is the incoming list-box index.
    ///
    /// ```text
    /// guard: element id == 0x10000174, first param == 7 (left press), coord valid,
    ///        game state == PlayingMyTurn
    /// nothing selected     -> select, if the square holds one of YOUR pieces
    /// same square again    -> deselect
    /// another square       -> prepare; r = validate and apply move(selected, here)
    ///                         r > 0  -> promotion if flagged; send move (0x026B); selection := here;
    ///                                   report move result r
    ///                         r < 1  -> selection cleared; report result r; nothing sent
    /// ```
    ///
    /// The local engine is the gate: a refused move **never reaches the wire**, which is the whole
    /// reason the chess engine is in the client at all.
    pub fn on_board_press(&mut self, cell: usize, req: &mut dyn RequestSink) -> bool {
        if self.state != GameState::PlayingMyTurn {
            return false;
        }
        let Some(here) = GameBoard::coord_of_cell(cell, self.team) else {
            return false;
        };
        let Some(from) = self.board.selected else {
            // select only a piece that exists and is yours.
            if self
                .board
                .logic
                .at(here)
                .is_some_and(|p| p.player == self.team)
            {
                self.board.selected = Some(here);
                self.board.draws += 1;
                return true;
            }
            return false;
        };
        if from == here {
            self.board.selected = None;
            self.board.draws += 1;
            return true;
        }
        self.board.prepare_new_move();
        let mut r = self.board.logic.do_move(from, here);
        if r > 0 {
            if r & mr::OK_MOVE_PROMOTION != 0 {
                r = self.board.do_promotion(here, r);
                self.promotions += 1;
            }
            // Send the move event `(x0, y0, x1, y1)`, then
            // transition to the PlayingTryingToMove state.
            req.send(Request::GameMove(dereth_protocol::trade::GameMove {
                x_from: from.x,
                y_from: from.y,
                x_to: here.x,
                y_to: here.y,
            }));
            self.moves_sent += 1;
            self.state = GameState::PlayingTryingToMove;
            self.board.selected = Some(here);
        } else {
            self.board.selected = None;
            self.moves_refused_locally += 1;
        }
        self.set_info_text(&show_move_result(r));
        self.board.draws += 1;
        true
    }
}

fn coord((x, y): (u32, u32)) -> Coord {
    // The message decoder reads the four coordinates as `ulong`; board coordinates are signed, and that
    // range check is what rejects anything outside 0..=7.
    Coord {
        x: i32::try_from(x).unwrap_or(-1),
        y: i32::try_from(y).unwrap_or(-1),
    }
}

// ---------------------------------------------------------------------------------------------
// The `World` seam
// ---------------------------------------------------------------------------------------------

impl crate::world::World {
    /// Receive the begin-game notice for `id`.
    ///
    /// **This is the hop that raises the board.** `use_object`'s arm 7 emits
    /// `Notice::BeginGame`; without this receiver nothing but a counter would read it.
    pub fn begin_game(&mut self, board: ObjectId, req: &mut dyn RequestSink) -> MiniGameEffect {
        self.minigame.on_begin_game(board, req)
    }

    /// Move whatever [`MiniGame::lines`] has composed into [`crate::scroll::Scroll`].
    ///
    /// This is the minigame information-text tail: adding `text` to the requested scroll channel,
    /// deferred by exactly one call so that
    /// the handlers can run with `&mut self.minigame` alone. Returns how many lines it routed, so
    /// a caller counts rather than assumes.
    pub fn drain_minigame_text(&mut self) -> usize {
        let lines = std::mem::take(&mut self.minigame.lines);
        for (chat_type, text) in &lines {
            self.scroll.add_feedback_to_scroll(
                text,
                *chat_type,
                true,
                0,
                dereth_client_contract::feedback::Feedback::LOCAL,
            );
        }
        lines.len()
    }

    /// Handle join-game response `0x0281`.
    pub fn recv_join_game_response(
        &mut self,
        m: &dereth_protocol::trade::GameJoinGameResponse,
    ) -> bool {
        self.minigame
            .recv_join_game_response(ObjectId(m.game_id), m.team)
    }

    /// Handle start-game opcode `0x0282`.
    pub fn recv_start_game(&mut self, m: &dereth_protocol::trade::GameStartGame) -> bool {
        self.minigame.recv_start_game(ObjectId(m.game_id), m.team)
    }

    /// Handle move-response opcode `0x0283`.
    pub fn recv_move_response(&mut self, m: &dereth_protocol::trade::GameMoveResponse) -> bool {
        self.minigame
            .recv_move_response(ObjectId(m.game_id), m.result)
    }

    /// Handle opponent-turn opcode `0x0284`.
    pub fn recv_opponent_turn(&mut self, m: &dereth_protocol::trade::GameOpponentTurn) -> bool {
        self.minigame
            .recv_opponent_turn(ObjectId(m.game_id), &m.move_data)
    }

    /// Behavior: `0x0285`. The spelling is the
    /// client's.
    pub fn recv_opponent_stalemate(
        &mut self,
        m: &dereth_protocol::trade::GameOpponentStalemateState,
    ) -> bool {
        self.minigame
            .recv_opponent_stalemate(ObjectId(m.game_id), m.on != 0)
    }

    /// Handle game-over opcode `0x028C`.
    pub fn recv_game_over(&mut self, m: &dereth_protocol::trade::GameGameOver) -> bool {
        self.minigame
            .recv_game_over(ObjectId(m.game_id), m.team_winner)
    }

    /// The three buttons, and the resign dialog's answer.
    pub fn minigame_button(&mut self, id: u32, req: &mut dyn RequestSink) -> bool {
        self.minigame.on_button(id, req)
    }

    /// Handle the minigame quit dialog's answer.
    pub fn minigame_quit_answer(&mut self, confirmed: bool, req: &mut dyn RequestSink) {
        self.minigame.recv_try_to_quit_game(confirmed, req);
    }

    /// A left press on one of the 64 board cells.
    pub fn minigame_board_press(&mut self, cell: usize, req: &mut dyn RequestSink) -> bool {
        self.minigame.on_board_press(cell, req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RecordingRequests;

    /// Everything the window has said so far, in order — both streams, because both end in the
    /// chat scroll and the order between them is what a player sees.
    fn said(g: &MiniGame) -> Vec<&str> {
        g.lines.iter().map(|(_, t)| t.as_str()).collect()
    }

    /// The 32 piece creations the client's table makes, recovered from the stack
    /// stores. The back rank has to be `R N B Q K B N R` on both sides or the recovery is wrong.
    #[test]
    fn the_reset_table_is_a_standard_opening_position() {
        let mut b = GameBoard::default();
        b.reset();
        assert_eq!(b.logic.pieces.len(), 32);
        let row: Vec<PieceType> = (0..8)
            .map(|x| b.logic.at(Coord { x, y: 0 }).expect("back rank").piece_type)
            .collect();
        assert_eq!(
            row,
            vec![
                PieceType::Rook,
                PieceType::Knight,
                PieceType::Bishop,
                PieceType::Queen,
                PieceType::King,
                PieceType::Bishop,
                PieceType::Knight,
                PieceType::Rook
            ]
        );
        for x in 0..8 {
            assert_eq!(b.logic.at(Coord { x, y: 1 }).expect("pawn").player, 0);
            assert_eq!(b.logic.at(Coord { x, y: 6 }).expect("pawn").player, 1);
            assert!(b.logic.at(Coord { x, y: 3 }).is_none());
        }
    }

    /// The press-handler and draw index mappings are inverses for both teams.
    #[test]
    fn the_cell_map_round_trips_for_both_teams() {
        for team in [0, 1] {
            for cell in 0..64 {
                let c = GameBoard::coord_of_cell(cell, team).expect("a square");
                assert_eq!(
                    GameBoard::cell_of_coord(c, team),
                    Some(cell),
                    "team {team} cell {cell}"
                );
            }
        }
        // The two teams do not agree on which cell a square is, which is the whole point.
        let c = Coord { x: 0, y: 0 };
        assert_ne!(
            GameBoard::cell_of_coord(c, 0),
            GameBoard::cell_of_coord(c, 1)
        );
    }

    /// The full round trip: use the board, join, start, move, and the opponent's reply.
    #[test]
    fn a_whole_game_reaches_the_wire_from_a_use() {
        let board = ObjectId(0x7000_0001);
        let mut g = MiniGame::new();
        let mut req = RecordingRequests::default();

        g.on_begin_game(board, &mut req);
        assert!(g.visible);
        assert_eq!(
            req.0,
            vec![Request::GameJoin(dereth_protocol::trade::GameJoin {
                game_id: board.0,
                which_team: JOIN_ANY_TEAM
            })]
        );
        assert_eq!(said(&g), vec![ATTEMPTING_TO_JOIN]);

        assert!(g.recv_join_game_response(board, 0));
        assert_eq!(g.team, 0);
        assert_eq!(g.state, GameState::WaitingForGameStart);
        assert_eq!(g.board.logic.pieces.len(), 32);

        assert!(g.recv_start_game(board, 0));
        assert_eq!(g.state, GameState::PlayingMyTurn);

        // e2 (4,1) to e4 (4,3), as team 0 sees the cells.
        let from = GameBoard::cell_of_coord(Coord { x: 4, y: 1 }, 0).expect("cell");
        let to = GameBoard::cell_of_coord(Coord { x: 4, y: 3 }, 0).expect("cell");
        req.0.clear();
        assert!(g.on_board_press(from, &mut req));
        assert_eq!(g.board.selected, Some(Coord { x: 4, y: 1 }));
        assert!(req.0.is_empty(), "selecting sends nothing");
        assert!(g.on_board_press(to, &mut req));
        assert_eq!(
            req.0,
            vec![Request::GameMove(dereth_protocol::trade::GameMove {
                x_from: 4,
                y_from: 1,
                x_to: 4,
                y_to: 3
            })]
        );
        assert_eq!(g.state, GameState::PlayingTryingToMove);

        assert!(g.recv_move_response(board, 1));
        assert_eq!(g.state, GameState::PlayingNotMyTurn);
        assert_eq!(said(&g).last(), Some(&WAITING_FOR_OPPONENT));

        let reply = dereth_protocol::trade::GameOpponentTurn {
            game_id: board.0,
            team: 1,
            move_data: dereth_protocol::trade::GameMoveData {
                move_type: dereth_protocol::trade::GameMoveData::FROM_TO,
                player: ObjectId(0),
                from: Some((4, 6)),
                to: Some((4, 4)),
                piece_index: None,
            },
        };
        assert!(g.recv_opponent_turn(board, &reply.move_data));
        assert_eq!(g.state, GameState::PlayingMyTurn);
        assert_eq!(said(&g).last(), Some(&YOUR_TURN));

        req.0.clear();
        assert!(g.recv_game_over(board, 0));
        assert_eq!(g.state, GameState::NotPlaying);
        assert_eq!(g.current_game, ObjectId(0));
        assert_eq!(
            said(&g).last(),
            Some(&format!("{VICTORIOUS}{DEFAULT_STATE}").as_str())
        );
    }

    /// A move the local engine refuses is never sent — the one thing the chess engine is in the
    /// client for.
    #[test]
    fn a_locally_illegal_move_never_reaches_the_wire() {
        let board = ObjectId(0x7000_0002);
        let mut g = MiniGame::new();
        let mut req = RecordingRequests::default();
        g.on_begin_game(board, &mut req);
        g.recv_join_game_response(board, 0);
        g.recv_start_game(board, 0);
        req.0.clear();
        // The king cannot move at all from the opening position: e1 (4,0) to e2 (4,1) is occupied
        // by your own pawn -> BAD_MOVE_WOULD_CLOBBER.
        let from = GameBoard::cell_of_coord(Coord { x: 4, y: 0 }, 0).expect("cell");
        let to = GameBoard::cell_of_coord(Coord { x: 4, y: 1 }, 0).expect("cell");
        g.on_board_press(from, &mut req);
        g.on_board_press(to, &mut req);
        assert!(req.0.is_empty(), "nothing goes out: {:?}", req.0);
        assert_eq!(g.moves_refused_locally, 1);
        assert_eq!(g.state, GameState::PlayingMyTurn);
        assert_eq!(
            said(&g).last(),
            Some(&format!("You cannot attack your own pieces{TRY_AGAIN}{YOUR_TURN}").as_str())
        );
    }

    /// Every message this window speaks is guarded on the game id, and two of them on the state
    /// as well. A stray message must change nothing.
    #[test]
    fn a_message_for_another_game_is_refused_by_the_guard() {
        let mine = ObjectId(0x7000_0003);
        let theirs = ObjectId(0x7000_0004);
        let mut g = MiniGame::new();
        let mut req = RecordingRequests::default();
        g.on_begin_game(mine, &mut req);
        g.recv_join_game_response(mine, 1);
        let before = g.state;
        assert!(!g.recv_start_game(theirs, 1));
        assert!(!g.recv_move_response(theirs, 1));
        assert!(!g.recv_opponent_stalemate(theirs, true));
        assert!(!g.recv_game_over(theirs, 1));
        assert_eq!(g.state, before);
        assert_eq!(g.rejected_by_guard, 4);
        assert_eq!(g.starts, 0);
    }

    /// A promotion ORs the queen's check result into the move's, keeping the pawn's: the pawn
    /// b7-b8 uncovers the h1 bishop's check on the a8 king (which could still step to a7), and the
    /// queen on b8, guarded by the d7 knight, makes it mate.
    #[test]
    fn a_promotion_keeps_the_pawns_check_bits_and_adds_the_queens() {
        let mut board = GameBoard::default();
        let l = &mut board.logic;
        l.place(PieceType::King, 0, Coord { x: 4, y: 0 });
        l.place(PieceType::Bishop, 0, Coord { x: 7, y: 0 });
        l.place(PieceType::Knight, 0, Coord { x: 3, y: 6 });
        l.place(PieceType::Pawn, 0, Coord { x: 1, y: 6 });
        l.place(PieceType::King, 1, Coord { x: 0, y: 7 });
        let to = Coord { x: 1, y: 7 };
        let r = board.logic.do_move(Coord { x: 1, y: 6 }, to);
        assert_eq!(
            r,
            mr::OK_MOVE_TO_EMPTY_SQUARE | mr::OK_MOVE_PROMOTION | mr::OK_MOVE_CHECK,
            "{r:#X}"
        );
        let r = board.do_promotion(to, r);
        assert_eq!(
            r,
            mr::OK_MOVE_TO_EMPTY_SQUARE
                | mr::OK_MOVE_PROMOTION
                | mr::OK_MOVE_CHECK
                | mr::OK_MOVE_CHECKMATE,
            "{r:#X}"
        );
        assert_eq!(
            board.logic.at(to).map(|p| p.piece_type),
            Some(PieceType::Queen)
        );
        assert!(board.logic.last_move_was_check);
    }

    /// Every refusal string can print, against the switch.
    #[test]
    fn the_move_result_text_matches_the_switch() {
        assert_eq!(show_move_result(mr::BAD_MOVE_NOT_YOUR_TURN), NOT_YOUR_TURN);
        assert_eq!(
            show_move_result(mr::BAD_MOVE_INVALID_COMMAND),
            format!("That move is invalid{TRY_AGAIN}{YOUR_TURN}")
        );
        // -111 is not in the switch and falls to the default, like -1 and -2.
        assert_eq!(
            show_move_result(mr::BAD_MOVE_INVALID_BOARD_STATE),
            format!("That move is invalid{TRY_AGAIN}{YOUR_TURN}")
        );
        assert_eq!(
            show_move_result(mr::OK_MOVE_TO_EMPTY_SQUARE),
            "Move in progress.\n"
        );
        assert_eq!(
            show_move_result(mr::OK_MOVE_TO_EMPTY_SQUARE | mr::OK_MOVE_CHECK),
            format!("{MOVE_IN_PROGRESS}{OPPONENT_IN_CHECK}")
        );
        assert_eq!(
            show_move_result(mr::OK_MOVE_TO_EMPTY_SQUARE | mr::OK_MOVE_CHECKMATE),
            YOU_CHECKMATED
        );
        assert_eq!(
            show_opponent_move_result(mr::OK_MOVE_TO_EMPTY_SQUARE),
            YOUR_TURN
        );
        assert_eq!(
            show_opponent_move_result(mr::OK_MOVE_TO_EMPTY_SQUARE | mr::OK_MOVE_CHECK),
            format!("{YOU_ARE_IN_CHECK}{YOUR_TURN}")
        );
        assert_eq!(
            show_opponent_move_result(mr::OK_MOVE_EN_PASSANT | mr::OK_MOVE_CHECKMATE),
            YOU_WERE_CHECKMATED
        );
    }

    /// The three buttons, and the refusal both of the sending ones share.
    #[test]
    fn the_three_buttons_send_what_retail_sends() {
        let board = ObjectId(0x7000_0005);
        let mut g = MiniGame::new();
        let mut req = RecordingRequests::default();

        // With no game: Pass and Stalemate refuse with the same sentence and send nothing.
        assert!(g.on_button(button::PASS, &mut req));
        assert!(g.on_button(button::STALEMATE, &mut req));
        assert!(req.0.is_empty());
        assert_eq!(said(&g), vec![NOT_PLAYING, NOT_PLAYING]);

        g.on_begin_game(board, &mut req);
        req.0.clear();
        assert!(g.on_button(button::PASS, &mut req));
        assert_eq!(
            req.0,
            vec![Request::GameMovePass(dereth_protocol::trade::GameMovePass)]
        );
        req.0.clear();
        assert!(g.on_button(button::STALEMATE, &mut req));
        assert!(g.stalemate);
        assert_eq!(
            req.0,
            vec![Request::GameStalemate(
                dereth_protocol::trade::GameStalemate { on: 1 }
            )]
        );
        req.0.clear();
        assert!(g.on_button(button::STALEMATE, &mut req));
        assert!(!g.stalemate);
        assert_eq!(
            req.0,
            vec![Request::GameStalemate(
                dereth_protocol::trade::GameStalemate { on: 0 }
            )]
        );

        // Resign raises the dialog and sends nothing until the answer comes back.
        req.0.clear();
        assert!(g.on_button(button::RESIGN, &mut req));
        assert!(req.0.is_empty());
        assert_ne!(g.resign_dialog, 0);
        g.recv_try_to_quit_game(false, &mut req);
        assert!(req.0.is_empty(), "a refused dialog sends nothing");
        g.on_button(button::RESIGN, &mut req);
        g.recv_try_to_quit_game(true, &mut req);
        assert_eq!(
            req.0,
            vec![Request::GameQuit(dereth_protocol::trade::GameQuit)]
        );
        assert_eq!(g.current_game, ObjectId(0));
        assert!(!g.visible, "the quit hides the window through EndGame");
    }

    /// Joining twice is refused, and the two refusals are different sentences on channel `0x1A`.
    #[test]
    fn a_second_join_is_refused_with_the_right_sentence() {
        let a = ObjectId(0x7000_0006);
        let b = ObjectId(0x7000_0007);
        let mut g = MiniGame::new();
        let mut req = RecordingRequests::default();
        g.on_begin_game(a, &mut req);
        req.0.clear();
        g.on_begin_game(a, &mut req);
        assert_eq!(said(&g).last(), Some(&ALREADY_THIS_GAME));
        g.on_begin_game(b, &mut req);
        assert_eq!(said(&g).last(), Some(&ALREADY_ANOTHER_GAME));
        assert!(req.0.is_empty(), "neither refusal sends: {:?}", req.0);
        assert_eq!(g.joins_refused, 2);
    }
}
