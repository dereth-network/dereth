// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventSalvageOperationsResult.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventSalvageOperationsResult.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::Skill;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_message::session_player;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::BinaryWriter;
use crate::network::game_messages::game_message::GameMessage;
use crate::network::structure::salvage_result::{self, SalvageMessage};
use crate::World;

// ACE: GameEventSalvageOperationsResult.GameEventSalvageOperationsResult
/// `new SalvageResult(message)` and its writer.
#[must_use]
pub fn game_event_salvage_operations_result(
    w: &mut World,
    session: SessionId,
    skill: Skill,
    messages: &[SalvageMessage],
) -> GameMessage {
    // 40 is the max seen in retail pcaps
    let mut msg = game_event_message_with_capacity(
        GameEventType::SalvageOperationsResult,
        GameMessageGroup::UIQueue,
        session_data(w, session),
        40,
    );
    msg.data.write_u32(skill.0.cs_cast());
    msg.data.write_i32(0); // not salvagable item guid list?
    msg.data
        .write_i32(i32::try_from(messages.len()).unwrap_or(i32::MAX));
    for message in messages {
        salvage_result::write(&mut msg.data, &salvage_result::salvage_result_new(message));
    }
    let bonus = if skill == Skill::Salvaging {
        let player = session_player(w, session);
        let player = w
            .objects
            .get(player)
            .expect("ACE: session.Player is null (NullReferenceException)");
        // `int * 25`, unchecked.
        player.augmentation_bonus_salvage().wrapping_mul(25)
    } else {
        0
    };
    msg.data.write_i32(bonus);
    msg
}
