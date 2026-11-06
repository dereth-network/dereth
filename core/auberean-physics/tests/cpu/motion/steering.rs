//! Bodies steered rather than driven: a move to a place the server ordered, a turn to a heading,
//! and a body following the positions the server sends for it.
//! Fixture: a flat 160 m block at height 10; the player's ground motion values (run 6.5 m/s,
//! turns 90 and 120 degrees a second, accelerations 15, turn acceleration 1000).

use auberean_assets::animation::MotionParams;
use auberean_physics::body::{Body, BodyConfig};
use auberean_physics::geom::{self, Heading};
use auberean_physics::land::{land_cell_id, Block, Land};
use auberean_physics::motion::params::MovementParameters;
use auberean_physics::motion::values::default_values;
use dereth_physics::geom::Sphere;
use dereth_primitives::{Frame, Position, Quat, Vec3};

const GROUND_Z: f32 = 10.0;

fn ground() -> MotionParams {
    MotionParams {
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
        ..default_values()
    }
}

fn world() -> Land {
    let land = Land::new();
    land.insert(Block::flat(0x40, 0x40, GROUND_Z));
    land
}

fn at(x: f32, y: f32) -> Position {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cell = land_cell_id(0x40, 0x40, (x / 20.0) as usize, (y / 20.0) as usize);
    Position::new(cell, Frame::new(Vec3::new(x, y, GROUND_Z), Quat::IDENTITY))
}

fn body(heading: f32) -> Body {
    let config = BodyConfig {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.48), 0.48)],
        step_up: 0.6,
        step_down: 0.6,
        scale: 1.0,
        ground: ground(),
        air: MotionParams {
            gravity: 1,
            ..default_values()
        },
    };
    Body::new(config, at(70.0, 70.0), heading)
}

fn dt() -> f32 {
    auberean_physics::body::tick_seconds()
}

fn distance(b: &Body, p: &Position) -> f32 {
    let o = geom::offset(&b.position, p);
    (o.x * o.x + o.y * o.y).sqrt()
}

/// A body ordered to a place behind it turns first, standing (it walks only facing within a
/// quarter turn of the place), then walks there and stops within the desired distance.
///
/// Behaviour: motion.moveto.a-body-turns-then-walks-to-the-place-and-stops
#[test]
fn a_body_turns_then_walks_to_the_place_and_stops() {
    let land = world();
    let mut b = body(180.0);
    let goal = at(70.0, 90.0);
    let mut p = MovementParameters::default();
    p.set_target_position(goal);
    p.set_max_success_distance(1.0);
    p.set_use_cylinders(false);
    let now = b.clock;
    let mut mi = std::mem::take(&mut b.minterp);
    assert!(mi.move_to_event(&mut b, now, &p));
    b.minterp = mi;
    // Facing away, it turns in place before it walks.
    for _ in 0..3 {
        b.tick(&land, dt());
    }
    assert!(
        distance(&b, &at(70.0, 70.0)) < 0.05,
        "it stood while facing away"
    );
    for _ in 0..80 {
        b.tick(&land, dt());
    }
    let d = distance(&b, &goal);
    assert!(d <= 1.0 + 0.05, "it came within the desired distance, {d}");
    let v = b.velocity;
    assert!(v.x * v.x + v.y * v.y < 1e-4, "and stopped there");
    assert!(
        b.minterp.is_moving_to(),
        "the server, not the client, ends the move"
    );
}

/// A turn to a heading stops on it: the turning slows to nothing on the heading, never past it.
///
/// Behaviour: motion.moveto.a-turn-stops-on-its-heading
#[test]
fn a_turn_stops_on_its_heading() {
    let land = world();
    let mut b = body(0.0);
    let mut p = MovementParameters {
        flags: 0,
        ..MovementParameters::default()
    };
    p.set_target_heading(Heading::new(90.0));
    let now = b.clock;
    let mut mi = std::mem::take(&mut b.minterp);
    assert!(mi.move_to_event(&mut b, now, &p));
    b.minterp = mi;
    let mut most: f32 = 0.0;
    for _ in 0..40 {
        b.tick(&land, dt());
        most = most.max(b.heading);
    }
    assert!(
        (b.heading - 90.0).abs() < 1.5,
        "it faces the heading, {}",
        b.heading
    );
    assert!(most < 90.0 + 1.5, "and never turned past it, {most}");
}

/// A body placed turns to the placement's heading at once on its next tick, the heading fallen to
/// a 255th of a turn: a new character made facing exactly 270 degrees stands at 269.647.
///
/// Behaviour: motion.turn.a-placed-body-turns-to-a-255th-of-a-turn
#[test]
fn a_placed_body_turns_to_a_255th_of_a_turn() {
    let land = world();
    let mut b = body(270.0);
    let mut placed = at(70.0, 70.0);
    placed.frame.rotation = auberean_physics::body::heading_rotation(270.0);
    b.set_placement_position(placed);
    assert_eq!(b.placement, Some(placed));
    b.tick(&land, dt());
    let byte = 191.0 * 360.0 / 255.0;
    assert!(
        (b.heading - byte).abs() < 1e-3,
        "turned at once: {}",
        b.heading
    );
    assert_eq!(b.turn_rate, 0.0, "and it is not left turning");
    for _ in 0..5 {
        b.tick(&land, dt());
    }
    assert!(!b.minterp.is_turning_to(), "the turn is done");
    assert!((b.heading - byte).abs() < 1e-3, "and stays: {}", b.heading);
}

/// A heading's rotation is one a body's move leaves bit for bit: with no distance to go, the
/// frame comes out equal, which is how a move that goes nowhere is known.
///
/// Behaviour: motion.turn.a-headings-rotation-comes-through-a-move-unchanged
#[test]
fn a_headings_rotation_comes_through_a_move_unchanged() {
    use dereth_physics::math::slerp_rotation_plain;
    for byte in 0..=255_u16 {
        let q = auberean_physics::body::heading_rotation(f32::from(byte) * 360.0 / 255.0);
        let from = Frame::new(Vec3::ZERO, q);
        let mut out = from;
        slerp_rotation_plain(&mut out, &from, &from, 1.0);
        assert_eq!(out.rotation, q, "byte {byte}");
    }
}

/// A body given the positions the server sends walks along them at its own speed and comes to rest
/// on the last.
///
/// Behaviour: motion.interp.a-body-follows-the-positions-it-is-sent
#[test]
fn a_body_follows_the_positions_it_is_sent() {
    let land = world();
    let mut b = body(0.0);
    b.is_player = false;
    for (i, y) in [72.0_f32, 76.0, 80.0].iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let time = i as f64 * 0.5;
        b.minterp.interpolate_to(
            time,
            at(70.0, *y),
            Vec3::new(0.0, if i < 2 { 1.0 } else { 0.0 }, 0.0),
            Heading::ZERO,
            true,
        );
    }
    let mut prev = b.position.frame.origin.y;
    let mut backwards = false;
    for _ in 0..60 {
        b.tick(&land, dt());
        let y = b.position.frame.origin.y;
        backwards |= y < prev - 1e-4;
        prev = y;
    }
    let d = distance(&b, &at(70.0, 80.0));
    assert!(d < 0.1, "it reached the last position, {d}");
    assert!(!backwards, "never going back along the way");
    assert!(
        !b.minterp.is_interpolating(),
        "and stopped interpolating there"
    );
}

/// A body that falls more than 15 m behind the positions it is sent is put at the last at once,
/// and moves on from there as the last asked (here a run: one tick's 0.65 m).
///
/// Behaviour: motion.interp.a-body-far-behind-is-put-at-the-end
#[test]
fn a_body_far_behind_is_put_at_the_end() {
    let land = world();
    let mut b = body(0.0);
    b.is_player = false;
    b.minterp.interpolate_to(
        0.0,
        at(70.0, 72.0),
        Vec3::new(0.0, 1.0, 0.0),
        Heading::ZERO,
        true,
    );
    b.minterp.interpolate_to(
        0.5,
        at(70.0, 110.0),
        Vec3::new(0.0, 1.0, 0.0),
        Heading::ZERO,
        true,
    );
    b.tick(&land, dt());
    let d = distance(&b, &at(70.0, 110.0));
    assert!(
        (d - 0.65).abs() < 0.01,
        "it was put at the last position and ran on, {d}"
    );
    assert!(b.position.frame.origin.y > 110.0);
    assert!(!b.minterp.is_interpolating());
}

/// On ground whose motion values have drag (some ground kinds have 0.3), a body nothing pushes
/// keeps `(1 - drag)` of its velocity a second: 0.7 to the power of a tick's tenth each tick.
///
/// Behaviour: motion.drag.a-body-on-dragging-ground-slows-by-its-drag
#[test]
fn a_body_on_dragging_ground_slows_by_its_drag() {
    let land = world();
    let mut b = body(0.0);
    let mut g = ground();
    g.drag = 0.3;
    g.forward_slowdown = 0.0;
    g.backward_slowdown = 0.0;
    b.minterp.desc = Some(std::sync::Arc::new(
        BodyConfig {
            ground: g,
            ..b.config.clone()
        }
        .description(),
    ));
    b.velocity = Vec3::new(0.0, 2.0, 0.0);
    b.tick(&land, dt());
    let k = dereth_primitives::num::math::powf(0.7, dt());
    assert!((b.velocity.y - 2.0 * k).abs() < 1e-4, "{}", b.velocity.y);
}
