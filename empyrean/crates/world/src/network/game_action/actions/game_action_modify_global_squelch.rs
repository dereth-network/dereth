// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionModifyGlobalSquelch.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionModifyGlobalSquelch.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::managers::squelch_manager;
use crate::World;

// ACE: GameActionModifyGlobalSquelch.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::comms::CommunicationModifyGlobalSquelch>()?;
    let squelch = to_boolean(m.add);
    let message_type = empyrean_entity::enums::ChatMessageType(m.msg_type);

    let player = session_player(w, session);
    squelch_manager::handle_action_modify_global_squelch(w, player, squelch, message_type);
    Ok(())
}
