// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterEnterWorldServerReady.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterEnterWorldServerReady.cs`.

use dereth_protocol::login as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageCharacterEnterWorldServerReady.GameMessageCharacterEnterWorldServerReady
#[must_use]
pub fn game_message_character_enter_world_server_ready() -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::CharacterEnterWorldServerReady,
        GameMessageGroup::UIQueue,
        &proto::LoginEnterGameServerReady,
    )
}
