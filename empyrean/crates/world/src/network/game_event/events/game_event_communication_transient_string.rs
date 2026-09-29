// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventCommunicationTransientString.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventCommunicationTransientString.cs`.

use dereth_protocol::comms as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto_strings;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventCommunicationTransientString.GameEventCommunicationTransientString
#[must_use]
pub fn game_event_communication_transient_string(
    session: &mut SessionData,
    message: &str,
) -> GameMessage {
    let body = proto::CommunicationTransientString {
        text: ace_str(message),
    };
    game_event_from_proto_strings(
        GameEventType::CommunicationTransientString,
        GameMessageGroup::UIQueue,
        session,
        &body,
        &[message],
    )
}
