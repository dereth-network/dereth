//! Swing reports: nothing for no target, once per target straight ahead, reach = radius + target,
//! scale scales reach and height, the wedge/cone admits only covered directions, hit-location band,
//! static/self/gated targets refused, a no-cell attacker is silent, two targets both reported,
//! reflex cone notch, scripted collision bit.
//! Fixture: synthetic state, geometry and reference vectors.

use std::sync::Arc;

use dereth_physics::detect::{hit_location, AttackCone};
use dereth_physics::geom::Sphere;
use dereth_physics::{PhysHandle, PhysicsState, PhysicsWorld, SetupGeometry};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, LandblockId, ObjectId, Position, Quat, Vec3};

const BLOCK: LandblockId = LandblockId(0xA9B4);
/// Ground height of a flat block at table index 10 with the linear `2 * i` table.
const GROUND: f32 = 20.0;

/// The attacker stands here, in the middle of the block so that every direction below stays inside
/// the nine loaded landblocks.
const CENTRE: Vec3 = Vec3 {
    x: 90.0,
    y: 90.0,
    z: GROUND,
};

/// **The commonest shipped cone, read out of `client_portal.dat`**: animation `0x0300000E`
/// part 5 — `left (-0.5, 0.8660254)`, `right (0.5, 0.8660254)`, `radius 2`, `height 1`. 188 of the
/// 1,053 instances use this exact edge pair. A fixture taken from the data rather than invented,
/// because the edge *order* is the one thing this whole file turns on.
fn shipped_cone() -> AttackCone {
    AttackCone {
        part_index: -1,
        left: (-0.5, 0.866_025_4),
        right: (0.5, 0.866_025_4),
        radius: 2.0,
        height: 1.0,
    }
}

fn cell_at(p: Vec3) -> CellId {
    let mut c = BLOCK.cell(1);
    let mut o = p;
    assert!(dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o));
    c
}

// A 0.5 m radius, 1 m tall body. Its effective dimensions are
// `setup->radius * scale.z` and `setup->height * scale.z`, so these two
// fields and the object scale supply everything the attack check needs from the part array.

fn tall_geometry(height: f32) -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, height * 0.5), 0.5)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, height * 0.5), height),
        radius: 0.5,
        height,
        ..SetupGeometry::default()
    })
}

fn place(
    w: &mut PhysicsWorld,
    id: u32,
    at: Vec3,
    g: Arc<SetupGeometry>,
    dynamic: bool,
) -> PhysHandle {
    let h = w.create(ObjectId(id), g, dynamic);
    let cell = cell_at(at);
    w.enter_cell(h, cell);
    let frame = Frame::new(at, Quat::IDENTITY);
    if let Some(o) = w.get_mut(h) {
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, true);
    h
}

/// The attacker at [`CENTRE`] facing +Y, and nothing else in the world.
fn lone_attacker() -> (PhysicsWorld, PhysHandle) {
    let mut w = flat_world();
    let a = place(&mut w, 1, CENTRE, body_geometry(), true);
    (w, a)
}

/// The attacker plus one target at `offset` from it.
fn attacker_and_target(offset: Vec3) -> (PhysicsWorld, PhysHandle, PhysHandle) {
    let (mut w, a) = lone_attacker();
    let t = place(&mut w, 2, CENTRE.add(offset), body_geometry(), true);
    (w, a, t)
}

use dereth_physics::V3 as _;

fn ids(hits: &[dereth_physics::detect::AtkCollisionProfile]) -> Vec<u32> {
    hits.iter().map(|p| p.id.0).collect()
}

// ---------------------------------------------------------------------------------------------
// 1. The control, and the reach
// ---------------------------------------------------------------------------------------------

/// **The control.** A swing into empty air reports nothing, and it does so without panicking —
/// which is the half that matters, because the branch every shipped cone takes was an
/// `unimplemented!()`.
#[test]
fn a_swing_at_nothing_reports_nothing() {
    let (mut w, a) = lone_attacker();
    assert_eq!(
        w.attack(a, &shipped_cone()),
        Vec::new(),
        "nothing is there to hit"
    );
}

/// Behaviour: combat.swing.the-attack-cone-reports-each-target-in-reach-once
/// **The acceptance test.** A target one metre straight ahead is reported once, with its own id and
/// a well-formed quadrant.
#[test]
fn a_target_straight_ahead_is_reported_once() {
    let (mut w, a, _t) = attacker_and_target(Vec3::new(0.0, 1.0, 0.0));
    let hits = w.attack(a, &shipped_cone());
    assert_eq!(
        ids(&hits),
        vec![2],
        "the target is hit exactly once: {hits:?}"
    );
    let p = hits[0];
    assert_eq!(
        p.part, -1,
        "the reported part index is the cone's own, 0xFFFFFFFF here"
    );
    // Exactly one height band and one of each lateral pair.
    let bands = [hit_location::HIGH, hit_location::MEDIUM, hit_location::LOW]
        .iter()
        .filter(|b| p.location & *b != 0)
        .count();
    assert_eq!(bands, 1, "{:#x}", p.location);
    assert_eq!(
        u8::from(p.location & hit_location::LEFT != 0)
            + u8::from(p.location & hit_location::RIGHT != 0),
        1,
        "{:#x}",
        p.location
    );
    // Both face +Y, so converting to the target's frame puts the attacker at negative Y.
    // The attack therefore selects the back hit-location bit, 0x40.
    assert_eq!(
        p.location & hit_location::BACK,
        hit_location::BACK,
        "{:#x}",
        p.location
    );
}

/// The cone's **reach** is `scale * cone.radius`; the extra attack radius stays zero
/// for the life of the client. The shipped radius 2 plus the target's own 0.5 m reaches 2.5 m
/// and no further: the check compares `(target_radius + cone_radius)^2` with `x^2 + y^2`.
#[test]
fn the_cone_reaches_its_radius_plus_the_targets_and_no_further() {
    for (dist, want) in [(2.4_f32, true), (2.6, false), (5.0, false)] {
        let (mut w, a, _t) = attacker_and_target(Vec3::new(0.0, dist, 0.0));
        let hits = w.attack(a, &shipped_cone());
        assert_eq!(!hits.is_empty(), want, "{dist} m ahead: {hits:?}");
    }
}

#[test]
fn the_reach_and_the_swing_height_both_scale_with_the_attackers_scale() {
    let big_target = |w: &mut PhysicsWorld| {
        place(
            w,
            2,
            CENTRE.add(Vec3::new(0.0, 4.0, 0.0)),
            tall_geometry(4.0),
            true,
        );
    };
    let (mut w, a) = lone_attacker();
    big_target(&mut w);
    assert!(
        w.attack(a, &shipped_cone()).is_empty(),
        "4 m is out of a radius-2 cone's reach"
    );

    let (mut w, a) = lone_attacker();
    big_target(&mut w);
    w.get_mut(a).expect("live").scale = 2.0;
    assert_eq!(
        ids(&w.attack(a, &shipped_cone())),
        vec![2],
        "and in reach at scale 2"
    );

    // The height half, on its own: the same scale-2 attacker against a 1 m target.
    let (mut w, a) = lone_attacker();
    place(
        &mut w,
        2,
        CENTRE.add(Vec3::new(0.0, 1.0, 0.0)),
        tall_geometry(1.0),
        true,
    );
    assert_eq!(
        ids(&w.attack(a, &shipped_cone())),
        vec![2],
        "in reach and in the band at scale 1"
    );
    w.get_mut(a).expect("live").scale = 2.0;
    assert!(
        w.attack(a, &shipped_cone()).is_empty(),
        "scale 2 makes cone_height 2.0 against a 1 m target, which the client refuses"
    );
}

/// A target behind the attacker is not hit.
#[test]
fn a_target_behind_the_attacker_is_not_hit() {
    let (mut w, a, _t) = attacker_and_target(Vec3::new(0.0, -1.0, 0.0));
    let hits = w.attack(a, &shipped_cone());
    assert!(
        hits.is_empty(),
        "a target 1 m behind is inside the 2 m reach and outside the plus-or-minus 30 degree \
         wedge; rejects it. got {hits:?}"
    );
}

/// The same swing, walked round the attacker in 24 steps at a fixed 1 m. **This is the test that
/// says the cone is a cone**: a build whose wedge test never rejects anything — which is what the
/// old reading of produced — hits all 24.
#[test]
fn the_wedge_admits_only_the_directions_it_covers() {
    let cone = shipped_cone();
    let mut hit_angles: Vec<i32> = Vec::new();
    for step in 0..24 {
        let deg = f32::from(i16::try_from(step).unwrap()) * 15.0;
        let r = deg.to_radians();
        // 0 degrees is straight ahead (+Y); positive is towards +X.
        let off = Vec3::new(math::sinf(r), math::cosf(r), 0.0);
        let (mut w, a, _t) = attacker_and_target(off);
        if !w.attack(a, &cone).is_empty() {
            hit_angles.push(i32::from(i16::try_from(step).unwrap()) * 15);
        }
    }
    // The wedge is plus-or-minus 30 degrees, widened by the target's own 0.5 m radius at 1 m
    // (`cl <= target_radius`), which is another ~30 degrees each way.
    assert!(
        hit_angles.contains(&0) && hit_angles.contains(&30) && hit_angles.contains(&330),
        "the wedge must cover its own 30 degrees: {hit_angles:?}"
    );
    assert!(
        !hit_angles.contains(&180) && !hit_angles.contains(&165) && !hit_angles.contains(&195),
        "and must not cover directly behind: {hit_angles:?}"
    );
    assert!(
        hit_angles.len() < 24,
        "a cone that hits in all 24 directions is a sphere; this is the shape the old reading of \
         the two edge tests produced: {hit_angles:?}"
    );
    assert!(
        hit_angles.len() >= 5,
        "and it must not have collapsed to nothing: {hit_angles:?}"
    );
}

/// A **narrower** shipped cone admits strictly fewer directions than a wider one. Two cones that
/// behave identically would satisfy every test above and still mean the angle is being ignored.
#[test]
fn a_ten_degree_cone_admits_fewer_directions_than_a_forty_five_degree_one() {
    let count = |half: f32| -> usize {
        let a = half.to_radians();
        let cone = AttackCone {
            part_index: -1,
            left: (-math::sinf(a), math::cosf(a)),
            right: (math::sinf(a), math::cosf(a)),
            radius: 2.0,
            height: 1.0,
        };
        (0..72)
            .filter(|step| {
                let deg = f32::from(i16::try_from(*step).unwrap()) * 5.0;
                let r = deg.to_radians();
                let (mut w, at, _t) =
                    attacker_and_target(Vec3::new(math::sinf(r) * 2.0, math::cosf(r) * 2.0, 0.0));
                !w.attack(at, &cone).is_empty()
            })
            .count()
    };
    // The four half-angles that appear in the dat, in order.
    let (ten, thirty, forty, forty_five) = (count(10.0), count(30.0), count(40.0), count(45.0));
    assert!(
        ten < thirty && thirty < forty && forty < forty_five,
        "the four shipped half-angles must widen monotonically: 10 -> {ten}, 30 -> {thirty}, \
         40 -> {forty}, 45 -> {forty_five}"
    );
}

/// The height band comes from `scale * cone.height` against the **target's** height, so the same
/// swing reads `Low` against a tall creature and `High` against a short one..
// which multiply the target's height by 0.333333 and 0..
#[test]
fn the_hit_location_band_is_the_cone_height_against_the_targets_height() {
    let band = |target_height: f32, cone_height: f32| -> u32 {
        let (mut w, a) = lone_attacker();
        let at = CENTRE.add(Vec3::new(0.0, 1.0, 0.0));
        place(&mut w, 2, at, tall_geometry(target_height), true);
        let cone = AttackCone {
            height: cone_height,
            ..shipped_cone()
        };
        let hits = w.attack(a, &cone);
        assert_eq!(
            hits.len(),
            1,
            "h={target_height} ch={cone_height}: {hits:?}"
        );
        hits[0].location
    };
    assert_eq!(band(3.0, 0.5) & hit_location::LOW, hit_location::LOW);
    assert_eq!(band(3.0, 1.5) & hit_location::MEDIUM, hit_location::MEDIUM);
    assert_eq!(band(1.2, 1.0) & hit_location::HIGH, hit_location::HIGH);
    // And a cone swung above the target's head is no hit at all:'s `je.
    let (mut w, a) = lone_attacker();
    place(
        &mut w,
        2,
        CENTRE.add(Vec3::new(0.0, 1.0, 0.0)),
        tall_geometry(1.0),
        true,
    );
    assert!(
        w.attack(
            a,
            &AttackCone {
                height: 1.5,
                ..shipped_cone()
            }
        )
        .is_empty(),
        "cone_height > target_height is a miss"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The five refusals above the geometry
// ---------------------------------------------------------------------------------------------

/// `physobj->id != attacker_id` first, then
/// `(state & STATIC_PS) == 0`. **A static is never attacked** — which is why a swing at a wall or a
/// baked tree reports nothing even though both are registered in the cell.
#[test]
fn a_static_target_is_never_attacked() {
    let (mut w, a) = lone_attacker();
    place(
        &mut w,
        2,
        CENTRE.add(Vec3::new(0.0, 1.0, 0.0)),
        body_geometry(),
        false,
    );
    assert!(
        !w.get(a).expect("live").state.is_static(),
        "the attacker itself is dynamic"
    );
    let hits = w.attack(a, &shipped_cone());
    assert!(
        hits.is_empty(),
        "the cell check tests `state & 1` before calling check_attack: {hits:?}"
    );
}

/// And the attacker never attacks itself, whatever is or is not standing with it.
#[test]
fn the_attacker_is_never_its_own_target() {
    let (mut w, a) = lone_attacker();
    // A second body in exactly the same place, so that "nothing was in range" cannot be the reason.
    place(
        &mut w,
        2,
        CENTRE.add(Vec3::new(0.0, 1.0, 0.0)),
        body_geometry(),
        true,
    );
    let hits = w.attack(a, &shipped_cone());
    assert_eq!(ids(&hits), vec![2], "only the other object: {hits:?}");
}

/// 's three gates, each asserted against a target that is
/// otherwise hit. All three are *target*-side, and the parent one is why a held weapon is not a
/// target.
#[test]
fn the_three_target_side_gates_each_refuse_on_their_own() {
    let ahead = Vec3::new(0.0, 1.0, 0.0);

    // The baseline, so that each refusal below is a refusal and not a miss.
    let (mut w, a, _t) = attacker_and_target(ahead);
    assert_eq!(
        ids(&w.attack(a, &shipped_cone())),
        vec![2],
        "baseline must hit"
    );

    // `state & 0x10` — IGNORE_COLLISIONS_PS.
    let (mut w, a, t) = attacker_and_target(ahead);
    w.get_mut(t)
        .expect("live")
        .state
        .set_ignores_collisions(true);
    assert!(
        w.attack(a, &shipped_cone()).is_empty(),
        "IGNORE_COLLISIONS_PS refuses"
    );

    // `state & 0x200000` — REPORT_COLLISIONS_AS_ENVIRONMENT_PS.
    let (mut w, a, t) = attacker_and_target(ahead);
    w.get_mut(t)
        .expect("live")
        .state
        .set_reports_as_environment(true);
    assert!(
        w.attack(a, &shipped_cone()).is_empty(),
        "REPORT_..._AS_ENVIRONMENT_PS refuses"
    );

    // `parent != NULL`.
    let (mut w, a, t) = attacker_and_target(ahead);
    let holder = place(
        &mut w,
        3,
        CENTRE.add(Vec3::new(0.0, 1.0, 0.0)),
        body_geometry(),
        true,
    );
    w.get_mut(t).expect("live").parent = Some(holder);
    assert_eq!(
        ids(&w.attack(a, &shipped_cone())),
        vec![3],
        "a parented object is refused; the holder is not"
    );
}

#[test]
fn report_collisions_is_not_a_gate_on_being_attacked() {
    let (mut w, a, t) = attacker_and_target(Vec3::new(0.0, 1.0, 0.0));
    w.get_mut(t)
        .expect("live")
        .state
        .set_reports_collisions(false);
    assert_eq!(ids(&w.attack(a, &shipped_cone())), vec![2]);
}

/// An object with no cell does not attack: the attack step compares the cell id against the
/// invalid id and returns before the `AttackManager` is even created.
#[test]
fn an_object_with_no_cell_does_not_attack() {
    let mut w = flat_world();
    let a = w.create(ObjectId(1), body_geometry(), true);
    place(
        &mut w,
        2,
        CENTRE.add(Vec3::new(0.0, 1.0, 0.0)),
        body_geometry(),
        true,
    );
    assert!(
        w.attack(a, &shipped_cone()).is_empty(),
        "no cell, no attack"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. Attack reporting — the dedupe
// ---------------------------------------------------------------------------------------------

/// Behaviour: combat.swing.the-attack-cone-reports-each-target-in-reach-once
/// Attack reporting scans `object_list` for the id before appending, so an object
/// with a shadow in more than one of the cells the search sphere touched is reported **once**.
///
/// A 25 m cone — the largest `radius` in the dat — over a land-cell grid of 24 m cells is
/// guaranteed to touch several, which is exactly the case the dedupe exists for.
#[test]
fn an_object_in_two_searched_cells_is_reported_once() {
    let (mut w, a) = lone_attacker();
    // Straddling a land-cell boundary: block-local 24.0 is the seam between cell columns.
    let on_the_seam = Vec3::new(90.0, 96.0, GROUND);
    let t = place(&mut w, 2, on_the_seam, body_geometry(), true);
    let shadows = w.get(t).expect("live").shadow_objects.len();
    let wide = AttackCone {
        radius: 25.0,
        ..shipped_cone()
    };
    let hits = w.attack(a, &wide);
    assert_eq!(
        ids(&hits),
        vec![2],
        "the target has {shadows} shadow(s) and must still be reported once: {hits:?}"
    );
}

/// Two different objects in reach are both reported, so the dedupe above is a dedupe and not a
/// "stop after the first".
#[test]
fn two_targets_in_reach_are_both_reported() {
    let (mut w, a) = lone_attacker();
    place(
        &mut w,
        2,
        CENTRE.add(Vec3::new(-0.3, 1.0, 0.0)),
        body_geometry(),
        true,
    );
    place(
        &mut w,
        3,
        CENTRE.add(Vec3::new(0.3, 1.4, 0.0)),
        body_geometry(),
        true,
    );
    let mut got = ids(&w.attack(a, &shipped_cone()));
    got.sort_unstable();
    assert_eq!(got, vec![2, 3]);
}

// ---------------------------------------------------------------------------------------------
// 5. The facts that shrink the function, asserted so they cannot quietly change
// ---------------------------------------------------------------------------------------------

/// The attack manager and physics object both initialize their extra attack radius
/// to `0.0`, and **neither field is assigned anywhere
/// else in the client**. So the search sphere's radius is exactly `scale * cone.radius` and
/// the attack check's fifth argument is always zero.
///
/// It is still read from the object, so this asserts the arithmetic rather than the constant: a
/// rebuild server that sets the field gets retail's `scale * cone.radius + attack_radius`.
#[test]
fn the_attack_radius_defaults_to_zero_and_is_added_to_the_reach_when_it_is_not() {
    let (mut w, a, _t) = attacker_and_target(Vec3::new(0.0, 3.0, 0.0));
    assert_eq!(
        w.get(a).expect("live").attack_radius,
        0.0,
        "the client's value, always"
    );
    assert!(
        w.attack(a, &shipped_cone()).is_empty(),
        "3 m is out of a radius-2 cone's reach"
    );
    w.get_mut(a).expect("live").attack_radius = 1.0;
    assert_eq!(
        ids(&w.attack(a, &shipped_cone())),
        vec![2],
        "and one more metre of attack_radius brings it into reach"
    );
}

/// A reflex cone — the shipped edge pair swapped — is a 300 degree wedge, and its *notch* is the
/// miss. is the branch no shipped cone takes and the one the knowledge base thought was
/// the ordinary case; it is transcribed, so this asserts it rather than a panic.
#[test]
fn a_reflex_cone_misses_only_its_notch() {
    let a30: f32 = 30.0_f32.to_radians();
    let reflex = AttackCone {
        part_index: -1,
        left: (math::sinf(a30), math::cosf(a30)),
        right: (-math::sinf(a30), math::cosf(a30)),
        radius: 2.0,
        height: 1.0,
    };
    assert!(dereth_physics::detect::cone_is_reflex(&reflex));
    assert!(
        !dereth_physics::detect::cone_is_reflex(&shipped_cone()),
        "and the shipped order is not"
    );

    // Straight ahead is the notch. The target's 0.5 m radius at 1 m widens the notch's edges, so
    // the target is put far enough out that `cl` and `cr` both clear it.
    let (mut w, at, _t) = attacker_and_target(Vec3::new(0.0, 1.8, 0.0));
    assert!(
        w.attack(at, &reflex).is_empty(),
        "the notch of a reflex wedge is the miss"
    );
    // Behind is inside the 300 degrees, and the ordinary cone misses exactly there: the two
    // branches are duals, which is the cheapest way to be sure neither is the other.
    let (mut w, at, _t) = attacker_and_target(Vec3::new(0.0, -1.0, 0.0));
    assert_eq!(
        ids(&w.attack(at, &reflex)),
        vec![2],
        "behind is inside a 300 degree wedge"
    );
    assert!(
        w.attack(at, &shipped_cone()).is_empty(),
        "and outside the shipped 60 degree one"
    );
}

/// The bit do collision tests is scripted collision.
#[test]
fn the_bit_do_collision_tests_is_scripted_collision() {
    let mut s = PhysicsState(0);
    s.set_scripted_collision(true);
    assert_eq!(s.0, 0x0000_8000, "the scripted-collision test is bit 15");
    assert!(PhysicsState(0x0002_8B48).has_scripted_collision());
}

use crate::common::physics_fixture::{flat_world, player_geometry as body_geometry};
