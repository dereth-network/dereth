// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterRestore.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterRestore.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::login as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageCharacterRestore.GameMessageCharacterRestore
#[must_use]
pub fn game_message_character_restore(
    object_guid: u32,
    name: &str,
    seconds_disabled: u32,
) -> GameMessage {
    // 44 is the max seen in retail pcaps
    let mut msg = GameMessage::new(
        GameMessageOpcode::CharacterRestoreResponse,
        GameMessageGroup::UIQueue,
    );
    msg.write_proto_strings(
        &proto::CharGenVerificationResponse {
            response_type: 1, /* Verification OK flag */
            identity: proto::CharacterIdentity {
                gid: ObjectId(object_guid),
                name: ace_str(name),
                seconds_greyed_out: seconds_disabled, /* secondsGreyedOut */
            },
        },
        &[name],
    );
    msg
}
