//! A body walking head-on at a solid object does not pass; a failed transition keeps origin and
//! takes only rotation; an object with no collision geometry rotates and never translates.
//! Fixture: synthetic state, geometry and reference vectors.

use std::sync::Arc;

use dereth_physics::geom::Sphere;
use dereth_physics::{PhysHandle, PhysicsWorld, SetupGeometry, V3};
use dereth_primitives::{Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

const BLOCK: LandblockId = LandblockId(0xA9B4);
/// `StaticLandSource::linear`'s table is `2 * i`, so height byte 10 is z = 20.
const GROUND_Z: f32 = 20.0;
const BODY_RADIUS: f32 = 0.5;
/// The collider's radius. Big enough that a body which passes through it is unmistakable.
const WALL_RADIUS: f32 = 1.5;

// Nine flat landblocks around `BLOCK`, ground at z = 20 — `walk_scenarios`' world.

fn wall_geometry() -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, WALL_RADIUS), WALL_RADIUS)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, WALL_RADIUS), WALL_RADIUS * 2.0),
        radius: WALL_RADIUS,
        height: 2.0 * WALL_RADIUS,
        ..SetupGeometry::default()
    })
}

fn cell_at(x: f32, y: f32, z: f32) -> (dereth_primitives::CellId, Vec3) {
    let mut c = BLOCK.cell(1);
    let mut origin = Vec3::new(x, y, z);
    dereth_physics::landdefs::adjust_to_outside(&mut c, &mut origin);
    (c, origin)
}

/// The walking body. `per_substep` is the animation offset the part update hands back every
/// sub-step, which is how a character actually moves — velocity alone is eaten by friction.
fn spawn_body(w: &mut PhysicsWorld, at: Vec3, per_substep: Vec3, steps: usize) -> PhysHandle {
    let h = w.create(ObjectId(1), player_geometry(), true);
    let (cell, _) = cell_at(at.x, at.y, at.z);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(cell, Frame::new(at, Quat::IDENTITY));
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            vec![Frame::new(per_substep, Quat::IDENTITY); steps],
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    h
}

/// A solid, immovable obstacle. `STATIC_PS` is what a piece of world furniture carries, and it is
/// also what keeps `update_object` from trying to walk it.
fn spawn_wall(w: &mut PhysicsWorld, at: Vec3) -> PhysHandle {
    let h = w.create(ObjectId(7), wall_geometry(), true);
    let (cell, _) = cell_at(at.x, at.y, at.z);
    w.enter_cell(h, cell);
    if let Some(o) = w.get_mut(h) {
        let frame = Frame::new(at, Quat::IDENTITY);
        o.state = dereth_physics::PhysicsState(o.state.0 | 0x0000_0001);
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
        o.transient_state.set_active_bit(false);
    }
    w.calc_cross_cells(h, true);
    h
}

/// A monotonic 30 Hz clock. resets its timestamp and steps nothing
/// on a *backwards* clock, so a test that runs two phases must carry one clock across both —
/// restarting it at zero is a silent no-op and reads exactly like a body that refused to move.
#[derive(Default)]
struct Clock(f64);

impl Clock {
    fn run(&mut self, w: &mut PhysicsWorld, seconds: f64) -> Vec<Vec3> {
        let dt = 1.0 / 30.0;
        let end = self.0 + seconds;
        let mut path = Vec::new();
        while self.0 < end {
            self.0 += dt;
            w.use_time(LocalTime(self.0), false);
            if let Some(h) = w.player() {
                if let Some(o) = w.get(h) {
                    path.push(o.position.frame.origin);
                }
            }
        }
        path
    }
}

/// **A body walked exactly head-on at a solid object must not end up on the far side of it.**
///
/// The judge is penetration, not displacement: a body that walks *past* an obstacle and one an
/// obstacle stopped both end outside its sphere, so the question is how many frames of the walk
/// were spent inside it (`dereth/client/tests/collision_probe`'s rule, applied to a sphere rather
/// than to a BSP).
///
/// The approach is *exactly* along +y at an obstacle whose centre is on the same line, which is
/// the configuration a move-to manufactures: its turn ends
/// with an exact heading snap onto the bearing of the target, and the
/// player then holds forward. No hand-set key sequence produces an exact heading; a move-to does.
#[test]
fn a_body_walking_exactly_head_on_at_a_solid_object_does_not_pass_through_it() {
    let mut w = flat_world();
    let start = Vec3::new(100.0, 100.0, GROUND_Z);
    let wall_at = Vec3::new(100.0, 105.0, GROUND_Z);
    let h = spawn_body(&mut w, start, Vec3::new(0.0, 0.12, 0.0), 600);
    w.set_player(h);
    spawn_wall(&mut w, wall_at);

    let path = Clock::default().run(&mut w, 8.0);

    // The obstacle's sphere in world space, and the body's own sphere: "inside" is the two
    // overlapping, matching the sphere collision test rather than a number this test chose.
    let wall_centre = wall_at.add(Vec3::new(0.0, 0.0, WALL_RADIUS));
    let inside = |p: Vec3| -> bool {
        let c = p.add(Vec3::new(0.0, 0.0, BODY_RADIUS));
        let d = c.sub(wall_centre);
        d.mag2().sqrt() < WALL_RADIUS + BODY_RADIUS - 0.02
    };

    let end = *path.last().expect("the walk produced frames");
    let frames_inside = path.iter().filter(|p| inside(**p)).count();

    // Calibration, before the verdict: the body must actually have set off, or "it did not pass
    // through" is satisfied by a body that never moved.
    assert!(
        end.y - start.y > 1.0,
        "the body must have walked at the obstacle before it can be said not to have passed \
         through it; it moved {:.3} m",
        end.y - start.y
    );

    assert_eq!(
        frames_inside,
        0,
        "the body was inside the obstacle on {frames_inside} of {} frames and ended at \
         y = {:.3} (the obstacle's far side is y = {:.3}). Transition acquisition fails \
         for an exactly head-on step and must then keep the \
         body's own origin",
        path.len(),
        end.y,
        wall_at.y + WALL_RADIUS,
    );
    assert!(
        end.y < wall_at.y - WALL_RADIUS,
        "and it must come to rest short of the obstacle, not past it: y = {:.3}",
        end.y
    );
}

// ---------------------------------------------------------------------------------------------
// Station 2 — the byte-level statement, in isolation.
// ---------------------------------------------------------------------------------------------

/// Behaviour: physics.transition.a-body-whose-transition-fails-keeps-its-origin
/// ** keeps the origin and takes the rotation.** The body is walked into the obstacle
/// until it is pressed against it, and then given a step that both advances it and turns it: the
/// turn must land and the advance must not.
#[test]
fn a_failed_transition_keeps_the_bodys_origin_and_takes_only_the_rotation() {
    let mut w = flat_world();
    let start = Vec3::new(100.0, 100.0, GROUND_Z);
    let wall_at = Vec3::new(100.0, 103.0, GROUND_Z);
    let h = spawn_body(&mut w, start, Vec3::new(0.0, 0.12, 0.0), 600);
    w.set_player(h);
    spawn_wall(&mut w, wall_at);

    // Walk until the body is pressed against the obstacle and its sliding normal is armed. One
    // clock across both phases -- see [`Clock`].
    let mut clock = Clock::default();
    let _ = clock.run(&mut w, 4.0);
    let pressed = w.get(h).expect("live").position.frame.origin;
    assert!(
        w.get(h).expect("live").transient_state.is_sliding(),
        "the body must be pressed against the obstacle for this station to be about the NULL arm"
    );

    // One more sub-step: forward (refused) plus a quarter turn (accepted).
    // A quarter turn about z: (cos(45 deg), 0, 0, sin(45 deg)).
    let turn = Quat::new(
        std::f32::consts::FRAC_1_SQRT_2,
        0.0,
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
    );
    if let Some(o) = w.get_mut(h) {
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            vec![Frame::new(Vec3::new(0.0, 0.12, 0.0), turn)],
            true,
        )));
    }
    let before_rot = w.get(h).expect("live").position.frame.rotation;
    // Half a second rather than one frame: 's gate does not open on
    // every call at a 30 Hz feed (see `an_opened_gate_stamps_the_clock_verbatim_and_discards_the_
    // residual`), so a single call can be a silent no-op. Only one offset is in the script; every
    // sub-step after it gets `ScriptedMotion`'s identity.
    let _ = clock.run(&mut w, 0.5);
    let o = w.get(h).expect("live");

    assert!(
        (o.position.frame.origin.y - pressed.y).abs() < 0.01,
        "the refused step must not translate the body: {:.4} -> {:.4}",
        pressed.y,
        o.position.frame.origin.y
    );
    assert!(
        (o.position.frame.rotation.w - before_rot.w).abs() > 1e-4,
        "but the rotation half of the requested frame is kept: w {:.6} -> {:.6}",
        before_rot.w,
        o.position.frame.rotation.w
    );
    assert_eq!(
        o.cached_velocity,
        Vec3::ZERO,
        "the failed-transition arm zeroes the cached velocity"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 3 — the other arm of the same rule.
// ---------------------------------------------------------------------------------------------

/// An object with no collision geometry rotates and never translates.
#[test]
fn an_object_with_no_collision_geometry_rotates_and_never_translates() {
    // A quarter turn about z: (cos(45 deg), 0, 0, sin(45 deg)).
    let turn = Quat::new(
        std::f32::consts::FRAC_1_SQRT_2,
        0.0,
        0.0,
        std::f32::consts::FRAC_1_SQRT_2,
    );
    let start = Vec3::new(100.0, 100.0, GROUND_Z);

    let arm = |has_geometry: bool, at: Vec3, offset: Frame| -> (Vec3, Quat) {
        let mut w = flat_world();
        let h = w.create(ObjectId(1), player_geometry(), true);
        let (cell, _) = cell_at(at.x, at.y, at.z);
        w.enter_cell(h, cell);
        {
            let o = w.get_mut(h).expect("live");
            o.position = Position::new(cell, Frame::new(at, Quat::IDENTITY));
            o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
                vec![offset; 400],
                has_geometry,
            )));
            o.transient_state.set_active_bit(true);
            o.calc_acceleration();
            o.update_time = 0.0;
        }
        w.calc_cross_cells(h, false);
        w.set_player(h);
        // Long enough for the body to settle onto the ground and pick up `ON_WALKABLE_TS`, which
        // is what un-zeroes the animation origin.
        let _ = Clock::default().run(&mut w, 4.0);
        let o = w.get(h).expect("live");
        (o.position.frame.origin, o.position.frame.rotation)
    };

    // Claim 1 — the translation, driven by **gravity** rather than by an animation offset. An
    // object with no collision geometry never acquires `ON_WALKABLE_TS` (nothing puts it in
    // contact with anything), and multiplies the animation
    // origin by `0.0` while that bit is clear -- so an animated walk is refused upstream of this
    // arm and could not falsify it. A fall is not: `update_physics_internal` writes the requested
    // origin whatever the contact state.
    let above = Vec3::new(start.x, start.y, GROUND_Z + 5.0);
    let still = Frame::new(Vec3::ZERO, Quat::IDENTITY);
    let (control, _) = arm(true, above, still);
    let (subject, _) = arm(false, above, still);
    assert!(
        (control.z - GROUND_Z).abs() < 0.05,
        "the control must fall to the ground, or the subject's stillness measures nothing: \
         z = {:.4}",
        control.z
    );
    assert!(
        (subject.z - above.z).abs() < 0.001,
        "no collision geometry means no translation at all (the client loads its current origin into the \
         requested frame's origin before `set_frame`): z {:.4} -> {:.4}",
        above.z,
        subject.z
    );

    // Claim 2 — the rotation survives on the same arm. A pure turn, no offset origin at all.
    let spin = Frame::new(Vec3::ZERO, turn);
    let (_, spun) = arm(false, start, spin);
    assert!(
        (spun.w - 1.0).abs() > 1e-4,
        "but the rotation half of the requested frame still lands: w = {:.6}",
        spun.w
    );
}

use crate::common::physics_fixture::{flat_world, player_geometry};
