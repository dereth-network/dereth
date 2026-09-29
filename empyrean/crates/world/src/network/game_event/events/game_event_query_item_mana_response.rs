// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventQueryItemManaResponse.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventQueryItemManaResponse.cs`.

use dereth_primitives::ObjectId;
use dereth_protocol::items as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventQueryItemManaResponse.GameEventQueryItemManaResponse
#[must_use]
pub fn game_event_query_item_mana_response(
    session: &mut SessionData,
    target: u32,
    mana: f32,
    success: u32,
) -> GameMessage {
    let body = proto::ItemQueryItemManaResponse {
        object: ObjectId(target),
        mana,
        success,
    };
    game_event_from_proto(
        GameEventType::QueryItemManaResponse,
        GameMessageGroup::UIQueue,
        session,
        &body,
    )
}
