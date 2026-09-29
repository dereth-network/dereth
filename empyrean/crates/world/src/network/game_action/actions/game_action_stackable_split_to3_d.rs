// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionStackableSplitTo3D.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionStackableSplitTo3D.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_inventory;
use crate::World;

// ACE: GameActionStackableSplitTo3D.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::items::InventoryStackableSplitTo3d>()?;
    // Read in the applicable data.
    let stack_id = m.stack.0;
    let amount = m.amount;

    let player = session_player(w, session);
    player_inventory::handle_action_stackable_split_to3_d(w, player, stack_id, amount);
    Ok(())
}
