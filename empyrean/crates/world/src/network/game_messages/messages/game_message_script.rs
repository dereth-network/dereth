// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageScript.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageScript.cs`.

use dereth_protocol::objects as proto;
use empyrean_entity::enums::PlayScript;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageScript.GameMessageScript
/// `speed` defaults to 1.0f in ACE.
#[must_use]
pub fn game_message_script(guid: ObjectGuid, script_id: PlayScript, speed: f32) -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::PlayEffect,
        GameMessageGroup::SmartboxQueue,
        &proto::EffectsPlayScriptType {
            id: guid.into(),
            script_type: script_id.0.cast_signed(),
            intensity: speed,
        },
    )
}
