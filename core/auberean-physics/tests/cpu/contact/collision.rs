//! A body's collisions: what it passes through, the touches it reports beginning and ending, the
//! pushes of what it touches, and the binary form of all of it.
//! Fixture: a hand-built property list and a world of bodies held in a map.

use std::collections::BTreeMap;

use auberean_assets::property::{
    PhysicsMaterial, Property, PropertyDescriptor, PropertyGroup, PropertyList, PropertyValue,
};
use auberean_assets::Context;
use auberean_dat::Reader;
use auberean_physics::collision::manager::{push, BodyKey, CollisionHost};
use auberean_physics::collision::{
    change, CollisionReport, CollisionSettings, Collisions, ContactProperty, ContactRecord, Push,
};
use auberean_physics::ethereal::{PhysicsTypes, SightLine, WATER};
use auberean_physics::pack::{Pack, Writer};
use auberean_primitives::Vec3 as AssetVec3;
use dereth_primitives::shape::Plane;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

const BOOL: u32 = 0x4000_0001;
const VECTOR: u32 = 0x4000_0004;
const ENUM: u32 = 0x4000_0007;

fn descriptor(value_kind: u32, data: u32, report: u32) -> PropertyDescriptor {
    PropertyDescriptor {
        value_kind,
        group: 0,
        data,
        report,
        required: false,
    }
}

/// Flags `0x4000_0100 + n` take bit `n`; the push properties are vectors and the local flag a
/// flag, all reported; the terrain property an enum.
fn property_list() -> PropertyList {
    let mut descriptors = Vec::new();
    for n in 0..64 {
        descriptors.push((0x4000_0100 + n, descriptor(BOOL, n, 0)));
    }
    descriptors.push((WATER, descriptor(BOOL, 5, 0)));
    for p in push::WALK_ON_AIR_VELOCITY..=push::WATER_ACCELERATION {
        descriptors.push((p, descriptor(VECTOR, 0, 0)));
    }
    descriptors.push((push::LOCAL, descriptor(BOOL, 70, 0)));
    descriptors.push((0x4000_0036, descriptor(ENUM, 0, 1)));
    PropertyList::from_descriptors(descriptors)
}

fn vector(id: u32, x: f32, y: f32, z: f32) -> Property {
    Property {
        id,
        value_type: VECTOR,
        group: 0,
        value: PropertyValue::Vector(AssetVec3 { x, y, z }),
    }
}

fn origin(cell: u32) -> Position {
    Position::new(CellId(cell), Frame::new(Vec3::ZERO, Quat::IDENTITY))
}

/// A world of bodies: each has an object id and may carry properties.
struct World {
    list: PropertyList,
    now: f64,
    ids: BTreeMap<BodyKey, u64>,
    properties: BTreeMap<BodyKey, Vec<PropertyGroup>>,
    removed: Vec<(BodyKey, u64)>,
}

impl World {
    fn new() -> Self {
        Self {
            list: property_list(),
            now: 0.0,
            ids: BTreeMap::from([(1, 0x100), (2, 0x200)]),
            properties: BTreeMap::new(),
            removed: Vec::new(),
        }
    }
}

impl CollisionHost for World {
    fn key(&self) -> BodyKey {
        1
    }
    fn object_id(&self, body: BodyKey) -> u64 {
        self.ids.get(&body).copied().unwrap_or(0)
    }
    fn find(&self, object: u64) -> Option<BodyKey> {
        self.ids
            .iter()
            .find(|(_, i)| **i == object)
            .map(|(k, _)| *k)
    }
    fn interpermeable(&self, _body: BodyKey, _other: BodyKey, _own: &CollisionSettings) -> bool {
        false
    }
    fn contact_record(&self, body: BodyKey) -> Option<ContactRecord> {
        Some(ContactRecord {
            object: self.object_id(body),
            ..ContactRecord::default()
        })
    }
    fn ignores_collisions(&self, _body: BodyKey) -> bool {
        false
    }
    fn collide_mask(&self, _body: BodyKey) -> PhysicsTypes {
        PhysicsTypes::default()
    }
    fn object_properties(&self, body: BodyKey) -> Option<Vec<PropertyGroup>> {
        self.properties.get(&body).cloned()
    }
    fn position(&self, _body: BodyKey) -> Position {
        origin(0x1234_0001)
    }
    fn add_to_temp_ethereal_list(&mut self, _body: BodyKey, _object: u64) {}
    fn collision_remove_object(&mut self, body: BodyKey, object: u64) {
        self.removed.push((body, object));
    }
    fn set_physics_collide_mask(&mut self, _mask: &PhysicsTypes) {}
    fn set_terrain_type(&mut self, _terrain: u32) {}
    fn reports_collisions(&self) -> bool {
        true
    }
    fn physics_material(&self, _id: u32) -> Option<PhysicsMaterial> {
        None
    }
    fn property_list(&self) -> &PropertyList {
        &self.list
    }
    fn now(&self) -> f64 {
        self.now
    }
}

/// Taking on another mask's physics types adds only the lowest one this mask lacks; setting bit 31
/// the client's way also sets the whole upper word, and bit 33 lands on bit 1.
///
/// Behaviour: collision.ethereal.a-mask-takes-one-new-type-at-a-time
#[test]
fn a_mask_takes_one_new_type_at_a_time() {
    let mut a = PhysicsTypes::new(0b0001);
    a.update_ethereality(&PhysicsTypes::new(0b1111));
    assert_eq!(a.mask, 0b0011);
    a.update_ethereality(&PhysicsTypes::new(0b0011));
    assert_eq!(a.mask, 0b0011);

    let mut b = PhysicsTypes::default();
    b.set_bit(31, true);
    assert_eq!(b.mask, 0xFFFF_FFFF_8000_0000);
    b.set_bit(31, false);
    assert_eq!(b.mask, 0);
    b.set_bit(33, true);
    assert_eq!(b.mask, 0b10);
    b.set_bit(64, true);
    assert_eq!(b.mask, 0b10);
}

/// Asking a sight line to pass through water still leaves water blocking it.
///
/// Behaviour: collision.ethereal.water-always-blocks-a-sight-line
#[test]
fn water_always_blocks_a_sight_line() {
    let list = property_list();
    for blocked in [false, true] {
        let mut s = SightLine::default();
        s.physics.mask = 1 << 5;
        s.set_blocked_by_water(&list, blocked);
        assert_eq!(s.physics.mask, 0, "blocked {blocked}");
    }
}

/// A sight line's final ray runs from its start to where the current ray stopped, across
/// landblocks, and its end point is in the final cell's landblock frame; it fails when only one
/// of the two cells is set.
///
/// Behaviour: collision.ethereal.a-sight-line-ends-where-its-ray-stopped
#[test]
fn a_sight_line_ends_where_its_ray_stopped() {
    let mut s = SightLine::default();
    let start = Position::new(
        CellId(0x1010_0001),
        Frame::new(Vec3::new(150.0, 10.0, 2.0), Quat::IDENTITY),
    );
    s.set_from_position_and_offset(&start, Vec3::new(20.0, 0.0, 0.0));
    s.reset_state();
    s.delta = 0.5;
    s.final_cell = 0x1010_0002;
    assert!(s.calc_final_ray());
    assert_eq!(s.final_ray.length, 10.0);
    assert_eq!(s.final_ray.dir, Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(s.end_point(), Vec3::new(160.0, 10.0, 2.0));
    // Ending in the next landblock east, the end point is in that block's frame.
    s.final_cell = 0x1110_0001;
    assert_eq!(s.end_point(), Vec3::new(0.0, 10.0, 2.0));

    s.final_cell = 0;
    let before = s.final_ray;
    assert!(!s.calc_final_ray());
    assert_eq!(s.final_ray, before);
}

/// A touched object begins when the body reports collisions; untouched and more than a second
/// old it ends, is forgotten, and is told so.
///
/// Behaviour: collision.contact.a-touch-ends-a-second-after-it-was-last-renewed
#[test]
fn a_touch_ends_a_second_after_it_was_last_renewed() {
    let mut w = World::new();
    let mut c = Collisions::new();
    c.last_check_time = 10.0;
    assert!(c.add_object_collision(2, &mut w));

    w.now = 10.0;
    let mut r = CollisionReport::default();
    c.report_collision(&mut r, false, false, &mut w);
    assert_eq!(r.records.len(), 1);
    assert!(r.records[0].begin && r.records[0].object == 0x200);

    // Still within a second, or renewed by this check: kept.
    w.now = 11.0;
    let mut r = CollisionReport::default();
    c.report_collision(&mut r, false, false, &mut w);
    assert!(r.records.is_empty() && c.contacts.contains_key(&0x200));
    assert!(!c.resolved);

    c.last_check_time = 11.5;
    w.now = 11.5;
    let mut r = CollisionReport::default();
    c.report_collision(&mut r, false, false, &mut w);
    assert_eq!(r.records.len(), 1);
    assert!(!r.records[0].begin);
    assert!(c.contacts.is_empty());
    assert_eq!(w.removed, vec![(2, 0x100)]);
}

/// A hit keeps its normal when it comes in faster vertically than the least reported speed (at
/// least 0.0002 m/s); its impact speed is the speed into the normal.
///
/// Behaviour: collision.contact.a-hit-reports-its-normal-and-impact-speed
#[test]
fn a_hit_reports_its_normal_and_impact_speed() {
    let mut w = World::new();
    let mut c = Collisions::new();
    c.set_min_reported_impact_z(2.0);
    c.set_collision_normal(Vec3::new(0.0, 0.0, 1.0), Vec3::new(5.0, 0.0, -1.5));
    assert!(!c.has_normal);
    c.set_collision_normal(Vec3::new(0.0, 0.6, 0.8), Vec3::new(5.0, 0.0, -2.5));
    assert!(c.has_normal);
    assert_eq!(c.impact_speed, 2.0);

    let mut r = CollisionReport::default();
    c.report_collision(&mut r, false, false, &mut w);
    assert_eq!(r.collided(), Some((Vec3::new(0.0, 0.6, 0.8), 2.0)));
    assert!(!c.has_normal);
}

/// An object's push properties become one push for the body touching it: a walk-on-air push
/// replaces what was summed before it with its own velocity and then adds it again, and a water
/// velocity alone gives no push.
///
/// Behaviour: collision.push.pushes-sum-and-walking-on-air-doubles
#[test]
fn pushes_sum_and_walking_on_air_doubles() {
    let mut w = World::new();
    w.ids.insert(3, 0x300);
    w.ids.insert(4, 0x400);
    let group = |props| PropertyGroup {
        group: 0,
        properties: props,
        optional: Vec::new(),
    };
    w.properties.insert(
        2,
        vec![group(vec![
            vector(push::VELOCITY, 1.0, 0.0, 0.0),
            vector(push::EARTH_VELOCITY, 0.0, 2.0, 0.0),
        ])],
    );
    w.properties.insert(
        3,
        vec![group(vec![vector(
            push::WALK_ON_AIR_VELOCITY,
            0.0,
            0.0,
            3.0,
        )])],
    );
    w.properties.insert(
        4,
        vec![group(vec![vector(push::WATER_VELOCITY, 9.0, 0.0, 0.0)])],
    );
    let mut c = Collisions::new();
    for b in [2, 3, 4] {
        c.add_object_collision(b, &mut w);
    }
    assert_eq!(c.pushers.len(), 2);
    assert!(!c.pushers.contains_key(&4));

    let (mut walk, mut v, mut a) = (false, Vec3::new(7.0, 7.0, 7.0), Vec3::ZERO);
    c.update_pusher_values(0x4000_0001, &mut walk, &mut v, &mut a);
    assert!(walk);
    assert_eq!(v, Vec3::new(0.0, 0.0, 6.0));
}

/// A contact property marked ended goes; one touched again with the same value, a vector's parts
/// within a tolerance, is unchanged, with a new value of its kind changed.
///
/// Behaviour: collision.contact.a-held-property-is-unchanged-or-changed
#[test]
fn a_held_property_is_unchanged_or_changed() {
    let mut p = ContactProperty::new(vector(push::VELOCITY, 1.0, 0.0, 0.0), 7);
    assert!(p.is_begin());
    p.update_value(&vector(push::VELOCITY, 1.0, 0.0, 0.0));
    assert_eq!(p.change, change::SAME);
    p.update_value(&vector(push::VELOCITY, 2.0, 0.0, 0.0));
    assert_eq!(p.change, change::CHANGED);
    assert_eq!(p.vector(), Some(Vec3::new(2.0, 0.0, 0.0)));
    p.update_value(&vector(push::VELOCITY, 2.0001, 0.0, 0.0));
    assert_eq!(p.change, change::SAME, "within the tolerance");
    assert_eq!(p.vector(), Some(Vec3::new(2.0, 0.0, 0.0)));
    p.set_as_same();
    p.update_value(&Property {
        id: push::VELOCITY,
        value: PropertyValue::Integer(3),
        ..Property::default()
    });
    assert_eq!(p.change, change::SAME);
}

fn round_trip<T: Pack + PartialEq + std::fmt::Debug>(v: &T, ctx: &Context) {
    let mut w = Writer::new();
    v.pack(&mut w);
    let mut r = Reader::new(w.as_bytes());
    let back = T::unpack(&mut r, ctx).expect("unpacks");
    assert!(r.at_end());
    assert_eq!(&back, v);
}

/// A body's collisions, a report and a sight line keep every field through their binary form.
///
/// Behaviour: collision.pack.collision-state-survives-packing
#[test]
fn collision_state_survives_packing() {
    let ctx = Context::with_properties(property_list());
    let mut w = World::new();
    w.properties.insert(
        2,
        vec![PropertyGroup {
            group: 0,
            optional: Vec::new(),
            properties: vec![
                vector(push::VELOCITY, 1.0, 0.0, 0.0),
                Property {
                    id: push::LOCAL,
                    value_type: BOOL,
                    group: 0,
                    value: PropertyValue::Bool(true),
                },
            ],
        }],
    );
    let mut c = Collisions::new();
    c.last_check_time = 3.0;
    c.add_object_collision(2, &mut w);
    c.set_contact_plane(Plane::new(Vec3::new(0.0, 0.0, 1.0), -2.0));
    c.set_in_water(0x3300_0001);
    c.add_to_temp_ethereal_list(0x900);
    c.current.add_to_ethereal_list(0x901);
    let mut r = CollisionReport::default();
    w.now = 3.0;
    c.report_collision(&mut r, false, false, &mut w);
    c.last_check_time = 0.0;
    round_trip(&c, &ctx);
    round_trip(&r, &ctx);
    round_trip(&CollisionSettings::default(), &ctx);
    round_trip(&Push::default(), &ctx);

    let mut s = SightLine::default();
    s.objects.add_to_solid_list(0x500);
    s.set_from_begin_and_end_position(&origin(0x1010_0001), &origin(0x1011_0002));
    s.reset_state();
    s.impact_normal = Vec3::new(0.0, 1.0, 0.0);
    round_trip(&s, &ctx);
}

/// A body running at a block the world's map says may not be walked on stops short of it, its
/// spheres never reaching the block's first land cell: it is turned back at the land cell's edge,
/// and told it collided along that axis. A block that may be walked on lets it through.
///
/// Behaviour: motion.collision.a-body-stops-at-the-edge-of-a-block-it-may-not-walk-on
#[test]
fn a_body_stops_at_the_edge_of_a_block_it_may_not_walk_on() {
    use auberean_physics::land::{Block, Land};
    use auberean_physics::motion::MotionInput;
    let east = |passable: bool| {
        let land = Land::new();
        land.insert(Block::flat(0x40, 0x40, 10.0));
        let mut next = Block::flat(0x41, 0x40, 10.0);
        next.passable = passable;
        land.insert(next);
        let mut b = crate::contact::objects::body(150.0, 80.0);
        b.position.frame.origin.z = 10.0;
        b.heading = 90.0;
        b.steer(MotionInput {
            forward: 1.0,
            ..MotionInput::default()
        });
        let mut furthest = 0.0_f32;
        let mut cells = std::collections::BTreeSet::new();
        for _ in 0..40 {
            b.tick(&land, auberean_physics::body::tick_seconds());
            furthest = furthest.max(b.position.frame.origin.x);
            cells.insert(b.position.cell.0 >> 16);
        }
        (furthest, cells, b.contact)
    };
    let (_, cells, _) = east(true);
    assert!(cells.contains(&0x4140), "through: {cells:x?}");
    let (shut, cells, standing) = east(false);
    assert_eq!(cells.into_iter().collect::<Vec<_>>(), [0x4040]);
    assert!(shut < 160.0 && shut > 158.0, "at the edge: {shut}");
    assert!(standing, "standing");
}
