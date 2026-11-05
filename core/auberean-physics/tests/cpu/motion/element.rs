//! A body's element from the water around it, the forces of water and pushers, its jump and its
//! standing state.
//! Fixture: a flat 160 m block with its ground at height 10, with and without water; a body 0.96 m
//! tall whose water line is three quarters of its height up and whose sea floor line a tenth; on
//! the earth and in the air it runs as the September 2013 player does (jump 6.26 m/s), in water it
//! has the lift its test gives it.

use std::sync::Arc;

use auberean_assets::animation::{ModeKey, MotionDesc, MotionParams, MotionSet};
use auberean_physics::body::{flags, Animate, Body, BodyConfig, Notice, JUMP_BEHAVIOR};
use auberean_physics::land::{land_cell_id, Block, Land};
use auberean_physics::motion::{AIR, EARTH, PEACE, SEA_FLOOR, WATER};
use dereth_physics::geom::Sphere;
use dereth_primitives::{Frame, Position, Quat, Vec3};

const GROUND_Z: f32 = 10.0;

fn earth() -> MotionParams {
    MotionParams {
        terminal_speed: 50.0,
        gravity: 1,
        jump_impulse: 6.26,
        max_forward: 6.5,
        forward_slowdown: 18.0,
        backward_slowdown: 25.0,
        strafe_slowdown: 25.0,
        turn_slowdown: 1850.0,
        forward_acceleration: 15.0,
        ..MotionParams::default()
    }
}

fn set(v: MotionParams) -> MotionSet {
    MotionSet {
        by_terrain: Vec::new(),
        by_modifier: Vec::new(),
        default: v,
    }
}

/// Values on the earth, in the air and (with `lift`) in water, with the water and sea floor lines.
fn description(lift: f32) -> MotionDesc {
    let key = |element| ModeKey {
        element,
        mode: PEACE,
    };
    MotionDesc {
        set_of: vec![(key(EARTH), 0), (key(AIR), 1), (key(WATER), 2)],
        sets: vec![
            set(earth()),
            set(MotionParams {
                terminal_speed: 50.0,
                gravity: 1,
                ..MotionParams::default()
            }),
            set(MotionParams {
                terminal_speed: 50.0,
                gravity: 1,
                lift,
                ..MotionParams::default()
            }),
        ],
        sea_floor_multiplier: 0.1,
        water_multiplier: 0.75,
    }
}

fn world(water: Option<f32>) -> Land {
    let land = Land::new();
    let mut block = Block::flat(0x40, 0x40, GROUND_Z);
    block.water = water;
    land.insert(block);
    land
}

/// A body 0.96 m tall at height `z` over the middle of the block.
fn body(z: f32, lift: f32) -> Body {
    let config = BodyConfig {
        spheres: vec![Sphere::new(Vec3::new(0.0, 0.0, 0.48), 0.48)],
        step_up: 0.6,
        step_down: 0.6,
        scale: 1.0,
        ground: earth(),
        air: MotionParams::default(),
    };
    let at = Position::new(
        land_cell_id(0x40, 0x40, 3, 3),
        Frame::new(Vec3::new(70.0, 70.0, z), Quat::IDENTITY),
    );
    let mut b = Body::new(config, at, 0.0);
    b.minterp.set_description(1, Arc::new(description(lift)));
    b
}

fn dt() -> f32 {
    auberean_physics::body::tick_seconds()
}

/// Out of the water a body touching the ground is on the earth and falls through the air
/// otherwise. Standing in water shallow enough that its water line is above the surface but its sea
/// floor line under it, it walks the sea floor; in water over its water line it swims.
///
/// Behaviour: motion.element.water-decides-swimming-and-the-sea-floor
#[test]
fn water_decides_swimming_and_the_sea_floor() {
    let dry = world(None);
    let mut b = body(GROUND_Z, 0.0);
    b.tick(&dry, dt());
    assert_eq!(b.element(), EARTH, "on the ground, out of water");
    let mut b = body(GROUND_Z + 5.0, 0.0);
    b.contact = false;
    b.on_walkable = false;
    b.tick(&dry, dt());
    assert_eq!(b.element(), AIR, "touching nothing");

    // Water line 10.72, sea floor line 10.096: a surface at 10.5 lies between them.
    let shallow = world(Some(GROUND_Z + 0.5));
    let mut b = body(GROUND_Z, 0.0);
    b.tick(&shallow, dt());
    assert_eq!(b.element(), SEA_FLOOR, "standing in shallow water");
    assert!((b.depth() - (0.5 - 0.72)).abs() < 1e-4, "{}", b.depth());

    let deep = world(Some(GROUND_Z + 2.0));
    let mut b = body(GROUND_Z, 0.0);
    b.tick(&deep, dt());
    assert_eq!(b.element(), WATER, "in water over its water line");
    assert!(b.has_water());
}

/// In water, gravity pulls and lift pushes up: by the whole lift more than a meter under, by the
/// lift times the depth within a meter of the surface. A swimmer whose lift matches gravity
/// stays at its depth.
///
/// Behaviour: motion.element.water-lifts-by-depth
#[test]
fn water_lifts_by_depth() {
    let land = world(Some(GROUND_Z + 20.0));
    let mut deep = body(GROUND_Z + 5.0, 19.6);
    deep.contact = false;
    deep.on_walkable = false;
    deep.tick(&land, dt());
    assert_eq!(deep.element(), WATER);
    assert!(deep.depth() > 1.0);
    assert!(
        (deep.acceleration.z - 9.8).abs() < 1e-4,
        "{}",
        deep.acceleration.z
    );

    // Water line half a meter under the surface: 19.6 times 0.5 cancels gravity.
    let z = GROUND_Z + 20.0 - 0.5 - 0.72;
    let mut near = body(z, 19.6);
    near.contact = false;
    near.on_walkable = false;
    near.tick(&land, dt());
    assert!((near.depth() - 0.5).abs() < 1e-4, "{}", near.depth());
    assert!(near.acceleration.z.abs() < 1e-4, "{}", near.acceleration.z);

    let mut floating = body(GROUND_Z + 5.0, 9.8);
    floating.contact = false;
    floating.on_walkable = false;
    for _ in 0..10 {
        floating.tick(&land, dt());
    }
    assert!(
        (floating.position.frame.origin.z - (GROUND_Z + 5.0)).abs() < 1e-4,
        "lift equal to gravity holds it at {}",
        floating.position.frame.origin.z
    );
}

/// A hover height the server sends through the body changes nothing: the motion interpreter's
/// setter does nothing.
///
/// Behaviour: motion.element.a-sent-hover-height-is-ignored
#[test]
fn a_sent_hover_height_is_ignored() {
    let mut b = body(GROUND_Z, 0.0);
    let before = b.hover_height();
    b.set_hover_height(before + 5.0);
    assert!((b.hover_height() - before).abs() < f32::EPSILON);
}

/// Asked to jump, the player's body standing or moving under 2 m/s along the ground starts the
/// jump's lead-in and stays on the ground until it ends; moving faster, or anyone else's body, jumps
/// at once at the jump's speed.
///
/// Behaviour: motion.jump.a-slow-player-leads-in-before-jumping
#[test]
fn a_slow_player_leads_in_before_jumping() {
    let land = world(None);
    let rested = || {
        let mut b = body(GROUND_Z, 0.0);
        for _ in 0..5 {
            b.tick(&land, dt());
        }
        b
    };
    let mut b = rested();
    assert!(b.jump_logic());
    assert!(b.contact, "still on the ground");
    assert!(b.behaviors.contains(&JUMP_BEHAVIOR), "the lead-in plays");
    assert!(b.velocity.z.abs() < f32::EPSILON);

    let mut b = rested();
    b.velocity = Vec3::new(0.0, 3.0, 0.0);
    assert!(b.jump_logic());
    assert!(!b.contact, "moving fast, it jumps at once");
    assert!((b.velocity.z - 6.26).abs() < 1e-4);
    assert_eq!(b.element(), AIR);

    let mut b = rested();
    b.is_player = false;
    assert!(b.jump_logic());
    assert!(!b.contact, "anyone else's body jumps at once");
}

/// A body's speeds and its jump take its own scale (the one its qualities give), not the scale its
/// look is drawn at: drawn at 0.9 of its size it still asks to run at the full 6.5 m/s and jumps at
/// the full 6.26 m/s; at an own scale of 1.2 it asks for 7.8 m/s and jumps at 7.512 m/s.
///
/// Behaviour: motion.run.a-body-runs-and-jumps-at-its-own-scale-not-its-looks
#[test]
fn a_body_runs_and_jumps_at_its_own_scale_not_its_looks() {
    let land = world(None);
    let rested = |own: f32| {
        let mut b = body(GROUND_Z, 0.0);
        b.look_scale = Vec3::new(0.9, 0.9, 0.9);
        b.set_scale(own);
        for _ in 0..5 {
            b.tick(&land, dt());
        }
        b.steer(auberean_physics::motion::MotionInput {
            forward: 1.0,
            ..auberean_physics::motion::MotionInput::default()
        });
        b
    };
    for (own, run, jump) in [(1.0, 6.5, 6.26), (1.2, 7.8, 7.512)] {
        let mut b = rested(own);
        let want = b.minterp.local_desired(&b, b.minterp.asked, false, false);
        assert!((want.y - run).abs() < 1e-4, "own {own}: asks for {want:?}");
        assert!(b.jump());
        assert!(
            (b.velocity.z - jump).abs() < 1e-4,
            "own {own}: jumps at {:?}",
            b.velocity
        );
    }
}

/// A body with a figure stays in its old element through the step that takes the figure to the
/// new one, while the element it is going to leads: it may not jump again until the figure stands
/// on the ground, and going back asks its animation again.
///
/// Behaviour: motion.element.a-body-with-a-figure-is-in-the-element-its-figure-stands-in
#[test]
fn a_body_with_a_figure_is_in_the_element_its_figure_stands_in() {
    let mut b = body(0.0, 0.0);
    b.contact = true;
    b.figure_stands_in(EARTH);
    b.notices.clear();
    b.do_element(AIR);
    assert_eq!((b.element(), b.desired_element()), (EARTH, AIR));
    let asked = |b: &mut Body| {
        std::mem::take(&mut b.notices)
            .into_iter()
            .filter(|n| matches!(n, Notice::Animate(Animate::Element(_))))
            .count()
    };
    assert_eq!(asked(&mut b), 1);
    b.do_element(AIR);
    assert_eq!(asked(&mut b), 0, "asked once");
    b.figure_stands_in(AIR);
    assert_eq!(b.element(), AIR, "the figure got there");
    b.do_element(EARTH);
    assert_eq!((b.element(), b.desired_element()), (AIR, EARTH));
    assert_eq!(asked(&mut b), 1);
    assert!(!b.jump(), "not yet on the ground");
    b.figure_stands_in(EARTH);
    assert!(b.jump(), "on the ground");
}

/// A body that goes into another element asks its animation to go there too, once: jumping, into
/// the air; landing, back onto the earth.
///
/// Behaviour: motion.element.a-new-element-goes-to-the-animation
#[test]
fn a_new_element_goes_to_the_animation() {
    let land = world(None);
    let mut b = body(GROUND_Z, 0.0);
    for _ in 0..5 {
        b.tick(&land, dt());
    }
    let elements = |b: &mut Body| -> Vec<u32> {
        b.take_requests()
            .into_iter()
            .filter_map(|n| match n {
                Notice::Animate(Animate::Element(e)) => Some(e),
                _ => None,
            })
            .collect()
    };
    let _ = elements(&mut b);
    b.is_player = false;
    assert!(b.jump_logic());
    assert_eq!(elements(&mut b), vec![AIR], "into the air");
    b.tick(&land, dt());
    assert!(elements(&mut b).is_empty(), "asked once");
    for _ in 0..40 {
        b.tick(&land, dt());
    }
    assert!(b.contact, "landed");
    assert_eq!(elements(&mut b), vec![EARTH], "back on the earth");
}

/// What a body stands in can push it: a pusher's velocity carries it that far over the tick, and a
/// pusher lifting it takes it off the ground and out of gravity.
///
/// Behaviour: motion.element.pushers-carry-and-lift
#[test]
fn pushers_carry_and_lift() {
    let land = world(None);
    let mut b = body(GROUND_Z, 0.0);
    b.tick(&land, dt());
    let x = b.position.frame.origin.x;
    b.pushers.velocity = Vec3::new(1.0, 0.0, 0.0);
    b.tick(&land, dt());
    assert!((b.position.frame.origin.x - x - 0.1).abs() < 1e-4);
    b.tick(&land, dt());
    assert!(
        (b.position.frame.origin.x - x - 0.1).abs() < 1e-4,
        "the push lasts its tick"
    );

    b.pushers.acceleration = Vec3::new(0.0, 0.0, 2.0);
    b.tick(&land, dt());
    assert!(!b.is_affected_by_gravity(), "lifted, gravity lets go");
    assert!(
        b.position.frame.origin.z > GROUND_Z + 0.005,
        "off the ground"
    );
    b.tick(&land, dt());
    assert!(b.is_affected_by_gravity(), "unlifted, it holds again");
}

/// A new body is mobile, edge sliding, lit, under gravity and reporting its collisions, with its
/// updates running; a body made not mobile stays where it is and its movement stops.
///
/// Behaviour: motion.element.a-body-that-is-not-mobile-stays
#[test]
fn a_body_that_is_not_mobile_stays() {
    let land = world(None);
    let mut b = body(GROUND_Z + 3.0, 0.0);
    assert_eq!(
        b.state(),
        flags::MOBILE
            | flags::EDGE_SLIDE
            | flags::LIGHTS
            | flags::GRAVITY
            | flags::REPORT_COLLISIONS
    );
    assert!(b.is_movement_active() && b.is_animation_active() && b.is_misc_active());
    b.set_mobile(false);
    b.contact = false;
    b.velocity = Vec3::new(1.0, 0.0, 0.0);
    let at = b.position;
    b.tick(&land, dt());
    assert_eq!(b.position, at);
    assert!(!b.is_movement_active());
}

/// An impulse away from another object pushes the body straight away from it at the given speed,
/// plus the upward part, and off the ground; asked to, the push is divided by the distance once
/// more.
///
/// Behaviour: motion.impulse.a-targeted-impulse-pushes-away
#[test]
fn a_targeted_impulse_pushes_away() {
    let target = |b: &Body| auberean_physics::motion::moveto::TargetBody {
        position: Position::new(
            b.position.cell,
            Frame::new(
                Vec3::new(
                    b.position.frame.origin.x,
                    b.position.frame.origin.y + 2.0,
                    b.position.frame.origin.z,
                ),
                Quat::IDENTITY,
            ),
        ),
        radius: 0.5,
        height: 1.0,
    };
    let mut b = body(GROUND_Z, 0.0);
    b.targets.insert(9, target(&b));
    assert!(b.add_targeted_impulse(9, 4.0, false, 1.5));
    assert!((b.velocity.x).abs() < 1e-5 && (b.velocity.y + 4.0).abs() < 1e-5);
    assert!((b.velocity.z - 1.5).abs() < 1e-5);
    assert!(!b.contact);
    assert_eq!(b.element(), AIR);

    let mut b = body(GROUND_Z, 0.0);
    b.targets.insert(9, target(&b));
    assert!(b.add_targeted_impulse(9, 4.0, true, 0.0));
    assert!(
        (b.velocity.y + 2.0).abs() < 1e-5,
        "halved at 2 m: {:?}",
        b.velocity
    );
    assert!(
        !b.add_targeted_impulse(10, 4.0, false, 0.0),
        "no such object"
    );
}

/// A heading set on the body falls to the 255th of a turn below it: 100 degrees becomes 70/255 of a
/// turn, 98.82 degrees.
///
/// Behaviour: motion.turn.a-set-heading-falls-to-a-255th-of-a-turn
#[test]
fn a_set_heading_falls_to_a_255th_of_a_turn() {
    let mut b = body(GROUND_Z, 0.0);
    b.set_heading(auberean_physics::geom::Heading::new(100.0));
    assert!(
        (b.heading - 70.0 * 360.0 / 255.0).abs() < 1e-4,
        "{}",
        b.heading
    );
    assert!((b.frame_heading().degrees() - 70.0 * 360.0 / 255.0).abs() < 1e-3);
}

/// A velocity set "in the body's frame" is taken as it is, not turned by the body's heading.
///
/// Behaviour: motion.impulse.a-local-velocity-is-not-turned
#[test]
fn a_local_velocity_is_not_turned() {
    let mut b = body(GROUND_Z, 0.0);
    b.set_heading(auberean_physics::geom::Heading::new(90.0));
    b.set_local_velocity(Vec3::new(0.0, 3.0, 0.0));
    assert!((b.velocity.y - 3.0).abs() < 1e-6 && b.velocity.x.abs() < 1e-6);
    assert!((b.anim_velocity.y - 3.0).abs() < 1e-6);
}

/// Between ticks the speed a body's figure is animated at (how far it moved over its last tick)
/// takes half its acceleration times each animation step squared, in its frame: a body falling
/// under gravity is animated a little faster downward with each step. A body whose animation is
/// frozen keeps it as it is.
///
/// Behaviour: motion.element.the-animation-speed-takes-the-acceleration-over-each-step
#[test]
fn the_animation_speed_takes_the_acceleration_over_each_step() {
    let land = world(None);
    let mut b = body(GROUND_Z + 20.0, 0.0);
    b.contact = false;
    b.on_walkable = false;
    for _ in 0..3 {
        b.tick(&land, dt());
    }
    assert!(
        b.acceleration.z < -9.0 && !b.contact,
        "falling: {:?}",
        b.acceleration
    );
    let was = b.anim_velocity;
    assert!(was.z < -1.0, "falling, animated falling: {was:?}");
    let step = 0.05;
    b.update_animation(step);
    let want = was.z + b.acceleration.z * 0.5 * step * step;
    assert!(
        (b.anim_velocity.z - want).abs() < 1e-5 && (b.anim_velocity.z - was.z).abs() > 0.01,
        "{:?} after {was:?}",
        b.anim_velocity
    );
    let was = b.anim_velocity;
    b.set_animation_frozen(true);
    b.update_animation(step);
    assert_eq!(b.anim_velocity, was, "frozen");
}

/// A move the server orders is taken only by the player's body, which keeps the order's stamp,
/// stops the behaviors a move cancels and sets off toward the place; anyone else's body stays.
///
/// Behaviour: motion.moveto.only-the-player-takes-an-ordered-move
#[test]
fn only_the_player_takes_an_ordered_move() {
    let mut p = auberean_physics::motion::params::MovementParameters::default();
    let goal = Position::new(
        land_cell_id(0x40, 0x40, 3, 3),
        Frame::new(Vec3::new(70.0, 80.0, GROUND_Z), Quat::IDENTITY),
    );
    p.set_target_position(goal);
    p.set_max_success_distance(1.0);
    p.set_use_cylinders(false);
    let mut other = body(GROUND_Z, 0.0);
    other.is_player = false;
    assert!(!other.receive_move_to(&p, 7));
    let land = world(None);
    for _ in 0..10 {
        other.tick(&land, dt());
    }
    assert!((other.position.frame.origin.y - 70.0).abs() < 1e-4);
    let mut me = body(GROUND_Z, 0.0);
    assert!(me.receive_move_to(&p, 7));
    assert_eq!(me.move_to_stamp(), 7);
    assert!(me
        .notices
        .contains(&auberean_physics::body::Notice::Animate(
            auberean_physics::body::Animate::StopMoveToCancels
        )));
    assert!(other.notices.is_empty());
    for _ in 0..10 {
        me.tick(&land, dt());
    }
    assert!(
        me.position.frame.origin.y > 71.0,
        "sets off toward the place"
    );
}

/// A position the server sends within 15 m is moved toward over the ticks; one farther away is
/// taken at once.
///
/// Behaviour: motion.interp.a-sent-position-far-away-is-taken-at-once
#[test]
fn a_sent_position_far_away_is_taken_at_once() {
    let land = world(None);
    let near = Position::new(
        land_cell_id(0x40, 0x40, 3, 3),
        Frame::new(Vec3::new(70.0, 75.0, GROUND_Z), Quat::IDENTITY),
    );
    let mut b = body(GROUND_Z, 0.0);
    b.is_player = false;
    let h = auberean_physics::geom::Heading::new(0.0);
    b.interpolate_to(1.0, near, Vec3::ZERO, h, true);
    assert!(
        (b.position.frame.origin.y - 70.0).abs() < 1e-4,
        "not there yet"
    );
    for _ in 0..30 {
        b.tick(&land, dt());
    }
    assert!(b.position.frame.origin.y > 72.0, "moved toward it");

    let far = Position::new(
        land_cell_id(0x40, 0x40, 3, 3),
        Frame::new(Vec3::new(70.0, 90.0, GROUND_Z), Quat::IDENTITY),
    );
    let mut b = body(GROUND_Z, 0.0);
    b.interpolate_to(1.0, far, Vec3::ZERO, h, true);
    assert!(
        (b.position.frame.origin.y - 90.0).abs() < 1e-4,
        "taken at once"
    );
}

/// Falling fast toward water that lies nearer than the ground, more than 2 m below its water line,
/// a body asks its animation for the air's mode, and the change is recorded for the host with its
/// animation set running; a body whose animation is frozen asks nothing.
///
/// Behaviour: motion.element.a-fall-toward-deep-water-asks-for-the-air-mode
#[test]
fn a_fall_toward_deep_water_asks_for_the_air_mode() {
    let deep = world(Some(GROUND_Z + 2.0));
    let falling = |frozen: bool| {
        let mut b = body(GROUND_Z + 20.0, 0.0);
        b.contact = false;
        b.on_walkable = false;
        b.velocity = Vec3::new(0.0, 0.0, -5.0);
        b.set_animation_frozen(frozen);
        b.set_animation_active(false);
        b.tick(&deep, dt());
        b
    };
    let b = falling(false);
    assert_eq!(b.modes, vec![(0, AIR)]);
    assert!(b.is_animation_active());
    assert!(falling(true).modes.is_empty());
}
