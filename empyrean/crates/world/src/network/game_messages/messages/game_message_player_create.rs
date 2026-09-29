// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessagePlayerCreate.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessagePlayerCreate.cs`.

use dereth_protocol::objects as proto;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessagePlayerCreate.GameMessagePlayerCreate
#[must_use]
pub fn game_message_player_create(guid: ObjectGuid) -> GameMessage {
    GameMessage::from_proto(
        GameMessageOpcode::PlayerCreate,
        GameMessageGroup::SmartboxQueue,
        &proto::LoginCreatePlayer {
            player_id: guid.into(),
        },
    )
}
