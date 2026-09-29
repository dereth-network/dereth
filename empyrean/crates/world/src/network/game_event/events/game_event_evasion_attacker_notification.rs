// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventEvasionAttackerNotification.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventEvasionAttackerNotification.cs`.

use dereth_protocol::combat as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto_strings;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventEvasionAttackerNotification.GameEventEvasionAttackerNotification
#[must_use]
pub fn game_event_evasion_attacker_notification(
    session: &mut SessionData,
    defender_name: &str,
) -> GameMessage {
    let body = proto::EvasionAttackerNotification {
        defender_name: ace_str(defender_name),
    };
    game_event_from_proto_strings(
        GameEventType::EvasionAttackerNotification,
        GameMessageGroup::UIQueue,
        session,
        &body,
        &[defender_name],
    )
}
