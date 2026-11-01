//! What a body's figure is told of the body's motion each frame, so its feet match the ground it
//! covered rather than the speed it asks for.
//! Fixture: a flat 160 m block at height 10; a body of the player races' ground values (run
//! 6.5 m/s, accelerations 15) with one 0.48 m sphere and a 0.6 m step up, running north from
//! (70, 70) toward an object at (70, 80): a post 1 m across and 2 m high, or a platform 8 m across
//! and 0.3 m high; physics ticks of 0.1 s, frames of 1/60 s.

use std::sync::Arc;

use auberean_assets::animation::MotionParams;
use auberean_physics::body::{Body, BodyConfig};
use auberean_physics::geom::BBox;
use auberean_physics::land::{Block, Land};
use auberean_physics::motion::values::default_values;
use auberean_physics::motion::MotionInput;
use auberean_physics::object::Shape;
use auberean_physics::world::{World, WorldObject};
use auberean_scene::figure_motion;
use dereth_physics::geom::Sphere;
use dereth_primitives::shape::CylSphere;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

const GROUND_Z: f32 = 10.0;
const FRAME: f32 = 1.0 / 60.0;

fn ground() -> MotionParams {
    MotionParams {
        max_forward: 6.5,
        max_backward: 4.0,
        max_strafe: 4.5,
        still_turn_rate: 90.0,
        running_turn_rate: 120.0,
        forward_acceleration: 15.0,
        backward_acceleration: 15.0,
        strafe_acceleration: 15.0,
        ..default_values()
    }
}

/// A place on the block's ground, in its land cell.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn at(x: f32, y: f32) -> Position {
    let cell = 0x4040_0000 | ((x / 20.0) as u32 * 8 + (y / 20.0) as u32 + 1);
    Position::new(
        CellId(cell),
        Frame::new(Vec3::new(x, y, GROUND_Z), Quat::IDENTITY),
    )
}

/// An upright cylinder `radius` across its middle and `height` high, standing on the ground.
fn cylinder(radius: f32, height: f32) -> Arc<Shape> {
    Arc::new(Shape {
        trees: Vec::new(),
        cylinders: vec![CylSphere {
            low_pt: Vec3::ZERO,
            radius,
            height,
        }],
        spheres: Vec::new(),
        physics_box: BBox::default(),
        scale: Vec3::new(1.0, 1.0, 1.0),
        containment_radius: radius,
        render_box: BBox::new(
            Vec3::new(-radius, -radius, 0.0),
            Vec3::new(radius, radius, height),
        ),
        properties: Vec::new(),
    })
}

/// A body running north from (70, 70) at `shape` standing at (70, 80): what its figure was told
/// on the last frame of each of `ticks` ticks (forward and vertical speed), with the body's
/// velocity ahead and its height then.
fn run_at(shape: Arc<Shape>, ticks: usize) -> Vec<(f32, f32, f32, f32)> {
    let land = Land::new();
    land.insert(Block::flat(0x40, 0x40, GROUND_Z));
    let mut world = World::new();
    world.insert(
        &land,
        0x77,
        &WorldObject {
            shape,
            position: at(70.0, 80.0),
            is_static: true,
            passable: false,
        },
    );
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
    let mut body = Body::new(config, at(70.0, 70.0), 0.0);
    world.insert_body(&land, 0x50, &mut body);
    body.steer(MotionInput {
        forward: 1.0,
        ..MotionInput::default()
    });
    let mut told = Vec::new();
    for _ in 0..ticks {
        body.tick_in(&land, &mut world, 0.1);
        // Six frames a tick, the figure told each.
        let mut last = (0.0, 0.0);
        for _ in 0..6 {
            let (v, _, _) = figure_motion(&mut body, FRAME);
            last = (v.y, v.z);
        }
        told.push((
            last.0,
            last.1,
            body.local_velocity().y,
            body.position.frame.origin.z,
        ));
    }
    told
}

/// A body that runs into a wall and is held there still asks to run (its velocity stays the full
/// 6.5 m/s ahead), but its figure is told how far it moved over its last tick: nothing, so it
/// stands; before the wall it was told the full speed.
///
/// Behaviour: desktop.bodies.a-body-held-against-a-wall-is-animated-standing
#[test]
fn a_body_held_against_a_wall_is_animated_standing() {
    let told = run_at(cylinder(0.5, 2.0), 30);
    let running = told[8];
    assert!(
        (running.0 - 6.5).abs() < 0.01,
        "running, the figure is told the full speed: {running:?}"
    );
    let held = told[told.len() - 1];
    assert!(
        held.2 > 6.4,
        "held against the post, the body still asks to run: {held:?}"
    );
    assert!(
        held.0.abs() < 0.01,
        "held against the post, the figure is told it stands: {held:?}"
    );
}

/// A body that runs onto a platform 0.3 m high is lifted onto it within one tick; its figure is
/// told, for the tick after, that it rose at 3 m/s (0.3 m over the 0.1 s tick), and when it steps
/// off the far side that it fell at 3 m/s; on the flat it is told no vertical speed. The body's
/// own velocity never has any.
///
/// Behaviour: desktop.bodies.a-step-up-or-down-shows-in-the-figures-vertical-speed
#[test]
fn a_step_up_or_down_shows_in_the_figures_vertical_speed() {
    let told = run_at(cylinder(4.0, 0.3), 30);
    let up = told
        .iter()
        .position(|t| t.3 > GROUND_Z + 0.29)
        .expect("the body steps onto the platform");
    assert!(
        (told[up + 1].1 - 3.0).abs() < 0.01,
        "the tick after stepping up: {:?}",
        told[up + 1]
    );
    let down = told[up..]
        .iter()
        .position(|t| t.3 < GROUND_Z + 0.01)
        .expect("the body steps off the far side")
        + up;
    assert!(
        (told[down + 1].1 + 3.0).abs() < 0.01,
        "the tick after stepping down: {:?}",
        told[down + 1]
    );
    for t in told[..up].iter().chain(&told[up + 2..down]) {
        assert!(t.1.abs() < 0.01, "on the flat: {t:?}");
    }
}
