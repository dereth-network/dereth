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
    let mut b = body(w, start, 0.0);
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
    let v = camera.place_inside(&land, at.cell, at.frame.origin, away);
    let dist =
        |a: Vec3, c: Vec3| ((a.x - c.x).powi(2) + (a.y - c.y).powi(2) + (a.z - c.z).powi(2)).sqrt();
    assert!(dist(look, wanted) > 4.4, "it wants 4.5 m");
    let got = dist(v.look_at, v.eye);
    assert!(got < 4.0, "pulled in from the wall: {got} m");
    let inside = (0x100u32..0x200).any(|k| {
        land.cell(CellId(0x0298_0000 | k)).is_some_and(|g| {
            let l = dereth_physics::math::globaltolocal(&g.frame, v.eye);
            !g.portals.is_empty()
                && g.cell_bsp
                    .as_ref()
                    .is_some_and(|t| t.point_inside_cell_bsp(l))
        })
    });
    assert!(inside, "the camera is inside a hall cell: {:?}", v.eye);
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
/// from the doorway behind it, the camera stands where it wants to, 4.5 m from the top of the body.
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
    let v = camera.place_inside(&land, at.cell, at.frame.origin, 270.0);
    let d = ((v.eye.x - v.look_at.x).powi(2)
        + (v.eye.y - v.look_at.y).powi(2)
        + (v.eye.z - v.look_at.z).powi(2))
    .sqrt();
    assert!((d - 4.5).abs() < 0.01, "4.5 m back: {d} ({v:?})");
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
    // The platform's top is above that place.
    let up = auberean_physics::interior::trace(
        &land,
        cell,
        inside,
        Vec3::new(inside.x, inside.y, inside.z + 2.0),
        true,
    );
    assert!(up.is_some(), "the platform is over the place");
    let camera = Camera::new(CameraSettings::default());
    let (look, _) = camera.wanted(inside, 270.0);
    let v = camera.place_inside(&land, cell, inside, 270.0);
    assert!(
        (v.look_at.z - look.z).abs() < 0.001,
        "looks at the top of the body: {:?} wanted {look:?}",
        v.look_at
    );
    let d = ((v.eye.x - v.look_at.x).powi(2) + (v.eye.y - v.look_at.y).powi(2)).sqrt();
    assert!(d > 3.0, "the camera stands back: {d} m ({v:?})");
}
