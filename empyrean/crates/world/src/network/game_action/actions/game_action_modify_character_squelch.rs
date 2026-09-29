// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionModifyCharacterSquelch.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionModifyCharacterSquelch.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::managers::inbound_message_manager::{
    session_player, to_boolean, HandlerResult, Payload,
};
use crate::world_objects::managers::squelch_manager;
use crate::World;

// ACE: GameActionModifyCharacterSquelch.Handle
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let m = message.decode::<proto::comms::CommunicationModifyCharacterSquelch>()?;
    let squelch = to_boolean(m.add);
    let player_guid = m.character_id.0;
    let player_name = m.character_name;
    let message_type = empyrean_entity::enums::ChatMessageType(m.msg_type);

    let player = session_player(w, session);
    squelch_manager::handle_action_modify_character_squelch(
        w,
        player,
        squelch,
        player_guid,
        &player_name,
        message_type,
    );
    Ok(())
}
