//! The look of an object the client was sent: its visual description followed to the record it
//! shows, or put together from its chain with its own changes last.

use auberean_animation::look::{Desc, Group, Keyed, Look, ObjectLook, GLOBAL_KEY};
use auberean_animation::CachedRecords;
use auberean_assets::common::Color32;
use auberean_assets::visual::IconLayer;
use auberean_client_sdk::protocol::table::{Table, BUCKET_COUNTS};
use auberean_client_sdk::protocol::types::{PartGroupDesc as WireGroup, VisualDesc as WireVisual};
use auberean_primitives::{DataId, Vec3};
use auberean_world::place::PlaceRecords;
use auberean_world::records::WorldRecords;

use crate::BLOCK_LENGTH;

/// The visual description a wire description shows: its own record, else the record it starts
/// from, an entity's object followed to its visual description.
#[must_use]
pub fn visual_of(records: &WorldRecords, v: &WireVisual) -> Option<DataId> {
    let mut id = if v.bits & WireVisual::DID != 0 && v.did != 0 {
        v.did
    } else if v.bits & WireVisual::PARENT != 0 && v.parent != 0 {
        v.parent
    } else {
        return None;
    };
    for _ in 0..8 {
        match id >> 24 {
            0x1F => return Some(DataId(id)),
            0x47 => id = records.entity(DataId(id))?.object?,
            _ => return None,
        }
    }
    None
}

fn buckets_of(size_class: u8) -> u32 {
    BUCKET_COUNTS
        .get(usize::from(size_class))
        .copied()
        .unwrap_or(BUCKET_COUNTS[0])
}

fn keyed<V: Clone>(t: &Table<V>) -> Keyed<V> {
    Keyed::stored(buckets_of(t.size_class), t.entries.clone())
}

fn group_of(g: &WireGroup, key: u32) -> Group {
    let has = |bit: u32| g.bits & bit != 0;
    let id = |bit: u32, v: u32| DataId(if has(bit) { v } else { 0 });
    let appearance = if has(WireGroup::APPEARANCE) {
        Keyed::stored(
            buckets_of(g.appearance.size_class),
            g.appearance
                .entries
                .iter()
                .map(|(apr, keys)| (*apr, keyed(keys)))
                .collect(),
        )
    } else {
        Keyed::default()
    };
    Group {
        key,
        parent_key: if has(WireGroup::PARENT_KEY) {
            g.parent_key
        } else {
            0
        },
        connection: if has(WireGroup::CONNECTION_POINT) {
            g.connection_point
        } else {
            0
        },
        setup: id(WireGroup::SETUP, g.setup),
        anim_map: id(WireGroup::ANIM_MAP, g.anim_map),
        appearance,
        fx_table: id(WireGroup::FX_TABLE, g.fx_table),
        startup_fx: if has(WireGroup::STARTUP_FX) {
            keyed(&g.startup_fx)
        } else {
            Keyed::default()
        },
        ..Group::default()
    }
}

/// A description as the client holds one it was sent: its fields, its changes to every group,
/// and its groups hung from their parents by key, in the order sent. A parent named by an entity
/// is that entity's description.
// D2-13814, D2-06199
#[must_use]
pub fn desc_of(records: &WorldRecords, v: &WireVisual) -> Desc {
    let has = |bit: u32| v.bits & bit != 0;
    let id = |bit: u32, x: u32| DataId(if has(bit) { x } else { 0 });
    let mut parent = id(WireVisual::PARENT, v.parent);
    if parent.0 >> 24 == 0x47 {
        parent = records
            .entity(parent)
            .and_then(|e| e.object)
            .map_or(DataId::NONE, DataId);
    }
    let mut groups: Vec<Group> = if has(WireVisual::PART_GROUPS) {
        v.part_groups
            .entries
            .iter()
            .map(|(k, g)| {
                let key = if g.bits & WireGroup::KEY != 0 {
                    g.key
                } else {
                    *k
                };
                group_of(g, key)
            })
            .collect()
    } else {
        Vec::new()
    };
    for i in 0..groups.len() {
        let pk = groups[i].parent_key;
        if pk == 0 {
            continue;
        }
        if let Some(p) = groups.iter().position(|g| g.key == pk) {
            let child = groups[i].key;
            groups[p].children.push(child);
            groups[i].parent = Some(pk);
        }
    }
    let mut global = if has(WireVisual::GLOBAL) {
        group_of(&v.global, GLOBAL_KEY)
    } else {
        Group::default()
    };
    global.key = GLOBAL_KEY;
    let f = |bit: u32, x: f32| if has(bit) { x } else { 1.0 };
    Desc {
        parent,
        motion_desc: id(WireVisual::MOTION_INTERP, v.motion_interp),
        behavior_table: id(WireVisual::BEHAVIOR_TABLE, v.behavior_table),
        modes: id(WireVisual::MODES, v.modes),
        scale: if has(WireVisual::SCALE) {
            v.scale
        } else {
            Vec3::new(1.0, 1.0, 1.0)
        },
        child_scale: f(WireVisual::CHILD_SCALE, v.child_scale),
        particle_scale: f(WireVisual::PARTICLE_SCALE, v.particle_scale),
        global,
        groups,
        icon: if has(WireVisual::ICON) {
            v.icon
                .iter()
                .map(|l| IconLayer {
                    layer: l.0,
                    picture: DataId(l.1),
                    shift_color: Color32(l.2),
                })
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// The total look a wire description gives: its own record's when it names one, else the object's
/// own description (a copy of the one sent) put together with its chain, its own changes last.
#[must_use]
pub fn look_of(records: &WorldRecords, anims: &CachedRecords<'_>, v: &WireVisual) -> Option<Look> {
    let mut dress = None;
    look_after(records, anims, v, &mut dress)
}

/// The object's own descriptions once the server sent it `v` (`dress`, what it kept before):
/// its own set to a copy of `v` with the appearance its effects lay kept over it; none for a
/// description that names its record or adds nothing. The total look to draw: the named record's,
/// else its own put together with its chain.
pub(crate) fn look_after(
    records: &WorldRecords,
    anims: &CachedRecords<'_>,
    v: &WireVisual,
    dress: &mut Option<ObjectLook>,
) -> Option<Look> {
    if v.bits & WireVisual::DID != 0 && v.did != 0 {
        *dress = None;
        return Look::of_record(anims, visual_of(records, v)?).ok();
    }
    let desc = desc_of(records, v);
    if desc.parent.is_none() && desc.groups.is_empty() {
        *dress = None;
        return None;
    }
    match dress {
        Some(d) => d.set_local(&desc),
        None => *dress = Some(ObjectLook::new(&desc)),
    }
    dress.as_ref()?.total(anims).ok()
}

/// Where an object is reported, on one grid across every block: meters east and north of the
/// world's south-west corner, and up.
pub(crate) fn global(cell: u32, origin: Vec3) -> Vec3 {
    #[allow(clippy::cast_precision_loss)]
    Vec3::new(
        (cell >> 24) as f32 * BLOCK_LENGTH + origin.x,
        ((cell >> 16) & 0xFF) as f32 * BLOCK_LENGTH + origin.y,
        origin.z,
    )
}

/// An opacity under this is none: the figure is not to be seen.
const UNSEEN: f32 = 0.0002;

/// Whether a figure is to be seen: its animation's opacity (its fade slider, held to 0..1) is not
/// under 0.0002.
// D2-05875
#[must_use]
pub fn is_visible(figure: &auberean_animation::body::Body) -> bool {
    let opacity = figure
        .sliders
        .current(auberean_physics::body::parent::FADE_SLIDER)
        .clamp(0.0, 1.0);
    opacity >= UNSEEN
}
