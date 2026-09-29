// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionStackableMerge.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionStackableMerge.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_inventory;
use crate::World;

// ACE: GameActionStackableMerge.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::items::InventoryStackableMerge>()?;
    let merge_from_guid = m.merge_from.0;
    let merge_to_guid = m.merge_to.0;
    let amount = m.amount;

    let player = session_player(w, session);
    player_inventory::handle_action_stackable_merge(
        w,
        player,
        merge_from_guid,
        merge_to_guid,
        amount,
    );
    Ok(())
}
