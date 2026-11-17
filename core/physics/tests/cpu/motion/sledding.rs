//! Which way a sledding body faces: along its velocity's horizontal part, and north while it has
//! none, as when a body set sledding where it stands on a slope only falls.
//! Fixture: one landblock rising 4 m per 24 m to the east (about 9.5 degrees), a one-sphere body.

use std::sync::Arc;

use dereth_physics::{math, PhysHandle, PhysicsWorld, ScriptedMotion, StaticLandSource};
use dereth_primitives::{Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

use crate::common::physics_fixture::player_geometry;

const STEP: f64 = 1.0 / 30.0;

/// One block whose ground rises 2 height steps (4 m) per 24 m column to the east, so downhill is
/// west.
fn slope_world() -> PhysicsWorld {
    let mut land = StaticLandSource::linear();
    land.add_ramp_block(LandblockId::new(0xA9, 0xB4), 10, 2);
    PhysicsWorld::new(Arc::new(land))
}

/// A body facing `heading` degrees, dropped onto the middle of the slope and left to settle.
fn standing_body(w: &mut PhysicsWorld, heading: f32) -> (PhysHandle, f64) {
    let h = w.create(ObjectId(1), player_geometry(), true);
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    w.enter_cell(h, cell);
    {
        let o = w.get_mut(h).expect("live");
        // The ground at x = 100 is 20 + 100 / 6 m high; start a little above it.
        o.position = Position::new(
            cell,
            Frame::new(Vec3::new(100.0, 100.0, 37.0), Quat::IDENTITY),
        );
        o.set_heading(heading);
        o.set_motion(Box::new(ScriptedMotion::new(Vec::new(), true)));
        o.transient_state.set_active_bit(true);
        o.calc_acceleration();
        o.update_time = 0.0;
    }
    w.calc_cross_cells(h, false);
    let t = run(w, 0.0, 60);
    let o = w.get(h).expect("live");
    assert!(
        o.transient_state.in_contact() && o.transient_state.on_walkable(),
        "the body stands on the slope: {:?}",
        o.position
    );
    (h, t)
}

/// `steps` physics steps from time `t`; answers the time reached.
fn run(w: &mut PhysicsWorld, mut t: f64, steps: u32) -> f64 {
    for _ in 0..steps {
        t += STEP;
        w.use_time(LocalTime(t), false);
    }
    t
}

fn heading(w: &PhysicsWorld, h: PhysHandle) -> f32 {
    math::get_heading(&w.get(h).expect("live").position.frame)
}

/// A body standing on a slope and set sledding falls straight down and creeps down the slope,
/// with nothing horizontal in its velocity, and faces north while it does, whichever way it
/// faced before. Given a push, it faces the way the push sends it.
///
/// Behaviour: physics.sledding.a-body-set-sledding-where-it-stands-faces-north
#[test]
fn a_body_set_sledding_where_it_stands_on_a_slope_faces_north_until_pushed() {
    let mut w = slope_world();
    let (h, t) = standing_body(&mut w, 180.0);
    assert!(
        (heading(&w, h) - 180.0).abs() < 1e-3,
        "it starts facing south"
    );
    let x0 = w.get(h).expect("live").position.frame.origin.x;
    {
        let o = w.get_mut(h).expect("live");
        o.state.set_sledding(true);
        o.calc_acceleration();
    }

    let t = run(&mut w, t, 30);
    let o = w.get(h).expect("live");
    let v = o.velocity_vector;
    assert!(
        v.x.abs() < 2e-4 && v.y.abs() < 2e-4 && v.z < 0.0,
        "a body sledding from a standstill only falls: {v:?}"
    );
    assert!(
        o.position.frame.origin.x < x0,
        "and creeps down the slope, west: {:?}",
        o.position.frame.origin
    );
    let standing = heading(&w, h);
    assert!(
        standing.abs() < 1e-3 || (standing - 360.0).abs() < 1e-3,
        "with no horizontal velocity it faces north, not {standing}"
    );

    w.get_mut(h)
        .expect("live")
        .set_velocity(Vec3::new(-3.0, 0.0, 0.0), t);
    let _ = run(&mut w, t, 3);
    let pushed = heading(&w, h);
    assert!(
        (pushed - 270.0).abs() < 1e-2,
        "pushed down the slope it faces west, not {pushed}"
    );
}
