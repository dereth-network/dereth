//! The chase camera behind a body on synthetic ground.
//! Fixture: a flat 160 m block at height 10, and one rising to the north.

use auberean_physics::camera::{Camera, CameraSettings};
use auberean_physics::land::{Block, Land};
use dereth_primitives::num::math;
use dereth_primitives::{LandblockId, Vec3};

/// At rest the camera looks at the top of the body (2.06 m up for a body of scale 1) from 4.5 m
/// behind it, pitched 0.25 rad (14.3 degrees) above level, with a 75 degree vertical view (65 plus
/// the camera effects' least 10); a taller body pushes both out by its height scale.
///
/// Behaviour: motion.camera.the-camera-starts-behind-and-above
#[test]
fn the_camera_starts_behind_and_above() {
    let s = CameraSettings::default();
    assert_eq!(
        (s.distance, s.tilt, s.fov, s.look_at_height),
        (4.5, 0.25, 75.0, 2.06)
    );
    let tall = CameraSettings::for_body(2.06, 1.15);
    assert!((tall.distance - 5.175).abs() < 1e-5 && (tall.look_at_height - 2.369).abs() < 1e-5);
    let cam = Camera::new(s);
    let (look, eye) = cam.wanted(Vec3::new(50.0, 50.0, 10.0), 0.0);
    let d = Vec3::new(look.x - eye.x, look.y - eye.y, look.z - eye.z);
    assert!(
        (d.x).abs() < 1e-5,
        "straight behind a body facing north: {d:?}"
    );
    assert!((d.y - 4.5 * math::cosf(0.25)).abs() < 1e-4);
    assert!((-d.z - 4.5 * math::sinf(0.25)).abs() < 1e-4);
}

/// When the place the camera wants is under the ground, the camera stops where the line from the
/// look-at point meets the ground.
///
/// Behaviour: motion.camera.the-camera-stops-at-the-ground
#[test]
fn the_camera_stops_at_the_ground() {
    let land = Land::new();
    let block = LandblockId::new(0x40, 0x40);
    land.insert(Block::flat(0x40, 0x40, 10.0));
    let mut cam = Camera::new(CameraSettings::default());
    cam.pitch = -0.6;
    let view = cam.place(&land, block, Vec3::new(50.0, 50.0, 10.0), 0.0);
    assert!(
        (view.eye.z - 10.0).abs() < 1e-3,
        "on the ground: {:?}",
        view.eye
    );
    let (_, wanted) = cam.wanted(Vec3::new(50.0, 50.0, 10.0), 0.0);
    assert!(wanted.z < 10.0, "it wanted to be under it: {wanted:?}");
}

/// An upright post 0.5 m wide and 5 m high.
fn post_behind_shape() -> std::sync::Arc<auberean_physics::object::Shape> {
    use auberean_physics::object::Shape;
    use dereth_primitives::shape::CylSphere;
    std::sync::Arc::new(Shape {
        trees: Vec::new(),
        cylinders: vec![CylSphere {
            low_pt: Vec3::new(0.0, 0.0, 0.0),
            radius: 0.25,
            height: 5.0,
        }],
        spheres: Vec::new(),
        physics_box: auberean_physics::geom::BBox::default(),
        scale: Vec3::new(1.0, 1.0, 1.0),
        containment_radius: 0.25,
        render_box: auberean_physics::geom::BBox::default(),
        properties: Vec::new(),
    })
}

/// A post 0.5 m wide standing 1.5 m behind a body facing north on flat ground, under the line from
/// the body's right shoulder to where the camera wants to be.
fn post_behind() -> auberean_physics::camera::Lines<'static> {
    use dereth_primitives::{CellId, Frame, Position, Quat};
    let land = Land::new();
    let mut block = Block::flat(0x40, 0x40, 10.0);
    let cell = CellId(0x4040_0001);
    block.objects.push((
        Position::new(
            cell,
            Frame::new(Vec3::new(50.4, 48.5, 10.0), Quat::IDENTITY),
        ),
        post_behind_shape(),
    ));
    land.insert(block);
    let land: &'static Land = Box::leak(Box::new(land));
    auberean_physics::camera::Lines {
        land,
        cell: CellId(0x4040_0001),
        origin: Vec3::new(50.0, 50.0, 10.0),
        target_offset: Vec3::new(0.42, 0.0, 1.545),
        wanted: Vec3::new(50.0, 45.64, 13.17),
        env_only: false,
    }
}

/// A line from a cell near a landblock's east edge meets what stands just across the edge in the
/// next landblock, 160 m on: the landblocks of this world are 160 m long.
///
/// Behaviour: motion.camera.a-line-meets-what-stands-in-the-next-landblock
#[test]
fn a_line_meets_what_stands_in_the_next_landblock() {
    use auberean_physics::camera::placed_hit;
    use dereth_primitives::{CellId, Frame, Position, Quat};
    let land = Land::new();
    land.insert(Block::flat(0x40, 0x40, 10.0));
    let mut east = Block::flat(0x41, 0x40, 10.0);
    let post = post_behind_shape();
    east.objects.push((
        Position::new(
            CellId(0x4140_0001),
            Frame::new(Vec3::new(2.0, 80.0, 10.0), Quat::IDENTITY),
        ),
        post,
    ));
    land.insert(east);
    let hit = placed_hit(
        &land,
        CellId(0x4040_0001),
        Vec3::new(150.0, 80.0, 11.0),
        Vec3::new(170.0, 80.0, 11.0),
    )
    .expect("the post across the edge");
    assert!(
        (hit - (161.75 - 150.0) / 20.0).abs() < 1e-3,
        "met at its near side: {hit}"
    );
}

/// The camera's second line, from the shoulder to where the camera wants to be, stops at a solid
/// object standing in the world (here a post's upright cylinder), and outdoors the camera stands
/// where it stopped; with only the surroundings counting (the map view) it passes.
///
/// Behaviour: motion.camera.solid-objects-stop-the-second-line
#[test]
fn solid_objects_stop_the_second_line() {
    let lines = post_behind();
    let none = |_: Vec3, _: Vec3| None;
    let eye = auberean_physics::camera::keep_off_the_world(&lines, &none).expect("a place");
    assert!(
        (eye.y - 48.75).abs() < 0.05,
        "at the post's near side: {eye:?}"
    );
    let wanted = lines.wanted;
    let mut env = lines;
    env.env_only = true;
    let eye = auberean_physics::camera::keep_off_the_world(&env, &none).expect("a place");
    assert!((eye.y - wanted.y).abs() < 1e-4, "passes: {eye:?}");
    let moving = |_: Vec3, _: Vec3| Some(0.5);
    let mut far = lines;
    far.target_offset = Vec3::new(-0.42, 0.0, 1.545);
    let eye = auberean_physics::camera::keep_off_the_world(&far, &moving).expect("a place");
    let look = Vec3::new(49.58, 50.0, 11.5454);
    let half = Vec3::new(
        (look.x + wanted.x) / 2.0,
        (look.y + wanted.y) / 2.0,
        (look.z + wanted.z) / 2.0,
    );
    assert!(
        (eye.x - half.x).abs() < 1e-3 && (eye.y - half.y).abs() < 1e-3,
        "a moving object half way: {eye:?}"
    );
}
