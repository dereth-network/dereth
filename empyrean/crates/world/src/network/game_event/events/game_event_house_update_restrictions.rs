// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventHouseUpdateRestrictions.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventHouseUpdateRestrictions.cs`.

use empyrean_net::GameMessageGroup;

use crate::network::game_event::game_event_message::game_event_message;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::sequence::sequence_manager::HasSequences;
use crate::network::sequence::sequence_type::SequenceType;
use crate::network::structure::restriction_db::{self, RestrictionDB};
use crate::sessions::SessionData;

// ACE: GameEventHouseUpdateRestrictions.GameEventHouseUpdateRestrictions
#[must_use]
pub fn game_event_house_update_restrictions(
    session: &mut SessionData,
    obj: &mut impl HasSequences,
    restrictions: &RestrictionDB,
) -> GameMessage {
    let mut msg = game_event_message(
        GameEventType::HouseUpdateRestrictions,
        GameMessageGroup::UIQueue,
        session,
    );
    //Console.WriteLine("Sending 0x248 - House - UpdateRestrictions");
    msg.data.write_bytes(
        &obj.sequences()
            .get_next_sequence(SequenceType::UpdateRestrictionDB),
    );
    msg.data.write_u32(obj.world_object().guid.full()); // The object restrictions are being updated for
    restriction_db::write(&mut msg.data, restrictions);
    msg
}
