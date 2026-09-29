// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventHouseAvailableHouses.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventHouseAvailableHouses.cs`.

use dereth_protocol::trade as proto;
use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::HouseType;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_from_proto;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::sessions::SessionData;

// ACE: GameEventHouseAvailableHouses.GameEventHouseAvailableHouses
#[must_use]
pub fn game_event_house_available_houses(
    session: &mut SessionData,
    r#type: HouseType,
    locations: &[u32],
    total_available: i32,
) -> GameMessage {
    //Console.WriteLine("Sending 0x271 - GameEvent - AvailableHouses");
    game_event_from_proto(
        GameEventType::AvailableHouses,
        GameMessageGroup::UIQueue,
        session,
        &proto::HouseAvailableHouses {
            house_type: r#type.0.cs_cast(),
            landcells: locations.to_vec(),
            num_houses: total_available,
        },
    )
}
