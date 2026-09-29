// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionHouseChangeStoragePermission.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionHouseChangeStoragePermission.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::player_house;
use crate::World;

// ACE: GameActionHouseChangeStoragePermission.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::trade::HouseChangeStoragePermission>()?;
    let (guest_name, has_permission) = (m.name, to_boolean(m.has_permission));

    let player = session_player(w, session);
    player_house::handle_action_modify_storage(w, player, &guest_name, has_permission, true);
    Ok(())
}
