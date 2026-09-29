// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageSystemChat.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageSystemChat.cs`.

use dereth_protocol::comms as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::ChatMessageType;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageSystemChat.GameMessageSystemChat
#[must_use]
pub fn game_message_system_chat(message: &str, chat_message_type: ChatMessageType) -> GameMessage {
    let ty: i32 = chat_message_type.0.cs_cast();
    let mut msg = GameMessage::new(GameMessageOpcode::ServerMessage, GameMessageGroup::UIQueue);
    msg.write_proto_strings(
        &proto::CommunicationTextboxString {
            text: ace_str(message),
            text_type: ty.cast_unsigned(),
        },
        &[message],
    );
    msg
}
