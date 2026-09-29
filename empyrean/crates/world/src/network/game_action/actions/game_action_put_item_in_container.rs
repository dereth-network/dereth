// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionPutItemInContainer.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionPutItemInContainer.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_inventory;
use crate::World;

// ACE: GameActionPutItemInContainer.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::items::InventoryPutItemInContainer>()?;
    let item_guid = m.item.0;
    let container_guid = m.container.0;
    let placement: i32 = m.slot.cs_cast();

    let player = session_player(w, session);
    player_inventory::handle_action_put_item_in_container(
        w,
        player,
        item_guid,
        container_guid,
        placement,
    );
    Ok(())
}
