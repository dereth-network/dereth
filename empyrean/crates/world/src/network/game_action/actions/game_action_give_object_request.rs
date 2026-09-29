// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionGiveObjectRequest.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionGiveObjectRequest.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_inventory;
use crate::World;

// ACE: GameActionGiveObjectRequest.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::items::InventoryGiveObjectRequest>()?;
    let target_guid = m.target.0;
    let object_guid = m.item.0;
    let amount: i32 = m.amount.cs_cast();

    let player = session_player(w, session);
    player_inventory::handle_action_give_object_request(
        w,
        player,
        target_guid,
        object_guid,
        amount,
    );
    Ok(())
}
