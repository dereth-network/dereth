// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveAllegianceOfficer.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionRemoveAllegianceOfficer.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_allegiance;
use crate::World;

// ACE: GameActionRemoveAllegianceOfficer.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let officer_name = message
        .decode_padded::<proto::social::AllegianceRemoveAllegianceOfficer>()?
        .name;

    let player = session_player(w, session);
    player_allegiance::handle_action_remove_allegiance_officer(w, player, &officer_name);
    Ok(())
}
