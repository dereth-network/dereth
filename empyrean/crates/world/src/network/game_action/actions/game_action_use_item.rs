// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionUseItem.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionUseItem.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_use;
use crate::World;

// ACE: GameActionUseItem.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::items::InventoryUseEvent>()?;
    let item_guid = m.object.0;

    let player = session_player(w, session);
    player_use::handle_action_use_item(w, player, item_guid);
    Ok(())
}
