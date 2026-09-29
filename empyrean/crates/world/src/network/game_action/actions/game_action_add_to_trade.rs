// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionAddToTrade.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionAddToTrade.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_trade;
use crate::World;

// ACE: GameActionAddToTrade.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::TradeAddToTrade>()?;
    let item_guid = m.item.0;
    let trade_slot = m.slot;

    let player = session_player(w, session);
    player_trade::handle_action_add_to_trade(w, player, item_guid, trade_slot);
    Ok(())
}
