// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageUpdateObject.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageUpdateObject.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::World;

// ACE: GameMessageUpdateObject.GameMessageUpdateObject
/// The body is `worldObject.SerializeUpdateObject` (through the virtual dispatch).
/// ACE's defaults: `adminvision = false`, `changenodraw = false`.
#[must_use]
pub fn game_message_update_object(
    w: &mut World,
    world_object: ObjectGuid,
    adminvision: bool,
    changenodraw: bool,
) -> GameMessage {
    let mut msg = GameMessage::new(
        GameMessageOpcode::UpdateObject,
        GameMessageGroup::SmartboxQueue,
    );
    crate::dispatch::serialize_update_object::serialize_update_object(
        w,
        world_object,
        &mut msg.data,
        adminvision,
        changenodraw,
    );
    msg
}
