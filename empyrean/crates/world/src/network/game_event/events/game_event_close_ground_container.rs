// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventCloseGroundContainer.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventCloseGroundContainer.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::objects as proto;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventCloseGroundContainer.GameEventCloseGroundContainer
/// `container.Guid.Full`.
#[must_use]
pub fn game_event_close_ground_container(
    session: &mut SessionData,
    container: ObjectGuid,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::CloseGroundContainer,
        GameMessageGroup::UIQueue,
        session,
        &proto::ItemStopViewingObjectContents {
            object: ObjectId(container.full()),
        },
    )
}
