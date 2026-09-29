// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionResetTrade.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionResetTrade.cs`.

use empyrean_net::SessionId;

use crate::managers::player_manager;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_trade;
use crate::World;

// ACE: GameActionResetTrade.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    let who_reset = player;

    let target =
        player_manager::get_online_player(w, player_trade::trade_partner(w, player).full());

    if let Some(target) = target {
        player_trade::handle_action_reset_trade(w, player, who_reset);

        //Send GameEvent to reset partner's trade window
        player_trade::handle_action_reset_trade(w, target, who_reset);
    }
    Ok(())
}
