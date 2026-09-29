// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/GameEvent/Events/GameEventFellowshipFullUpdate.cs
//! Port of `Source/ACE.Server/Network/GameEvent/Events/GameEventFellowshipFullUpdate.cs`.

use empyrean_entity::ObjectGuid;
use empyrean_net::GameMessageGroup;
use empyrean_net::SessionId;

use crate::entity::fellowship;
use crate::network::game_event::game_event_message::game_event_message_with_capacity;
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_event::game_event_type::GameEventType;
use crate::network::game_messages::game_message::{ace_str, GameMessage};
use crate::network::structure::fellowship_lock_data;
use crate::network::structure::hash_comparer::{self, HashComparer};
use crate::world_objects::entity::creature_attribute::StatCtx;
use crate::world_objects::player_fellowship;
use crate::World;
use dereth_protocol::archive::PackedHash;
use dereth_protocol::social as proto;

/// `GameEventFellowshipFullUpdate.FellowComparer`: `new HashComparer(16)`.
const FELLOW_COMPARER: HashComparer = HashComparer::new(16);

// ACE: GameEventFellowshipFullUpdate.GameEventFellowshipFullUpdate
/// 338 is the average seen in retail pcaps, 1,264 is the max seen in retail pcaps (the 512
/// capacity is only a hint). ACE returns an empty body when the player has no fellowship.
#[must_use]
pub fn game_event_fellowship_full_update(w: &mut World, session: SessionId) -> GameMessage {
    let mut msg = game_event_message_with_capacity(
        GameEventType::FellowshipFullUpdate,
        GameMessageGroup::UIQueue,
        session_data(w, session),
        512,
    );

    let player = w
        .sessions
        .player(session)
        .expect("ACE: session.Player is null (NullReferenceException)");
    let Some(fellowship) = player_fellowship::fellowship(w, player) else {
        return msg;
    };

    let fellows = fellowship::get_fellowship_members(w, &fellowship);
    let _ = i32::try_from(fellows.len()).expect("a dictionary count is an int");

    let sorted = hash_comparer::sorted(fellows.iter().map(|(k, v)| (*k, *v)), &FELLOW_COMPARER);

    // Each fellow is its guid (the table's key) and `WriteFellow`'s record.
    let members = sorted
        .into_iter()
        .map(|(_, fellow)| (fellow.full(), fellow_wire_record(w, fellow)))
        .collect();

    let f = fellowship.get(w);
    let body = proto::FellowshipFullUpdate(proto::Fellowship {
        members: PackedHash {
            table_size: u32::from(FELLOW_COMPARER.num_buckets),
            entries: members,
        },
        name: ace_str(f.fellowship_name.as_str()),
        leader: dereth_primitives::ObjectId(f.fellowship_leader_guid),
        share_xp: i32::from(f.share_xp),
        even_xp_split: i32::from(f.even_share),
        open_fellow: i32::from(f.open),
        locked: i32::from(f.is_locked),
        fellows_departed: fellowship::departed_fellows_record(&f.departed_members),
        // The lock table after the departed fellows: the retail server sent it too, empty for an
        // unlocked fellowship and holding the locks of a locked one (V256).
        locks: fellowship_lock_data::locks_record(&f.fellowship_locks),
    });
    let mut strings: Vec<&str> = body
        .0
        .members
        .entries
        .iter()
        .map(|(_, m)| m.name.as_str())
        .collect();
    strings.push(body.0.name.as_str());
    strings.extend(body.0.locks.0.entries.iter().map(|(k, _)| k.as_str()));
    msg.write_proto_strings(&body, &strings);

    msg
}

/// What [`write_fellow`] writes, as dereth-protocol's fellow record: the ten dwords in ACE's order,
/// then the name.
fn fellow_wire_record(w: &mut World, fellow: ObjectGuid) -> proto::Fellow {
    let r = fellow_record(w, fellow);
    proto::Fellow {
        cp_cache: 0,  // TODO: cpCached - Perhaps cp stored up before distribution?
        lum_cache: 0, // TODO: lumCached - Perhaps lum stored up before distribution?
        level: r.level.cast_unsigned(),
        max_health: r.max[0],
        max_stamina: r.max[1],
        max_mana: r.max[2],
        current_health: r.current[0],
        current_stamina: r.current[1],
        current_mana: r.current[2],
        // todo: share loot with this fellow?
        share_loot: 0x10, // TODO: shareLoot - if 0 then noSharePhatLoot, if 16(0x0010) then sharePhatLoot
        name: ace_str(r.name.as_str()),
    }
}

/// `fellow.Level ?? 1`, the max and current vitals and the name, read in ACE's order.
pub(crate) struct FellowRecord {
    pub level: i32,
    pub max: [u32; 3],
    pub current: [u32; 3],
    pub name: String,
}

/// Reads what `WriteFellow` and `GameEventFellowshipUpdateFellow` write about a fellow.
pub(crate) fn fellow_record(w: &mut World, fellow: ObjectGuid) -> FellowRecord {
    let o = w
        .objects
        .get(fellow)
        .expect("ACE: fellow is null (NullReferenceException)");
    let (health, stamina, mana) = (o.health(), o.stamina(), o.mana());
    let level = o.level().unwrap_or(1);
    let current = [health.current(o), stamina.current(o), mana.current(o)];
    let name = crate::dispatch::name::name(w, fellow).unwrap_or_default();
    let c = &mut StatCtx::in_world(w, fellow);
    let max = [health.max_value(c), stamina.max_value(c), mana.max_value(c)];
    FellowRecord {
        level,
        max,
        current,
        name,
    }
}

// ACE: GameEventFellowshipFullUpdate.WriteFellow
pub fn write_fellow(w: &mut World, msg: &mut GameMessage, fellow: ObjectGuid) {
    let record = fellow_wire_record(w, fellow);
    let strings = [record.name.as_str()];
    crate::network::game_messages::game_message::write_record(&mut msg.data, &strings, |wr| {
        wr.u32(fellow.full());
        record.write(wr)
    });
}
