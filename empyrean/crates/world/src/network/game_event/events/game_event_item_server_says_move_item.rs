// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventItemServerSaysMoveItem.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventItemServerSaysMoveItem.cs`.

use dereth_protocol::objects as proto;
use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventItemServerSaysMoveItem.GameEventItemServerSaysMoveItem
#[must_use]
pub fn game_event_item_server_says_move_item(
    session: &mut SessionData,
    world_object: ObjectGuid,
) -> GameMessage {
    game_event_from_proto(
        GameEventType::InventoryPutObjectIn3D,
        GameMessageGroup::UIQueue,
        session,
        &proto::ItemServerSaysMoveItem {
            item: world_object.into(),
        },
    )
}
