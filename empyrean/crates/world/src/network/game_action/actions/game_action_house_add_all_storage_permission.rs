// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionHouseAddAllStoragePermission.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionHouseAddAllStoragePermission.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_house;
use crate::World;

// ACE: GameActionHouseAddAllStoragePermission.Handle
pub fn handle(w: &mut World, _message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let player = session_player(w, session);
    player_house::handle_action_all_storage(w, player);
    Ok(())
}
