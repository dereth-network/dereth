// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventVictimNotification.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventVictimNotification.cs`.

use dereth_protocol::combat as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto_strings;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::ace_str;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventVictimNotification.GameEventVictimNotification
#[must_use]
pub fn game_event_victim_notification(
    session: &mut SessionData,
    death_message: &str,
) -> GameMessage {
    // sent to player when they die
    let body = proto::VictimNotificationSelf {
        message: ace_str(death_message),
    };
    game_event_from_proto_strings(
        GameEventType::VictimNotification,
        GameMessageGroup::UIQueue,
        session,
        &body,
        &[death_message],
    )
}
