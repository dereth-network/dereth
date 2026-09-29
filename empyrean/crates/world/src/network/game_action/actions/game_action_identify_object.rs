// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionIdentifyObject.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionIdentifyObject.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player;
use crate::World;

// ACE: GameActionIdentifyObject.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::objects::ItemAppraise>()?;
    let object_guid = m.target.0;

    let player = session_player(w, session);
    player::handle_action_identify_object(w, player, object_guid);
    Ok(())
}
