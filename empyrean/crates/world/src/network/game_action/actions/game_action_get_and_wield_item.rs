// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionGetAndWieldItem.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionGetAndWieldItem.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_inventory;
use crate::World;

// ACE: GameActionGetAndWieldItem.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::items::InventoryGetAndWieldItem>()?;
    let item_guid = m.item.0;
    let location = empyrean_entity::enums::EquipMask(m.slot);

    let player = session_player(w, session);
    player_inventory::handle_action_get_and_wield_item(w, player, item_guid, location);
    Ok(())
}
