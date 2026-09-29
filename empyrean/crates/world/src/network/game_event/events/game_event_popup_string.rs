// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventPopupString.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventPopupString.cs`.

use dereth_protocol::comms as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto_strings;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventPopupString.GameEventPopupString
#[must_use]
pub fn game_event_popup_string(session: &mut SessionData, message: &str) -> GameMessage {
    let body = proto::CommunicationPopUpString {
        message: ace_str(message),
    };
    game_event_from_proto_strings(
        GameEventType::PopupString,
        GameMessageGroup::UIQueue,
        session,
        &body,
        &[message],
    )
}
