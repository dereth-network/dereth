//! What a body asks of a setup: a bone's type and parent, the bone of a type, the places it holds
//! things and where things attach, its named poses, the records it names; and the setup packed
//! back into its stored form.
//!
//! A setup keeps its poses and its two kinds of places in small tables by kind, filled in their
//! stored order with each new entry put first among those of its bucket: so where two share a
//! kind the one stored last is found, and packing writes them bucket by bucket, each bucket's
//! entries from the last stored to the first. The poses' table has two buckets, the places' four;
//! a kind's bucket is `(kind >> 8 ^ kind)` masked to the bucket count.

use std::collections::HashMap;

use auberean_assets::setup::{DetailDistances, Location, Pose, Setup};
use auberean_dat::writer::Writer;
use auberean_primitives::DataId;

/// The buckets of a setup's pose table and of its place tables.
const POSE_BUCKETS: u32 = 2;
const PLACE_BUCKETS: u32 = 4;
/// How far a kind is shifted before it is folded into its bucket.
const KIND_SHIFT: u32 = 8;

/// A bone's parent; `u32::MAX` for a root or a setup without bones.
// D2-03464
#[must_use]
pub fn parent_index(setup: &Setup, bone: usize) -> u32 {
    setup.bones.get(bone).map_or(u32::MAX, |b| b.parent)
}

/// The bone of each type: the first bone of the type wins.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoneTypes(HashMap<u32, u32>);

impl BoneTypes {
    /// The table a setup builds once loaded (its rest pose is the skeleton's, see
    /// [`crate::skeleton`]).
    // D2-12908
    #[must_use]
    pub fn new(setup: &Setup) -> Self {
        let mut t = HashMap::new();
        for (i, b) in setup.bones.iter().enumerate() {
            t.entry(b.id)
                .or_insert(u32::try_from(i).unwrap_or(u32::MAX));
        }
        Self(t)
    }

    /// The first bone of a type; `None` when no bone is of it.
    // D2-12906
    #[must_use]
    pub fn find(&self, kind: u32) -> Option<u32> {
        self.0.get(&kind).copied()
    }
}

fn bucket(kind: u32, buckets: u32) -> u32 {
    ((kind >> KIND_SHIFT) ^ kind) & (buckets - 1)
}

/// The entries of a table filled in stored order, each first in its bucket: bucket by bucket, each
/// bucket from the last stored to the first.
fn table_order<T>(items: &[T], kind: impl Fn(&T) -> u32, buckets: u32) -> Vec<&T> {
    let mut out = Vec::with_capacity(items.len());
    for b in 0..buckets {
        out.extend(items.iter().rev().filter(|x| bucket(kind(x), buckets) == b));
    }
    out
}

/// Where the setup holds things of a kind (the last stored of the kind).
// D2-12895
#[must_use]
pub fn holding_location(setup: &Setup, kind: u32) -> Option<&Location> {
    setup
        .holding_locations
        .iter()
        .rev()
        .find(|l| l.kind == kind)
}

/// Where things of a kind attach to the setup (the last stored of the kind).
// D2-12896
#[must_use]
pub fn connection_point(setup: &Setup, kind: u32) -> Option<&Location> {
    setup
        .connection_points
        .iter()
        .rev()
        .find(|l| l.kind == kind)
}

/// The setup's pose of a kind (the last stored of the kind).
// D2-12897
#[must_use]
pub fn placement_frame(setup: &Setup, kind: u32) -> Option<&Pose> {
    setup.poses.iter().rev().find(|p| p.kind == kind)
}

/// The records a setup names, for the data cache to fetch with it: its meshes.
// D2-12871
#[must_use]
pub fn sub_ids(setup: &Setup) -> Vec<DataId> {
    setup.meshes.clone()
}

/// A pose packed: its kind, its bone count, each bone's transform.
// D2-12866, D2-13261
fn pack_pose(w: &mut Writer, p: &Pose) {
    w.u32(p.kind);
    w.list(&p.bones, |w, b| {
        w.vec3(b.offset);
        w.vec3(b.scale);
        w.quat(b.rotation);
    });
}

/// The detail distances packed, level by level.
// D2-12868
fn pack_detail(w: &mut Writer, d: &[DetailDistances; 3]) {
    for l in d {
        for v in [
            l.visible,
            l.near_degrade,
            l.ideal_degrade,
            l.far_degrade,
            l.high_detail,
            l.medium_detail,
        ] {
            w.f32(v);
        }
    }
}

fn pack_places(w: &mut Writer, places: &[Location]) {
    let order = table_order(places, |l| l.kind, PLACE_BUCKETS);
    w.count(order.len());
    for l in order {
        w.u32(l.kind);
        w.u32(l.bone);
        w.vec3(l.transform.offset);
        w.vec3(l.transform.scale);
        w.quat(l.transform.rotation);
    }
}

/// Setup `id` packed into its stored form: the id; the flags (only whether it turns freely and
/// whether it has a physics tree survive); the meshes; the bones (parent, type); the poses and
/// the two kinds of places in their tables' order; the cylinders and spheres; the heights and
/// radius; the bounding and selection spheres; the detail distances; the shadow kind; the
/// physics radius and bounds.
// D2-12899
#[must_use]
pub fn pack(id: u32, setup: &Setup) -> Vec<u8> {
    let mut w = Writer::new();
    w.u32(id);
    let mut flags = 0;
    if setup.turns_freely() {
        flags |= 0x4;
    }
    if setup.has_physics_tree() {
        flags |= 0x8;
    }
    w.u32(flags);
    w.list(&setup.meshes, |w, m| w.data_id(*m));
    w.list(&setup.bones, |w, b| {
        w.u32(b.parent);
        w.u32(b.id);
    });
    let poses = table_order(&setup.poses, |p| p.kind, POSE_BUCKETS);
    w.count(poses.len());
    for p in poses {
        pack_pose(&mut w, p);
    }
    pack_places(&mut w, &setup.holding_locations);
    pack_places(&mut w, &setup.connection_points);
    w.list(&setup.cylinders, |w, c| {
        w.vec3(c.low);
        w.f32(c.radius);
        w.f32(c.height);
    });
    w.list(&setup.spheres, |w, s| w.sphere(*s));
    for v in [
        setup.height,
        setup.radius,
        setup.step_up_height,
        setup.step_down_height,
    ] {
        w.f32(v);
    }
    w.sphere(setup.bounding_sphere);
    w.sphere(setup.selection_sphere);
    pack_detail(&mut w, &setup.detail);
    w.u32(setup.shadow);
    w.f32(setup.physics_radius);
    w.bbox(setup.physics_bounds);
    w.finish()
}

/// The size of a setup's packed form, in bytes.
// D2-12898, D2-13260
#[must_use]
pub fn pack_size(setup: &Setup) -> usize {
    let transform = 40;
    let places = |l: &[Location]| 4 + l.len() * (8 + transform);
    4 + 4
        + 4
        + 4 * setup.meshes.len()
        + 4
        + 8 * setup.bones.len()
        + 4
        + setup
            .poses
            .iter()
            .map(|p| 8 + transform * p.bones.len())
            .sum::<usize>()
        + places(&setup.holding_locations)
        + places(&setup.connection_points)
        + 4
        + 20 * setup.cylinders.len()
        + 4
        + 16 * setup.spheres.len()
        + 16
        + 16
        + 16
        + 72
        + 4
        + 4
        + 24
}
