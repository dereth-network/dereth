// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageObjDescEvent.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageObjDescEvent.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::World;

// ACE: GameMessageObjDescEvent.GameMessageObjDescEvent
/// Sent whenever a character changes their clothes (Skunkworks F625 "Change Model"). The body is
/// `worldObject.SerializeUpdateModelData` (through the virtual dispatch).
#[must_use]
pub fn game_message_obj_desc_event(w: &mut World, world_object: ObjectGuid) -> GameMessage {
    let mut msg = GameMessage::new(
        GameMessageOpcode::ObjDescEvent,
        GameMessageGroup::SmartboxQueue,
    );
    crate::dispatch::serialize_update_model_data::serialize_update_model_data(
        w,
        world_object,
        &mut msg.data,
    );
    msg
}
