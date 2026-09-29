// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionOpenTradeNegotiations.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionOpenTradeNegotiations.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_trade;
use crate::World;

// ACE: GameActionOpenTradeNegotiations.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::TradeOpenTradeNegotiations>()?;
    let trade_partner_guid = m.partner.0;

    let player = session_player(w, session);
    player_trade::handle_action_open_trade_negotiations(w, player, trade_partner_guid, true);
    Ok(())
}
