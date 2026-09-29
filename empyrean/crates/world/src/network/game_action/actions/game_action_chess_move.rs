// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionChessMove.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionChessMove.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::entity::chess::chess_piece_coord::ChessPieceCoord;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_chess;
use crate::World;

// ACE: GameActionChessMove.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::GameMove>()?;
    let from = ChessPieceCoord::new_xy(m.x_from, m.y_from); // ReadChessPieceCoord: X then Y
    let to = ChessPieceCoord::new_xy(m.x_to, m.y_to);

    let player = session_player(w, session);
    player_chess::handle_action_chess_move(w, player, from, to);
    Ok(())
}
