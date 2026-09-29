// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Chess.cs
//! Port of `Source/ACE.Server/WorldObjects/Player_Chess.cs`: the player's chess actions, each handed
//! to the match the player is in.

use empyrean_entity::ObjectGuid;

use crate::entity::chess::chess_match::{self, ChessMatchRef};
use crate::entity::chess::chess_piece_coord::ChessPieceCoord;
use crate::entity::landblock;
use crate::World;

/// Non-property fields declared in `Player_Chess.cs`.
#[derive(Debug, Default)]
pub struct PlayerChessFields {
    // ACE: Player.ChessMatch
    /// The match this player is in (the match is shared with the board and its pieces).
    pub chess_match: Option<ChessMatchRef>,
}

/// `Player.ChessMatch` (`None` for null or a non-player).
#[must_use]
pub fn chess_match(w: &World, this: ObjectGuid) -> Option<ChessMatchRef> {
    w.objects
        .get(this)?
        .player
        .as_ref()?
        .player_chess
        .chess_match
}

/// `Player.ChessMatch = value`.
pub fn set_chess_match(w: &mut World, this: ObjectGuid, value: Option<ChessMatchRef>) {
    if let Some(p) = w.objects.get_mut(this).and_then(|o| o.player.as_mut()) {
        let old = std::mem::replace(&mut p.player_chess.chess_match, value);
        w.chess_matches.replace_reference(old, value);
    }
}

// ACE: Player.HandleActionChessJoin
/// Joins a chess game. `boardGuid` is the guid of the chess board.
pub fn handle_action_chess_join(w: &mut World, this: ObjectGuid, board_guid: u32) {
    let lb = w
        .objects
        .get(this)
        .and_then(|o| o.current_landblock)
        .expect("ACE: Player.CurrentLandblock is null (NullReferenceException)");
    let chessboard = landblock::get_object(w, lb, ObjectGuid::new(board_guid), true)
        .filter(|&g| w.objects.get(g).is_some_and(|o| o.is_game()));
    let Some(chessboard) = chessboard else {
        return;
    };

    crate::dispatch::act_on_use::act_on_use(w, chessboard, this);
}

// ACE: Player.HandleActionChessMove
/// Performs a move in a chess game: `from` and `to` are the chessboard x/y coordinates.
pub fn handle_action_chess_move(
    w: &mut World,
    this: ObjectGuid,
    from: ChessPieceCoord,
    to: ChessPieceCoord,
) {
    //Console.WriteLine($"{Name}.HandleActionChessMove({from}, {to})");

    if let Some(m) = chess_match(w, this) {
        chess_match::move_enqueue(w, &m, this, from, to);
    }
}

// ACE: Player.HandleActionChessMovePass
pub fn handle_action_chess_move_pass(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionChessMovePass()");

    if let Some(m) = chess_match(w, this) {
        chess_match::move_pass_enqueue(w, &m, this);
    }
}

// ACE: Player.HandleActionChessQuit
/// Called when the 'Resign' button is clicked.
pub fn handle_action_chess_quit(w: &mut World, this: ObjectGuid) {
    //Console.WriteLine($"{Name}.HandleActionChessQuit()");

    if let Some(m) = chess_match(w, this) {
        chess_match::quit_enqueue(w, &m, this);
    }
}

// ACE: Player.HandleActionChessStalemate
/// Offer or confirm a stalemate; `stalemate` false retracts the offer.
pub fn handle_action_chess_stalemate(w: &mut World, this: ObjectGuid, stalemate: bool) {
    //Console.WriteLine($"{Name}.HandleActionChessStalemate({stalemate})");

    if let Some(m) = chess_match(w, this) {
        chess_match::stalemate_enqueue(w, &m, this, stalemate);
    }
}
