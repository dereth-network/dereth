//! A cylsphere-only object and a row of them stop a body; the cylsphere arm runs only when
//! cylspheres exist and wins over spheres; the cap bounds the cylinder; object scale reaches radius
//! and height.
//! Fixture: synthetic state, geometry and reference vectors.

use std::sync::Arc;

use dereth_physics::geom::{CylSphere, Sphere};
use dereth_physics::transition::collide;
use dereth_physics::{PhysHandle, PhysicsWorld, SetupGeometry, Transition, TransitionState, V3};
use dereth_primitives::num::math;
use dereth_primitives::{Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

const BLOCK: LandblockId = LandblockId(0xA9B4);
/// Ground height of a flat block at table index 10 with the linear `2 * i` table.
const GROUND: f32 = 20.0;

// Nine flat landblocks around `(0xA9, 0xB4)`, ground at z = 20.

/// which land cell of the block a block-local point
/// falls in. A literal `cell(1)` is the wrong cell for anything but the first 24 m square, and an
/// object registered into one cell and looked for in another is silently not there.
fn cell_at(p: Vec3) -> dereth_primitives::CellId {
    let mut c = BLOCK.cell(1);
    let mut o = p;
    assert!(dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o));
    c
}

/// The obstacle: a pillar 2 m tall and 0.6 m in radius standing on the ground, with **no**
/// [`Sphere`] and **no** part BSP — the shape of the 88 cylinder-sphere-only placements.
const PILLAR: CylSphere = CylSphere {
    low_pt: Vec3::new(0.0, 0.0, 0.0),
    height: 2.0,
    radius: 0.6,
};

/// The subject and its control differ in exactly one field.
///
/// The sorting sphere is **not** that field and is the same in both: cross-cell registration
/// uses the setup's sorting sphere, so without one neither
/// build would see the object at all and the comparison would be between two absences.
fn pillar_geometry(with_cylspheres: bool) -> Arc<SetupGeometry> {
    Arc::new(SetupGeometry {
        cyl_spheres: if with_cylspheres {
            vec![PILLAR]
        } else {
            Vec::new()
        },
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 1.0), 2.2),
        radius: PILLAR.radius,
        height: PILLAR.height,
        ..SetupGeometry::default()
    })
}

fn spawn(w: &mut PhysicsWorld, at: Vec3, per_substep: Vec3) -> PhysHandle {
    let h = w.create(ObjectId(1), player_geometry(), true);
    let cell = cell_at(at);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(cell, Frame::new(at, Quat::IDENTITY));
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            vec![Frame::new(per_substep, Quat::IDENTITY); 400],
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    h
}

///  for one placement, reduced to a single object so
/// that the differential has exactly one variable.
fn register(w: &mut PhysicsWorld, at: Vec3, g: Arc<SetupGeometry>) {
    let h = w.create(ObjectId(0), g, false);
    let cell = cell_at(at);
    w.enter_cell(h, cell);
    let frame = Frame::new(at, Quat::IDENTITY);
    if let Some(o) = w.get_mut(h) {
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
    }
    w.calc_cross_cells(h, true);
}

/// The same walk, sampled: every frame's resting origin, so that "the body passed through the
/// object" is a claim about the whole path and not only about where it happened to stop.
fn run_sampled(w: &mut PhysicsWorld, h: PhysHandle, seconds: f64) -> Vec<Vec3> {
    let dt = 1.0 / 30.0;
    let mut t = 0.0;
    let mut out = Vec::new();
    while t < seconds {
        t += dt;
        w.use_time(LocalTime(t), false);
        out.push(w.get(h).expect("live").position.frame.origin);
    }
    out
}

/// Is the body **inside** the pillar at a resting position — the point-in-solid test?
///
/// This is the cylinder's counterpart of the
/// verdict `dereth/client/tests/dat/objects/mesh_collision.rs` uses for the BSP arm, and the "point in solid"
/// shape is deliberate. cannot serve as the verdict:
/// it is the arm's own `<=` **contact** test, so a body correctly stopped *touching* the pillar
/// satisfies it to within a rounding error. Contact and interior have to be told apart, and the
/// body's **centre** being inside the solid column is the unambiguous half. The margin the arm
/// actually holds is asserted separately, as a closest approach against the cylinder's own radius.
fn inside_pillar(pillar_at: Vec3, body: Vec3) -> bool {
    let low = PILLAR.low_pt.add(pillar_at);
    let d = body.add(Vec3::new(0.0, 0.0, 0.5)).sub(low);
    d.x * d.x + d.y * d.y <= PILLAR.radius * PILLAR.radius && d.z >= 0.0 && d.z <= PILLAR.height
}

/// How far a body walked, in the ground plane.
fn travel(from: Vec3, to: Vec3) -> f32 {
    math::hypotf(to.x - from.x, to.y - from.y)
}

/// One collision query against a pillar standing at `pillar`, driven
/// by hand so the counters survive: `PhysicsWorld` hands its transition back to the pool at the
/// end of every step.
///
/// The mover stands at `from` and the check position is `from + by`, which is what one sub-step of
/// a transition sweep would set up. Returns the state and how many cylinder-spheres
/// the arm actually tested.
fn probe(
    g: Arc<SetupGeometry>,
    pillar: Vec3,
    scale: f32,
    from: Vec3,
    by: Vec3,
) -> (TransitionState, u32) {
    let mut w = flat_world();
    let mover = spawn(&mut w, from, Vec3::ZERO);
    let cell = cell_at(pillar);
    let h = w.create(ObjectId(9), g, false);
    w.enter_cell(h, cell);
    if let Some(o) = w.get_mut(h) {
        let frame = Frame::new(pillar, Quat::IDENTITY);
        o.set_frame(frame);
        o.position = Position::new(cell, frame);
        o.scale = scale;
    }
    w.calc_cross_cells(h, true);

    let mut t = Transition::default();
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
    let start = Position::new(cell_at(from), Frame::new(from, Quat::IDENTITY));
    t.sphere_path
        .init_path(Some(start.cell), Some(start), &start);
    t.sphere_path.set_check_pos(&start, Some(start.cell));
    t.sphere_path.add_offset_to_check_pos(by);
    t.object_info.object = Some(mover);
    let ctx = w.transition_ctx(Some(mover));
    let r = collide::cell_find_obj_collisions(&ctx, &mut t, cell);
    (r, t.counters.object_cylspheres_tested)
}

fn approaches(pillar: Vec3, range: f32) -> Vec<(Vec3, Vec3)> {
    let contact = (PILLAR.radius - dereth_physics::globals::EPSILON) + 0.5;
    let mut v = Vec::new();
    for k in 0..24_i8 {
        let a = f32::from(k) * std::f32::consts::TAU / 24.0;
        let out = Vec3::new(math::cosf(a), math::sinf(a), 0.0);
        let tangent = Vec3::new(-out.y, out.x, 0.0);
        for frac in [0.5_f32, 0.65, 0.8] {
            for sign in [1.0_f32, -1.0] {
                let start = pillar
                    .add(out.mul(range))
                    .add(tangent.mul(contact * frac * sign));
                v.push((start, out.negate().mul(0.12)));
            }
        }
    }
    v
}

// ---------------------------------------------------------------------------------------------
// 1. The acceptance line
// ---------------------------------------------------------------------------------------------

/// Behaviour: physics.collision.a-cylinder-only-object-stops-a-walking-body
/// **The acceptance line.** A body walking at an object whose only collision volume is a
/// [`CylSphere`] is stopped short of it, where the same body in the same world with the cylinder-sphere
/// list empty walks straight through the cylinder — the same object solid and intangible, not
/// solid at two different shapes.
///
/// The claim is about the whole **path**, not only the resting place: each trajectory is sampled
/// every frame and checked for overlap, because a body that walked
/// through an object and out the far side has a final position outside it and has still proved the
/// object was not there.
#[test]
fn a_cylsphere_only_object_stops_a_body_that_used_to_walk_through_it() {
    let pillar = Vec3::new(105.0, 100.0, GROUND);

    let trial = |with: bool, start: Vec3, step: Vec3| -> Vec<Vec3> {
        let mut w = flat_world();
        register(&mut w, pillar, pillar_geometry(with));
        let h = spawn(&mut w, start, step);
        run_sampled(&mut w, h, 5.0)
    };
    let inside_frames = |path: &[Vec3]| path.iter().filter(|p| inside_pillar(pillar, **p)).count();
    let closest = |path: &[Vec3]| {
        path.iter()
            .map(|p| math::hypotf(p.x - pillar.x, p.y - pillar.y))
            .fold(f32::INFINITY, f32::min)
    };

    let mut compared = 0usize;
    let mut worst = 0.0_f32;
    let mut reported = false;
    for range in [2.0_f32, 2.5, 3.0] {
        for (start, step) in approaches(pillar, range) {
            if inside_pillar(pillar, start) {
                continue;
            }
            let through = trial(false, start, step);
            let end_through = *through.last().expect("frames");
            if inside_frames(&through) == 0 || travel(start, end_through) <= 0.5 {
                continue;
            }
            let stopped = trial(true, start, step);
            let end_stopped = *stopped.last().expect("frames");
            assert_eq!(
                inside_frames(&stopped),
                0,
                "from ({:.2}, {:.2}) heading ({:.3}, {:.3}) the body entered the cylinder with \
                 the arm on; closest approach {:.3} m, ended at {end_stopped:?}",
                start.x,
                start.y,
                step.x,
                step.y,
                closest(&stopped)
            );
            let gained = travel(end_stopped, end_through);
            worst = worst.max(gained);
            if !reported {
                reported = true;
                eprintln!(
                    "pillar r={} h={} at ({:.1}, {:.1}); from ({:.2}, {:.2}) the control \
                     walked {:.3} m to ({:.3}, {:.3}), passing INSIDE the cylinder on {} of {} \
                     frames and closing to {:.3} m of the axis; with the cylsphere arm the body \
                     walked {:.3} m to ({:.3}, {:.3}), never closer than {:.3} m, and ended \
                     {gained:.3} m short of the control",
                    PILLAR.radius,
                    PILLAR.height,
                    pillar.x,
                    pillar.y,
                    start.x,
                    start.y,
                    travel(start, end_through),
                    end_through.x,
                    end_through.y,
                    inside_frames(&through),
                    through.len(),
                    closest(&through),
                    travel(start, end_stopped),
                    end_stopped.x,
                    end_stopped.y,
                    closest(&stopped),
                );
            }
            compared += 1;
        }
    }
    eprintln!(
        "{compared} oblique approach(es) drove the control into the cylinder; the largest \
         difference the arm made was {worst:.3} m"
    );
    assert!(
        compared >= 1,
        "no approach drove the control into the cylinder, so the test is not measuring anything"
    );
    assert!(
        worst > 0.5,
        "the arm made almost no difference anywhere: {worst:.3} m"
    );
}

/// **The stop.** One cylsphere deflects a glancing body around itself, which is what a 0.6 m
/// pillar should do; a *row* of them is a wall, and a wall is where "solid" and "intangible" are
/// visible as "crossed it" and "did not".
///
/// The row is **one object carrying seven cylspheres**, so this also drives the arm's loop over
///  entries rather than only its first. Three of the five
/// shipped door setups carry two each.
#[test]
fn a_row_of_cylspheres_on_one_object_is_a_wall_the_body_cannot_cross() {
    let at = Vec3::new(105.0, 100.0, GROUND);
    // Seven pillars a metre apart: 0.6 m radii, so the gaps are closed.
    let row: Vec<CylSphere> = (-3_i8..=3)
        .map(|i| CylSphere {
            low_pt: Vec3::new(0.0, f32::from(i), 0.0),
            height: PILLAR.height,
            radius: PILLAR.radius,
        })
        .collect();
    let wall = |with: bool| {
        Arc::new(SetupGeometry {
            cyl_spheres: if with { row.clone() } else { Vec::new() },
            sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 1.0), 6.0),
            ..SetupGeometry::default()
        })
    };

    // Thirty degrees off the wall's normal, so the contact is oblique and the crease branch of
    // offset adjustment is not what decides the answer.
    let start = Vec3::new(101.0, 99.0, GROUND);
    let step = Vec3::new(
        math::cosf(30.0_f32.to_radians()),
        math::sinf(30.0_f32.to_radians()),
        0.0,
    )
    .mul(0.12);

    let trial = |with: bool| -> Vec<Vec3> {
        let mut w = flat_world();
        register(&mut w, at, wall(with));
        let h = spawn(&mut w, start, step);
        run_sampled(&mut w, h, 5.0)
    };
    let through = trial(false);
    let stopped = trial(true);
    let far_side = |path: &[Vec3]| path.iter().filter(|p| p.x > at.x).count();
    let end_through = *through.last().expect("frames");
    let end_stopped = *stopped.last().expect("frames");

    eprintln!(
        "a seven-cylsphere wall at x = {:.1}; from ({:.1}, {:.1}) the control walked \
         {:.3} m to ({:.3}, {:.3}), crossing to the far side on {} of {} frames; with the arm the \
         body walked {:.3} m to ({:.3}, {:.3}) and never crossed, stopping {:.3} m short of the \
         wall",
        at.x,
        start.x,
        start.y,
        travel(start, end_through),
        end_through.x,
        end_through.y,
        far_side(&through),
        through.len(),
        travel(start, end_stopped),
        end_stopped.x,
        end_stopped.y,
        at.x - end_stopped.x,
    );
    assert!(
        far_side(&through) > 0,
        "the control never reached the wall; it ended {end_through:?}"
    );
    assert_eq!(
        far_side(&stopped),
        0,
        "the body crossed the wall: it ended {end_stopped:?}"
    );
    assert!(
        end_stopped.x < at.x - PILLAR.radius,
        "the body ended inside the wall's own radius: {end_stopped:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Which arm ran, and what the arm read
// ---------------------------------------------------------------------------------------------

/// The counter half of the acceptance claim: the cylsphere arm is **entered**, and it is not
/// entered when the list is empty. Without this a passing walk could be a body that stopped for
/// some unrelated reason.
#[test]
fn the_arm_taken_is_the_cylsphere_arm_and_only_when_there_are_cylspheres() {
    let pillar = Vec3::new(101.0, 100.0, GROUND);
    let from = Vec3::new(100.0, 100.0, GROUND);
    let by = Vec3::new(0.4, 0.0, 0.0);

    let (with_r, with_n) = probe(pillar_geometry(true), pillar, 1.0, from, by);
    assert_eq!(with_n, 1, "exactly the one cylsphere was tested");
    assert_ne!(with_r, TransitionState::Ok, "the cylsphere is solid");

    let (without_r, without_n) = probe(pillar_geometry(false), pillar, 1.0, from, by);
    assert_eq!(
        without_n, 0,
        "an empty cylsphere list must not enter the arm"
    );
    assert_eq!(
        without_r,
        TransitionState::Ok,
        "with no spheres and no cylspheres the object is intangible, which is the pre-build"
    );
}

/// The bare sphere entry point can never reach the cylsphere arm.
#[test]
fn the_bare_sphere_entry_point_can_never_reach_the_cylsphere_arm() {
    let pillar = Vec3::new(101.0, 100.0, GROUND);
    let from = Vec3::new(100.0, 100.0, GROUND);
    let cell = cell_at(pillar);
    let mut w = flat_world();
    let mover = spawn(&mut w, from, Vec3::ZERO);
    let obstacle = w.create(ObjectId(9), pillar_geometry(true), false);
    w.enter_cell(obstacle, cell);
    w.get_mut(obstacle).expect("live").position =
        Position::new(cell, Frame::new(pillar, Quat::IDENTITY));
    w.calc_cross_cells(obstacle, false);

    let mut t = Transition::default();
    t.sphere_path
        .init_sphere(&[Sphere::new(Vec3::new(0.0, 0.0, 0.5), 0.5)], 1.0);
    let start = Position::new(cell_at(from), Frame::new(from, Quat::IDENTITY));
    t.sphere_path
        .init_path(Some(start.cell), Some(start), &start);
    t.sphere_path.set_check_pos(&start, Some(start.cell));
    t.sphere_path
        .add_offset_to_check_pos(Vec3::new(0.4, 0.0, 0.0));
    t.object_info.object = Some(mover);

    let ctx = w.transition_ctx(Some(mover));
    let state = w.get(obstacle).expect("live").state;
    let pos = w.get(obstacle).expect("live").position;
    let r = collide::find_obj_collisions(&ctx, &mut t, obstacle, state, &[], 1.0, &pos);
    assert_eq!(r, TransitionState::Ok);
    assert_eq!(
        t.counters.object_cylspheres_tested, 0,
        "ObjGeometry::Spheres models a NULL part array; it has no cylsphere list to offer"
    );
}

/// Cylspheres win over spheres when an object carries both.
#[test]
fn cylspheres_win_over_spheres_when_an_object_carries_both() {
    let pillar = Vec3::new(101.0, 100.0, GROUND);
    let both = Arc::new(SetupGeometry {
        cyl_spheres: vec![PILLAR],
        // A sphere 50 m up: it could not possibly be what stopped the body, and if the sphere arm
        // ran instead this probe would come back `OK_TS` with no cylsphere tested.
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 50.0), 0.1)],
        sorting_sphere: Sphere::new(Vec3::new(0.0, 0.0, 1.0), 2.2),
        ..SetupGeometry::default()
    });
    let (r, n) = probe(
        both,
        pillar,
        1.0,
        Vec3::new(100.0, 100.0, GROUND),
        Vec3::new(0.4, 0.0, 0.0),
    );
    assert_eq!(n, 1, "GetNumCylsphere() != 0 decides the arm");
    assert_ne!(r, TransitionState::Ok);
}

/// A cylinder is not its bounding sphere: it is bounded above by a flat cap at `height`, and a
/// body whose sphere clears that cap passes over it. The same body at chest height does not.
#[test]
fn the_cap_bounds_the_cylinder_where_a_bounding_sphere_would_not() {
    let pillar = Vec3::new(101.0, 100.0, GROUND);
    let by = Vec3::new(0.4, 0.0, 0.0);
    // On the ground: the mover's sphere spans z = 20.0 .. 21.0, well inside the 2 m column.
    let (low, low_n) = probe(
        pillar_geometry(true),
        pillar,
        1.0,
        Vec3::new(100.0, 100.0, GROUND),
        by,
    );
    assert_eq!(low_n, 1);
    assert_ne!(
        low,
        TransitionState::Ok,
        "at chest height the column is solid"
    );

    // Above the cap: the sphere spans z = 23.0 .. 24.0 against a cap at z = 22.0, so it is clear
    // of it by 1 m -- more than the sphere's own radius, which is what the slab test allows.
    let (high, high_n) = probe(
        pillar_geometry(true),
        pillar,
        1.0,
        Vec3::new(100.0, 100.0, GROUND + 3.0),
        by,
    );
    assert_eq!(
        high_n, 1,
        "the arm still ran; it is the geometry that answered no"
    );
    assert_eq!(
        high,
        TransitionState::Ok,
        "a 2 m cylinder must not stop a body passing 1 m above its cap"
    );
}

/// The scaling wrapper multiplies `low_pt`, `radius`
/// **and** `height` by the object's scale before the dispatcher sees them. At half scale the
/// pillar is 0.3 m in radius, so a lane a full-scale one blocks it does not.
#[test]
fn the_object_scale_reaches_the_cylinder_and_not_just_its_position() {
    let pillar = Vec3::new(101.0, 100.0, GROUND);
    // The check position ends level with the axis and 0.95 m to one side of it: inside
    // 0.6 + 0.5 but outside 0.3 + 0.5.
    let from = Vec3::new(100.6, 100.95, GROUND);
    let by = Vec3::new(0.4, 0.0, 0.0);
    let (full, full_n) = probe(pillar_geometry(true), pillar, 1.0, from, by);
    let (half, half_n) = probe(pillar_geometry(true), pillar, 0.5, from, by);
    assert_eq!((full_n, half_n), (1, 1), "the arm ran in both");
    assert_ne!(
        full,
        TransitionState::Ok,
        "0.6 + 0.5 > 0.95, so the full-scale pillar is in the way"
    );
    assert_eq!(
        half,
        TransitionState::Ok,
        "0.3 + 0.5 < 0.95, so the half-scale one is not"
    );
}

/// The object scale reaches the cylinders height as well as its radius.
#[test]
fn the_object_scale_reaches_the_cylinders_height_as_well_as_its_radius() {
    let pillar = Vec3::new(101.0, 100.0, GROUND);
    let by = Vec3::new(0.4, 0.0, 0.0);
    // A body whose sphere spans z = 21.4 .. 22.4 against a 2 m cap at z = 22.0: inside the slab.
    let from = Vec3::new(100.0, 100.0, GROUND + 1.4);
    let (full, _) = probe(pillar_geometry(true), pillar, 1.0, from, by);
    assert_ne!(
        full,
        TransitionState::Ok,
        "the full-height column reaches z = 22.0"
    );
    // At half scale the cap is at z = 21.0 and the sphere's underside at 21.4 clears it.
    let (half, half_n) = probe(pillar_geometry(true), pillar, 0.5, from, by);
    assert_eq!(half_n, 1);
    assert_eq!(
        half,
        TransitionState::Ok,
        "the half-height column stops at z = 21.0"
    );
}

use crate::common::physics_fixture::{flat_world, player_geometry};
