// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageCreateObject.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageCreateObject.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;
use crate::World;

// ACE: GameMessageCreateObject.GameMessageCreateObject
/// The body is `worldObject.SerializeCreateObject` (through the virtual dispatch).
/// ACE's defaults: `adminvision = false`, `adminnodraw = false`.
#[must_use]
pub fn game_message_create_object(
    w: &mut World,
    world_object: ObjectGuid,
    adminvision: bool,
    adminnodraw: bool,
) -> GameMessage {
    let mut msg = GameMessage::new(
        GameMessageOpcode::ObjectCreate,
        GameMessageGroup::SmartboxQueue,
    );
    crate::dispatch::serialize_create_object::serialize_create_object(
        w,
        world_object,
        &mut msg.data,
        adminvision,
        adminnodraw,
    );
    msg
}
