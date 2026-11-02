//! What an object's qualities make of its body when the scene makes it: the body takes the
//! profile its qualities give before anything else is done with it (before its physics
//! description, before it is put in the world).
//!
//! The qualities are asked as the object answers them: its description (for the qualities it
//! holds), then its own, then those of the qualities record they derive from, whose own
//! description is asked before its tables.
//!
//! Whether the body moves at all is asked of one qualities record, its entity template's (else its
//! script package's): an object whose record the data does not hold never moves.

use auberean_assets::script::Qualities as Record;
use auberean_client_sdk::model::Object;
use auberean_client_sdk::protocol::types::WeenieDesc;
use auberean_client_sdk::runtime::description;
use auberean_client_sdk::runtime::host::qualities_record;
use auberean_dat::qualified::divine_type;
use auberean_dat::RecordType;
use auberean_physics::body::profile::{Profile, ProfileQualities};
use auberean_physics::body::Body;
use auberean_world::records::WorldRecords;

/// One object's qualities, with its record read once.
struct Asked<'a> {
    o: &'a Object,
    record: Option<Record>,
}

fn find<V: Copy>(t: &[(u32, V)], key: u32) -> Option<V> {
    t.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

impl Asked<'_> {
    fn ask<V>(
        &self,
        desc: impl Fn(&auberean_client_sdk::protocol::types::WeenieDesc) -> Option<V>,
        own: Option<V>,
        table: impl Fn(&Record) -> Option<V>,
    ) -> Option<V> {
        desc(&self.o.weenie).or(own).or_else(|| {
            let r = self.record.as_ref()?;
            r.description
                .as_ref()
                .map(description::of_record)
                .and_then(|w| desc(&w))
                .or_else(|| table(r))
        })
    }
}

impl ProfileQualities for Asked<'_> {
    fn truth(&mut self, key: u32) -> Option<bool> {
        let own = self.o.qualities.bools.get(&key).copied();
        self.ask(|w| description::bool(w, key), own, |r| find(&r.bools, key))
            .map(|v| v != 0)
    }
    fn int(&mut self, key: u32) -> Option<i32> {
        let own = self.o.qualities.ints.get(&key).copied();
        self.ask(|w| description::int(w, key), own, |r| find(&r.ints, key))
    }
    fn float(&mut self, key: u32) -> Option<f32> {
        let own = self.o.qualities.floats.get(&key).copied();
        self.ask(
            |w| description::float(w, key),
            own,
            |r| find(&r.floats, key),
        )
    }
    fn data_id(&mut self, key: u32) -> Option<u32> {
        let own = self.o.qualities.data_ids.get(&key).copied();
        self.ask(
            |w| description::data_id(w, key),
            own,
            |r| find(&r.data_ids, key).map(|d| d.0),
        )
    }
}

/// The profile object `o`'s qualities give, its qualities record (by id) from `record`.
#[must_use]
pub fn profile_of(o: &Object, record: impl FnOnce(u32) -> Option<Record>) -> Profile {
    let id = qualities_record(o);
    let record = (id != 0).then(|| record(id)).flatten();
    Profile::of(&mut Asked { o, record })
}

/// The truth value of a qualities record that says whether what it describes moves.
pub const MOBILE: u32 = 8;

/// The first qualities record id.
const QUALITIES_BASE: u32 = 0x8000_0000;

/// The qualities record that says whether an object moves: its entity template's, when its
/// description names an entity (none when the id is not an entity's), else its script package's;
/// a description with no package names package `0xFFFF_FFFF`.
// D2-08101, D2-07987, D2-07986
#[must_use]
pub fn mobility_record(w: &WeenieDesc) -> u32 {
    let entity = if w.mask & WeenieDesc::ENTITY != 0 {
        w.entity_did
    } else {
        0
    };
    if entity != 0 {
        return if divine_type(entity) == RecordType::ENTITY.0 {
            (QUALITIES_BASE + 0x0100_0000) | (entity & 0x00FF_FFFF)
        } else {
            0
        };
    }
    let package = if w.mask & WeenieDesc::PACKAGE != 0 {
        w.package_id
    } else {
        u32::MAX
    };
    QUALITIES_BASE | (package & 0xFFFF)
}

/// Whether an object with description `w` moves, by the record [`mobility_record`] names, read
/// from `record`: not when the data has no such record; else as the record says (its description,
/// then its truth values), moving when it does not say.
// D2-08101, D2-07254
#[must_use]
pub fn is_mobile(w: &WeenieDesc, record: impl FnOnce(u32) -> Option<Record>) -> bool {
    let id = mobility_record(w);
    let Some(r) = (id != 0).then(|| record(id)).flatten() else {
        return false;
    };
    r.description
        .as_ref()
        .map(description::of_record)
        .and_then(|w| description::bool(&w, MOBILE))
        .or_else(|| find(&r.bools, MOBILE))
        .is_none_or(|v| v != 0)
}

/// Body `b`, just made for object `o`, takes the profile `o`'s qualities give, then moves or not
/// as [`is_mobile`] says.
// D2-08211, D2-08122
pub(crate) fn init_physics(records: &WorldRecords, o: &Object, b: &mut Body) {
    b.apply_profile(&profile_of(o, |id| records.get::<Record>(id)));
    b.set_mobile(is_mobile(&o.weenie, |id| records.get::<Record>(id)));
}
