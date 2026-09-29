// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionChessStalemate.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionChessStalemate.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::player_chess;
use crate::World;

// ACE: GameActionChessStalemate.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::GameStalemate>()?;
    let stalemate = to_boolean(m.on);

    let player = session_player(w, session);
    player_chess::handle_action_chess_stalemate(w, player, stalemate);
    Ok(())
}
