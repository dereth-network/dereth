// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionHouseAddPermanentGuest.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionHouseAddPermanentGuest.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_house;
use crate::World;

// ACE: GameActionHouseAddPermanentGuest.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let guest_name = message
        .decode_padded::<proto::trade::HouseAddPermanentGuest>()?
        .name;

    let player = session_player(w, session);
    player_house::handle_action_add_guest(w, player, &guest_name);
    Ok(())
}
