// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventQueryAgeResponse.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventQueryAgeResponse.cs`.

use dereth_protocol::admin as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto_strings;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventQueryAgeResponse.GameEventQueryAgeResponse
#[must_use]
pub fn game_event_query_age_response(
    session: &mut SessionData,
    target_name: &str,
    age: &str,
) -> GameMessage {
    let body = proto::CharacterQueryAgeResponse {
        target_name: ace_str(target_name),
        age: ace_str(age),
    };
    game_event_from_proto_strings(
        GameEventType::QueryAgeResponse,
        GameMessageGroup::UIQueue,
        session,
        &body,
        &[target_name, age],
    )
}
