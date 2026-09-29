// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventHouseUpdateRentTime.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventHouseUpdateRentTime.cs`.

use dereth_protocol::trade as proto;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventHouseUpdateRentTime.GameEventHouseUpdateRentTime
#[must_use]
pub fn game_event_house_update_rent_time(session: &mut SessionData) -> GameMessage {
    let rent_time: i32 = 0; // when the current maintenance period began (unix timestamp)
    game_event_from_proto(
        GameEventType::UpdateRentTime,
        GameMessageGroup::UIQueue,
        session,
        &proto::HouseUpdateRentTime { rent_time },
    )
}
