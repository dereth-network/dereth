// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterCreateResponse.cs
//! Port of `Source/ACE.Server/Network/GameMessages/Messages/GameMessageCharacterCreateResponse.cs`.

use dereth_protocol::login as proto;
use empyrean_entity::ObjectGuid;
use empyrean_net::enums::CharacterGenerationVerificationResponse;
use empyrean_net::GameMessageGroup;

use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::game_messages::game_message_opcode::GameMessageOpcode;

// ACE: GameMessageCharacterCreateResponse.GameMessageCharacterCreateResponse
#[must_use]
pub fn game_message_character_create_response(
    response: CharacterGenerationVerificationResponse,
    guid: ObjectGuid,
    char_name: &str,
) -> GameMessage {
    let mut msg = GameMessage::new(
        GameMessageOpcode::CharacterCreateResponse,
        GameMessageGroup::UIQueue,
    );
    msg.write_proto_strings(
        &proto::CharGenVerificationResponse {
            response_type: response.0,
            identity: if response == CharacterGenerationVerificationResponse::Ok {
                proto::CharacterIdentity {
                    gid: guid.into(),
                    name: ace_str(char_name),
                    seconds_greyed_out: 0,
                }
            } else {
                proto::CharacterIdentity::default()
            },
        },
        &[char_name],
    );
    msg
}
