// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionDropItem.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionDropItem.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_inventory;
use crate::World;

// ACE: GameActionDropItem.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::items::InventoryDropItem>()?;
    let item_guid = m.item.0;

    let player = session_player(w, session);
    player_inventory::handle_action_drop_item(w, player, item_guid);
    Ok(())
}
