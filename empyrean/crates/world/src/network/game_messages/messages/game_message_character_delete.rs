// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterDelete.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterDelete.cs`.

use dereth_protocol::login as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageCharacterDelete.GameMessageCharacterDelete
#[must_use]
pub fn game_message_character_delete() -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::CharacterDelete,
        GameMessageGroup::UIQueue,
        &proto::CharacterDeleteAck,
    )
}
