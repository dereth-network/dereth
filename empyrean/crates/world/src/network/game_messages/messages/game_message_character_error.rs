// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterError.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterError.cs`.

use dereth_protocol::login as proto;
use empyrean_net::enums::CharacterError;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageCharacterError.GameMessageCharacterError
#[must_use]
pub fn game_message_character_error(error: CharacterError) -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::CharacterError,
        GameMessageGroup::UIQueue,
        &proto::CharacterError {
            char_error: error as u32,
        },
    )
}
