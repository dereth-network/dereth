// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionDeclineTrade.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionDeclineTrade.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_trade;
use crate::World;

// ACE: GameActionDeclineTrade.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_trade::handle_action_decline_trade(w, player, session);
    Ok(())
}
