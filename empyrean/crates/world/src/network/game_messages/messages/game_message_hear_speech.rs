// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageHearSpeech.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageHearSpeech.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::comms as proto;
use empyrean_entity::enums::ChatMessageType;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageHearSpeech.GameMessageHearSpeech
#[must_use]
pub fn game_message_hear_speech(
    message_text: &str,
    sender_name: &str,
    sender_id: u32,
    chat_message_type: ChatMessageType,
) -> GameMessage {
    let mut msg = GameMessage::new(GameMessageOpcode::HearSpeech, GameMessageGroup::UIQueue);
    msg.write_proto_strings(
        &proto::CommunicationHearSpeech {
            message: ace_str(message_text),
            sender_name: ace_str(sender_name),
            sender_id: ObjectId(sender_id),
            text_type: chat_message_type.0,
        },
        &[message_text, sender_name],
    );
    msg
}
