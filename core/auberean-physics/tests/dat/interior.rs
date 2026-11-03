//! The player's body inside: the human training hall's cells, their floors, walls and doorways.
//! Fixture: hall copy 0x0298 of the September 2013 cell file; cell 0x0109, where the 2017 servers
//! kept a hall's players; the human male's collision spheres and step heights and the player's
//! motion values.

use auberean_assets::animation::MotionDesc;
use auberean_assets::setup::Setup;
use auberean_physics::body::{setup_spheres, Body, BodyConfig};
use auberean_physics::interior::add_block_cells;
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;
use auberean_physics::motion::MotionInput;
use auberean_physics::world::World;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::need_world;

pub(crate) fn body(w: &crate::common::World, at: Position, heading: f32) -> Body {
    let setup: Setup = w.records.get(0x0400_0029).expect("the human setup");
    let md: MotionDesc = w.records.get(0x3200_0000).expect("the player's motion");
    let pick = |element: u32| {
        let (_, set) = md
            .set_of
            .iter()
            .find(|(k, _)| k.element == element && k.mode == 0x4000_0001)
            .expect("a set");
        md.sets[*set as usize].default
    };
    let config = BodyConfig {
        spheres: setup_spheres(&setup),
        step_up: setup.step_up_height,
        step_down: setup.step_down_height,
        scale: 1.0,
        ground: pick(0x4000_0001),
        air: pick(0x4000_0002),
    };
    Body::new(config, at, heading)
}

/// A body dropped in a hall cell lands on the cell's floor and stays on it; running, it moves
/// from cell to cell through the doorways, never falls through a floor, and comes to rest against
/// a wall.
///
/// Behaviour: motion.interior.a-body-stands-and-runs-on-an-interior-floor
#[test]
fn a_body_stands_and_runs_on_an_interior_floor() {
    let w = need_world!();
    let land = Land::new();
    let added = add_block_cells(&land, &w.records, &mut Trees::default(), 0x02, 0x98);
    assert!(added > 50, "the hall's cells: {added}");
    let start = Position::new(
        CellId(0x0298_0109),
        Frame::new(Vec3::new(59.0, 240.0, -44.0), Quat::IDENTITY),
    );
    let mut b = body(w, start, 180.0);
    b.contact = false;
    b.on_walkable = false;
    let mut cells = std::collections::BTreeSet::new();
    let mut lowest = f32::MAX;
    let mut track = Vec::new();
    for k in 0..200 {
        b.steer(MotionInput {
            forward: if k >= 20 { 1.0 } else { 0.0 },
            turn: if (60..66).contains(&k) || (120..130).contains(&k) {
                1.0
            } else {
                0.0
            },
            ..MotionInput::default()
        });
        b.tick(&land, 0.1);
        let p = b.position.frame.origin;
        lowest = lowest.min(p.z);
        cells.insert(b.position.cell.0);
        track.push((k, b.position.cell.0, p, b.contact));
    }
    let (_, _, settled, contact) = track[19];
    assert!(contact, "standing after it was dropped: {settled:?}");
    assert!(
        (settled.z - -44.0).abs() < 1.5,
        "on the floor below where it was dropped: {settled:?}"
    );
    assert!(lowest > settled.z - 1.0, "never falls through: {lowest}");
    assert!(cells.len() >= 2, "through a doorway: {cells:08X?}");
    let (_, _, end, standing) = track[track.len() - 1];
    let (_, _, second_before, _) = track[track.len() - 11];
    // How far it got: since what stands in the cells is solid, it may end among them nearer
    // where it started than its farthest point.
    let ran = track
        .iter()
        .map(|(_, _, p, _)| ((p.x - settled.x).powi(2) + (p.y - settled.y).powi(2)).sqrt())
        .fold(0.0f32, f32::max);
    assert!(ran > 10.0, "it ran: {ran} m");
    let moved = ((end.x - second_before.x).powi(2) + (end.y - second_before.y).powi(2)).sqrt();
    assert!(
        moved < 0.5,
        "at rest against a wall, still asking to run: moved {moved} m in the last second"
    );
    assert!(standing, "standing");
}

/// A body moved by hand, a step at a time with no motion of its own, goes through the
/// transition as a tick's move does: moved north from the hall's cell 0x0109 it never sinks
/// through the floor (it climbs the steps on its way), takes the cells it passes into through the
/// doorways, and comes to rest against a wall.
///
/// Behaviour: motion.interior.a-body-moved-by-hand-meets-the-walls
#[test]
fn a_body_moved_by_hand_meets_the_walls() {
    let w = need_world!();
    let land = Land::new();
    add_block_cells(&land, &w.records, &mut Trees::default(), 0x02, 0x98);
    let mut world = auberean_physics::world::World::new();
    let mut b = body(w, at_the_start(), 0.0);
    let floor = at_the_start().frame.origin.z;
    let mut cells = std::collections::BTreeSet::new();
    let mut track = Vec::new();
    for _ in 0..120 {
        b.move_through(&land, &mut world, Vec3::new(0.0, 0.5, 0.0), 0.0);
        cells.insert(b.position.cell.0);
        track.push(b.position.frame.origin);
    }
    let end = track[track.len() - 1];
    let before = track[track.len() - 11];
    assert!(end.y - 240.2 > 10.0, "it went north: {end:?}");
    assert!(
        track.iter().all(|p| p.z > floor - 0.5 && p.z < floor + 3.0),
        "on the floor or the steps: {track:?}"
    );
    assert!(cells.len() >= 2, "through a doorway: {cells:08X?}");
    assert!(
        ((end.x - before.x).powi(2) + (end.y - before.y).powi(2)).sqrt() < 0.3,
        "at rest against a wall: {before:?} then {end:?}"
    );
}

/// Inside, the chase camera behind a body with its back to a wall stops at the wall: nearer the
/// look-at point than the distance it wants, and still inside the hall's cells.
///
/// Behaviour: motion.camera.inside-the-camera-stops-at-the-walls
#[test]
fn inside_the_camera_stops_at_the_walls() {
    use auberean_physics::camera::{Camera, CameraSettings};
    let w = need_world!();
    let land = Land::new();
    add_block_cells(&land, &w.records, &mut Trees::default(), 0x02, 0x98);
    // Run north from the hall's cell 0x0109 until a wall stops the body.
    let start = Position::new(
        CellId(0x0298_0109),
        Frame::new(Vec3::new(59.0, 240.0, -44.0), Quat::IDENTITY),
    );
    let mut b = body(w, start, 0.0);
    for _ in 0..80 {
        b.steer(MotionInput {
            forward: 1.0,
            ..MotionInput::default()
        });
        b.tick(&land, 0.1);
    }
    let at = b.position;
    // The camera behind a body facing away from that wall wants to be in it.
    let camera = Camera::new(CameraSettings::default());
    let away = (b.heading + 180.0).rem_euclid(360.0);
    let (look, wanted) = camera.wanted(at.frame.origin, away);
    let world = with_the_player(w, &land, &mut b);
    let seen = sighted(&world, &land, &at, look, wanted);
    let dist =
        |a: Vec3, c: Vec3| ((a.x - c.x).powi(2) + (a.y - c.y).powi(2) + (a.z - c.z).powi(2)).sqrt();
    assert!(dist(look, wanted) > 4.4, "it wants 4.5 m");
    let got = dist(seen.look.frame.origin, seen.eye.frame.origin);
    assert!(got < 4.0, "pulled in from the wall: {got} m");
    assert!(
        holds(&land, &seen.eye),
        "the camera stands in a hall cell that holds it: {:?}",
        seen.eye
    );
}

/// The camera's lines of sight for a body at `at` looking at `look` from where it wants to be,
/// `wanted` (both in the body's landblock frame).
pub(crate) fn sighted(
    world: &World,
    land: &Land,
    at: &Position,
    look: Vec3,
    wanted: Vec3,
) -> auberean_physics::camera::Sighted {
    let o = at.frame.origin;
    auberean_physics::camera::sight_lines(
        world,
        &auberean_physics::camera::Lines {
            land,
            cell: at.cell,
            origin: o,
            target_offset: Vec3::new(look.x - o.x, look.y - o.y, look.z - o.z),
            wanted,
            env_only: false,
        },
    )
    .expect("the body's cell is loaded")
}

/// A world holding body `b` as the player's, of the physics type the recorded players' are (one
/// the camera passes through), with the install's property list.
pub(crate) fn with_the_player(w: &crate::common::World, land: &Land, b: &mut Body) -> World {
    let mut world = World::new();
    if let Some(list) = w
        .records
        .get::<auberean_assets::property::PropertyList>(0x3400_0000)
    {
        world.set_property_list(list);
    }
    let key = world.insert_body(land, 0x5000_0001, b);
    world.set_ethereality(
        key,
        auberean_physics::solid::Ethereality {
            physics_type: 0x0480_0020,
            ..auberean_physics::solid::Ethereality::default()
        },
    );
    world
}

/// Whether interior cell `at.cell` holds `at`'s point.
pub(crate) fn holds(land: &Land, at: &Position) -> bool {
    land.cell(at.cell).is_some_and(|g| {
        let l = dereth_physics::math::globaltolocal(&g.frame, at.frame.origin);
        g.cell_bsp
            .as_ref()
            .is_some_and(|t| t.point_inside_cell_bsp(l))
    })
}

/// The place in the training hall where a new character stands, facing the platform of steps the
/// Shelter Guard stands on.
pub(crate) fn at_the_start() -> Position {
    Position::new(
        CellId(0x0298_0109),
        Frame::new(Vec3::new(59.1, 240.2, -44.6), Quat::IDENTITY),
    )
}

/// Run a body from the hall's starting place toward the platform for `ticks` tenths of a second;
/// the highest it stood and where it ended.
fn run_to_the_platform(w: &crate::common::World, land: &Land, ticks: u32) -> (Body, f32, f32) {
    let mut b = body(w, at_the_start(), 270.0);
    b.contact = false;
    b.on_walkable = false;
    let mut floor = f32::MAX;
    let mut highest = f32::MIN;
    for k in 0..ticks {
        b.steer(MotionInput {
            forward: if k >= 10 { 1.0 } else { 0.0 },
            ..MotionInput::default()
        });
        b.tick(land, 0.1);
        let z = b.position.frame.origin.z;
        if k < 10 {
            floor = floor.min(z);
        }
        highest = highest.max(z);
    }
    (b, floor, highest)
}

/// Inside, what stands in the cells is solid: a body running from the hall's starting place onto
/// the platform of steps climbs them (each lower than the body's step-up height) and crosses its
/// top, instead of running through it at the floor's height.
///
/// Behaviour: motion.interior.a-body-steps-onto-what-stands-in-the-cells
#[test]
fn a_body_steps_onto_what_stands_in_the_cells() {
    let w = need_world!();
    let land = Land::new();
    add_block_cells(&land, &w.records, &mut Trees::default(), 0x02, 0x98);
    let (b, floor, highest) = run_to_the_platform(w, &land, 40);
    assert!(
        b.statics_in(CellId(0x0298_0109)) > 0,
        "the starting cell's solids are in its collision list"
    );
    assert!(
        highest - floor > 0.5,
        "up on the platform: the floor at {floor}, the highest {highest}"
    );
    let x = b.position.frame.origin.x;
    assert!(x < 50.0, "it crossed onto the platform: x {x}");
    assert!(b.contact, "standing");
}

/// Inside, a cell's doorways do not stop the camera's lines: at the hall's starting place, 0.9 m
/// from the doorway behind it, the camera stands where it wants to, 4.5 m from the top of the body,
/// in the room beyond the doorway, which holds it.
///
/// Behaviour: motion.camera.inside-doorways-do-not-stop-the-camera
#[test]
fn inside_doorways_do_not_stop_the_camera() {
    use auberean_physics::camera::{Camera, CameraSettings};
    let w = need_world!();
    let land = Land::new();
    add_block_cells(&land, &w.records, &mut Trees::default(), 0x02, 0x98);
    let at = at_the_start();
    let camera = Camera::new(CameraSettings::default());
    let (look, eye) = camera.wanted(at.frame.origin, 270.0);
    // The doorway to the next cell east is a polygon of the cell's mesh at x = 60, between the
    // body and where the camera wants to be.
    assert!(
        look.x < 60.0 && eye.x > 60.0,
        "the doorway is between: {look:?} {eye:?}"
    );
    let world = with_the_player(w, &land, &mut body(w, at, 270.0));
    let seen = sighted(&world, &land, &at, look, eye);
    let (l, e) = (seen.look.frame.origin, seen.eye.frame.origin);
    let d = ((e.x - l.x).powi(2) + (e.y - l.y).powi(2) + (e.z - l.z).powi(2)).sqrt();
    assert!((d - 4.5).abs() < 0.01, "4.5 m back: {d} ({seen:?})");
    assert_ne!(seen.eye.cell, at.cell, "through the doorway");
    assert!(holds(&land, &seen.eye), "in the room beyond: {seen:?}");
}

/// Inside, the camera's first line, from the body up to the top of it, stops only at the cells'
/// walls, floors and ceilings, not at what stands in them: from a place inside the hall's platform
/// of steps (where a body that ran through it stood), the camera still looks at the top of the
/// body and stands back from it, not inside the steps.
///
/// Behaviour: motion.camera.inside-the-first-line-passes-what-stands-in-the-cells
#[test]
fn inside_the_first_line_passes_what_stands_in_the_cells() {
    use auberean_physics::camera::{Camera, CameraSettings};
    let w = need_world!();
    let land = Land::new();
    add_block_cells(&land, &w.records, &mut Trees::default(), 0x02, 0x98);
    let cell = CellId(0x0298_0109);
    let inside = Vec3::new(49.7, 240.2, -44.9);
    let at = Position::new(cell, Frame::new(inside, Quat::IDENTITY));
    let world = with_the_player(w, &land, &mut body(w, at, 270.0));
    // The platform's top is above that place: a line of sight up from it that stops at what
    // stands in the cells is stopped.
    let mut up = auberean_physics::ethereal::SightLine::default();
    up.set_ray(
        auberean_physics::geom::Ray::from_offset(inside, Vec3::new(0.0, 0.0, 2.0)),
        cell.0,
    );
    assert!(world.los_check(&land, &mut up));
    assert!(up.delta < 1.0, "the platform is over the place");
    let camera = Camera::new(CameraSettings::default());
    let (look, wanted) = camera.wanted(inside, 270.0);
    let seen = sighted(&world, &land, &at, look, wanted);
    let l = seen.look.frame.origin;
    assert!(
        (l.z - look.z).abs() < 0.001,
        "looks at the top of the body: {l:?} wanted {look:?}"
    );
    let e = seen.eye.frame.origin;
    let d = ((e.x - l.x).powi(2) + (e.y - l.y).powi(2)).sqrt();
    assert!(d > 3.0, "the camera stands back: {d} m ({seen:?})");
}

/// The training ground's second area is one closed room with no doorways (cell `0x0298014D`). A
/// body arriving where the menhir portal puts a player stands there, and it runs on the floor: from
/// the arrival north over the floor's cracks, and on up the room's length over the rises, within
/// 10 cm of the heights a recorded 2017 session reported running the same line, never leaving the
/// cell.
///
/// Behaviour: motion.interior.a-body-stands-and-runs-in-a-room-with-no-doorways
#[test]
fn a_body_stands_and_runs_in_a_room_with_no_doorways() {
    // (north, height) of the recorded player's reports along x = 0.55, in order.
    #[rustfmt::skip]
    const RECORDED: [(f32, f32); 87] = [
        (17.371, 1.205), (18.647, 1.375), (19.905, 1.688), (21.845, 1.822), (23.139, 1.912),
        (24.404, 2.128), (25.659, 2.365), (26.914, 2.603), (28.137, 2.920), (29.349, 3.245),
        (30.562, 3.571), (31.767, 3.910), (32.970, 4.252), (34.172, 4.595), (35.375, 4.849),
        (36.633, 5.078), (37.905, 5.233), (39.154, 5.508), (40.375, 5.817), (41.597, 6.126),
        (42.819, 6.435), (44.041, 6.744), (45.265, 7.019), (46.500, 7.319), (47.760, 7.542),
        (49.021, 7.765), (50.281, 7.989), (51.542, 8.212), (52.802, 8.435), (54.063, 8.658),
        (55.323, 8.881), (56.584, 9.104), (57.844, 9.328), (59.104, 9.551), (60.365, 9.774),
        (61.625, 9.997), (62.886, 10.220), (64.146, 10.443), (65.408, 10.636), (66.672, 10.851),
        (68.567, 11.174), (69.816, 11.450), (71.023, 11.803), (72.235, 12.120),
        (73.447, 12.405), (74.671, 12.789), (75.971, 12.823), (77.270, 12.858),
        (78.569, 12.893), (79.868, 12.830), (81.168, 12.666), (82.468, 12.506),
        (83.768, 12.346), (85.068, 12.186), (86.368, 12.037), (87.668, 11.894),
        (88.968, 11.751), (90.268, 11.608), (91.568, 11.465), (92.868, 11.322),
        (94.168, 11.179), (95.468, 11.035), (96.768, 10.893), (98.068, 10.750),
        (99.368, 10.608), (100.668, 10.466), (101.968, 10.324), (103.268, 10.183),
        (104.568, 10.041), (105.868, 9.898), (107.168, 9.755), (108.468, 9.612),
        (109.768, 9.469), (111.068, 9.327), (112.368, 9.185), (113.668, 9.044),
        (115.618, 8.832), (116.918, 8.690), (118.218, 8.546), (119.518, 8.403),
        (120.818, 8.262), (122.118, 8.122), (123.418, 7.980), (124.718, 7.839),
        (126.018, 7.698), (127.318, 7.557), (128.618, 7.467),
    ];
    let recorded = |y: f32| {
        let k = RECORDED.iter().rposition(|p| p.0 <= y)?;
        let (a, b) = (RECORDED[k], *RECORDED.get(k + 1)?);
        Some(a.1 + (b.1 - a.1) * (y - a.0) / (b.0 - a.0))
    };
    let w = need_world!();
    let land = Land::new();
    add_block_cells(&land, &w.records, &mut Trees::default(), 0x02, 0x98);
    let cell = CellId(0x0298_014D);
    assert!(land.cell(cell).expect("the room").portals.is_empty());
    let dt = auberean_physics::body::tick_seconds();
    let run = MotionInput {
        forward: 1.0,
        ..MotionInput::default()
    };
    let at = |x, y, z| Position::new(cell, Frame::new(Vec3::new(x, y, z), Quat::IDENTITY));

    let mut b = body(w, at(1.147_527_9, -7.586_839_7, 1.304_330_3), 0.0);
    for _ in 0..10 {
        b.tick(&land, dt);
    }
    assert!(b.contact && b.on_walkable, "standing where it arrived");
    let p = b.position.frame.origin;
    assert!((p.z - 1.304).abs() < 0.01, "on the floor: {p:?}");
    assert!((p.y + 7.587).abs() < 0.01, "stayed: {p:?}");
    b.steer(run);
    for _ in 0..30 {
        b.tick(&land, dt);
        let p = b.position.frame.origin;
        assert_eq!(b.position.cell, cell);
        assert!(p.z > 0.95, "above the floor: {p:?}");
        assert!(b.contact, "on the floor: {p:?}");
    }
    assert!(b.position.frame.origin.y > 10.0, "it ran");

    let mut b = body(w, at(0.586, 17.371, 1.205), 0.0);
    b.steer(run);
    let mut compared = 0;
    let mut worst = 0.0_f32;
    for _ in 0..200 {
        b.tick(&land, dt);
        assert_eq!(b.position.cell, cell);
        let p = b.position.frame.origin;
        if let Some(z) = recorded(p.y) {
            worst = worst.max((p.z - z).abs());
            compared += 1;
        }
    }
    let end = b.position.frame.origin;
    assert!(end.y > 125.0, "it ran the room's length: {end:?}");
    assert!(compared > 150, "{compared} ticks compared");
    assert!(worst < 0.1, "off the recorded heights by {worst} m");
}

/// The great caverns' cells (block `0x0286`) are open air: their meshes have no collision tree and
/// their polygons are their doorways. They are cells all the same: all 13 are built with their
/// doorways, and a body in one falls through the air and on through the doorway in the floor
/// instead of standing on nothing.
///
/// Behaviour: motion.interior.a-cell-with-no-collision-tree-is-open-air
#[test]
fn a_cell_with_no_collision_tree_is_open_air() {
    let w = need_world!();
    let land = Land::new();
    let added = add_block_cells(&land, &w.records, &mut Trees::default(), 0x02, 0x86);
    assert_eq!(added, 13);
    let start = CellId(0x0286_0101);
    let g = land.cell(start).expect("a cell of the caverns");
    assert!(g.physics_bsp.is_none());
    assert_eq!(g.portals.len(), 5);
    let at = g.frame.origin;
    let mut b = body(w, Position::new(start, Frame::new(at, Quat::IDENTITY)), 0.0);
    b.contact = false;
    b.on_walkable = false;
    let dt = auberean_physics::body::tick_seconds();
    let mut cells = std::collections::BTreeSet::new();
    for _ in 0..60 {
        b.tick(&land, dt);
        cells.insert(b.position.cell.0);
    }
    assert!(!b.contact, "nothing to stand on");
    assert!(b.position.frame.origin.z < at.z - 80.0, "it fell");
    assert_eq!(
        cells.into_iter().collect::<Vec<_>>(),
        [0x0286_0101, 0x0286_0104],
        "down through the doorway in the floor"
    );
}

/// A body falls by the gravity where it is. Each room's own properties may set it, and the region
/// lays it over the land: dropped in a room whose gravity is 3 m/s2 (cell `0x04970195`) a body is
/// going 0.3 m/s faster each tick; in a room whose gravity pulls up at 1 m/s2 (cell `0x03C70125`) it
/// goes up; on the land the region's blocks say 125/255 of 20 m/s2 (9.8039), and in the low blocks
/// 2/255 of it (block `0xF6F8`).
///
/// Behaviour: motion.interior.a-body-falls-by-the-gravity-where-it-is
#[test]
fn a_body_falls_by_the_gravity_where_it_is() {
    let w = need_world!();
    let list: auberean_assets::property::PropertyList =
        w.records.get(0x3400_0000).expect("the property list");
    let dt = auberean_physics::body::tick_seconds();
    let fall = |cell: u32, origin: Vec3| {
        let land = Land::new();
        let mut trees = Trees::default();
        let (x, y) = ((cell >> 24) as u8, ((cell >> 16) & 0xFF) as u8);
        if cell & 0xFFFF >= 0x100 {
            add_block_cells(&land, &w.records, &mut trees, x, y);
        } else {
            land.add_block(&w.records, &w.region, &mut trees, x, y);
        }
        let mut world = World::new();
        world.set_property_list(list.clone());
        let at = Position::new(CellId(cell), Frame::new(origin, Quat::IDENTITY));
        let mut b = body(w, at, 0.0);
        b.contact = false;
        b.on_walkable = false;
        let before = b.velocity.z;
        assert!(
            b.current_properties().is_none(),
            "nothing before the first cell"
        );
        b.tick_in(&land, &mut world, dt);
        let named = b
            .current_property(&list, auberean_physics::material::GRAVITY)
            .and_then(|p| p.value.inq_vector())
            .expect("the gravity is among what the cell made of it");
        assert_eq!(named.z, b.current_gravity().z);
        (b.current_gravity(), b.velocity.z - before)
    };
    let room = |cell: u32| {
        let (x, y, low) = (
            (cell >> 24) as i32,
            ((cell >> 16) & 0xFF) as i32,
            (cell & 0xFFFF) as u16,
        );
        let c = w.records.cell(x, y, low).expect("the room");
        Vec3::new(c.frame.origin.x, c.frame.origin.y, c.frame.origin.z)
    };
    let (g, dv) = fall(0x0497_0195, room(0x0497_0195));
    assert_eq!(g, Vec3::new(0.0, 0.0, -3.0));
    assert!((dv + 0.3).abs() < 1e-4, "{dv}");
    let (g, dv) = fall(0x03C7_0125, room(0x03C7_0125));
    assert_eq!(g, Vec3::new(0.0, 0.0, 1.0));
    assert!((dv - 0.1).abs() < 1e-4, "{dv}");
    let (g, _) = fall(0x0298_0001, Vec3::new(12.0, 12.0, 150.0));
    assert!((g.z + 9.8039).abs() < 1e-3, "{g:?}");
    let (g, _) = fall(0xF6F8_0001, Vec3::new(12.0, 12.0, 400.0));
    assert!((g.z + 0.1568).abs() < 1e-3, "{g:?}");
}
