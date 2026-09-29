// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventUpdateTitle.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventUpdateTitle.cs`.

use dereth_protocol::social as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventUpdateTitle.GameEventUpdateTitle
/// `setAsDisplayTitle` defaults to false in ACE.
#[must_use]
pub fn game_event_update_title(
    session: &mut SessionData,
    title: u32,
    set_as_display_title: bool,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::UpdateTitle,
        GameMessageGroup::UIQueue,
        session,
        &proto::SocialAddOrSetCharacterTitle {
            new_title: title,
            set_as_display_title: i32::from(set_as_display_title),
        },
    )
}
