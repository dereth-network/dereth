// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionStackableSplitToContainer.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionStackableSplitToContainer.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_inventory;
use crate::World;

// ACE: GameActionStackableSplitToContainer.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::items::InventoryStackableSplitToContainer>()?;
    // Read in the applicable data.
    let stack_id = m.stack.0;
    let container_id = m.container.0;
    let place: i32 = m.slot.cs_cast();
    let amount = m.amount;

    let player = session_player(w, session);
    player_inventory::handle_action_stackable_split_to_container(
        w,
        player,
        stack_id,
        container_id,
        place,
        amount,
    );
    Ok(())
}
