//! Set_omega does not wake a sleeping body; a body woken otherwise turns by omega x dt; the vector-
//! update pair still wakes and spins; the activity radius supplies activation.
//! Fixture: synthetic state, geometry and reference vectors.

use std::sync::Arc;

use dereth_physics::{math, PhysHandle, PhysicsWorld, SetupGeometry, StaticLandSource};
use dereth_primitives::{Frame, LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};

/// One flat block, ground at z = 20.
fn world() -> PhysicsWorld {
    let mut land = StaticLandSource::linear();
    land.add_flat_block(LandblockId::new(0xA9, 0xB4), 10);
    PhysicsWorld::new(Arc::new(land))
}

fn floating_object(w: &mut PhysicsWorld, id: u32) -> PhysHandle {
    let geometry = Arc::new(SetupGeometry {
        spheres: vec![dereth_physics::geom::Sphere::new(
            Vec3::new(0.0, 0.0, 0.5),
            0.5,
        )],
        radius: 0.5,
        height: 1.0,
        ..SetupGeometry::default()
    });
    let h = w.create(ObjectId(id), geometry, true);
    let cell = LandblockId::new(0xA9, 0xB4).cell(1);
    w.enter_cell(h, cell);
    let o = w.get_mut(h).expect("live");
    o.position = Position::new(
        cell,
        Frame::new(Vec3::new(12.0, 12.0, 100.0), Quat::IDENTITY),
    );
    o.state = dereth_physics::PhysicsState(o.state.0 & !0x0000_0400);
    o.calc_acceleration();
    o.update_time = 0.0;
    assert!(
        !o.state.has_gravity(),
        "the fixture is weightless by construction, not by omission"
    );
    assert!(
        !o.state.is_static(),
        "a static body would refuse set_active for the wrong reason"
    );
    assert!(
        !o.transient_state.is_active(),
        "the fixture must start asleep"
    );
    assert!(!o.transient_state.on_walkable(), "...and off the ground");
    h
}

/// One second of simulated time at 30 Hz, which is exactly the rate the physics time gate
/// passes.
fn run_one_second(w: &mut PhysicsWorld) {
    let mut t = 0.0;
    for _ in 0..30 {
        t += 1.0 / 30.0;
        w.use_time(LocalTime(t), false);
    }
}

/// `PI` rad/s about z: a half turn per simulated second, which is impossible to mistake for noise.
fn half_turn_per_second() -> Vec3 {
    Vec3::new(0.0, 0.0, std::f32::consts::PI)
}

/// **The defect.** `set_omega` on a sleeping body must leave it asleep — and the observable is that
/// the body **does not move**, not that a bit is clear.
///
/// The velocity is written straight into `velocity_vector` rather than through `set_velocity @
// because `set_velocity` activates and would hand the subject its alibi. So the body is
/// Behaviour: physics.motion.setting-spin-does-not-wake-a-sleeping-body
/// loaded with 5 m/s of upward velocity and 180 deg/s of spin and asked to simulate a second:
/// retail's `set_omega` stores twelve bytes and nothing happens.
#[test]
fn set_omega_does_not_wake_a_sleeping_body() {
    let mut w = world();
    let h = floating_object(&mut w, 1);
    w.get_mut(h).expect("live").velocity_vector = Vec3::new(0.0, 0.0, 5.0);
    let before = w.get(h).expect("live").position.frame;
    let update_time_before = w.get(h).expect("live").update_time;

    w.get_mut(h)
        .expect("live")
        .set_omega(half_turn_per_second());

    assert_eq!(
        w.get(h).expect("live").omega_vector,
        half_turn_per_second(),
        "the three omega floats are the only thing the setter writes"
    );
    assert_eq!(
        w.get(h).expect("live").update_time,
        update_time_before,
        "and `update_time` is NOT one of them -- only set_active writes it"
    );

    run_one_second(&mut w);

    let after = w.get(h).expect("live").position.frame;
    assert_eq!(
        after.origin, before.origin,
        "a sleeping body does not translate: is wholly inside \
         `if (ACTIVE_TS)`, and 5 m/s for a simulated second would have lifted it 5 m"
    );
    assert_eq!(
        after.rotation, before.rotation,
        "and does not turn either: `grotate(frame, omega_vector * q)` is past the same gate"
    );
    assert!(
        !w.get(h).expect("live").transient_state.is_active(),
        "asserted last and on purpose -- the positions above are the claim, this is corroboration"
    );
}

/// The positive control, so that "make `set_omega` a no-op" is not a way to pass the test above: a
/// body something **else** woke spins by `omega * t`, and the frame is read at three points across
/// the second so a single end-state coincidence cannot carry it.
#[test]
fn a_body_woken_by_anything_else_turns_by_omega_times_elapsed_time() {
    let mut w = world();
    let h = floating_object(&mut w, 1);
    // Activate the body directly, as an independent source of the activation retail has,
    // arriving from somewhere other than `set_omega`.
    w.get_mut(h).expect("live").set_active(true, 0.0);
    w.get_mut(h)
        .expect("live")
        .set_omega(half_turn_per_second());

    let heading = |w: &PhysicsWorld| math::get_heading(&w.get(h).expect("live").position.frame);
    let h0 = heading(&w);

    let mut t = 0.0;
    let mut samples = Vec::new();
    for i in 0..30 {
        t += 1.0 / 30.0;
        w.use_time(LocalTime(t), false);
        if i == 9 || i == 19 || i == 29 {
            samples.push(heading(&w));
        }
    }

    // Unsigned turn, with the one wrap a half turn can cross unwrapped; the sign of
    //  against a positive omega is not the subject.
    let turned = |deg: f32| {
        let d = (h0 - deg).rem_euclid(360.0);
        if d > 180.0 {
            360.0 - d
        } else {
            d
        }
    };
    let (a, b, c) = (turned(samples[0]), turned(samples[1]), turned(samples[2]));
    assert!(
        a > 1.0,
        "after a third of a second the body has turned, not sat still: {a} deg"
    );
    assert!(
        b > a + 1.0,
        "and the turn accumulates with simulated time: {a} -> {b} deg"
    );
    assert!(c > b + 1.0, "and keeps accumulating: {b} -> {c} deg");
    assert!(
        (c - 180.0).abs() < 10.0,
        "PI rad/s for one simulated second is a half turn, less whatever the 30 Hz gate's dropped \
         remainder costs: {c} deg"
    );
}

/// The vector update pair still wakes the body and still spins it.
#[test]
fn the_vector_update_pair_still_wakes_the_body_and_still_spins_it() {
    let mut w = world();
    let h = floating_object(&mut w, 1);
    let before = w.get(h).expect("live").position.frame;

    // Set linear velocity, then angular velocity, with no condition between the calls.
    w.get_mut(h)
        .expect("live")
        .set_velocity(Vec3::new(0.0, 0.0, 5.0), 0.0);
    w.get_mut(h)
        .expect("live")
        .set_omega(half_turn_per_second());
    assert!(
        w.get(h).expect("live").transient_state.is_active(),
        "'s inlined `set_active` is the activation this path relies on"
    );

    run_one_second(&mut w);

    let after = w.get(h).expect("live").position.frame;
    assert!(
        after.origin.z > before.origin.z + 1.0,
        "the velocity limb still moves the body: {} -> {}",
        before.origin.z,
        after.origin.z
    );
    assert_ne!(
        after.rotation, before.rotation,
        "and the omega limb still turns it"
    );
    let turned = (math::get_heading(&before) - math::get_heading(&after)).rem_euclid(360.0);
    assert!(
        turned > 90.0,
        "by most of a half turn, not a rounding error: {turned} deg"
    );
}

/// The activity radius supplies the activation set omega must not.
#[test]
fn the_activity_radius_supplies_the_activation_set_omega_must_not() {
    let mut w = world();
    let subject = floating_object(&mut w, 1);
    let player = floating_object(&mut w, 2);
    w.set_player(player);

    w.get_mut(subject)
        .expect("live")
        .set_omega(half_turn_per_second());
    assert!(
        !w.get(subject).expect("live").transient_state.is_active(),
        "set_omega itself left it asleep"
    );

    w.use_time(LocalTime(1.0 / 30.0), false);

    assert!(
        w.get(subject).expect("live").player_distance < 96.0,
        "the two bodies are in the same place"
    );
    assert!(
        w.get(subject).expect("live").transient_state.is_active(),
        "and the sweep woke it anyway: this is why the other tests set no player"
    );
}
