// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameAction/Actions/GameActionQueryAge.cs
//! Port of `Source/ACE.Server/Network/GameAction/Actions/GameActionQueryAge.cs`.

use dereth_protocol as proto;
use empyrean_net::SessionId;

use crate::network::game_event::events::game_event_query_age_response::game_event_query_age_response;
use crate::network::game_messages::game_message::enqueue_send;
use crate::network::managers::inbound_message_manager::{session_player, HandlerResult, Payload};
use crate::World;

// ACE: GameActionQueryAge.Handle
// Not ACE's (retail, V301): the target is read as the u32 object id the client
// sends (0 for the player itself), so /age on any target is answered, and
// the answer is always about the requester, whatever target is sent (a PSR account's selected
// object included); the target is read and ignored.
pub fn handle(w: &mut World, message: &mut Payload<'_>, session: SessionId) -> HandlerResult {
    let _target = message.decode::<proto::admin::CharacterQueryAge>()?.target;

    let player = session_player(w, session);
    let age = w
        .objects
        .get(player)
        .and_then(crate::world_objects::world_object::WorldObject::age)
        .unwrap_or(0);
    let age_msg = calculate_age_message(age);
    let msg =
        game_event_query_age_response(w.sessions.get_mut(session).expect("session"), "", &age_msg);
    enqueue_send(w, session, msg);
    Ok(())
}

const SECONDS_PER_MINUTE: i32 = 60;
const MINUTES_PER_HOUR: i32 = 60;
const HOURS_PER_DAY: i32 = 24;
const DAYS_PER_MONTH: i32 = 30;

const SECONDS_PER_HOUR: i32 = SECONDS_PER_MINUTE * MINUTES_PER_HOUR; // 3600
const SECONDS_PER_DAY: i32 = SECONDS_PER_HOUR * HOURS_PER_DAY; // 86400
const SECONDS_PER_MONTH: i32 = SECONDS_PER_DAY * DAYS_PER_MONTH; // 2592000

// ACE: GameActionQueryAge.CalculateAgeMessage
// Not ACE's (retail, V312): retail's text, `[Nmo] [Nd] [Nh] [Nm] Ns`. Months are
// 30 days and are the largest unit (no years, no weeks: 26 months prints as `26mo`, 18 days as
// `18d`); a zero month, day, hour or minute is left out, but the seconds are always printed, zero
// included (`5h 44m 0s`, `1h 1s`). A negative age is treated as zero (`0s`).
pub fn calculate_age_message(age_seconds: i32) -> String {
    let mut remaining = age_seconds.max(0);

    let months = remaining / SECONDS_PER_MONTH;
    remaining -= months * SECONDS_PER_MONTH;

    let days = remaining / SECONDS_PER_DAY;
    remaining -= days * SECONDS_PER_DAY;

    let hours = remaining / SECONDS_PER_HOUR;
    remaining -= hours * SECONDS_PER_HOUR;

    let minutes = remaining / SECONDS_PER_MINUTE;
    remaining -= minutes * SECONDS_PER_MINUTE;

    let seconds = remaining;

    let mut pieces = Vec::new();

    if months > 0 {
        pieces.push(format!("{months}mo"));
    }
    if days > 0 {
        pieces.push(format!("{days}d"));
    }
    if hours > 0 {
        pieces.push(format!("{hours}h"));
    }
    if minutes > 0 {
        pieces.push(format!("{minutes}m"));
    }
    pieces.push(format!("{seconds}s"));

    pieces.join(" ")
}
