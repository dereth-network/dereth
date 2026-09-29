// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventHouseProfile.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventHouseProfile.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::house_profile::{self, HouseProfile};
use crate::sessions::SessionData;

// ACE: GameEventHouseProfile.GameEventHouseProfile
#[must_use]
pub fn game_event_house_profile(
    session: &mut SessionData,
    crystal: ObjectGuid,
    profile: &HouseProfile,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::HouseProfile,
        GameMessageGroup::UIQueue,
        session,
    );
    //Console.WriteLine("Sending 0x21D - GameEventHouseProfile");
    msg.data.write_u32(crystal.full());
    house_profile::write(&mut msg.data, profile);
    msg
}
