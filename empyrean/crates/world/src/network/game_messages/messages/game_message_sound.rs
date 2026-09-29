// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageSound.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageSound.cs`.

use dereth_protocol::objects as proto;
use empyrean_entity::enums::Sound;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageSound.GameMessageSound
/// `volume` defaults to 1.0f in ACE.
#[must_use]
pub fn game_message_sound(guid: ObjectGuid, sound_id: Sound, volume: f32) -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::Sound,
        GameMessageGroup::SmartboxQueue,
        &proto::EffectsSoundEvent {
            id: guid.into(),
            sound_type: sound_id.0.cast_signed(),
            volume,
        },
    )
}
