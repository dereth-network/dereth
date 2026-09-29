// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventWeenieErrorWithString.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventWeenieErrorWithString.cs`.

use dereth_protocol::comms as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::WeenieErrorWithString;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto_strings;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventWeenieErrorWithString.GameEventWeenieErrorWithString
#[must_use]
pub fn game_event_weenie_error_with_string(
    session: &mut SessionData,
    error_type: WeenieErrorWithString,
    message: &str,
) -> GameMessage {
    let body = proto::CommunicationWeenieErrorWithString {
        error_type: error_type.0.cs_cast(),
        text: ace_str(message),
    };
    game_event_from_proto_strings(
        GameEventType::WeenieErrorWithString,
        GameMessageGroup::UIQueue,
        session,
        &body,
        &[message],
    )
}
