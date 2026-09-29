// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Chess/ChessMatch.cs
//! Port of `Source/ACE.Server/Entity/Chess/ChessMatch.cs` ("ported from Anahera's code" in ACE):
//! one game on a chessboard, its two sides (a player or the AI), the delayed-action queue the
//! board's heartbeat drains, the weenie pieces that walk and fight on the board, and the win/loss
//! bookkeeping.
//!
//! The board (`Game.ChessMatch`), both players (`Player.ChessMatch`) and every piece
//! (`GamePiece.ChessMatch`) hold the same match object in ACE; here the matches live in the
//! world's store, `World.chess_matches` ([`ChessMatchStore`]), and each holder keeps the
//! match's [`ChessMatchRef`] id. Every function takes the world and the reference and reads the
//! match through the world (`m.get(w)`), never across a call that takes the world. A match nothing
//! refers to any more (ACE's garbage) is removed from the store when the next one is created.
//!
//! ACE's `StartAiMove` runs the "async" AI search synchronously on the world thread (its comment:
//! "todo: execute ai work on a separate thread"), and so does this port.

use std::collections::VecDeque;

use dereth_physics::math as pmath;
use dereth_primitives::{Frame, Vec3};
use empyrean_common::dotnet::{CsCast, DotNetDateTime};
use empyrean_entity::enums::{
    ChatMessageType, ChessAiState, ChessColor, ChessDelayedActionType, ChessMoveFlag,
    ChessMoveResult, ChessMoveType, ChessPieceType, ChessState, PropertyInt,
};
use empyrean_entity::{ObjectGuid, Position};
use empyrean_net::SessionId;

use super::chess::{self, CHESS_WINNER_END_GAME, CHESS_WINNER_STALEMATE, NUM_COLORS};
use super::chess_ai_async_turn_key::ChessAiAsyncTurnKey;
use super::chess_ai_move_result::ChessAiMoveResult;
use super::chess_delayed_action::ChessDelayedAction;
use super::chess_logic::{has_result_flag, ChessLogic};
use super::chess_move::ChessMove;
use super::chess_piece_coord::ChessPieceCoord;
use super::chess_side::ChessSide;
use super::game_move_data::GameMoveData;
use crate::entity::i_player::{self, IPlayer};
use crate::entity::landblock;
use crate::managers::player_manager;
use crate::network::game_event::events::{
    game_event_game_over::game_event_game_over,
    game_event_join_game_response::game_event_join_game_response,
    game_event_move_response::game_event_move_response,
    game_event_opponent_stalemate::game_event_opponent_stalemate,
    game_event_opponent_turn::game_event_opponent_turn,
    game_event_start_game::game_event_start_game,
};
use crate::network::game_messages::game_message::{enqueue_send, GameMessage};
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::structure::chess_move_data::ChessMoveData;
use crate::physics::phys_ext;
use crate::sessions::SessionData;
use crate::world_objects::{game, game_piece, player_chess, world_object};
use crate::World;

/// How quickly the rankings will raise / lower.
// ACE: ChessMatch.RankFactor
pub const RANK_FACTOR: i32 = 50;

/// The retail client's OKMoveEnPassant move result (3): both square bits.
const OK_MOVE_EN_PASSANT: i32 = 3;

// ACE: ChessMatch
#[derive(Debug)]
pub struct ChessMatch {
    pub chess_board: ObjectGuid,
    pub state: ChessState,
    pub sides: [Option<ChessSide>; NUM_COLORS],
    pub logic: ChessLogic,
    /// delayed action queue
    pub actions: VecDeque<ChessDelayedAction>,

    pub ai_state: ChessAiState,
    pub ai_future: Option<ChessAiMoveResult>,

    pub move_result: ChessMoveResult,
    pub waiting_for_motion: bool,
    pub motions: Vec<ObjectGuid>,

    pub next_range_check: Option<DotNetDateTime>,
    pub start_ai_time: Option<DotNetDateTime>,

    pub log: Vec<ChessMove>,
}

/// `ChessMatch`, the reference: an id into `World.chess_matches`. Copying it copies the
/// reference; equality is C#'s reference equality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChessMatchRef(u32);

impl ChessMatchRef {
    /// The match's fields.
    ///
    /// # Panics
    /// When the match has left the store (nothing referred to it any more).
    #[must_use]
    pub fn get(self, w: &World) -> &ChessMatch {
        &w.chess_matches
            .entries
            .get(&self.0)
            .expect("a live chess match")
            .chess_match
    }

    /// The match's fields, to change.
    ///
    /// # Panics
    /// When the match has left the store.
    pub fn get_mut(self, w: &mut World) -> &mut ChessMatch {
        &mut w
            .chess_matches
            .entries
            .get_mut(&self.0)
            .expect("a live chess match")
            .chess_match
    }
}

/// One match in the store, with the number of holders (the board, the players, the pieces).
#[derive(Debug)]
struct ChessMatchEntry {
    chess_match: ChessMatch,
    references: u32,
}

/// `World.chess_matches`: every chess match, by id. `Game.ChessMatch`,
/// `Player.ChessMatch` and `GamePiece.ChessMatch` count their references through their setters;
/// a match nothing refers to is dropped at the next [`new`].
#[derive(Debug, Default)]
pub struct ChessMatchStore {
    entries: std::collections::HashMap<u32, ChessMatchEntry>,
    next_id: u32,
}

impl ChessMatchStore {
    /// The number of matches in the store.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Replaces one holder's reference: `old` loses one, `new` gains one.
    pub fn replace_reference(&mut self, old: Option<ChessMatchRef>, new: Option<ChessMatchRef>) {
        if let Some(e) = old.and_then(|m| self.entries.get_mut(&m.0)) {
            e.references = e.references.saturating_sub(1);
        }
        if let Some(e) = new.and_then(|m| self.entries.get_mut(&m.0)) {
            e.references += 1;
        }
    }

    fn insert(&mut self, chess_match: ChessMatch) -> ChessMatchRef {
        self.entries.retain(|_, e| e.references > 0);
        self.next_id = self.next_id.wrapping_add(1);
        self.entries.insert(
            self.next_id,
            ChessMatchEntry {
                chess_match,
                references: 0,
            },
        );
        ChessMatchRef(self.next_id)
    }
}

fn idx(color: ChessColor) -> usize {
    usize::try_from(color.0)
        .ok()
        .filter(|&i| i < NUM_COLORS)
        .expect("ACE: Sides[(int)color] out of range (IndexOutOfRangeException)")
}

impl ChessMatch {
    // ACE: ChessMatch.ChessMatch
    #[must_use]
    pub fn new(chess_board: ObjectGuid) -> Self {
        Self {
            chess_board,
            state: ChessState::WaitingForPlayers,
            sides: [None, None],
            logic: ChessLogic::new(),
            actions: VecDeque::new(),
            ai_state: ChessAiState::None,
            ai_future: None,
            move_result: ChessMoveResult::NoMoveResult,
            waiting_for_motion: false,
            motions: Vec::new(),
            next_range_check: None,
            start_ai_time: None,
            log: Vec::new(),
        }
    }

    // ACE: ChessMatch.GetColor
    #[must_use]
    pub fn get_color(&self, player_guid: ObjectGuid) -> ChessColor {
        for side in self.sides.iter().flatten() {
            if side.player_guid == player_guid {
                return side.color;
            }
        }
        ChessColor::None
    }

    // ACE: ChessMatch.InMatch
    #[must_use]
    pub fn in_match(&self, player_guid: ObjectGuid) -> bool {
        self.get_color(player_guid) != ChessColor::None
    }

    // ACE: ChessMatch.GetFreeColor
    #[must_use]
    pub fn get_free_color(&self) -> ChessColor {
        for (i, side) in self.sides.iter().enumerate() {
            if side.is_none() {
                return ChessColor(i32::try_from(i).unwrap_or(0));
            }
        }

        ChessColor::None
    }

    // ACE: ChessMatch.AsyncMoveAiSimple
    pub fn async_move_ai_simple(
        &mut self,
        key: &ChessAiAsyncTurnKey,
        result: &mut ChessAiMoveResult,
    ) {
        // Debug.Assert(AiState == ChessAiState.WaitingForWorker): compiled out
        self.ai_state = ChessAiState::InProgress;

        let (move_result, from, to) = self.logic.async_calculate_ai_simple_move(key);
        result.set_result(move_result, from, to);

        self.ai_state = ChessAiState::WaitingForFinish;
    }

    // ACE: ChessMatch.AsyncMoveAiComplex
    pub fn async_move_ai_complex(
        &mut self,
        key: &ChessAiAsyncTurnKey,
        result: &mut ChessAiMoveResult,
    ) {
        // Debug.Assert(AiState == ChessAiState.WaitingForWorker): compiled out
        self.ai_state = ChessAiState::InProgress;

        let mut counter = 0u32;
        let (move_result, from, to) = self
            .logic
            .async_calculate_ai_complex_move(key, &mut counter);
        result.set_result(move_result, from, to);
        result.profiling_counter = counter;

        self.ai_state = ChessAiState::WaitingForFinish;
    }

    // ACE: ChessMatch.AddPendingWeenieMotion
    pub fn add_pending_weenie_motion(&mut self, piece_guid: ObjectGuid) {
        self.motions.push(piece_guid);
        self.waiting_for_motion = true;
    }

    // ACE: ChessMatch.DebugMove
    /// The move log as ACE prints it (returned here for the caller to log).
    #[must_use]
    pub fn debug_move(&self) -> String {
        let mut out = String::from("  | White | Black\n-----------------\n");
        let mut num = 1;

        for (i, m) in self.log.iter().enumerate() {
            if i % 2 == 0 {
                out.push_str(&format!("{num:<2}| "));
                num += 1;
            }

            out.push_str(&format!("{}-{}", m.from, m.to));

            if i % 2 == 0 {
                out.push_str(" | ");
            } else {
                out.push('\n');
            }
        }
        out.push('\n');
        out
    }
}

/// `new ChessMatch(chessBoard)`, as the shared reference.
#[must_use]
pub fn new(w: &mut World, chess_board: ObjectGuid) -> ChessMatchRef {
    w.chess_matches.insert(ChessMatch::new(chess_board))
}

// ================================================================================ helpers

/// `player.Session` (a player without one is ACE's `NullReferenceException`).
fn session_of(w: &World, player: ObjectGuid) -> SessionId {
    player_manager::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `player.Session.Network.EnqueueSend(msg)`.
fn send(w: &mut World, player: ObjectGuid, msg: GameMessage) {
    let session = session_of(w, player);
    enqueue_send(w, session, msg);
}

/// Builds a game event on the player's session (its `GameEventSequence` is consumed here) and
/// sends it.
fn send_event(
    w: &mut World,
    player: ObjectGuid,
    build: impl FnOnce(&mut SessionData) -> GameMessage,
) {
    let session = session_of(w, player);
    let data = w
        .sessions
        .get_mut(session)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let msg = build(data);
    enqueue_send(w, session, msg);
}

/// `new GameMessagePrivateUpdatePropertyInt(player, property, value)`, sent to the player.
fn send_private_int(w: &mut World, player: ObjectGuid, property: PropertyInt, value: i32) {
    let o = w
        .objects
        .get_mut(player)
        .expect("ACE: onlinePlayer is null (NullReferenceException)");
    let msg = game_message_private_update_property_int(o, property, value);
    send(w, player, msg);
}

/// `ChessBoard.Location`.
fn board_location(w: &World, board: ObjectGuid) -> Position {
    w.objects
        .get(board)
        .and_then(world_object::WorldObject::location)
        .expect("ACE: ChessBoard.Location is null (NullReferenceException)")
}

/// `ChessBoard.PhysicsObj.Position.Frame`.
fn board_frame(w: &World, board: ObjectGuid) -> Frame {
    let h = phys_ext::physics_obj(w, board)
        .expect("ACE: ChessBoard.PhysicsObj is null (NullReferenceException)");
    phys_ext::position(w, h)
        .expect("ACE: ChessBoard.PhysicsObj is null (NullReferenceException)")
        .frame
}

/// `ChessBoard.CurrentLandblock.GetObject(guid) as GamePiece`.
fn board_game_piece(w: &World, board: ObjectGuid, guid: ObjectGuid) -> Option<ObjectGuid> {
    let lb = w
        .objects
        .get(board)
        .and_then(|o| o.current_landblock)
        .expect("ACE: ChessBoard.CurrentLandblock is null (NullReferenceException)");
    landblock::get_object(w, lb, guid, true).filter(|&g| {
        w.objects
            .get(g)
            .is_some_and(world_object::WorldObject::is_game_piece)
    })
}

/// `new Position(ChessBoard.Location)` with `Pos` and `Rotation` from `frame`.
fn position_from_frame(board_location: &Position, frame: &Frame) -> Position {
    let mut position = Position::from_position(board_location);
    position.set_pos(empyrean_common::dotnet::Vector3::new(
        frame.origin.x,
        frame.origin.y,
        frame.origin.z,
    ));
    let r = frame.rotation;
    position.set_rotation(empyrean_common::dotnet::Quaternion::new(r.x, r.y, r.z, r.w));
    position
}

// ================================================================================ ChessMatch.cs

// ACE: ChessMatch.Update
/// The board's heartbeat: the AI's start timer and its move, then (unless the AI is working or
/// pieces are still moving) every queued action, then the 5 s leash check.
pub fn update(w: &mut World, m: &ChessMatchRef) {
    let (state, start_ai_time, ai_state) = {
        let g = m.get_mut(w);
        (g.state, g.start_ai_time, g.ai_state)
    };
    match state {
        ChessState::WaitingForPlayers => {
            if start_ai_time.is_some_and(|t| w.now.utc >= t) {
                add_ai(w, m);
            }
        }

        ChessState::InProgress => match ai_state {
            ChessAiState::WaitingToStart => start_ai_move(w, m),
            ChessAiState::WaitingForFinish => finish_ai_move(w, m),
            _ => {}
        },
        _ => {}
    }

    // don't handle any delayed actions while ai is working to prevent races
    if m.get_mut(w).ai_state != ChessAiState::None {
        return;
    }

    // don't handle any delayed actions while weenie pieces are moving or attacking
    if m.get_mut(w).waiting_for_motion {
        return;
    }

    while let Some(action) = m.get_mut(w).actions.pop_front() {
        match action.action {
            ChessDelayedActionType::Start => start(w, m),
            ChessDelayedActionType::Move => move_delayed(w, m, &action),
            ChessDelayedActionType::MovePass => move_pass_delayed(w, m, &action),
            ChessDelayedActionType::Stalemate => stalemate_delayed(w, m, &action),
            ChessDelayedActionType::Quit => quit_delayed(w, m, action.color),
            _ => {}
        }
    }

    let next_range_check = m.get_mut(w).next_range_check;
    if let Some(next) = next_range_check {
        if next <= w.now.utc {
            let (sides, board) = {
                let g = m.get_mut(w);
                (g.sides.clone(), g.chess_board)
            };
            for side in sides.iter().flatten() {
                if side.is_ai() {
                    continue;
                }

                let Some(player) = side.get_player(w) else {
                    quit_delayed(w, m, side.color);
                    return;
                };

                // arbitrary distance, should there be some warning before reaching leash range?
                let player_location = w
                    .objects
                    .get(player)
                    .and_then(world_object::WorldObject::location)
                    .expect("ACE: player.Location is null (NullReferenceException)");
                let distance_to_game = player_location.distance_to(&board_location(w, board));
                if distance_to_game > 40.0 {
                    quit_delayed(w, m, side.color);
                    return;
                }
            }
            m.get_mut(w).next_range_check = Some(w.now.utc.add_seconds(5.0));
        }
    }
}

// ACE: ChessMatch.AddSide
/// Seats `player` (the AI when `None`, guid 0) as `color`, spawns that side's pieces, then either
/// arms the AI start timer (no opponent yet, `chess_ai_start_time` > 0) or queues the start.
pub fn add_side(w: &mut World, m: &ChessMatchRef, player: Option<ObjectGuid>, color: ChessColor) {
    // ai is represented with object guid 0
    let player_guid = player.unwrap_or_else(|| ObjectGuid::new(0));

    m.get_mut(w).sides[idx(color)] = Some(ChessSide::new(player_guid, color));

    if let Some(player) = player {
        player_chess::set_chess_match(w, player, Some(*m));
    }

    // spawn weenie pieces in the world for side
    let coords = m.get_mut(w).logic.walk_piece_coords();
    for coord in coords {
        let is_color = m
            .get_mut(w)
            .logic
            .get_piece(&coord)
            .is_some_and(|p| p.color == color);
        if is_color {
            add_weenie_piece(w, m, coord);
        }
    }

    let other_free = m.get_mut(w).sides[idx(chess::inverse_color(color))].is_none();
    if other_free {
        let ai_enabled =
            crate::managers::property_manager::get_double(w, "chess_ai_start_time", 0.0, true).item;
        if ai_enabled > 0.0 {
            let player = player.expect("ACE: player is null (NullReferenceException)");
            let text = format!(
                "If another player doesn't join within {} seconds, the game will automatically start with AI",
                empyrean_common::dotnet::format::to_string(ai_enabled)
            );
            send(
                w,
                player,
                game_message_system_chat(&text, ChatMessageType::Broadcast),
            );
            m.get_mut(w).start_ai_time = Some(w.now.utc.add_seconds(ai_enabled));
        }
    } else {
        m.get_mut(w)
            .actions
            .push_back(ChessDelayedAction::new(ChessDelayedActionType::Start));
    }
}

// ACE: ChessMatch.AddAi
pub fn add_ai(w: &mut World, m: &ChessMatchRef) {
    let color = {
        let g = m.get_mut(w);
        if g.state != ChessState::WaitingForPlayers {
            return;
        }
        g.get_free_color()
    };

    if color == ChessColor::None {
        return;
    }

    add_side(w, m, None, color);
}

// ACE: ChessMatch.Join
pub fn join(w: &mut World, m: &ChessMatchRef, player: ObjectGuid) {
    let color = m.get_mut(w).get_free_color();
    if color != ChessColor::None {
        {
            let now = w.now.utc;
            let g = m.get_mut(w);
            if g.next_range_check.is_none() {
                g.next_range_check = Some(now.add_seconds(5.0));
            }
        }

        add_side(w, m, Some(player), color);
    }

    let board = m.get_mut(w).chess_board;
    send_event(w, player, |s| {
        game_event_join_game_response(s, board, color)
    });
}

// ACE: ChessMatch.MoveEnqueue
pub fn move_enqueue(
    w: &mut World,
    m: &ChessMatchRef,
    player: ObjectGuid,
    from: ChessPieceCoord,
    to: ChessPieceCoord,
) {
    let g = m.get_mut(w);
    if g.state != ChessState::InProgress {
        return;
    }

    let color = g.get_color(player);

    g.actions.push_back(ChessDelayedAction::with_move(
        ChessDelayedActionType::Move,
        color,
        Some(from),
        Some(to),
        false,
    ));
}

// ACE: ChessMatch.MovePassEnqueue
pub fn move_pass_enqueue(w: &mut World, m: &ChessMatchRef, player: ObjectGuid) {
    let g = m.get_mut(w);
    if g.state != ChessState::InProgress {
        return;
    }

    let color = g.get_color(player);

    g.actions.push_back(ChessDelayedAction::with_color(
        ChessDelayedActionType::MovePass,
        color,
        false,
    ));
}

// ACE: ChessMatch.QuitEnqueue
pub fn quit_enqueue(w: &mut World, m: &ChessMatchRef, player: ObjectGuid) {
    let g = m.get_mut(w);
    if g.state != ChessState::WaitingForPlayers && g.state != ChessState::InProgress {
        return;
    }

    let color = g.get_color(player);

    g.actions.push_back(ChessDelayedAction::with_color(
        ChessDelayedActionType::Quit,
        color,
        false,
    ));
}

// ACE: ChessMatch.StalemateEnqueue
pub fn stalemate_enqueue(w: &mut World, m: &ChessMatchRef, player: ObjectGuid, stalemate: bool) {
    let g = m.get_mut(w);
    if g.state != ChessState::InProgress {
        return;
    }

    let color = g.get_color(player);

    g.actions.push_back(ChessDelayedAction::with_color(
        ChessDelayedActionType::Stalemate,
        color,
        stalemate,
    ));
}

// ACE: ChessMatch.PieceReady
/// A weenie piece finished its walk (or its fight and walk): after a promotion the piece is
/// replaced by a queen; the last one ready finishes the turn.
pub fn piece_ready(w: &mut World, m: &ChessMatchRef, piece_guid: ObjectGuid) {
    let promoted = {
        let g = m.get_mut(w);
        if has_result_flag(g.move_result, ChessMoveResult::OKMovePromotion) {
            // Debug.Assert(piece != null): compiled out; UpgradeWeeniePiece dereferences it
            let piece = g
                .logic
                .get_piece_by_guid(piece_guid)
                .expect("ACE: ChessMatch.PieceReady: piece is null (NullReferenceException)");
            Some(piece.coord)
        } else {
            None
        }
    };
    if let Some(coord) = promoted {
        upgrade_weenie_piece(w, m, coord);
    }

    let finish = {
        let g = m.get_mut(w);
        if let Some(i) = g.motions.iter().position(|&x| x == piece_guid) {
            g.motions.remove(i);
        }
        if g.motions.is_empty() {
            g.waiting_for_motion = false;
            true
        } else {
            false
        }
    };
    if finish {
        finish_turn(w, m);
    }
}

// ACE: ChessMatch.Start
pub fn start(w: &mut World, m: &ChessMatchRef) {
    let (sides, turn) = {
        let g = m.get_mut(w);
        // Debug.Assert(State == ChessState.WaitingForPlayers): compiled out
        g.state = ChessState::InProgress;
        (g.sides.clone(), g.logic.turn)
    };

    for side in &sides {
        let side = side
            .as_ref()
            .expect("ACE: ChessMatch.Start: side is null (NullReferenceException)");
        if side.is_ai() {
            continue;
        }

        let player = side.get_player(w);
        // Debug.Assert(player != null): compiled out; SendStartGame dereferences it
        let player =
            player.expect("ACE: ChessMatch.Start: player is null (NullReferenceException)");
        send_start_game(w, m, player, turn);
    }
}

// ACE: ChessMatch.Finish
/// Ends the match: game over to each online player, the chess games/won/lost counters (unless
/// `winner` is `ChessWinnerEndGame`), the rank adjustment for a win, and every weenie piece removed.
pub fn finish(w: &mut World, m: &ChessMatchRef, winner: i32) {
    let sides = {
        let g = m.get_mut(w);
        if g.state != ChessState::WaitingForPlayers && g.state != ChessState::InProgress {
            return;
        }
        g.sides.clone()
    };

    for side in sides.iter().flatten() {
        if side.is_ai() {
            continue;
        }

        let (player, is_online) = player_manager::find_by_guid(w, side.player_guid.full());
        let player =
            player.expect("ACE: ChessMatch.Finish: player is null (NullReferenceException)");
        let mut online_player = None;
        if is_online {
            online_player = player_manager::get_online_player(w, side.player_guid.full());
            let op = online_player.expect("ACE: onlinePlayer is null (NullReferenceException)");
            send_game_over(w, m, op, winner);
        }

        if winner != CHESS_WINNER_END_GAME {
            let total_games = i_player::get_property(w, player, PropertyInt::ChessTotalGames)
                .unwrap_or(0)
                .wrapping_add(1);
            i_player::set_property(w, player, PropertyInt::ChessTotalGames, total_games);

            if let Some(op) = online_player {
                send_private_int(w, op, PropertyInt::ChessTotalGames, total_games);
            }
        }

        if winner >= 0 {
            let winner_color = ChessColor(winner);
            if winner_color == side.color {
                let won = i_player::get_property(w, player, PropertyInt::ChessGamesWon)
                    .unwrap_or(0)
                    .wrapping_add(1);
                i_player::set_property(w, player, PropertyInt::ChessGamesWon, won);

                if let Some(op) = online_player {
                    send_private_int(w, op, PropertyInt::ChessGamesWon, won);
                }
            } else {
                let lost = i_player::get_property(w, player, PropertyInt::ChessGamesLost)
                    .unwrap_or(0)
                    .wrapping_add(1);
                i_player::set_property(w, player, PropertyInt::ChessGamesLost, lost);

                if let Some(op) = online_player {
                    send_private_int(w, op, PropertyInt::ChessGamesLost, lost);
                }
            }
        }

        if let Some(op) = online_player {
            player_chess::set_chess_match(w, op, None);
        }
    }

    if winner >= 0 {
        // adjust player ranks
        let player_guid = sides[0]
            .as_ref()
            .expect("ACE: Sides[0] is null (NullReferenceException)")
            .player_guid;
        let opponent_guid = sides[1]
            .as_ref()
            .expect("ACE: Sides[1] is null (NullReferenceException)")
            .player_guid;
        let winner_guid = if winner == 0 {
            player_guid
        } else {
            opponent_guid
        };

        adjust_player_ranks(w, player_guid, opponent_guid, winner_guid);
    }

    let (coords, board) = {
        let g = m.get_mut(w);
        g.actions.clear();
        (g.logic.walk_piece_coords(), g.chess_board)
    };

    for coord in coords {
        remove_weenie_piece(w, m, coord);
    }

    {
        let g = m.get_mut(w);
        g.state = ChessState::Finished;
        g.next_range_check = None;
    }

    game::set_chess_match(w, board, None);
}

// ACE: ChessMatch.FinishTurn
/// The mover gets its move result; the side to move next gets the opponent's move (or the AI is
/// told to start).
pub fn finish_turn(w: &mut World, m: &ChessMatchRef) {
    let (side, move_result) = {
        let g = m.get_mut(w);
        (
            g.sides[idx(chess::inverse_color(g.logic.turn))].clone(),
            g.move_result,
        )
    };
    // ACE-BUG: `side.GetPlayer()` and `side.PlayerGuid` are read before the `side != null` test
    // (a null side would throw first); both sides are seated whenever a turn finishes.
    let side = side.expect("ACE: ChessMatch.FinishTurn: side is null (NullReferenceException)");
    let opponent = side.get_player(w);
    let opponent_guid = side.player_guid;
    if !side.is_ai() {
        let opponent = opponent
            .expect("ACE: ChessMatch.FinishTurn: opponent is null (NullReferenceException)");
        // Not ACE's (retail, V306): an en passant capture is reported to the
        // mover as OKMoveEnPassant (3, both square bits), keeping its check, checkmate and
        // promotion bits; ACE sent it as a capture onto an occupied square (2). The match's own
        // result (which picks the pieces' walk or attack) is left as a capture.
        let en_passant = m
            .get_mut(w)
            .logic
            .get_last_move()
            .flags
            .contains(ChessMoveFlag::EnPassantCapture);
        let move_result = if en_passant {
            ChessMoveResult(move_result.0 | OK_MOVE_EN_PASSANT)
        } else {
            move_result
        };
        send_move_response(w, m, opponent, move_result);
    }

    // Not ACE's (retail, V311): a move that leaves the side to move without a
    // legal move ends the game once both sides have seen it, as the client expects: checkmate is
    // a win for the mover (GameOver with its team: "You are victorious!" / "You have been
    // defeated!", after the client's own "You have checkmated your opponent!" / "You have been
    // checkmated!"), and a side with no move that is not in check ends it as a stalemate
    // (GameOver -1). The client has no stalemate rule of its own and waits for the server. ACE
    // never ended the game on a player's move (its checkmate test missed real mates), so the mated
    // player had to resign.
    let (side, game_over) = {
        let g = m.get_mut(w);
        let turn = g.logic.turn;
        (g.sides[idx(turn)].clone(), g.logic.game_over(turn))
    };
    if let Some(side) = side {
        if side.is_ai() {
            if game_over.is_none() {
                m.get_mut(w).ai_state = ChessAiState::WaitingToStart;
            }
        } else {
            let (piece, data) = {
                let g = m.get_mut(w);
                let mv = g.logic.get_last_move();
                let piece = g.logic.get_piece(&mv.to).map(|p| p.guid);
                (
                    piece,
                    GameMoveData::new(ChessMoveType::FromTo, mv.color, Some(mv.from), Some(mv.to)),
                )
            };
            let player = side
                .get_player(w)
                .expect("ACE: ChessMatch.FinishTurn: player is null (NullReferenceException)");
            let piece =
                piece.expect("ACE: ChessMatch.FinishTurn: piece is null (NullReferenceException)");
            send_opponent_turn(w, m, player, opponent_guid, piece, &data);
        }
    }
    if let Some(winner) = game_over {
        finish(w, m, winner);
    }
}

// ACE: ChessMatch.StartAiMove
pub fn start_ai_move(w: &mut World, m: &ChessMatchRef) {
    let g = m.get_mut(w);
    // Debug.Assert(AiState == ChessAiState.WaitingToStart): compiled out
    g.ai_state = ChessAiState::WaitingForWorker;

    // todo: execute ai work on a separate thread
    let key = ChessAiAsyncTurnKey;
    let mut future = ChessAiMoveResult::new();
    g.async_move_ai_simple(&key, &mut future);
    g.ai_future = Some(future);
}

// ACE: ChessMatch.FinishAiMove
pub fn finish_ai_move(w: &mut World, m: &ChessMatchRef) {
    let result = {
        let g = m.get_mut(w);
        // Debug.Assert(AiState == ChessAiState.WaitingForFinish): compiled out
        g.ai_state = ChessAiState::None;

        let result = g
            .ai_future
            .clone()
            .expect("ACE: ChessMatch.AiFuture is null (NullReferenceException)");
        g.move_result = result.result;
        result
    };

    if result.result == ChessMoveResult::NoMoveResult {
        // Not ACE's (a fix, V311): the side that could not move is the AI's, the
        // side to move. ACE took the side to move's opponent, which was the AI only because its
        // search's undo had handed the move to the opponent. (A game where the AI has no move has
        // normally ended already, on the move that left it so.)
        let (color, side, op_side, checkmate) = {
            let g = m.get_mut(w);
            let color = g.logic.turn;
            let side = g.sides[idx(color)].clone();
            let op_side = g.sides[idx(chess::inverse_color(color))].clone();
            let checkmate = g.logic.in_checkmate(color, true);
            (color, side, op_side, checkmate)
        };

        // checkmate
        if checkmate {
            finish(w, m, chess::inverse_color(color).0);
        }
        // stalemate
        else {
            let side =
                side.expect("ACE: ChessMatch.FinishAiMove: side is null (NullReferenceException)");
            if let Some(s) = m.get_mut(w).sides[idx(color)].as_mut() {
                s.stalemate = true;
            }

            let op_side = op_side
                .expect("ACE: ChessMatch.FinishAiMove: opSide is null (NullReferenceException)");
            if op_side.stalemate {
                finish(w, m, CHESS_WINNER_STALEMATE);
            } else {
                let player = op_side.get_player(w).expect(
                    "ACE: ChessMatch.FinishAiMove: player is null (NullReferenceException)",
                );
                send_opponent_stalemate(w, m, player, side.color, true);
            }
        }
    } else {
        finalize_weenie_move(w, m, result.result);
    }

    //Console.WriteLine($"Calculated Chess AI move in {result.ProfilingTime} ms with {result.ProfilingCounter} minimax calculations.");
}

// ACE: ChessMatch.FinalizeWeenieMove
/// Logs the move and sets the pieces walking: the mover (and a castling rook) to their squares, or
/// the mover to attack the captured piece.
pub fn finalize_weenie_move(w: &mut World, m: &ChessMatchRef, result: ChessMoveResult) {
    let mv = {
        let g = m.get_mut(w);
        let mv = g.logic.get_last_move().clone();
        g.log.push(mv.clone());
        mv
    };

    // need to use destination coordinate as Logic has already moved the piece
    let piece = mv.to;

    if has_result_flag(result, ChessMoveResult::OKMoveToEmptySquare) {
        move_weenie_piece(w, m, piece);

        let flags = mv.flags;
        if flags.intersects(ChessMoveFlag::KingSideCastle | ChessMoveFlag::QueenSideCastle) {
            let mut castling_to = mv.to;
            if flags.contains(ChessMoveFlag::KingSideCastle) {
                castling_to.move_offset(-1, 0);
            }
            if flags.contains(ChessMoveFlag::QueenSideCastle) {
                castling_to.move_offset(1, 0);
            }

            // Debug.Assert(rookPiece != null): compiled out; MoveWeeniePiece dereferences it
            move_weenie_piece(w, m, castling_to);
        }
    } else if has_result_flag(result, ChessMoveResult::OKMoveToOccupiedSquare) {
        attack_weenie_piece(w, m, piece, mv.captured_guid);
    }
}

// ACE: ChessMatch.MoveDelayed
pub fn move_delayed(w: &mut World, m: &ChessMatchRef, action: &ChessDelayedAction) {
    let side = {
        let g = m.get_mut(w);
        if g.logic.turn != action.color {
            return;
        }
        g.sides[idx(action.color)].clone()
    };

    let side = side.expect("ACE: ChessMatch.MoveDelayed: side is null (NullReferenceException)");
    let Some(player) = side.get_player(w) else {
        quit_delayed(w, m, action.color);
        return;
    };

    let from = action
        .from
        .expect("ACE: action.From is null (NullReferenceException)");
    let to = action
        .to
        .expect("ACE: action.To is null (NullReferenceException)");
    let result = m.get_mut(w).logic.do_move(action.color, &from, &to);
    if result < ChessMoveResult::NoMoveResult {
        send_move_response(w, m, player, result);
        return;
    }

    m.get_mut(w).move_result = result;
    finalize_weenie_move(w, m, result);
}

// ACE: ChessMatch.MovePassDelayed
/// Empty in ACE.
pub fn move_pass_delayed(_w: &mut World, _m: &ChessMatchRef, _action: &ChessDelayedAction) {}

// ACE: ChessMatch.QuitDelayed
/// Waiting for players: the match ends with no winner; in progress: the other colour wins.
pub fn quit_delayed(w: &mut World, m: &ChessMatchRef, color: ChessColor) {
    let state = m.get_mut(w).state;
    match state {
        ChessState::WaitingForPlayers => finish(w, m, CHESS_WINNER_END_GAME),
        ChessState::InProgress => finish(w, m, chess::inverse_color(color).0),
        _ => {}
    }
}

// ACE: ChessMatch.StalemateDelayed
pub fn stalemate_delayed(w: &mut World, m: &ChessMatchRef, action: &ChessDelayedAction) {
    let (side_color, op_side) = {
        let g = m.get_mut(w);
        let op_side = g.sides[idx(chess::inverse_color(action.color))].clone();
        let side = g.sides[idx(action.color)]
            .as_mut()
            .expect("ACE: ChessMatch.StalemateDelayed: side is null (NullReferenceException)");
        side.stalemate = action.stalemate;
        (side.color, op_side)
    };
    let op_side =
        op_side.expect("ACE: ChessMatch.StalemateDelayed: opSide is null (NullReferenceException)");

    if action.stalemate && op_side.stalemate {
        finish(w, m, CHESS_WINNER_STALEMATE);
    } else if !op_side.is_ai() {
        let player = op_side
            .get_player(w)
            .expect("ACE: ChessMatch.StalemateDelayed: player is null (NullReferenceException)");
        send_opponent_stalemate(w, m, player, side_color, action.stalemate);
    }
}

// ACE: ChessMatch.CalculateWeeniePosition
/// The square's frame: the board's frame moved by the square's offset from the board's centre (in
/// landblock axes, not turned with the board) and facing the board's heading, turned 180 for Black.
pub fn calculate_weenie_position(
    w: &World,
    board: ObjectGuid,
    coord: ChessPieceCoord,
    color: ChessColor,
    frame: &mut Frame,
) {
    let board_heading = pmath::get_heading(&board_frame(w, board));
    let mut heading: u32 = board_heading.cs_cast();
    heading = heading.wrapping_add(if color == ChessColor::Black { 180 } else { 0 });
    heading %= 360;

    #[allow(clippy::cast_precision_loss)]
    let offset = Vec3::new(coord.x as f32 - 3.5, coord.y as f32 - 3.5, 0.0);
    frame.origin = Vec3::new(
        frame.origin.x + offset.x,
        frame.origin.y + offset.y,
        frame.origin.z + offset.z,
    );
    #[allow(clippy::cast_precision_loss)]
    pmath::set_heading(frame, heading as f32);
}

// ACE: ChessMatch.AddWeeniePiece
/// Creates the piece's weenie (`drudge*` for White, `mosswart*` for Black) on its square, enters it
/// into the world and records its guid on the logic's piece.
pub fn add_weenie_piece(w: &mut World, m: &ChessMatchRef, coord: ChessPieceCoord) {
    let (board, piece_type, piece_color) = {
        let g = m.get_mut(w);
        let piece = g
            .logic
            .get_piece(&coord)
            .expect("ACE: piece is null (NullReferenceException)");
        (g.chess_board, piece.r#type, piece.color)
    };

    let mut frame = board_frame(w, board);
    calculate_weenie_position(w, board, coord, piece_color, &mut frame);

    let monster = if piece_color == ChessColor::White {
        "drudge"
    } else {
        "mosswart"
    };
    let weeniename = match piece_type {
        ChessPieceType::Pawn => format!("{monster}pawn"),
        ChessPieceType::Rook => format!("{monster}rook"),
        ChessPieceType::Knight => format!("{monster}knight"),
        ChessPieceType::Bishop => format!("{monster}bishop"),
        ChessPieceType::Queen => format!("{monster}queen"),
        ChessPieceType::King => format!("{monster}king"),
        _ => String::new(),
    };

    let wo = crate::factories::world_object_factory::create_new_world_object_by_name_in_world(
        w,
        &weeniename,
    )
    .expect("ACE: ChessMatch.AddWeeniePiece: wo is null (NullReferenceException)");
    //Console.WriteLine($"AddWeeniePiece: {weeniename}, {piece.Coord}, {frame.Origin}");

    // add to position, spawn
    let location = position_from_frame(&board_location(w, board), &frame);
    if let Some(o) = w.objects.get_mut(wo) {
        o.set_location(Some(location));
    }

    crate::dispatch::enter_world::enter_world(w, wo);

    if let Some(piece) = m.get_mut(w).logic.get_piece_mut(&coord) {
        piece.guid = wo;
    }
    game_piece::set_chess_match_checked(w, wo, Some(*m));
}

// ACE: ChessMatch.MoveWeeniePiece
/// Sends the piece on `coord` walking to its square.
pub fn move_weenie_piece(w: &mut World, m: &ChessMatchRef, coord: ChessPieceCoord) {
    let (board, piece_guid, piece_color) = {
        let g = m.get_mut(w);
        let piece = g
            .logic
            .get_piece(&coord)
            .expect("ACE: piece is null (NullReferenceException)");
        (g.chess_board, piece.guid, piece.color)
    };
    let Some(game_piece) = board_game_piece(w, board, piece_guid) else {
        let g = m.get_mut(w);
        log::info!(
            "ChessMatch.MoveWeeniePiece({piece_guid}): couldn't find game piece\n{}{}",
            g.debug_move(),
            g.logic.debug_board()
        );
        return;
    };

    let mut frame = board_frame(w, board);
    calculate_weenie_position(w, board, coord, piece_color, &mut frame);

    let position = position_from_frame(&board_location(w, board), &frame);

    game_piece::move_enqueue(w, game_piece, position);

    add_pending_weenie_motion(w, m, piece_guid);
}

// ACE: ChessMatch.AttackWeeniePiece
/// Sends the piece on `coord` to attack `victim` on its square.
pub fn attack_weenie_piece(
    w: &mut World,
    m: &ChessMatchRef,
    coord: ChessPieceCoord,
    victim: ObjectGuid,
) {
    let (board, piece_guid, piece_color) = {
        let g = m.get_mut(w);
        let piece = g
            .logic
            .get_piece(&coord)
            .expect("ACE: piece is null (NullReferenceException)");
        (g.chess_board, piece.guid, piece.color)
    };
    let game_piece = board_game_piece(w, board, piece_guid);
    if game_piece.is_none() {
        let g = m.get_mut(w);
        log::info!("{}{}", g.debug_move(), g.logic.debug_board());
    }
    // Debug.Assert(gamePiece != null): compiled out; AttackEnqueue dereferences it
    let game_piece = game_piece
        .expect("ACE: ChessMatch.AttackWeeniePiece: gamePiece is null (NullReferenceException)");

    let mut frame = board_frame(w, board);
    calculate_weenie_position(w, board, coord, piece_color, &mut frame);

    let position = position_from_frame(&board_location(w, board), &frame);

    game_piece::attack_enqueue(w, game_piece, position, victim);

    add_pending_weenie_motion(w, m, piece_guid);
}

// ACE: ChessMatch.RemoveWeeniePiece
pub fn remove_weenie_piece(w: &mut World, m: &ChessMatchRef, coord: ChessPieceCoord) {
    let (board, piece_guid) = {
        let g = m.get_mut(w);
        let piece = g
            .logic
            .get_piece(&coord)
            .expect("ACE: piece is null (NullReferenceException)");
        (g.chess_board, piece.guid)
    };
    let Some(game_piece) = board_game_piece(w, board, piece_guid) else {
        log::info!("RemoveWeeniePiece - couldn't find {piece_guid} @ {coord}");
        return;
    };
    if let Some(piece) = m.get_mut(w).logic.get_piece_mut(&coord) {
        piece.guid = ObjectGuid::new(0);
    }
    world_object::destroy(w, game_piece, true, false);
}

// ACE: ChessMatch.UpgradeWeeniePiece
pub fn upgrade_weenie_piece(w: &mut World, m: &ChessMatchRef, coord: ChessPieceCoord) {
    remove_weenie_piece(w, m, coord);

    // AC's Chess implementation doesn't support underpromotion
    if let Some(piece) = m.get_mut(w).logic.get_piece_mut(&coord) {
        piece.r#type = ChessPieceType::Queen;
    }
    add_weenie_piece(w, m, coord);
}

/// [`ChessMatch::add_pending_weenie_motion`] through the reference.
fn add_pending_weenie_motion(w: &mut World, m: &ChessMatchRef, piece_guid: ObjectGuid) {
    m.get_mut(w).add_pending_weenie_motion(piece_guid);
}

// ACE: ChessMatch.SendStartGame
pub fn send_start_game(w: &mut World, m: &ChessMatchRef, player: ObjectGuid, color: ChessColor) {
    let board = m.get_mut(w).chess_board;
    send_event(w, player, |s| game_event_start_game(s, board, color));
}

// ACE: ChessMatch.SendMoveResponse
pub fn send_move_response(
    w: &mut World,
    m: &ChessMatchRef,
    player: ObjectGuid,
    result: ChessMoveResult,
) {
    let board = m.get_mut(w).chess_board;
    send_event(w, player, |s| game_event_move_response(s, board, result));
}

// ACE: ChessMatch.SendOpponentTurn
pub fn send_opponent_turn(
    w: &mut World,
    m: &ChessMatchRef,
    player: ObjectGuid,
    opponent_guid: ObjectGuid,
    piece_guid: ObjectGuid,
    data: &GameMoveData,
) {
    let board = m.get_mut(w).chess_board;
    let move_data = ChessMoveData::new(opponent_guid, piece_guid, data);
    send_event(w, player, |s| {
        game_event_opponent_turn(s, board, &move_data)
    });
}

// ACE: ChessMatch.SendOpponentStalemate
pub fn send_opponent_stalemate(
    w: &mut World,
    m: &ChessMatchRef,
    player: ObjectGuid,
    color: ChessColor,
    stalemate: bool,
) {
    let board = m.get_mut(w).chess_board;
    send_event(w, player, |s| {
        game_event_opponent_stalemate(s, board, color, stalemate)
    });
}

// ACE: ChessMatch.SendGameOver
pub fn send_game_over(w: &mut World, m: &ChessMatchRef, player: ObjectGuid, winner: i32) {
    //Console.WriteLine($"Sending game over({winner}) to {player.Name}");

    let board = m.get_mut(w).chess_board;
    send_event(w, player, |s| game_event_game_over(s, board, winner));
}

// ACE: ChessMatch.AdjustPlayerRanks
/// Adjusts the chess ranks for 2 players after a match (Elo, `RankFactor` 50; the AI has no rank
/// record and counts as 1400).
pub fn adjust_player_ranks(
    w: &mut World,
    player_guid: ObjectGuid,
    opponent_guid: ObjectGuid,
    winner_guid: ObjectGuid,
) {
    let (player, player_is_online) = player_manager::find_by_guid(w, player_guid.full());
    let (opponent, opponent_is_online) = player_manager::find_by_guid(w, opponent_guid.full());

    // ACE-BUG: `player` is Sides[0]; were the AI White (guid 0, no player record) this throws and the
    // rest of Finish never runs. Latent: the first to join always takes White, so the AI is Black.
    let player: IPlayer =
        player.expect("ACE: ChessMatch.AdjustPlayerRanks: player is null (NullReferenceException)");
    let rank = i_player::get_property(w, player, PropertyInt::ChessRank).unwrap_or(1400);
    let mut op_rank = 1400;
    if let Some(opponent) = opponent {
        // chess ai
        op_rank = i_player::get_property(w, opponent, PropertyInt::ChessRank).unwrap_or(1400);
    }

    let chance = expectation_to_win(rank, op_rank);

    let win: f32 = if player_guid == winner_guid { 1.0 } else { 0.0 };
    let delta: i32 =
        empyrean_common::dotnet::math::round(f64::from(RANK_FACTOR) * (f64::from(win) - chance))
            .cs_cast();

    i_player::set_property(w, player, PropertyInt::ChessRank, rank.wrapping_add(delta));

    if let Some(opponent) = opponent {
        i_player::set_property(
            w,
            opponent,
            PropertyInt::ChessRank,
            op_rank.wrapping_sub(delta),
        );
    }

    if player_is_online {
        let online_player = player_manager::get_online_player(w, player_guid.full())
            .expect("ACE: onlinePlayer is null (NullReferenceException)");
        send_private_int(
            w,
            online_player,
            PropertyInt::ChessRank,
            rank.wrapping_add(delta),
        );
    }

    if opponent.is_some() && opponent_is_online {
        let online_op = player_manager::get_online_player(w, opponent_guid.full())
            .expect("ACE: onlineOp is null (NullReferenceException)");
        send_private_int(
            w,
            online_op,
            PropertyInt::ChessRank,
            op_rank.wrapping_sub(delta),
        );
    }
}

// ACE: ChessMatch.ExpectationToWin
/// The expected chance of `rank` beating `op_rank` (Elo). The exponent is computed in `float`
/// (`rankDiff / 400.0f`) before `Math.Pow`.
#[must_use]
pub fn expectation_to_win(rank: i32, op_rank: i32) -> f64 {
    let rank_diff = op_rank.wrapping_sub(rank);

    #[allow(clippy::cast_precision_loss)]
    let exponent = rank_diff as f32 / 400.0f32;
    1.0 / (1.0 + empyrean_common::math::pow(10.0, f64::from(exponent)))
}
