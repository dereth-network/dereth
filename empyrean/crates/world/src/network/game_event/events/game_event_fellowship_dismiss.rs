// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventFellowshipDismiss.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventFellowshipDismiss.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::social as proto;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventFellowshipDismiss.GameEventFellowshipDismiss
#[must_use]
pub fn game_event_fellowship_dismiss(
    session: &mut SessionData,
    dismissed_player: ObjectGuid,
) -> GameMessage {
    // can be both S2C and C2S?
    game_event_from_proto(
        GameEventType::FellowshipDismiss,
        GameMessageGroup::UIQueue,
        session,
        &proto::FellowshipDismiss {
            target: ObjectId(dismissed_player.full()),
        },
    )
}
