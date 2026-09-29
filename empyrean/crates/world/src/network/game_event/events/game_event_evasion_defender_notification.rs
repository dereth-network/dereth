// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventEvasionDefenderNotification.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventEvasionDefenderNotification.cs`.

use dereth_protocol::combat as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto_strings;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventEvasionDefenderNotification.GameEventEvasionDefenderNotification
#[must_use]
pub fn game_event_evasion_defender_notification(
    session: &mut SessionData,
    attacker_name: &str,
) -> GameMessage {
    let body = proto::EvasionDefenderNotification {
        attacker_name: ace_str(attacker_name),
    };
    game_event_from_proto_strings(
        GameEventType::EvasionDefenderNotification,
        GameMessageGroup::UIQueue,
        session,
        &body,
        &[attacker_name],
    )
}
