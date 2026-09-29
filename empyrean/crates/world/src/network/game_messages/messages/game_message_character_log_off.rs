// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterLogOff.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterLogOff.cs`.

use dereth_protocol::login as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageCharacterLogOff.GameMessageCharacterLogOff
#[must_use]
pub fn game_message_character_log_off() -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::CharacterLogOff,
        GameMessageGroup::UIQueue,
        &proto::LoginExecuteLogOff,
    )
}
