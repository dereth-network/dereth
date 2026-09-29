// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionAbandonContract.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionAbandonContract.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_contracts;
use crate::World;

// ACE: GameActionAbandonContract.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::SocialAbandonContract>()?;
    // Read in the applicable data.
    let contract_id = m.contract_id;

    let player = session_player(w, session);
    player_contracts::handle_action_abandon_contract(w, player, contract_id);
    Ok(())
}
