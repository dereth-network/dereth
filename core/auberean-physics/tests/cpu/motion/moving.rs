//! A body on level ground, moved tick by tick.
//! Fixture: a flat 160 m block at height 10; the player's ground and air motion values as the
//! September 2013 data gives them (run 6.5, back 4, sideways 4.5 m/s; turns 90 and 120 degrees a
//! second; accelerations 15; slowdowns 18, 25, 25; jump 6.26).

use auberean_assets::animation::MotionParams;
use auberean_physics::body::{Body, BodyConfig, Ticker, TICK};
use auberean_physics::land::{land_cell_id, Block, Land};
use auberean_physics::motion::MotionInput;
use dereth_physics::geom::Sphere;
use dereth_physics::V3;
use dereth_primitives::{Frame, Position, Quat, Vec3};

fn ground() -> MotionParams {
    MotionParams {
        terminal_speed: 50.0,
        gravity: 1,
        jump_impulse: 6.26,
        max_forward: 6.5,
        max_backward: 4.0,
        max_strafe: 4.5,
        still_turn_rate: 90.0,
        running_turn_rate: 120.0,
        forward_slowdown: 18.0,
        backward_slowdown: 25.0,
        strafe_slowdown: 25.0,
        turn_slowdown: 1850.0,
        forward_acceleration: 15.0,
        backward_acceleration: 15.0,
        strafe_acceleration: 15.0,
        turn_acceleration: 1000.0,
        ..MotionParams::default()
    }
}

fn air() -> MotionParams {
    MotionParams {
        terminal_speed: 50.0,
        gravity: 1,
        max_forward: 6.5,
        max_backward: 4.0,
        max_strafe: 4.5,
        still_turn_rate: 120.0,
        running_turn_rate: 120.0,
        ..MotionParams::default()
    }
}

const GROUND_Z: f32 = 10.0;

fn world() -> Land {
    let land = Land::new();
    land.insert(Block::flat(0x40, 0x40, GROUND_Z));
    land
}

fn body() -> Body {
    let config = BodyConfig {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.48), 0.48)],
        step_up: 0.6,
        step_down: 0.6,
        scale: 1.0,
        ground: ground(),
        air: air(),
    };
    let at = Position::new(
        land_cell_id(0x40, 0x40, 3, 3),
        Frame::new(Vec3::new(70.0, 70.0, GROUND_Z), Quat::IDENTITY),
    );
    Body::new(config, at, 0.0)
}

fn dt() -> f32 {
    auberean_physics::body::tick_seconds()
}

/// The distance moved in each of `n` ticks.
fn steps(b: &mut Body, land: &Land, n: usize) -> Vec<f32> {
    (0..n)
        .map(|_| {
            let before = b.position.frame.origin;
            b.tick(land, dt());
            let a = b.position.frame.origin;
            ((a.x - before.x).powi(2) + (a.y - before.y).powi(2)).sqrt()
        })
        .collect()
}

fn close(got: &[f32], want: &[f32]) -> bool {
    got.len() == want.len() && got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-3)
}

/// A body starting to run from rest moves nothing on the first tick (only its speed rises), then
/// 0.225, 0.375, 0.525 and 0.625 m, reaching full speed on the fifth tick; let go, it stops in three
/// ticks covering 0.525, 0.275 and 0.075 m.
///
/// Behaviour: motion.run.a-run-starts-and-stops-tick-by-tick
#[test]
fn a_run_starts_and_stops_tick_by_tick() {
    let land = world();
    let mut b = body();
    b.steer(MotionInput {
        forward: 1.0,
        ..MotionInput::default()
    });
    let start = steps(&mut b, &land, 6);
    assert!(
        close(&start, &[0.0, 0.225, 0.375, 0.525, 0.625, 0.65]),
        "{start:?}"
    );
    assert!(
        (b.local_velocity().y - 6.5).abs() < 1e-4,
        "{:?}",
        b.local_velocity()
    );
    b.steer(MotionInput::default());
    let stop = steps(&mut b, &land, 4);
    assert!(close(&stop, &[0.525, 0.275, 0.075, 0.0]), "{stop:?}");
    assert!(
        (b.position.frame.origin.z - GROUND_Z).abs() < 1e-3,
        "stays on the ground"
    );
}

/// Forward with a sideways step is no faster than forward: the request is cut to length 1, and
/// past 0.7 forward the sideways part runs at the forward speed.
///
/// Behaviour: motion.run.diagonals-are-not-faster
#[test]
fn diagonals_are_not_faster() {
    let land = world();
    let mut b = body();
    b.steer(MotionInput {
        forward: 1.0,
        sideways: 1.0,
        ..MotionInput::default()
    });
    let _ = steps(&mut b, &land, 12);
    let v = b.local_velocity();
    let speed = (v.x * v.x + v.y * v.y).sqrt();
    assert!((speed - 6.5).abs() < 1e-3, "{v:?}");
    assert!((v.x - v.y).abs() < 1e-3, "an even diagonal: {v:?}");
}

/// Backing up runs at 4 m/s and sidestepping at 4.5.
///
/// Behaviour: motion.run.backward-and-sideways-speeds
#[test]
fn backward_and_sideways_speeds() {
    let land = world();
    for (input, want) in [
        (
            MotionInput {
                forward: -1.0,
                ..MotionInput::default()
            },
            Vec3::new(0.0, -4.0, 0.0),
        ),
        (
            MotionInput {
                sideways: 1.0,
                ..MotionInput::default()
            },
            Vec3::new(4.5, 0.0, 0.0),
        ),
    ] {
        let mut b = body();
        b.steer(input);
        let _ = steps(&mut b, &land, 10);
        let v = b.local_velocity();
        assert!(
            (v.x - want.x).abs() < 1e-3 && (v.y - want.y).abs() < 1e-3,
            "{v:?} want {want:?}"
        );
    }
}

/// Holding a turn key standing still turns the body at 90 degrees a second once the turn is up to
/// speed, the heading measured clockwise from north (a left turn lowers it).
///
/// Behaviour: motion.turn.standing-turns-at-the-still-rate
#[test]
fn standing_turns_at_the_still_rate() {
    let land = world();
    let mut b = body();
    let mut headings = Vec::new();
    for _ in 0..20 {
        b.steer(MotionInput {
            turn: 1.0,
            ..MotionInput::default()
        });
        b.tick(&land, dt());
        headings.push(b.heading);
    }
    let per_tick = (headings[18] - headings[19]).rem_euclid(360.0);
    assert!(
        (per_tick - 9.0).abs() < 1e-3,
        "{per_tick} degrees a tick, {headings:?}"
    );
    assert!((b.turn_rate - 90.0).abs() < 1e-3);
}

/// A jump stops the behaviors a move cancels: leaving the ground is a move. A jump the body may
/// not make stops nothing.
///
/// Behaviour: motion.jump.a-jump-stops-the-behaviors-a-move-cancels
#[test]
fn a_jump_stops_the_behaviors_a_move_cancels() {
    let stops =
        auberean_physics::body::Notice::Animate(auberean_physics::body::Animate::StopMoveToCancels);
    let land = world();
    let mut b = body();
    for _ in 0..5 {
        b.tick(&land, dt());
    }
    b.notices.clear();
    assert!(b.jump());
    assert!(b.notices.contains(&stops));
    b.notices.clear();
    assert!(!b.jump(), "already off the ground");
    assert!(!b.notices.contains(&stops));
}

/// A jump leaves the ground at 6.26 m/s and follows `6.26 t - 4.9 t^2` at each tick until it lands
/// back on the ground; the body cannot jump again until it has stood half a second.
///
/// Behaviour: motion.jump.a-jump-follows-its-arc-and-waits-to-repeat
#[test]
fn a_jump_follows_its_arc_and_waits_to_repeat() {
    let land = world();
    let mut b = body();
    for _ in 0..5 {
        b.tick(&land, dt());
    }
    assert!(b.jump(), "standing half a second, it jumps");
    let mut landed = None;
    for k in 1..=20 {
        b.tick(&land, dt());
        #[allow(clippy::cast_precision_loss)]
        let t = k as f32 * dt();
        let arc = (6.26 * t - 4.9 * t * t).max(0.0);
        if b.contact {
            landed = Some(k);
            break;
        }
        let z = b.position.frame.origin.z - GROUND_Z;
        assert!((z - arc).abs() < 1e-3, "tick {k}: {z} against {arc}");
    }
    let k = landed.expect("it lands");
    assert_eq!(k, 13, "back on the ground on the thirteenth tick");
    assert!(!b.jump(), "not straight away");
    for _ in 0..5 {
        b.tick(&land, dt());
    }
    assert!(b.jump(), "after half a second on the ground");
}

/// Physics ticks in whole tenths of a second: a frame less than a tenth after the last tick runs
/// none, a frame two and a half tenths after runs two, and a backlog of more than a second runs
/// none and is dropped.
///
/// Behaviour: motion.tick.physics-runs-in-tenths-of-a-second
#[test]
fn physics_runs_in_tenths_of_a_second() {
    let mut t = Ticker::new(100.0);
    assert_eq!(t.ticks(100.05), 0);
    assert_eq!(t.ticks(100.25), 2);
    assert!((t.reached - 100.2).abs() < 1e-9);
    assert_eq!(t.ticks(101.5), 0, "a backlog over a second is dropped");
    assert!((t.reached - 101.5).abs() < 1e-9);
    assert_eq!(t.ticks(101.5 + 1.5 * TICK), 1);
}

/// Animation steps once a frame at least a thirtieth of a second after the last, by all the time
/// since up to a fifth of a second; after a longer gap it steps a twentieth of a second only.
///
/// Behaviour: motion.tick.animation-steps-by-the-frames-time
#[test]
fn animation_steps_by_the_frames_time() {
    let mut c = auberean_physics::body::AnimationClock::new(0.0);
    assert_eq!(c.step(0.02), None);
    assert!((c.step(0.05).unwrap() - 0.05).abs() < 1e-6);
    assert!((c.step(0.25).unwrap() - 0.2).abs() < 1e-6, "a fifth whole");
    assert_eq!(c.step(1.0), Some(0.05), "after a longer gap, a twentieth");
    assert_eq!(c.step(0.5), None, "time running back restarts the clock");
    assert_eq!(c.step(0.52), None);
    assert!(c.step(0.534).is_some());
}

/// Between ticks a body is drawn between where it was before its last tick and where it is, by how
/// far the frame is past that tick: at the tick itself where it was, a tick later where it is, half
/// way between half way along; its heading turns the short way. Put somewhere at once, it is drawn
/// there.
///
/// Behaviour: motion.draw.a-body-is-drawn-between-its-last-two-ticks
#[test]
fn a_body_is_drawn_between_its_last_two_ticks() {
    let land = world();
    let mut b = body();
    let mut ticker = Ticker::new(0.0);
    // Run and turn long enough to be moving and turning, frames of 17.5 ms, then stop at a frame
    // 0.04 s past a tick.
    for frame in 1..=40 {
        for _ in 0..ticker.ticks(f64::from(frame) * 0.0175 + 0.005) {
            b.steer(MotionInput {
                forward: 1.0,
                turn: 1.0,
                ..MotionInput::default()
            });
            b.tick(&land, dt());
        }
    }
    let now = ticker.reached + 0.04;
    let alpha = ticker.alpha(now);
    assert!((alpha - 0.4).abs() < 1e-4, "0.04 s past the tick: {alpha}");
    assert!(
        (ticker.alpha(ticker.reached + 0.5) - 1.0).abs() < 1e-6,
        "never past where it is"
    );
    let (before, was) = b.previous;
    let here = b.position.frame.origin;
    let moved = here.sub(before.frame.origin);
    assert!(moved.mag2() > 0.01, "it moved on the last tick");
    let (at0, h0) = b.drawn(0.0);
    let (at1, h1) = b.drawn(1.0);
    let (mid, hm) = b.drawn(alpha);
    assert!(
        at0.frame.origin.sub(before.frame.origin).mag2() < 1e-8,
        "{at0:?}"
    );
    assert!(
        (h0 - was).abs() < 1e-4 && (h1 - b.heading).abs() < 1e-4,
        "{h0} {h1}"
    );
    assert!(at1.frame.origin.sub(here).mag2() < 1e-8, "{at1:?}");
    let want = before.frame.origin.add(moved.mul(alpha));
    assert!(mid.frame.origin.sub(want).mag2() < 1e-8, "{mid:?} {want:?}");
    let turn = (b.heading - was + 540.0).rem_euclid(360.0) - 180.0;
    assert!(turn.abs() > 1.0, "it turned on the last tick: {turn}");
    assert!(
        ((hm - (was + turn * alpha)).rem_euclid(360.0)).abs() < 1e-3,
        "{hm}"
    );
    // Across north: from 359 to 1 degree is two degrees, not 358.
    b.previous.1 = 359.0;
    b.heading = 1.0;
    let (_, h) = b.drawn(0.5);
    assert!(
        h.abs() < 1e-3 || (h - 360.0).abs() < 1e-3,
        "the short way: {h}"
    );
    // Put somewhere: drawn there whatever the share of a tick.
    let there = Position::new(
        b.position.cell,
        Frame::new(Vec3::new(20.0, 30.0, GROUND_Z), Quat::IDENTITY),
    );
    b.place(there, 90.0);
    let (p, h) = b.drawn(0.3);
    assert!(p.frame.origin.sub(there.frame.origin).mag2() < 1e-8 && (h - 90.0).abs() < 1e-4);
}

/// A body drawn between ticks keeps its frame's whole rotation: one pitched along its path, as a
/// missile is, stays pitched rather than being stood level at its heading.
///
/// Behaviour: motion.draw.a-body-drawn-between-ticks-keeps-its-whole-rotation
#[test]
fn a_body_drawn_between_ticks_keeps_its_whole_rotation() {
    let land = world();
    let mut b = body();
    for _ in 0..5 {
        b.steer(MotionInput {
            forward: 1.0,
            ..MotionInput::default()
        });
        b.tick(&land, dt());
    }
    let (s, c) = (
        dereth_primitives::num::math::sinf(0.2),
        dereth_primitives::num::math::cosf(0.2),
    );
    // Pitched about its right axis.
    let pitched = Quat::new(c, s, 0.0, 0.0);
    b.previous.0.frame.rotation = pitched;
    b.position.frame.rotation = pitched;
    assert!(b.is_movement_active(), "it is moving");
    let (at, _) = b.drawn(0.5);
    let q = at.frame.rotation;
    let same =
        (q.w - c).abs() < 1e-4 && (q.x - s).abs() < 1e-4 && q.y.abs() < 1e-4 && q.z.abs() < 1e-4;
    assert!(same, "still pitched: {q:?}");
}

/// A body that follows another asks it to be watched; the one followed tells it at once where it
/// is (its place, velocity, size), and stops when the follower lets go.
///
/// Behaviour: motion.target.a-followed-body-tells-its-follower-where-it-is
#[test]
fn a_followed_body_tells_its_follower_where_it_is() {
    use auberean_physics::body::Notice;
    let mut follower = body();
    follower.id = 0x10;
    let mut target = body();
    target.id = 0x20;
    target.position.frame.origin.x += 3.0;
    target.velocity = Vec3::new(1.0, 0.0, 0.0);

    follower.set_target(4, 0x20, 2.0, 0.5);
    assert_eq!(follower.target_id(), 0x20);
    let asked: Vec<Notice> = std::mem::take(&mut follower.notices);
    assert!(matches!(
        asked.as_slice(),
        [Notice::Watch {
            target: 0x20,
            watcher: 0x10,
            ..
        }]
    ));
    assert!(target.deliver(&asked[0]));
    assert!(!follower.deliver(&asked[0]), "not for the follower");

    let told: Vec<Notice> = std::mem::take(&mut target.notices);
    assert!(matches!(
        told.as_slice(),
        [Notice::Whereabouts { watcher: 0x10, .. }]
    ));
    assert!(follower.deliver(&told[0]));
    let info = follower.target_info().expect("following");
    assert_eq!(info.context, 4);
    assert_eq!(info.target_position, target.position);
    assert_eq!(info.velocity, Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(
        info.interpolated_position, target.position,
        "told at once: where it is"
    );
    assert!((info.object_radius - target.radius()).abs() < 1e-6);

    follower.clear_target();
    assert_eq!(follower.target_id(), 0);
    let gone: Vec<Notice> = std::mem::take(&mut follower.notices);
    assert!(target.deliver(&gone[0]));
    assert!(!target.remove_voyeur(0x10), "no longer watched");
}

/// An update's stamp is newer than the one kept when it is ahead by less than half the stamps'
/// range, counting round through 65536; a newer stamp is kept, an equal or older one refused.
///
/// Behaviour: motion.stamps.a-newer-stamp-is-kept-and-an-older-one-refused
#[test]
fn a_newer_stamp_is_kept_and_an_older_one_refused() {
    let mut b = body();
    assert!(b.is_newer_than_update(0, 5));
    assert_eq!(b.update_time(0), 5);
    assert!(!b.is_newer_than_update(0, 5), "equal");
    assert!(!b.is_newer_than_update(0, 4), "older");
    assert!(
        b.is_newer_than_update(0, 5 + 0x7FFF),
        "ahead by just under half"
    );
    assert!(
        b.is_newer_than_update(0, 4),
        "half the range behind wraps round to ahead"
    );
    b.update_times[1] = 0xFFF0;
    assert!(b.is_newer_than_update(1, 0x0003), "round through 65536");
    assert!(!b.is_newer_than_update(1, 0xFFF8), "now behind");
    b.inc_update_time(1);
    assert_eq!(b.update_time(1), 4);
    assert_eq!(b.update_time(2), 0, "each kind kept apart");
}

/// The place on the ground under a body is its place lowered by its height above what is walkable
/// under it; a body out of the world answers its own place.
///
/// Behaviour: motion.element.the-ground-under-a-body-is-its-height-above-walkable-below
#[test]
fn the_ground_under_a_body_is_its_height_above_walkable_below() {
    let land = world();
    let mut b = body();
    b.position.frame.origin.z = GROUND_Z + 3.0;
    b.contact = false;
    b.on_walkable = false;
    b.tick(&land, 0.1);
    let under = b.ground_position();
    assert!(
        (under.frame.origin.z - GROUND_Z).abs() < 0.2,
        "{:?}",
        under.frame.origin
    );
    b.in_world = false;
    assert_eq!(b.ground_position(), b.position);
}

/// A body's height is above what is under it, not above the land: over something solid, how far
/// down that stands; touching something, none; and under the land, how far up the land is, below
/// nothing.
///
/// Behaviour: motion.element.a-bodys-height-is-above-what-is-under-it
#[test]
fn a_bodys_height_is_above_what_is_under_it() {
    use auberean_physics::object::Shape;
    use auberean_physics::world::{World, WorldObject};
    use dereth_primitives::shape::CylSphere;
    use std::sync::Arc;
    let land = world();
    let height = |z: f32, stands_under: bool, contact: bool| {
        let mut b = body();
        b.position.frame.origin.z = z;
        b.contact = contact;
        b.on_walkable = contact;
        let mut table = World::new();
        if stands_under {
            let mut at = b.position;
            at.frame.origin.z = GROUND_Z;
            table.insert(
                &land,
                7,
                &WorldObject {
                    shape: Arc::new(Shape {
                        cylinders: vec![CylSphere {
                            low_pt: Vec3::ZERO,
                            radius: 3.0,
                            height: 5.0,
                        }],
                        ..(*crate::contact::objects::post()).clone()
                    }),
                    position: at,
                    is_static: true,
                    passable: false,
                },
            );
        }
        b.tick_in(&land, &mut table, 0.001);
        b.height_above_walkable()
    };
    let near = |a: f32, b: f32| (a - b).abs() < 0.05;
    let over_land = height(GROUND_Z + 8.0, false, false);
    assert!(near(over_land, 8.0), "over the land: {over_land}");
    let over_it = height(GROUND_Z + 8.0, true, false);
    assert!(near(over_it, 3.0), "over the object: {over_it}");
    assert_eq!(height(GROUND_Z + 8.0, false, true), 0.0, "touching");
    let under = height(GROUND_Z - 2.0, false, false);
    assert!(near(under, -2.0), "under the land: {under}");
}

/// A placement asks that a change of mode under way finish at once.
///
/// Behaviour: motion.element.a-placement-finishes-a-change-of-mode
#[test]
fn a_placement_finishes_a_change_of_mode() {
    use auberean_physics::body::{Animate, Notice};
    let mut b = body();
    let at = b.position;
    b.notices.clear();
    b.set_placement_position(at);
    assert!(b
        .notices
        .iter()
        .any(|n| matches!(n, Notice::Animate(Animate::SnapMode))));
}

/// A motion asked with a heading moves the body as asked and turns it to the heading (made a
/// 255th of a turn), its movement woken.
///
/// Behaviour: motion.turn.a-motion-with-a-heading-moves-and-turns-to-it
#[test]
fn a_motion_with_a_heading_moves_and_turns_to_it() {
    use auberean_physics::geom::Heading;
    let land = world();
    let mut b = body();
    b.set_movement_active(false);
    let start = b.position.frame.origin;
    b.do_motion(Vec3::new(0.0, 1.0, 0.0), Heading::new(90.0));
    assert!(b.is_movement_active());
    for _ in 0..30 {
        b.tick(&land, 0.1);
    }
    // The heading turned to falls to a 255th of a turn below it.
    let face = Heading::from_u8(Heading::new(90.0).to_u8()).degrees();
    assert!(
        (b.heading - face).abs() < 0.01,
        "faces the heading: {}",
        b.heading
    );
    let moved = b.position.frame.origin;
    assert!(moved.x - start.x > 5.0, "ran east: {moved:?}");
}

/// A teleport beginning drops the body's target and tells its watchers it teleported, stops it
/// following sent positions and moves its teleport stamp on; a server's leave-world with a newer
/// position stamp takes another's body out of the world, and an older one does nothing.
///
/// Behaviour: motion.target.a-teleport-tells-watchers-and-a-newer-leave-world-takes-a-body-out
#[test]
fn a_teleport_tells_watchers_and_a_newer_leave_world_takes_a_body_out() {
    use auberean_physics::body::missile::NoObjects;
    use auberean_physics::body::{Notice, TELEPORT_STAMP};
    use auberean_physics::target::status;
    use auberean_physics::world::World;
    let mut b = body();
    b.id = 0x20;
    b.set_target(0, 0x30, 1.0, 0.5);
    b.add_voyeur(0x10, 1.0, 0.5);
    b.notices.clear();
    b.handle_teleport_start();
    assert_eq!(b.target_id(), 0);
    assert_eq!(b.update_time(TELEPORT_STAMP), 1);
    assert!(b.notices.iter().any(|n| matches!(
        n,
        Notice::Whereabouts { watcher: 0x10, info } if info.status == status::TELEPORTED
    )));
    assert!(b
        .notices
        .iter()
        .any(|n| matches!(n, Notice::Unwatch { target: 0x30, .. })));

    let land = world();
    let mut w = World::new();
    let mut other = body();
    other.is_player = false;
    other.tick_in(&land, &mut w, 0.1);
    let held_at = other.position;
    other.minterp.constrain_to(0x21, false, held_at);
    let mut far = held_at;
    far.frame.origin.x += 100.0;
    assert_eq!(other.minterp.constraint_factor(&far), 0.0, "tethered");
    assert!(other.is_newer_than_update(0, 5));
    assert!(!other.receive_leave_world(&land, &mut w, &mut NoObjects, None, 4));
    assert!(other.in_world, "an older stamp does nothing");
    assert!(other.receive_leave_world(&land, &mut w, &mut NoObjects, None, 6));
    assert!(!other.in_world);
    assert!(other.notices.contains(&Notice::LeftWorld));
    assert_eq!(
        other.minterp.constraint_factor(&far),
        1.0,
        "leaving the world lets go of its tether"
    );
}

mod holding {
    //! Objects held by others: taken up and let go of, where each hangs as its holder is drawn, and
    //! its fade when the holder goes into water.
    //! Fixture: bodies standing on synthetic level ground, a made-up hanging point.

    use auberean_physics::body::missile::NoObjects;
    use auberean_physics::body::parent::{
        set_parent, ChildInfo, ChildKind, TakeUp, FADE_SLIDER, OWN_FRAME,
    };
    use auberean_physics::body::{heading_rotation, Animate, Body, Notice};
    use auberean_physics::motion::WATER;
    use auberean_physics::world::World;
    use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

    use crate::contact::objects::{body, land};

    const HOLDER: u64 = 0x10;
    const SWORD: u64 = 0x20;
    const HOLDER_MODE: u32 = 0x4000_0005;

    fn two() -> (Body, Body) {
        let mut holder = body(50.0, 50.0);
        holder.id = HOLDER;
        let mut sword = body(52.0, 50.0);
        sword.id = SWORD;
        sword.is_player = false;
        (holder, sword)
    }

    fn take_up(holder: &mut Body, sword: &mut Body, location: u32, holds: bool) -> bool {
        let land = land();
        let mut world = World::new();
        set_parent(
            sword,
            holder,
            TakeUp {
                location,
                holds,
                parent_mode: HOLDER_MODE,
                child_is_temporary: false,
            },
            &land,
            &mut world,
            &mut NoObjects,
        )
    }

    /// A holder takes up a child at its own frame, or at a holding location its setup has (not at
    /// one it lacks); the child leaves the world, goes to the holder's mode, fades in, and knows its
    /// holder. Nothing takes up itself or its own holder.
    ///
    /// Behaviour: motion.hold.a-holder-takes-up-a-child-which-leaves-the-world
    #[test]
    fn a_holder_takes_up_a_child_which_leaves_the_world() {
        let (mut holder, mut sword) = two();
        assert!(
            !take_up(&mut holder, &mut sword, 7, false),
            "no such location"
        );
        assert_eq!(holder.num_children(), 0);
        assert!(take_up(&mut holder, &mut sword, OWN_FRAME, false));
        assert_eq!(holder.num_children(), 1);
        assert_eq!(
            holder.child(0).map(|c| c.kind),
            Some(ChildKind::HoldingLocation)
        );
        assert_eq!(sword.parent, HOLDER);
        assert!(sword.has_parent);
        assert!(!sword.in_world);
        let asked = sword.take_requests();
        assert!(asked.contains(&Notice::Animate(Animate::Mode {
            context: 0,
            mode: HOLDER_MODE
        })));
        assert!(asked.contains(&Notice::Animate(Animate::Slider {
            context: 0,
            slider: FADE_SLIDER,
            value: 1.0,
            time: 0.5
        })));

        // The holder's own holder cannot be taken up by it.
        let (mut a, mut b) = two();
        a.parent = SWORD;
        assert!(!take_up(&mut a, &mut b, OWN_FRAME, true));
        assert_eq!(a.num_children(), 0);
    }

    /// A child let go of tells its holder, which takes it off its list, and goes back to the game's
    /// default mode; the server's word that it is let go of counts only with a newer position stamp,
    /// and then it also leaves the world.
    ///
    /// Behaviour: motion.hold.a-child-let-go-of-tells-its-holder
    #[test]
    fn a_child_let_go_of_tells_its_holder() {
        let (mut holder, mut sword) = two();
        assert!(take_up(&mut holder, &mut sword, OWN_FRAME, false));
        drop(sword.take_requests());
        sword.unset_parent();
        assert_eq!(sword.parent, 0);
        let asked = sword.take_requests();
        let to_holder: Vec<_> = asked
            .iter()
            .filter(|n| n.addressee() == Some(HOLDER))
            .collect();
        assert_eq!(to_holder.len(), 1);
        assert!(holder.deliver(to_holder[0]));
        assert_eq!(holder.num_children(), 0);
        assert!(asked.contains(&Notice::Animate(Animate::DefaultMode)));

        let land = land();
        let mut world = World::new();
        let (mut holder, mut sword) = two();
        assert!(take_up(&mut holder, &mut sword, OWN_FRAME, false));
        sword.update_times[0] = 5;
        assert!(!sword.receive_deparent(&land, &mut world, &mut NoObjects, 5));
        assert_eq!(sword.parent, HOLDER, "not newer");
        assert!(sword.receive_deparent(&land, &mut world, &mut NoObjects, 6));
        assert_eq!(sword.parent, 0);
    }

    fn holder_at() -> Position {
        // Facing east: its +y turned to +x.
        Position::new(
            CellId(0x4040_0001),
            Frame::new(Vec3::new(10.0, 20.0, 1.0), heading_rotation(90.0)),
        )
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4 && (a.z - b.z).abs() < 1e-4
    }

    /// A child hangs where its holder is drawn: the hanging point's place scaled by the holder's
    /// scale and turned with it, then the child's own offset; one that drops the hanging point's turn
    /// keeps only its place. With no hanging point it hangs at the holder's own frame.
    ///
    /// Behaviour: motion.hold.a-child-hangs-where-its-holder-is-drawn
    #[test]
    fn a_child_hangs_where_its_holder_is_drawn() {
        let drawn = holder_at();
        // A hanging point 1 m ahead (+y), turned a quarter about z.
        let hang = [
            [0.0, 1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0, 1.0],
        ];
        let info = ChildInfo {
            id: SWORD,
            kind: ChildKind::HoldingLocation,
            location: 1,
            offset: Frame::new(Vec3::new(0.0, 0.5, 0.0), Quat::IDENTITY),
            remove_rotation: false,
        };
        let scale = Vec3::new(2.0, 2.0, 2.0);
        let p = Body::child_place(&drawn, scale, &info, Some(&hang));
        assert_eq!(p.cell, drawn.cell);
        // 2 m ahead of the holder (east), then half a metre along the point's own +y, which the
        // point's quarter turn has made the holder's -x: north of the point.
        assert!(
            close(p.frame.origin, Vec3::new(12.0, 20.5, 1.0)),
            "{:?}",
            p.frame.origin
        );
        let kept = Body::child_place(
            &drawn,
            scale,
            &ChildInfo {
                remove_rotation: true,
                ..info
            },
            Some(&hang),
        );
        assert!(
            close(kept.frame.origin, Vec3::new(12.5, 20.0, 1.0)),
            "{:?}",
            kept.frame.origin
        );
        let own = Body::child_place(&drawn, scale, &info, None);
        assert!(close(own.frame.origin, Vec3::new(10.5, 20.0, 1.0)));
    }

    /// When the holder's element changes, a child in its first holding location (or hung at its
    /// second, whatever from) fades out headed into water and back in otherwise; others, temporary
    /// children, and a holder whose change was already drawn, do not fade.
    ///
    /// Behaviour: motion.hold.a-held-child-fades-in-water
    #[test]
    fn a_held_child_fades_in_water() {
        let (mut holder, mut sword) = two();
        assert!(take_up(&mut holder, &mut sword, OWN_FRAME, false));
        let first = ChildInfo {
            id: SWORD,
            kind: ChildKind::HoldingLocation,
            location: 1,
            offset: Frame::default(),
            remove_rotation: false,
        };
        assert_eq!(holder.child_fade(&first, false), None, "no change yet");
        holder.do_element(WATER);
        assert_eq!(holder.child_fade(&first, false), Some(0.0));
        assert_eq!(holder.child_fade(&first, true), None, "temporary");
        let second_bone = ChildInfo {
            kind: ChildKind::BoneName,
            location: 2,
            ..first
        };
        assert_eq!(holder.child_fade(&second_bone, false), Some(0.0));
        let first_bone = ChildInfo {
            kind: ChildKind::BoneName,
            ..first
        };
        assert_eq!(holder.child_fade(&first_bone, false), None);
        holder.children_drawn();
        assert_eq!(holder.child_fade(&first, false), None, "drawn already");
        holder.do_element(auberean_physics::motion::EARTH);
        assert_eq!(holder.child_fade(&first, false), Some(1.0));
    }
    /// A held object's whole scale takes its holder's: its look's scale times its own, times, while
    /// held and not flying as a missile, the holder's whole scale and the scale the holder's look
    /// gives what it holds.
    ///
    /// Behaviour: motion.hold.a-held-childs-scale-takes-its-holders
    #[test]
    fn a_held_childs_scale_takes_its_holders() {
        let (mut holder, mut sword) = two();
        sword.look_scale = Vec3::new(2.0, 2.0, 2.0);
        sword.object_scale = Vec3::new(1.0, 1.0, 0.5);
        holder.look_scale = Vec3::new(1.5, 1.5, 1.5);
        holder.child_scale = 0.5;
        let holders = (holder.total_scale(None), holder.child_scale);
        assert_eq!(
            sword.total_scale(Some(holders)),
            Vec3::new(2.0, 2.0, 1.0),
            "not held"
        );
        assert!(take_up(&mut holder, &mut sword, OWN_FRAME, true));
        assert_eq!(
            sword.total_scale(Some(holders)),
            Vec3::new(1.5, 1.5, 0.75),
            "held"
        );
    }

    /// A body sticks to another where it stands: held by the holder's bone nearest it, at its turn
    /// as seen from the holder (the place dropped), and out of the world; a holder with no bones
    /// cannot hold it so.
    ///
    /// Behaviour: motion.hold.a-body-sticks-to-the-bone-nearest-it
    #[test]
    fn a_body_sticks_to_the_bone_nearest_it() {
        use auberean_physics::body::parent::sticky_set_parent;
        let (mut holder, mut sword) = two();
        let land = land();
        let mut world = World::new();
        assert!(
            !sticky_set_parent(
                &mut sword,
                &mut holder,
                false,
                &land,
                &mut world,
                &mut NoObjects
            ),
            "no bones to stick to"
        );
        holder.set_parts(crate::contact::objects::body_parts());
        // The holder's bones stand 1 m up and 1 m up and 1 m north; the sword beside the second.
        sword.position.frame.origin = Vec3::new(50.0, 51.2, 11.0);
        assert!(sticky_set_parent(
            &mut sword,
            &mut holder,
            false,
            &land,
            &mut world,
            &mut NoObjects
        ));
        let info = holder.children.as_ref().expect("held").children[0];
        assert_eq!((info.kind, info.location), (ChildKind::BoneIndex, 1));
        assert_eq!(info.offset.origin, Vec3::ZERO);
        assert!(!sword.in_world && sword.has_parent && sword.parent == HOLDER);
    }
}

/// A position too far to follow is placed by the body's next step: in the air it touches nothing,
/// and it falls.
///
/// Behaviour: motion.element.a-position-too-far-to-follow-is-placed
#[test]
fn a_position_too_far_to_follow_is_placed() {
    let land = world();
    let mut b = body();
    b.tick(&land, 0.1);
    assert!(b.contact, "it stands");
    let mut far = b.position;
    far.frame.origin.y += 20.0;
    far.frame.origin.z = GROUND_Z + 3.0;
    b.interpolate_to(
        1.0,
        far,
        Vec3::ZERO,
        auberean_physics::geom::Heading::new(0.0),
        false,
    );
    b.tick(&land, 0.1);
    assert!(!b.contact, "it touches nothing");
    let z = b.position.frame.origin.z;
    assert!(z > GROUND_Z + 2.0, "up there: {z}");
    for _ in 0..30 {
        b.tick(&land, 0.1);
    }
    assert!(b.contact, "it landed");
}
