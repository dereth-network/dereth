// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionNoLongerViewingContents.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionNoLongerViewingContents.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player_use;
use crate::World;

// ACE: GameActionNoLongerViewingContents.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::objects::InventoryNoLongerViewingContents>()?;
    let object_guid = m.container.0;

    let player = session_player(w, session);
    player_use::handle_action_no_longer_viewing_contents(w, player, object_guid);
    Ok(())
}
