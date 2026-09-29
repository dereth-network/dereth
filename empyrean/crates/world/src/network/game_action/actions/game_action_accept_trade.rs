// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionAcceptTrade.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionAcceptTrade.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_trade;
use crate::World;

// ACE: GameActionAcceptTrade.Handle
/// Reads the whole packed trade as the client writes it — the six scalars, then both item lists —
/// so the message is consumed exactly. None of it is used: the server acts on its own trade state.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let _trade = message.decode::<dereth_protocol::trade::TradeAcceptTradeRequest>()?;

    let player = session_player(w, session);
    player_trade::handle_action_accept_trade(w, player);
    Ok(())
}
