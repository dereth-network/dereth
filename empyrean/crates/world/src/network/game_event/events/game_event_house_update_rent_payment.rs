// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventHouseUpdateRentPayment.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventHouseUpdateRentPayment.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::house_payment::{self, HousePayment};
use crate::sessions::SessionData;

// ACE: GameEventHouseUpdateRentPayment.GameEventHouseUpdateRentPayment
#[must_use]
pub fn game_event_house_update_rent_payment(session: &mut SessionData) -> GameMessage {
    let mut msg = game_event_message_with_capacity(
        GameEventType::UpdateRentPayment,
        GameMessageGroup::UIQueue,
        session,
        80,
    );
    //Console.WriteLine("Sending 0x228 - House - UpdateRentPayment");
    let payments: Vec<HousePayment> = Vec::new();
    house_payment::write_list(&mut msg.data, &payments);
    msg
}
