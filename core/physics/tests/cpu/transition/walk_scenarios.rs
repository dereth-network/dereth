//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! End-to-end PhysicsWorld: a drop lands with contact, resting does not sink, crossing a landblock
//! boundary updates the cell, walkable slope boundary, cached velocity is achieved velocity, 30 Hz
//! gate, static obstacle reported, missiles stop, shadow registration/unregistration, transition
//! pool depth one.
//! Fixture: synthetic state, geometry and reference vectors.

use std::sync::Arc;

use dereth_physics::land::VERTEX_COUNT;
use dereth_physics::{
    globals, is_valid_walkable, LandblockCollision, PhysicsWorld, StaticLandSource,
};
use dereth_primitives::num::math;
use dereth_primitives::{Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

/// The linear `2 * i` height table, which is what the retail table is over its first 201 entries.
fn table() -> [f32; 256] {
    let mut t = [0.0_f32; 256];
    for (i, v) in t.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        {
            *v = i as f32 * 2.0;
        }
    }
    t
}

// A world of nine flat landblocks around `(0xA9, 0xB4)`, ground at z = 20.

/// A world whose single landblock rises `rise * 2` metres per 24 m column in +X.
fn ramp_world(rise: u8) -> PhysicsWorld {
    let mut h = [0_u8; VERTEX_COUNT];
    for i in 0..9_usize {
        for j in 0..9_usize {
            #[allow(clippy::cast_possible_truncation)]
            {
                h[i * 9 + j] = (i as u8).saturating_mul(rise);
            }
        }
    }
    let block = LandblockCollision::build(
        LandblockId::new(0xA9, 0xB4),
        Box::new(h),
        Box::new([0; VERTEX_COUNT]),
        false,
        8,
        &table(),
    )
    .expect("full detail");
    let mut land = StaticLandSource::new(table());
    land.add_block(block);
    PhysicsWorld::new(Arc::new(land))
}

fn spawn(w: &mut PhysicsWorld, id: u32, x: f32, y: f32, z: f32) -> dereth_physics::PhysHandle {
    let h = w.create(ObjectId(id), player_geometry(), true);
    let cell = {
        let mut c = LandblockId::new(0xA9, 0xB4).cell(1);
        let mut origin = Vec3::new(x, y, z);
        dereth_physics::landdefs::adjust_to_outside(&mut c, &mut origin);
        c
    };
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        o.position = Position::new(cell, Frame::new(Vec3::new(x, y, z), Quat::IDENTITY));
        o.set_motion(Box::new(dereth_physics::ScriptedMotion::new(
            Vec::new(),
            true,
        )));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    h
}

/// Give an object an animation-driven walk: `ScriptedMotion` hands back one frame offset per
/// sub-step, which is how a character actually moves in the client. Velocity alone does not work:
/// `calc_friction` with the default `friction = 0.95` costs 95% of the speed every second, so a
/// thrown object travels little over a metre.
///
/// The offset is applied only while `ON_WALKABLE_TS` is set (
/// multiplies it by `scale` on the ground and by `0.0` in the air), so the first sub-step or two
/// go nowhere while contact is established.
fn walk(w: &mut PhysicsWorld, h: dereth_physics::PhysHandle, per_substep: Vec3, steps: usize) {
    let offsets = vec![Frame::new(per_substep, Quat::IDENTITY); steps];
    w.get_mut(h)
        .expect("live")
        .set_motion(Box::new(dereth_physics::ScriptedMotion::new(offsets, true)));
}

/// Drive the world for `seconds` at a fixed frame rate, returning how many times the 30 Hz gate
/// actually opened.
fn run(w: &mut PhysicsWorld, seconds: f64, fps: f64) -> u32 {
    let dt = 1.0 / fps;
    let mut t = 0.0;
    let mut ticks = 0;
    while t < seconds {
        t += dt;
        if w.use_time(LocalTime(t), false) {
            ticks += 1;
        }
    }
    ticks
}

#[test]
fn an_object_dropped_above_flat_ground_lands_and_reports_contact() {
    let mut w = flat_world();
    // Ground at z = 20; drop from 25 with a 0.5 m sphere sitting at local (0, 0, 0.5).
    let h = spawn(&mut w, 1, 100.0, 100.0, 25.0);
    let _ = run(&mut w, 4.0, 30.0);
    let o = w.get(h).expect("live");
    let z = o.position.frame.origin.z;
    assert!(
        (z - 20.0).abs() < 0.05,
        "the object should come to rest with its sphere's bottom on the ground: z = {z}"
    );
    assert!(o.transient_state.in_contact(), "and report contact");
    assert!(o.transient_state.on_walkable(), "on walkable ground");
    assert!(is_valid_walkable(o.contact_plane.normal));
    assert!(
        o.velocity_vector.z.abs() < 1.0,
        "and stop falling: {:?}",
        o.velocity_vector
    );
}

#[test]
fn an_object_already_resting_on_the_ground_does_not_sink() {
    let mut w = flat_world();
    let h = spawn(&mut w, 1, 100.0, 100.0, 20.0);
    let _ = run(&mut w, 2.0, 30.0);
    let z = w.get(h).expect("live").position.frame.origin.z;
    assert!((z - 20.0).abs() < 0.01, "z drifted to {z}");
}

#[test]
fn a_walking_object_crosses_a_landblock_boundary_and_its_cell_id_follows() {
    let mut w = flat_world();
    // Start 3 m short of the block's east edge and walk east at 4 m/s.
    let h = spawn(&mut w, 1, 189.0, 100.0, 20.0);
    let start_block = w.get(h).expect("live").position.cell.landblock();
    assert_eq!(start_block, LandblockId::new(0xA9, 0xB4));
    // 0.15 m per 1/30 s sub-step is 4.5 m/s; 90 sub-steps carries it 13 m, well past the seam.
    walk(&mut w, h, Vec3::new(0.15, 0.0, 0.0), 200);
    let _ = run(&mut w, 3.0, 30.0);
    let o = w.get(h).expect("live");
    assert_eq!(
        o.position.cell.landblock(),
        LandblockId::new(0xAA, 0xB4),
        "the object must end up in the block to the east, at {:?}",
        o.position
    );
    assert!(
        o.position.frame.origin.x >= 0.0 && o.position.frame.origin.x < 192.0,
        "and its origin must be re-based into that block: {:?}",
        o.position.frame.origin
    );
    assert!(
        o.transient_state.in_contact(),
        "while staying on the ground"
    );
}

/// The `floor_z` constant is `cos(48.381 degrees)`, so a 45-degree ramp is
/// walkable and a 60-degree one is not — and the classification is what decides whether the
/// object gets `ON_WALKABLE_TS`.
#[test]
fn the_walkable_slope_boundary_is_the_baked_floor_z() {
    // rise = 12 table steps per 24 m column is 24 m of rise over 24 m of run: 45 degrees.
    let shallow = ramp_world(12);
    let n = shallow
        .land()
        .landblock(LandblockId::new(0xA9, 0xB4))
        .expect("resident")
        .polygons[0]
        .plane
        .normal;
    let cos45 = 1.0_f32 / 2.0_f32.sqrt();
    assert!((n.z - cos45).abs() < 1e-4, "a 45 degree ramp: {n:?}");
    assert!(
        is_valid_walkable(n),
        "45 degrees is inside the 48.381 degree limit"
    );

    // rise = 21 is 42 m over 24 m, about 60.3 degrees.
    let steep = ramp_world(21);
    let n = steep
        .land()
        .landblock(LandblockId::new(0xA9, 0xB4))
        .expect("resident")
        .polygons[0]
        .plane
        .normal;
    assert!(!is_valid_walkable(n), "60 degrees is outside it: {n:?}");
    // And the boundary really is the baked constant, not a rounded 48 or 50 degrees.
    let limit = math::acosf(globals::FLOOR_Z).to_degrees();
    assert!((limit - 48.381).abs() < 0.01, "{limit}");
}

#[test]
fn an_object_resting_on_a_walkable_ramp_reports_on_walkable() {
    let mut w = ramp_world(6); // 12 m over 24 m, about 26.6 degrees
    let h = spawn(&mut w, 1, 100.0, 100.0, 60.0);
    let _ = run(&mut w, 6.0, 30.0);
    let o = w.get(h).expect("live");
    assert!(o.transient_state.in_contact(), "{:?}", o.position);
    assert!(o.transient_state.on_walkable());
    assert!(
        o.contact_plane.normal.z > globals::FLOOR_Z,
        "the contact plane is the ramp: {:?}",
        o.contact_plane
    );
}

/// `get_velocity()` is `cached_velocity`, the **achieved** velocity, and it is
/// zero whenever the object was blocked.
#[test]
fn cached_velocity_is_the_achieved_velocity_not_the_requested_one() {
    let mut w = flat_world();
    let h = spawn(&mut w, 1, 100.0, 100.0, 20.0);
    walk(&mut w, h, Vec3::new(0.15, 0.0, 0.0), 200);
    // 20 fps: every frame clears the 30 Hz gate cleanly, so each sub-step is exactly 0.05 s and
    // the achieved velocity is 0.15 / 0.05 = 3 m/s. (At exactly 30 fps the accumulated wall clock
    // straddles the gate and roughly every other frame is skipped, which is correct behaviour but
    // makes the arithmetic here ambiguous.)
    let _ = run(&mut w, 1.0, 20.0);
    let o = w.get(h).expect("live");
    assert!(
        (o.velocity().x - 3.0).abs() < 0.2,
        "an unobstructed walk reports the achieved velocity: {:?}",
        o.velocity()
    );

    // Now a stationary object: the achieved velocity is zero even though velocity_vector is not
    // necessarily, because the sub-step produced no displacement.
    let mut w = flat_world();
    let h = spawn(&mut w, 1, 100.0, 100.0, 20.0);
    let _ = run(&mut w, 1.0, 30.0);
    assert_eq!(
        w.get(h).expect("live").velocity(),
        Vec3::ZERO,
        "an object that did not move reports zero"
    );
}

#[test]
fn the_thirty_hertz_gate_bounds_how_often_physics_runs() {
    // Whatever the frame rate, the gate opens at most 30 times a second: two seconds cost at most
    // ~60 ticks even at 250 fps. It can open slightly *less* often than 30 Hz, because the
    // accumulated wall clock straddles `1/30` and the residual is carried forward rather than
    // dropped - that is the same float behaviour the client has.
    let mut positions = Vec::new();
    for fps in [250.0_f64, 120.0, 60.0] {
        let mut w = flat_world();
        let h = spawn(&mut w, 1, 100.0, 100.0, 20.0);
        walk(&mut w, h, Vec3::new(0.15, 0.0, 0.0), 400);
        let ticks = run(&mut w, 2.0, fps);
        assert!(
            ticks <= 61,
            "{fps} fps produced {ticks} physics ticks in 2 s; the gate is 30 Hz"
        );
        assert!(ticks >= 40, "{fps} fps produced only {ticks} ticks");
        positions.push((fps, ticks, w.get(h).expect("live").position.frame.origin.x));
    }
    // Each tick advances the walk by one 0.15 m sub-step, so the distance travelled tracks the
    // tick count and not the frame rate.
    for (fps, ticks, x) in &positions {
        #[allow(clippy::cast_precision_loss)]
        let expect = 100.0 + (*ticks as f32 - 1.0) * 0.15;
        assert!(
            (x - expect).abs() < 0.2,
            "{fps} fps: {ticks} ticks should have walked to about {expect}, reached {x}"
        );
    }
}

/// A static obstacle is detected and reported as an environment collision.
#[test]
fn a_static_obstacle_is_detected_and_reported_as_an_environment_collision() {
    let mut w = flat_world();
    let mover = spawn(&mut w, 1, 100.0, 100.0, 20.0);

    // A static sphere 2 m to the east, registered as a shadow in the same cell.
    let blocker = w.create(ObjectId(2), player_geometry(), false);
    let cell = w.get(mover).expect("live").position.cell;
    w.enter_cell(blocker, cell);
    {
        let o = w.get_mut(blocker).expect("live");
        o.position = Position::new(
            cell,
            Frame::new(Vec3::new(102.0, 100.0, 20.0), Quat::IDENTITY),
        );
        assert!(o.state.is_static());
    }
    w.calc_cross_cells(blocker, false);

    walk(&mut w, mover, Vec3::new(0.15, 0.0, 0.0), 200);

    let mut saw_env_collision = false;
    let mut saw_blocked_substep = false;
    let mut t = 0.0;
    while t < 2.0 {
        t += 0.05;
        w.use_time(LocalTime(t), false);
        for n in w.drain_notices() {
            if matches!(
                n,
                dereth_physics::PhysicsNotice::EnvironmentCollision { .. }
            ) {
                saw_env_collision = true;
            }
            assert!(
                !matches!(n, dereth_physics::PhysicsNotice::ObjectCollision { .. }),
                "a STATIC_PS obstacle must report as environment, not as an object"
            );
        }
        let o = w.get(mover).expect("live");
        if o.position.frame.origin.x > 100.9 && o.velocity().x.abs() < 0.01 {
            saw_blocked_substep = true;
        }
    }
    assert!(
        saw_env_collision,
        "the static obstacle must raise an environment collision"
    );
    assert!(
        saw_blocked_substep,
        "a blocked sub-step must report zero achieved velocity"
    );
}

/// A missile that reaches something stops being a missile.
#[test]
fn a_missile_that_reaches_something_stops_being_a_missile() {
    /// `0x28B48`, one of the three state words the locked corpus's velocity-carrying creates
    /// actually carry: `INELASTIC | SCRIPTED_COLLISION | LIGHTING_ON | PATHCLIPPED | ALIGNPATH |
    /// MISSILE | REPORT_COLLISIONS`.
    const BOLT: u32 = 0x0002_8B48;

    let run = |state: u32| -> u32 {
        let mut w = flat_world();
        let mover = spawn(&mut w, 1, 100.0, 100.0, 20.0);
        let blocker = w.create(ObjectId(2), player_geometry(), false);
        let cell = w.get(mover).expect("live").position.cell;
        w.enter_cell(blocker, cell);
        {
            let o = w.get_mut(blocker).expect("live");
            o.position = Position::new(
                cell,
                Frame::new(Vec3::new(102.0, 100.0, 20.0), Quat::IDENTITY),
            );
        }
        w.calc_cross_cells(blocker, false);
        w.get_mut(mover).expect("live").state = dereth_physics::PhysicsState(state);

        walk(&mut w, mover, Vec3::new(0.15, 0.0, 0.0), 200);
        let mut t = 0.0;
        while t < 2.0 {
            t += 0.05;
            w.use_time(LocalTime(t), false);
            let _ = w.drain_notices().count();
        }
        w.get(mover).expect("live").state.0
    };

    assert_eq!(
        run(BOLT),
        BOLT & dereth_physics::PhysicsState::MISSILE_CLEAR_MASK,
        "a missile that reached the obstacle keeps every bit except MISSILE | ALIGNPATH | \
         PATHCLIPPED -- the client clears exactly those three bits together"
    );
    let plain = BOLT & !dereth_physics::PhysicsState::MISSILE_PS;
    assert_eq!(
        run(plain),
        plain,
        "and the control: the same walk into the same obstacle by a body that is not a missile \
         leaves the whole word alone. The sampled missile bit (`0x40`) \
         is the gate, and a build that cleared unconditionally would pass the \
         assertion above and fail this one"
    );
}

#[test]
fn shadow_registration_covers_every_cell_the_object_overlaps() {
    let mut w = flat_world();
    // Straddling the boundary between cells (4, 4) and (5, 4): x = 120 is the seam.
    let h = spawn(&mut w, 1, 119.8, 108.0, 20.0);
    let shadows = w.get(h).expect("live").shadow_objects.len();
    assert!(
        shadows >= 2,
        "a straddling object registers in both cells, got {shadows}"
    );
    assert!(w.get(h).expect("live").is_completely_visible());
}

#[test]
fn destroying_an_object_unregisters_it_from_every_cell() {
    let mut w = flat_world();
    let h = spawn(&mut w, 1, 100.0, 100.0, 20.0);
    let id = w.get(h).expect("live").id;
    assert_eq!(w.by_object_id(id), Some(h));
    w.destroy(h);
    assert!(w.get(h).is_none());
    assert_eq!(w.by_object_id(id), None);
    // and a tick afterwards does not panic on the stale table entry
    let _ = run(&mut w, 0.5, 30.0);
}

#[test]
fn destroying_many_objects_at_once_leaves_what_destroying_each_leaves() {
    let spread = |w: &mut PhysicsWorld| -> Vec<dereth_physics::PhysHandle> {
        // Some straddle a cell seam (x = 120), so they are registered in two cells.
        (0..6u32)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let x = 110.0 + 2.0 * i as f32;
                spawn(w, 10 + i, x, 108.0, 20.0)
            })
            .collect()
    };
    let gauges = |w: &PhysicsWorld| {
        (
            w.body_count(),
            w.object_table_count(),
            w.cell_object_count(),
            w.cell_shadow_count(),
        )
    };
    let (mut each, mut all) = (flat_world(), flat_world());
    let (he, ha) = (spread(&mut each), spread(&mut all));
    let keep_each = spawn(&mut each, 99, 60.0, 60.0, 20.0);
    let keep_all = spawn(&mut all, 99, 60.0, 60.0, 20.0);
    assert!(
        gauges(&all).3 > 6,
        "the seam objects are listed in two cells each"
    );
    for h in &he {
        each.destroy(*h);
    }
    all.destroy_all(&ha);
    assert_eq!(
        gauges(&all),
        gauges(&each),
        "bodies, table, cell lists and registrations"
    );
    assert_eq!(gauges(&all), (1, 1, 1, 1), "only the kept body is left");
    assert!(ha.iter().all(|h| all.get(*h).is_none()));
    assert!(each.get(keep_each).is_some() && all.get(keep_all).is_some());
}

#[test]
fn the_transition_pool_never_goes_deeper_than_one_during_ordinary_walking() {
    let mut w = flat_world();
    let h = spawn(&mut w, 1, 100.0, 100.0, 25.0);
    walk(&mut w, h, Vec3::new(0.1, 0.05, 0.0), 200);
    let _ = run(&mut w, 4.0, 30.0);
    assert_eq!(
        w.transition_high_water(),
        1,
        "a plain walk uses one transition at a time; deeper means something re-entered"
    );
}

use crate::common::physics_fixture::{flat_world, player_geometry};
