// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageSoulEmote.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageSoulEmote.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::comms as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageSoulEmote.GameMessageSoulEmote
#[must_use]
pub fn game_message_soul_emote(sender_id: u32, sender_name: &str, emote_text: &str) -> GameMessage {
    let mut msg = GameMessage::new(GameMessageOpcode::SoulEmote, GameMessageGroup::UIQueue);
    msg.write_proto_strings(
        &proto::CommunicationHearSoulEmote {
            sender: ObjectId(sender_id),
            sender_name: ace_str(sender_name),
            text: ace_str(emote_text),
        },
        &[sender_name, emote_text],
    );
    msg
}
