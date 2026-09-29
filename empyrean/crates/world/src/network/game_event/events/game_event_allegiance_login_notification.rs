// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventAllegianceLoginNotification.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventAllegianceLoginNotification.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventAllegianceLoginNotification.GameEventAllegianceLoginNotification
#[must_use]
pub fn game_event_allegiance_login_notification(
    session: &mut SessionData,
    player_guid: u32,
    is_logged_in: bool,
) -> GameMessage {
    let mut msg = game_event_message_with_capacity(
        GameEventType::AllegianceLoginNotification,
        GameMessageGroup::UIQueue,
        session,
        12,
    );
    msg.data.write_u32(player_guid);
    msg.data.write_u32(u32::from(is_logged_in));
    msg
}
