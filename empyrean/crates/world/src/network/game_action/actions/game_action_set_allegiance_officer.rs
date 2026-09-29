// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionSetAllegianceOfficer.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionSetAllegianceOfficer.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_allegiance;
use crate::World;

// ACE: GameActionSetAllegianceOfficer.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::social::AllegianceSetAllegianceOfficer>()?;
    let player_name = m.name; // The allegiance officer's name
    let officer_level = m.level;

    let player = session_player(w, session);
    player_allegiance::handle_action_set_allegiance_officer(w, player, &player_name, officer_level);
    Ok(())
}
