//! The contact-plane flag matches the client at every site; the step-down plane is the obstacle
//! tangent; the walkable gate uses the post-step normal; the flag changes the water-contact bit not
//! position; a body riding a dome keeps a unit contact normal; the anti-sink push is what the flag
//! suppresses.
//! Fixture: synthetic state, geometry and reference vectors.

use std::sync::Arc;

use dereth_physics::geom::{CylSphere, Sphere};
use dereth_physics::transition::{collide, cylinder, insert, walk};
use dereth_physics::{
    globals, PhysHandle, PhysicsWorld, SetupGeometry, Transition, TransitionState, V3,
};
use dereth_primitives::{Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

const BLOCK: LandblockId = LandblockId(0xA9B4);
/// Ground height of a flat block at table index 10 with the linear `2 * i` table.
const GROUND: f32 = 20.0;
/// The obstacle: a sphere of radius 3 sitting on the ground, so its centre is at `z = 23` and its
/// crown at `z = 26`. A dome is the shape that makes "standing on another object's sphere" an
/// ordinary thing to do rather than a contrivance.
const DOME_R: f32 = 3.0;
const DOME_AT: Vec3 = Vec3::new(100.0, 100.0, GROUND);
/// The mover's one sphere: local centre `(0, 0, 0.5)`, radius `0.5`.
const MOVER_R: f32 = 0.5;
/// The step-down probe distance in these tests.
const STEP: f32 = 0.3;
/// How far above its resting place a probe starts. It has to be **less** than `STEP`: the probe
/// drops the sphere by `STEP` first, and the sphere step-down test only runs when the dropped
/// sphere actually overlaps the obstacle. Starting exactly `STEP` up lands the sphere exactly on
/// the surface, where `collides_with_sphere`'s `sum = R + r - 0.0002` says "not touching" and the
/// step-down declines — a knife edge, not a bug.
const START_ABOVE: f32 = 0.05;

fn dome_centre() -> Vec3 {
    DOME_AT.add(Vec3::new(0.0, 0.0, DOME_R))
}

/// which land cell of the block a point falls in. An
/// object registered into one cell and looked for in another is silently not there.
fn cell_at(p: Vec3) -> dereth_primitives::CellId {
    let mut c = BLOCK.cell(1);
    let mut o = p;
    assert!(dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o));
    c
}

fn dome_geometry() -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, DOME_R), DOME_R)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, DOME_R), DOME_R * 2.0),
        radius: DOME_R,
        height: DOME_R * 2.0,
        ..SetupGeometry::default()
    })
}

fn spawn(w: &mut PhysicsWorld, at: Vec3) -> PhysHandle {
    let h = w.create(ObjectId(1), player_geometry(), true);
    let cell = cell_at(at);
    w.enter_cell(h, cell);
    if let Some(o) = w.get_mut(h) {
        o.position = Position::new(cell, Frame::new(at, Quat::IDENTITY));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    h
}

/// The environment cell's static-object initialisation for one placement.
fn register(w: &mut PhysicsWorld, at: Vec3, g: Arc<SetupGeometry>) {
    let h = w.create(ObjectId(9), g, false);
    let cell = cell_at(at);
    w.enter_cell(h, cell);
    let frame = Frame::new(at, Quat::IDENTITY);
    if let Some(o) = w.get_mut(h) {
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
    }
    w.calc_cross_cells(h, true);
}

/// One from `from`, in a world that holds `obstacle` or does
/// not.
///
/// This is the real entry: `step_down` sets `sphere_path.step_down`, drops the check position by
/// `STEP` and runs the whole collision walk, so sphere step-down is reached the way the
/// client reaches it rather than by being called directly.
fn step_down_from(obstacle: Option<Arc<SetupGeometry>>, from: Vec3) -> (bool, Transition) {
    let mut w = flat_world();
    if let Some(g) = obstacle {
        register(&mut w, DOME_AT, g);
    }
    let mover = spawn(&mut w, from);
    let mut t = Transition::default();
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, MOVER_R), MOVER_R)], 1.0);
    let start = Position::new(cell_at(from), Frame::new(from, Quat::IDENTITY));
    t.sphere_path
        .init_path(Some(start.cell), Some(start), &start);
    t.sphere_path.set_check_pos(&start, Some(start.cell));
    t.sphere_path.check_cell = Some(start.cell);
    t.object_info.object = Some(mover);
    t.object_info.step_down = true;
    t.object_info.step_down_height = STEP;
    t.object_info.step_up_height = STEP;
    let ctx = w.transition_ctx(Some(mover));
    let ok = walk::step_down(&ctx, &mut t, STEP, globals::FLOOR_Z);
    (ok, t)
}

/// The origin a body rests at when standing on the dome `off` metres off its axis, and the origin
/// `STEP` above that — where a step-down probe starts.
fn on_dome(off: f32) -> (f32, f32) {
    let c = dome_centre();
    let r = DOME_R + MOVER_R;
    let rest_origin = c.z + (r * r - off * off).sqrt() - MOVER_R;
    (rest_origin, rest_origin + START_ABOVE)
}

/// `n.z` of the contact normal for a body resting on the dome `off` metres off its axis.
fn dome_normal_z(off: f32) -> f32 {
    let r = DOME_R + MOVER_R;
    (r * r - off * off).sqrt() / r
}

// ---------------------------------------------------------------------------------------------
// 1. The flag, at every call site
// ---------------------------------------------------------------------------------------------

/// **The acceptance line, first half.** The flag this crate records must be the one the client
/// pushes, at each site that can record one.
///
/// The land arm is the control and it is not decoration: without it, "the flag is set" would be
/// satisfied by a crate that hard-coded `true` everywhere, which is the mirror of the defect.
#[test]
fn the_contact_plane_flag_matches_the_client_at_every_site_that_records_one() {
    // (a) the sphere's step down: the contact-plane flag is 1.
    let (ok, t) = step_down_from(
        Some(dome_geometry()),
        Vec3::new(100.4, 100.0, on_dome(0.4).1),
    );
    assert!(ok, "the body must find the dome under it");
    assert!(t.collision_info.contact_plane_valid);
    assert!(
        t.collision_info.contact_plane_is_water,
        "step_sphere_down passes 1, so a body standing on another object's sphere carries the flag"
    );

    // (b) the sphere's object-versus-object walkable branch: also 1.
    let (r, t) = walkable_branch_probe();
    assert_eq!(
        r,
        TransitionState::Adjusted,
        "the walkable branch must have run"
    );
    assert!(
        t.collision_info.contact_plane_is_water,
        "the sphere's check_walkable branch passes 1 too"
    );

    let (r, t) = cyl_step_down_probe();
    assert_eq!(r, TransitionState::Adjusted);
    assert!(
        t.collision_info.contact_plane_is_water,
        "the cylinder's step-down passes 1"
    );

    // (d) the cylinder's `collide` branch: 1.
    let (r, t) = cyl_collide_probe();
    assert_eq!(r, TransitionState::Adjusted);
    assert!(
        t.collision_info.contact_plane_is_water,
        "the cylinder's collide branch passes 1"
    );

    // (e) The land arm — passes the cell's own water
    // type, and a flat dry landblock is not water. Same helper, same probe, no obstacle.
    let (ok, t) = step_down_from(None, Vec3::new(100.4, 100.0, GROUND + START_ABOVE));
    assert!(ok, "the body must find the terrain under it");
    assert!(t.collision_info.contact_plane_valid);
    assert!(
        !t.collision_info.contact_plane_is_water,
        "dry terrain records 0; a crate that hard-coded the flag would fail here"
    );
}

const PILLAR: CylSphere = CylSphere {
    low_pt: Vec3::new(100.0, 100.0, GROUND),
    height: 2.0,
    radius: 0.6,
};

/// A transition whose mover sat at `from` and whose check position is `at`.
fn cyl_mover(from: Vec3, at: Vec3) -> Transition {
    let mut t = Transition::default();
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, MOVER_R), MOVER_R)], 1.0);
    let start = Position::new(cell_at(from), Frame::new(from, Quat::IDENTITY));
    let here = Position::new(cell_at(at), Frame::new(at, Quat::IDENTITY));
    t.sphere_path
        .init_path(Some(start.cell), Some(start), &here);
    t.sphere_path.curr_pos = start;
    t.sphere_path.set_check_pos(&here, Some(here.cell));
    t.sphere_path.cache_global_curr_center();
    t
}

/// the sphere's underside is 0.3 m under the cap, so
/// the lift is `(height + radius) - delta.z = 0.3`.
fn cyl_step_down_probe() -> (TransitionState, Transition) {
    let mut t = cyl_mover(
        Vec3::new(100.0, 100.0, GROUND + 1.7),
        Vec3::new(100.0, 100.0, GROUND + 1.7),
    );
    t.sphere_path.step_down = true;
    t.sphere_path.step_down_amt = 0.6;
    t.sphere_path.walk_interp = 1.0;
    let s0 = t.sphere_path.global_sphere[0];
    let delta = s0.center.sub(PILLAR.low_pt);
    let sum = (PILLAR.radius - globals::EPSILON) + s0.radius;
    let r = cylinder::step_cyl_sphere_down(&mut t, &PILLAR, &s0, delta, sum);
    (r, t)
}

/// 's `collide` branch: the re-validation after a
/// landing, which puts the sphere's underside back on the cap. Falling 0.6 m from `z = 2.8` to
/// `z = 2.2` above the low point crosses the cap at `time = (2.5 - 2.2) / 0.6 = 0.5`.
fn cyl_collide_probe() -> (TransitionState, Transition) {
    let mut w = flat_world();
    let mover = spawn(&mut w, Vec3::new(100.0, 100.0, GROUND + 2.3));
    let mut t = cyl_mover(
        Vec3::new(100.0, 100.0, GROUND + 2.3),
        Vec3::new(100.0, 100.0, GROUND + 1.7),
    );
    t.object_info.object = Some(mover);
    t.sphere_path.collide = true;
    t.sphere_path.walk_interp = 1.0;
    let ctx = w.transition_ctx(Some(mover));
    let r = cylinder::cyl_intersects_sphere(&ctx, &mut t, &PILLAR);
    (r, t)
}

/// 's `check_walkable` branch, driven directly: the mover
/// is coming down from `curr` onto the dome and the branch solves for the moment of contact.
fn walkable_branch_probe() -> (TransitionState, Transition) {
    let mut w = flat_world();
    register(&mut w, DOME_AT, dome_geometry());
    let curr = Vec3::new(100.4, 100.0, 26.3);
    let check = Vec3::new(100.4, 100.0, 25.9);
    let mover = spawn(&mut w, curr);
    let mut t = Transition::default();
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, MOVER_R), MOVER_R)], 1.0);
    let from = Position::new(cell_at(curr), Frame::new(curr, Quat::IDENTITY));
    let to = Position::new(cell_at(check), Frame::new(check, Quat::IDENTITY));
    t.sphere_path.init_path(Some(from.cell), Some(from), &to);
    t.sphere_path.set_check_pos(&to, Some(to.cell));
    t.object_info.object = Some(mover);
    t.sphere_path.check_walkable = true;
    t.sphere_path.walk_interp = 1.0;
    t.sphere_path.walkable_allowance = globals::FLOOR_Z;
    let obstacle = Sphere::new(dome_centre(), DOME_R);
    let ctx = w.transition_ctx(Some(mover));
    let r = collide::sphere_intersects_sphere(&ctx, &mut t, &obstacle, false);
    (r, t)
}

// ---------------------------------------------------------------------------------------------
// 2. The plane itself — the two details retail settled
// ---------------------------------------------------------------------------------------------

/// The step down plane is the obstacles own tangent plane.
#[test]
fn the_step_down_plane_is_the_obstacles_own_tangent_plane() {
    for off in [0.0_f32, 0.4, 0.8, 1.2, 2.0] {
        let (ok, t) = step_down_from(
            Some(dome_geometry()),
            Vec3::new(100.0 + off, 100.0, on_dome(off).1),
        );
        assert!(ok, "off {off}: the dome is under the body");
        let plane = t.collision_info.contact_plane;

        // Detail 1: the lift goes into the delta before the normalise, so `|n| == 1`.
        let len = plane.normal.mag2().sqrt();
        assert!(
            (len - 1.0).abs() < 1e-5,
            "off {off}: |n| = {len}, not a unit normal"
        );

        // Detail 2: the plane passes through the obstacle's own surface, so the obstacle's centre
        // is exactly `DOME_R` behind it.
        let to_centre = plane.dot_point(dome_centre());
        assert!(
            (to_centre + DOME_R).abs() < 1e-3,
            "off {off}: the obstacle's centre is {to_centre} from the plane, not {}",
            -DOME_R
        );

        // And the mover ends exactly its own radius above it, which is what makes
        // `adjust_offset`'s anti-sink push a no-op against this plane.
        let d = plane.dot_point(t.sphere_path.global_sphere[0].center);
        assert!(
            (d - MOVER_R).abs() < 1e-3,
            "off {off}: the mover's centre is {d} from its own contact plane, not {MOVER_R}"
        );

        // The body really is on the sphere: centre-to-centre is the sum of the radii.
        let sep = t.sphere_path.global_sphere[0]
            .center
            .sub(dome_centre())
            .mag2()
            .sqrt();
        assert!(
            (sep - (DOME_R + MOVER_R)).abs() < 1e-3,
            "off {off}: centre separation {sep}, not {}",
            DOME_R + MOVER_R
        );
    }
}

/// The walkable gate uses the post step normal and the two straddle floor z.
#[test]
fn the_walkable_gate_uses_the_post_step_normal_and_the_two_straddle_floor_z() {
    // 2.5 m off the axis. The body's centre rides the sphere of radius `R + r = 3.5`, so the true
    // contact normal has `n.z = sqrt(3.5^2 - 2.5^2) / 3.5 = 0.6999`, above `floor_z = 0.6642`.
    let off = 2.5_f32;
    assert!(
        dome_normal_z(off) > globals::FLOOR_Z,
        "the subject slope is walkable by construction"
    );
    let (ok, t) = step_down_from(
        Some(dome_geometry()),
        Vec3::new(100.0 + off, 100.0, on_dome(off).1),
    );
    assert!(ok, "the client walks on a slope this shallow");
    let n = t.collision_info.contact_plane.normal;
    assert!(n.z > globals::FLOOR_Z, "n.z = {} must clear floor_z", n.z);

    let r = DOME_R + MOVER_R;
    let (rest, _) = on_dome(off);
    let pre = Vec3::new(
        off,
        0.0,
        (rest + MOVER_R) - dome_centre().z - (STEP - START_ABOVE),
    );
    let old = pre.mul(1.0 / r);
    assert!(
        old.mag2().sqrt() < 0.99,
        "the pre-step delta is short: {}",
        old.mag2().sqrt()
    );
    assert!(
        old.z < globals::FLOOR_Z,
        "and its Z is {} — below floor_z, so the pre-arithmetic refused this ground",
        old.z
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The differential: what changes for a body standing on another object's sphere
// ---------------------------------------------------------------------------------------------

/// The flag changes the water contact bit and not where the body stands.
#[test]
fn the_flag_changes_the_water_contact_bit_and_not_where_the_body_stands() {
    let off = 0.8_f32;
    let (ok, t) = step_down_from(
        Some(dome_geometry()),
        Vec3::new(100.0 + off, 100.0, on_dome(off).1),
    );
    assert!(ok);
    assert!(
        t.collision_info.contact_plane_is_water,
        "the subject carries the client's flag"
    );

    // The next sub-step's `adjust_offset`, run twice on the same state. `offset` is one sub-step of
    // a walk in +X, which is what `find_transitional_position` hands it.
    let offset = Vec3::new(0.03, 0.0, 0.0);

    // Two identical runs of the same probe rather than a clone: the transition is not clonable in
    // the client either, and re-running the whole step-down proves the state is reproducible
    // before the one variable is changed.
    let (_, mut subject) = step_down_from(
        Some(dome_geometry()),
        Vec3::new(100.0 + off, 100.0, on_dome(off).1),
    );
    let (_, mut control) = step_down_from(
        Some(dome_geometry()),
        Vec3::new(100.0 + off, 100.0, on_dome(off).1),
    );
    assert_eq!(
        subject.sphere_path.check_pos.frame.origin, control.sphere_path.check_pos.frame.origin,
        "the two runs must start identical"
    );
    let a = insert::adjust_offset(&mut subject, offset);
    control.collision_info.contact_plane_is_water = false;
    let b = insert::adjust_offset(&mut control, offset);

    assert_eq!(
        a, b,
        "the offset adjust_offset returns is the same either way"
    );
    assert_eq!(
        subject.sphere_path.check_pos.frame.origin, control.sphere_path.check_pos.frame.origin,
        "and so is the check position: the anti-sink push cannot fire against a plane the body is \
         already exactly `radius` from"
    );
    // Which is why: with the flag cleared the guard opens, and the push still declines.
    let plane = control.collision_info.contact_plane;
    let d = plane.dot_point(control.sphere_path.global_sphere[0].center);
    assert!(
        d >= MOVER_R - globals::EPSILON,
        "the anti-sink test `d < radius - EPSILON` must be false: d = {d}"
    );
}

/// And the bit does reach the object. A body that walks onto another object's sphere ends the
/// frame with `WATER_CONTACT_TS`; the same body, same walk, on the terrain does not.
///
/// This is the end-to-end half — the physics time step, the 30 Hz gate, gravity, the sub-step
/// ladder and the commit that copies `contact_plane_is_water` into `transient_state` — rather than
/// a transition driven by hand.
#[test]
fn a_body_walking_onto_another_objects_sphere_reports_water_contact() {
    let on_sphere = walk_across(true);
    let on_terrain = walk_across(false);

    assert!(on_sphere.0, "the body must end up standing on the dome");
    assert!(
        on_sphere.1,
        "and carry WATER_CONTACT_TS: step_sphere_down set the flag and the commit copies it"
    );
    assert!(on_terrain.0, "the control body stands on the terrain");
    assert!(!on_terrain.1, "and does not carry it");
}

/// Walk a body in +X from the crown of the dome (or from the same place with no dome, in which
/// case it falls to the ground and walks along it) and report `(in_contact, in_water_contact)`.
fn walk_across(with_dome: bool) -> (bool, bool) {
    let mut w = flat_world();
    if with_dome {
        register(&mut w, DOME_AT, dome_geometry());
    }
    let h = spawn(&mut w, Vec3::new(100.0, 100.0, 26.05));
    if let Some(o) = w.get_mut(h) {
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            vec![Frame::new(Vec3::new(0.03, 0.0, 0.0), Quat::IDENTITY); 400],
            true,
        )));
    }
    let dt = 1.0 / 30.0;
    let mut time = 0.0;
    for _ in 0..120 {
        time += dt;
        w.use_time(LocalTime(time), false);
    }
    let o = w.get(h).expect("live");
    // The subject really is standing on the dome, not on the ground beside it.
    if with_dome {
        let sep = o
            .position
            .frame
            .origin
            .add(Vec3::new(0.0, 0.0, MOVER_R))
            .sub(dome_centre());
        assert!(
            (sep.mag2().sqrt() - (DOME_R + MOVER_R)).abs() < 1e-2,
            "the body must be on the dome's surface: {:?}",
            o.position.frame.origin
        );
    } else {
        assert!(
            (o.position.frame.origin.z - GROUND).abs() < 0.05,
            "the control body must be on the terrain: {:?}",
            o.position.frame.origin
        );
    }
    (
        o.transient_state.in_contact(),
        o.transient_state.in_water_contact(),
    )
}

/// Behaviour: physics.step-down.a-body-stepping-onto-a-sphere-rides-its-tangent-plane
/// The body rides the dome frame by frame with a unit contact normal.
#[test]
fn the_body_rides_the_dome_frame_by_frame_with_a_unit_contact_normal() {
    let mut w = flat_world();
    register(&mut w, DOME_AT, dome_geometry());
    let h = spawn(&mut w, Vec3::new(100.0, 100.0, 26.05));
    if let Some(o) = w.get_mut(h) {
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            vec![Frame::new(Vec3::new(0.03, 0.0, 0.0), Quat::IDENTITY); 400],
            true,
        )));
    }
    let dt = 1.0 / 30.0;
    let mut time = 0.0;
    let mut on_surface = 0_u32;
    let mut frames = 0_u32;
    let mut last_x = 100.0_f32;
    for _ in 0..300 {
        time += dt;
        w.use_time(LocalTime(time), false);
        let o = w.get(h).expect("live");
        if !o.transient_state.in_contact() {
            continue;
        }
        frames += 1;
        let centre = o.position.frame.origin.add(Vec3::new(0.0, 0.0, MOVER_R));
        let sep = centre.sub(dome_centre()).mag2().sqrt();
        assert!(
            (sep - (DOME_R + MOVER_R)).abs() < 1e-2,
            "frame {frames}: the body left the dome's surface, separation {sep}"
        );
        on_surface += 1;
        let n = o.contact_plane.normal;
        let len = n.mag2().sqrt();
        assert!((len - 1.0).abs() < 1e-4, "frame {frames}: |n| = {len}");
        // And the normal is the surface normal where it stands, not some other plane.
        let want = centre.sub(dome_centre()).mul(1.0 / sep);
        assert!(
            n.sub(want).mag2().sqrt() < 1e-3,
            "frame {frames}: normal {n:?} is not the dome's own normal {want:?}"
        );
        assert!(
            o.transient_state.on_walkable(),
            "frame {frames}: and the dome is walkable here"
        );
        last_x = o.position.frame.origin.x;
    }
    assert!(
        on_surface > 100,
        "the walk must actually spend frames on the dome: {on_surface}"
    );

    // It stops where the dome stops being walkable: the last horizontal offset whose normal still
    // clears `floor_z` is `off = (R + r) * sqrt(1 - floor_z^2) = 2.616 m`.
    let limit = (DOME_R + MOVER_R) * (1.0 - globals::FLOOR_Z * globals::FLOOR_Z).sqrt();
    let off = last_x - DOME_AT.x;
    assert!(
        off < limit,
        "the body walked past the walkable band: {off} >= {limit}"
    );
    assert!(
        off > limit - 0.15,
        "and it should get most of the way there before the slope refuses it: {off} vs {limit}"
    );
    assert!(
        dome_normal_z(off) >= globals::FLOOR_Z,
        "so its last contact is still walkable"
    );
}

/// The consumer the flag guards is real — it just cannot fire against a step-down-on-sphere plane.
///
/// Offset adjustment ends with an anti-sink push: if the mover's centre is
/// closer to the contact plane than its own radius, shove it back out along the plane's normal.
/// The push is skipped when `contact_plane_is_water`. Here the body is put 0.2 m **into** a
/// horizontal contact plane by hand — a state a correct `step_sphere_down` never produces — and
/// the two settings of the flag are run through the real `adjust_offset`.
///
/// Without this, `the_flag_changes_the_water_contact_bit_and_not_where_the_body_stands` could be
/// green because the consumer does not exist rather than because it declines.
#[test]
fn the_anti_sink_push_is_what_the_flag_suppresses() {
    let sunk = |water: bool| {
        let at = Vec3::new(100.0, 100.0, GROUND - 0.2);
        let cell = cell_at(at);
        let mut t = Transition::default();
        t.sphere_path
            .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, MOVER_R), MOVER_R)], 1.0);
        let pos = Position::new(cell, Frame::new(at, Quat::IDENTITY));
        t.sphere_path.init_path(Some(cell), Some(pos), &pos);
        t.sphere_path.set_check_pos(&pos, Some(cell));
        // The mover's centre is at z = 20.3, so it is 0.3 above a plane at z = 20 and its radius
        // is 0.5: sunk by 0.2.
        t.collision_info.set_contact_plane(
            dereth_physics::Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: -GROUND,
            },
            water,
        );
        t.collision_info.contact_plane_cell_id = cell;
        let before = t.sphere_path.check_pos.frame.origin.z;
        let _ = insert::adjust_offset(&mut t, Vec3::new(0.03, 0.0, 0.0));
        t.sphere_path.check_pos.frame.origin.z - before
    };

    let dry = sunk(false);
    assert!(
        (dry - 0.2).abs() < 1e-4,
        "a dry contact plane pushes the sphere back out by (radius - d) / n.z = 0.2, not {dry}"
    );
    let wet = sunk(true);
    assert!(
        (wet - 0.0).abs() < 1e-6,
        "the flag suppresses that push entirely, not {wet}"
    );
}

use crate::common::physics_fixture::{flat_world, player_geometry};
