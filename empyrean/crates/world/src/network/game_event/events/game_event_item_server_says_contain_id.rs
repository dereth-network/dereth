// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventItemServerSaysContainId.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventItemServerSaysContainId.cs`.

use dereth_protocol::objects as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;
use crate::world_objects::world_object::WorldObject;

// ACE: GameEventItemServerSaysContainId.GameEventItemServerSaysContainId
#[must_use]
pub fn game_event_item_server_says_contain_id(
    session: &mut SessionData,
    item_to_be_contained: &WorldObject,
    container: ObjectGuid,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::InventoryPutObjInContainer,
        GameMessageGroup::UIQueue,
        session,
        &proto::ItemServerSaysContainId {
            item: item_to_be_contained.guid.into(),
            container: container.into(),
            slot: item_to_be_contained
                .placement_position()
                .unwrap_or(0)
                .cast_unsigned(),
            container_properties: item_to_be_contained.container_type().0.cs_cast(),
        },
    )
}
