// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageEmoteText.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageEmoteText.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::comms as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageEmoteText.GameMessageEmoteText
#[must_use]
pub fn game_message_emote_text(sender_id: u32, sender_name: &str, emote_text: &str) -> GameMessage {
    let mut msg = GameMessage::new(GameMessageOpcode::EmoteText, GameMessageGroup::UIQueue);
    msg.write_proto_strings(
        &proto::CommunicationHearEmote {
            sender: ObjectId(sender_id),
            sender_name: ace_str(sender_name),
            text: ace_str(emote_text),
        },
        &[sender_name, emote_text],
    );
    msg
}
