// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionChessJoin.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionChessJoin.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_chess;
use crate::World;

// ACE: GameActionChessJoin.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::GameJoin>()?;
    let board_guid = m.game_id; // chessboard guid
    let _color: i32 = m.which_team.cs_cast(); // expecting -1 here, unused?

    let player = session_player(w, session);
    player_chess::handle_action_chess_join(w, player, board_guid);
    Ok(())
}
