// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventHouseUpdateHAR.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventHouseUpdateHAR.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::house_access;
use crate::World;

// ACE: GameEventUpdateHAR.GameEventUpdateHAR
/// Update House Access Records: `new HouseAccess(house)` and its writer.
#[must_use]
pub fn game_event_update_har(w: &mut World, session: SessionId, house: ObjectGuid) -> GameMessage {
    // Only 40 and 56 seen in retail pcaps
    let mut msg = game_event_message_with_capacity(
        GameEventType::UpdateHAR,
        GameMessageGroup::UIQueue,
        session_data(w, session),
        56,
    );
    //Console.WriteLine("Sending 0x257 - Update House Access Records (HAR)");
    let har = house_access::house_access_new(w, Some(house));
    house_access::write(&mut msg.data, &har);
    msg
}
