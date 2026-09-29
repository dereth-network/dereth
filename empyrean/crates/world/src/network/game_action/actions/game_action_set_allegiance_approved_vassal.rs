// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionSetAllegianceApprovedVassal.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionSetAllegianceApprovedVassal.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_allegiance;
use crate::World;

// ACE: GameActionSetAllegianceApprovedVassal.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player_name = message
        .decode_padded::<proto::social::AllegianceSetAllegianceApprovedVassal>()?
        .name; // the approved vassal

    let player = session_player(w, session);
    player_allegiance::handle_action_set_allegiance_approved_vassal(w, player, &player_name);
    Ok(())
}
