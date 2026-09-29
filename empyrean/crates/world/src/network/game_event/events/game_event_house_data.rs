// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventHouseData.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventHouseData.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::house_data::{self, HouseData};
use crate::sessions::SessionData;

// ACE: GameEventHouseData.GameEventHouseData
#[must_use]
pub fn game_event_house_data(session: &mut SessionData, data: &HouseData) -> GameMessage {
    let mut msg = game_event_message(GameEventType::HouseData, GameMessageGroup::UIQueue, session);
    //Console.WriteLine("Sending 0x225 - GameEventHouseData");
    house_data::write(&mut msg.data, data);
    msg
}
