// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionChessMovePass.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionChessMovePass.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_chess;
use crate::World;

// ACE: GameActionChessMovePass.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_chess::handle_action_chess_move_pass(w, player);
    Ok(())
}
