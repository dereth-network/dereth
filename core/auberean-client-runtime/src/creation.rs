//! An object's creation asks the data cache for the records it is made from, as the client does
//! for every object the server creates.
//!
//! The records are its visual description's (its parent description, motion description and
//! behavior table, every part group's setup, animation map, appearance tables and effect table,
//! its icon's pictures; the valid modes are left out for this gathering), then the object's script
//! state and qualities records and its entity: by its entity when it has one, else by its
//! package. They are asked for together, each with the records it names in turn, without waiting:
//! one in neither memory nor the files is asked of the server, and a refusal fails it. The script
//! state names its records through the client's scripts (the records its scenes, string references
//! and race and sex looks need, and the other states it names, each asked in turn).
//!
//! The client makes the object only when every request has finished, whatever their outcome:
//! a refused record leaves the object made without it. Here the object is made at once and the
//! records come into the data cache behind it (divergence `CD2-objects-made-before-their-records`).

use auberean_assets::requests::{Batch, Listener, StateRecords};
use auberean_dat::qualified::divine_type;
use auberean_dat::{QualifiedId, QualifiedIds, RecordType};
use auberean_protocol::physics::CreateObject;
use auberean_protocol::types::{PartGroupDesc, VisualDesc, WeenieDesc};

use crate::download::DataDownload;

/// The gathering an object's records are asked for by (it leaves the valid modes out).
pub const GATHER_CREATION: u32 = 0x4000_0009;

/// The records a part group's description names: its setup and animation map, each appearance
/// table it keys, and its effect table.
// D2-14043
fn part_group_sub_ids(g: &PartGroupDesc, out: &mut QualifiedIds) {
    for id in [g.setup, g.anim_map] {
        out.add(id);
    }
    for (apr, _) in &g.appearance.entries {
        out.add(*apr);
    }
    out.add(g.fx_table);
}

/// The records a visual description names, as an object's creation gathers them: its parent
/// description, motion description and behavior table, its valid modes (unless the gathering is
/// creation's), the whole object's part group and each part group, and its icon's pictures.
// D2-13806
pub fn visual_sub_ids(v: &VisualDesc, gathering: u32, out: &mut QualifiedIds) {
    for id in [v.parent, v.motion_interp, v.behavior_table] {
        out.add(id);
    }
    if gathering != GATHER_CREATION {
        out.add(v.modes);
    }
    part_group_sub_ids(&v.global, out);
    for (_, g) in &v.part_groups.entries {
        part_group_sub_ids(g, out);
    }
    for layer in &v.icon {
        out.add(layer.1);
    }
}

/// A type's first record id (0 for a type with none).
fn base(kind: RecordType) -> u32 {
    kind.base_id()
}

/// The records an object's creation asks for: its visual description's, then its script state
/// and qualities records, by its entity when it has one (an entity's id: no records otherwise),
/// else by the low word of its package, then the entity itself.
// D2-05724, D2-12021, D2-12023
#[must_use]
pub fn records(c: &CreateObject) -> Vec<QualifiedId> {
    let mut set = QualifiedIds::new();
    visual_sub_ids(&c.visual, GATHER_CREATION, &mut set);
    let w: &WeenieDesc = &c.weenie;
    let entity = if w.mask & WeenieDesc::ENTITY != 0 {
        w.entity_did
    } else {
        0
    };
    let (state, qualities) = if entity == 0 {
        let package = if w.mask & WeenieDesc::PACKAGE != 0 {
            w.package_id
        } else {
            u32::MAX
        };
        let low = package & 0xFFFF;
        (
            base(RecordType::SCRIPT_STATE).wrapping_add(0x0100_0000) | low,
            base(RecordType::QUALITIES) | low,
        )
    } else if divine_type(entity) == RecordType::ENTITY.0 {
        let low = entity & 0x00FF_FFFF;
        (
            base(RecordType::SCRIPT_STATE).wrapping_add(0x0200_0000) | low,
            base(RecordType::QUALITIES).wrapping_add(0x0100_0000) | low,
        )
    } else {
        (0, 0)
    };
    set.add_typed(state, RecordType::SCRIPT_STATE.0);
    set.add_typed(qualities, RecordType::QUALITIES.0);
    set.add_typed(entity, RecordType::ENTITY.0);
    set.iter().collect()
}

/// Who is told when an object's records have all finished: the creation itself.
pub const CREATION: Listener = Listener {
    to: crate::download::listeners::CREATION,
    user: 0,
};

/// Ask the data cache for an object's records, as its creation starts (`states` names a saved
/// state's records). The batch they are in.
// D2-05724
pub fn start(
    download: &mut DataDownload,
    c: &CreateObject,
    states: Option<&dyn StateRecords>,
) -> Batch {
    let ids = records(c);
    let batch = download.get_many_named(GATHER_CREATION, &ids, Some(CREATION), states);
    download.creating(batch, c.id, ids);
    batch
}

/// Every record of an object's creation has finished, whatever the outcome: the object is made
/// from them (here it was made already, so it takes them now): each record that came goes into
/// its type's cache, held by the object while it lives, so the next object made from it finds it
/// in memory. Then the creation lets its batch go, and the requests nobody else waits on are
/// forgotten, so a refused record is asked for again by the next object made from it.
// D2-05731, D2-05737, D2-05739
pub fn finish(download: &mut DataDownload, batch: Batch) {
    download.made(batch);
    download.release(batch);
}
