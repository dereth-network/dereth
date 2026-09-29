// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventWieldItem.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventWieldItem.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::objects as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::EquipMask;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventWieldItem.GameEventWieldItem
#[must_use]
pub fn game_event_wield_item(
    session: &mut SessionData,
    object_id: u32,
    new_location: EquipMask,
) -> GameMessage {
    let new_location: i32 = new_location.0.cs_cast();
    game_event_from_proto(
        GameEventType::WieldObject,
        GameMessageGroup::UIQueue,
        session,
        &proto::ItemWearItem {
            item: ObjectId(object_id),
            slot: new_location.cast_unsigned(),
        },
    )
}
