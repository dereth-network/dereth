// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageHearRangedSpeech.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageHearRangedSpeech.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::comms as proto;
use empyrean_entity::enums::ChatMessageType;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageHearRangedSpeech.GameMessageHearRangedSpeech
#[must_use]
pub fn game_message_hear_ranged_speech(
    message_text: &str,
    sender_name: &str,
    sender_id: u32,
    range: f32,
    chat_message_type: ChatMessageType,
) -> GameMessage {
    let mut msg = GameMessage::new(
        GameMessageOpcode::HearRangedSpeech,
        GameMessageGroup::UIQueue,
    );
    msg.write_proto_strings(
        &proto::CommunicationHearRangedSpeech {
            message: ace_str(message_text),
            sender_name: ace_str(sender_name),
            sender_id: ObjectId(sender_id),
            range,
            text_type: chat_message_type.0,
        },
        &[message_text, sender_name],
    );
    msg
}
