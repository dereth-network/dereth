// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionUseWithTarget.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionUseWithTarget.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_use;
use crate::World;

// ACE: GameActionUseWithTarget.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::items::InventoryUseWithTargetEvent>()?;
    let source_object_guid = m.object.0;
    let target_object_guid = m.target.0;

    let player = session_player(w, session);
    player_use::handle_action_use_with_target(w, player, source_object_guid, target_object_guid);
    Ok(())
}
