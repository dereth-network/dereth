// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionQueryBirth.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionQueryBirth.cs`.

use dereth_protocol as proto;
use empyrean_common::dotnet::DotNetDateTime;
use empyrean_entity::enums::ChatMessageType;
use empyrean_net::SessionId;

use crate::network::game_messages::game_message::enqueue_send;
use crate::network::game_messages::messages::game_message_system_chat::game_message_system_chat;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::World;

// ACE: GameActionQueryBirth.Handle
// Not ACE's (retail, V301): as /age, the target is read as the u32 object id
// the client sends (0 for the player itself), and ignored: the
// answer is always about the requester, as for /age (retail's client always sends 0 here).
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let _target = message
        .decode::<proto::admin::CharacterQueryBirth>()?
        .target;

    let player = session_player(w, session);
    let creation_timestamp = w
        .objects
        .get(player)
        .and_then(crate::world_objects::world_object::WorldObject::creation_timestamp)
        .expect("ACE: Nullable object must have a value (CreationTimestamp)");
    let dob_event =
        game_message_system_chat(&birth_text(creation_timestamp), ChatMessageType::Broadcast);
    enqueue_send(w, session, dob_event);
    Ok(())
}

/// The answer's text for a `CreationTimestamp` (seconds after 1970-01-01 UTC): "You were born on
/// M/D/YYYY h:mm:ss AM|PM." in US Eastern time.
// Not ACE's (retail, V333): ACE prints the server machine's local time. Retail
// prints US Eastern time with daylight saving, applying the 2007 US rule to every year (DST from
// 2:00 local on the second Sunday of March to 2:00 local on the first Sunday of November, even for
// births before 2007), whatever zone the host is in. The rule is fixed here: the historical
// Eastern rules would print some pre-2007 births an hour off.
#[must_use]
pub fn birth_text(creation_timestamp: i32) -> String {
    let utc = DotNetDateTime::UNIX_EPOCH.add_seconds(f64::from(creation_timestamp));
    let local = utc.add_hours(f64::from(eastern_offset_hours(utc)));
    format!("You were born on {}.", local.format("M/d/yyyy h:mm:ss tt"))
}

/// US Eastern's offset from UTC, in hours, at a UTC instant: -4 while daylight saving is in force
/// under the 2007 rule, -5 otherwise. DST starts at 2:00 EST (07:00 UTC) on the second Sunday of
/// March and ends at 2:00 EDT (06:00 UTC) on the first Sunday of November. Both changes fall well
/// inside the UTC year, so the UTC year is the local year for the comparison.
fn eastern_offset_hours(utc: DotNetDateTime) -> i32 {
    let year = utc.year();
    let first_sunday = |month: i32| 1 + (7 - DotNetDateTime::new(year, month, 1).day_of_week()) % 7;
    let dst_start = DotNetDateTime::new_hms(year, 3, first_sunday(3) + 7, 7, 0, 0);
    let dst_end = DotNetDateTime::new_hms(year, 11, first_sunday(11), 6, 0, 0);
    if (dst_start.ticks()..dst_end.ticks()).contains(&utc.ticks()) {
        -4
    } else {
        -5
    }
}
