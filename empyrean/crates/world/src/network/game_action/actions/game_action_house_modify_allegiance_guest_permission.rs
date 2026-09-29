// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionHouseModifyAllegianceGuestPermission.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionHouseModifyAllegianceGuestPermission.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::player_house;
use crate::World;

// ACE: GameActionHouseModifyAllegianceGuestPermission.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::HouseModifyAllegianceGuestPermission>()?;
    // whether we are adding or removing permissions
    let add = to_boolean(m.allow);

    let player = session_player(w, session);
    player_house::handle_action_modify_allegiance_guest_permission(w, player, add);
    Ok(())
}
