// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventUpdateHealth.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventUpdateHealth.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::combat as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventUpdateHealth.GameEventUpdateHealth
#[must_use]
pub fn game_event_update_health(
    session: &mut SessionData,
    objectid: u32,
    health: f32,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::UpdateHealth,
        GameMessageGroup::UIQueue,
        session,
        &proto::CombatQueryHealthResponse {
            object: ObjectId(objectid),
            health,
        },
    )
}
