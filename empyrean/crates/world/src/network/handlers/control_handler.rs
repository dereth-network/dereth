// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/Handlers/ControlHandler.cs
//! Port of `Source/ACE.Server/Network/Handlers/ControlHandler.cs`.

use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::world_objects::player;
use crate::World;

// ACE: ControlHandler.ControlResponse
pub fn control_response(
    w: &mut World,
    message: &mut Payload<'_>,
    session: SessionId,
) -> HandlerResult {
    let item_guid = message.read_u32()?;
    player::handle_action_force_obj_desc_send(w, session_player(w, session), item_guid);
    Ok(())
}
