// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceSwearAllegiance.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionAllegianceSwearAllegiance.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_allegiance;
use crate::World;

// ACE: GameActionAllegianceSwearAllegiance.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::AllegianceSwearAllegiance>()?;
    let target_guid = m.target.0;

    let player = session_player(w, session);
    player_allegiance::handle_action_swear_allegiance(w, player, target_guid);
    Ok(())
}
