// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionSetAllegianceName.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionSetAllegianceName.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_allegiance;
use crate::World;

// ACE: GameActionSetAllegianceName.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let allegiance_name = message
        .decode_padded::<proto::social::AllegianceSetAllegianceName>()?
        .name;

    let player = session_player(w, session);
    player_allegiance::handle_action_set_allegiance_name(w, player, &allegiance_name);
    Ok(())
}
