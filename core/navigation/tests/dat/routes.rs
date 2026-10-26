//! Behaviour: none (checks the planner's routes against the physics walk over retail dungeon geometry)
//!
//! Routes between the two ends of each dungeon's largest walkable area, planned over the retail
//! cells and statics and then walked leg by leg by a human body through the physics world, which
//! holds the same cells and every static object as a body. Dungeons of five shapes: six cells; a
//! many-level one with stairs and ramps; rooms of pillars and furniture; a maze of over a thousand
//! cells; a long winding one. A route is only accepted when the body walks every leg of it.

use dereth_navigation::{svg, NavPath, NoPath};
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::*;

/// What one far route did.
struct Route {
    path: NavPath,
    straight: f32,
    walked_to: Position,
}

/// Plan between the two ends of the largest walkable area of `block`, walk it, and keep a picture.
fn far_route(block: u16) -> Route {
    let mut d = dungeon(block);
    let first = d.first_cell();
    let (from, to) = {
        let region = d.nav.region(first).expect("the first cell is loaded");
        let (a, b) = far_pair(region);
        (at(region, a), at(region, b))
    };
    let mut walker = dereth_navigation::PhysicsWalker::new(
        &mut d.world,
        std::sync::Arc::clone(&d.player),
        dereth_navigation::WalkerConfig::default(),
    );
    walker.keep_trail = true;
    let (path, report) = d.nav.plan_checked(&from, &to, &mut walker, 4);
    let trail = walker.trail.clone();
    drop(walker);
    let path = path.unwrap_or_else(|e| panic!("{block:04X}: no route: {e} ({report:?})"));
    assert!(
        report.walked,
        "{block:04X}: the body did not walk the route: {report:?}"
    );
    let region = d.nav.region(first).expect("built");
    std::fs::write(
        out_dir().join(format!("route-{block:04X}.svg")),
        svg(region, Some(&path), &trail),
    )
    .expect("the picture is written");
    let walked_to = *trail.last().expect("the body moved");
    let last = path.waypoints.last().expect("a route has an end").position;
    let gap = dereth_primitives::num::math::hypotf(
        walked_to.frame.origin.x - last.frame.origin.x,
        walked_to.frame.origin.y - last.frame.origin.y,
    );
    assert!(gap <= 0.25, "{block:04X}: the body stopped {gap} m short");
    println!(
        "{block:04X}: {:.1} m in {} legs over {} cells; {report:?}",
        path.length(),
        path.waypoints.len() - 1,
        path.cells().len()
    );
    Route {
        straight: dist(from.frame.origin, to.frame.origin),
        path,
        walked_to,
    }
}

fn dist(a: Vec3, b: Vec3) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2) + (a.z - b.z).powi(2)).sqrt()
}

/// The route's lowest and highest floor.
fn height_span(path: &NavPath) -> f32 {
    let zs = path.legs.iter().flatten().map(|p| p.frame.origin.z);
    let lo = zs.clone().fold(f32::MAX, f32::min);
    let hi = zs.fold(f32::MIN, f32::max);
    hi - lo
}

#[test]
fn a_route_across_a_six_cell_dungeon_is_walked_end_to_end() {
    let r = far_route(0x010D);
    assert!(r.path.length() >= r.straight);
    assert!(r.path.length() > 5.0, "{}", r.path.length());
    assert_eq!(r.walked_to.cell.0 >> 16, 0x010D);
}

#[test]
fn a_route_that_climbs_between_levels_is_walked_end_to_end() {
    let r = far_route(0x01EE);
    let climb = height_span(&r.path);
    assert!(climb >= 6.0, "the route changes level by {climb} m");
    assert!(r.path.cells().len() >= 20);
}

#[test]
fn a_route_through_rooms_of_pillars_and_furniture_is_walked_end_to_end() {
    let r = far_route(0x0106);
    assert!(r.path.length() > 150.0, "{}", r.path.length());
    assert!(r.path.cells().len() >= 30);
}

#[test]
fn a_route_across_a_maze_of_a_thousand_cells_is_walked_end_to_end() {
    let r = far_route(0x00D1);
    assert!(r.path.length() > 300.0, "{}", r.path.length());
    assert!(r.path.cells().len() >= 100, "{}", r.path.cells().len());
}

#[test]
fn a_long_winding_route_is_walked_end_to_end() {
    let r = far_route(0x8602);
    assert!(r.path.length() > 150.0, "{}", r.path.length());
}

#[test]
fn a_floor_no_stair_reaches_has_no_route_and_another_dungeon_or_the_outdoors_is_disconnected() {
    let mut d = dungeon(0x0106);
    let first = d.first_cell();
    let (from, cut_off) = {
        let region = d.nav.region(first).expect("loaded");
        let (label, biggest) = largest_component(region);
        let from = region
            .places()
            .find(|i| label.get(i) == Some(&biggest))
            .expect("a member");
        // A clear place outside the largest set: a floor walled off from it, or above it with no
        // stair (the cap over a vaulted room is one).
        let other = region
            .places()
            .find(|i| label.get(i).is_some_and(|l| *l != biggest))
            .expect("the dungeon has a floor no route reaches");
        (at(region, from), at(region, other))
    };
    assert_eq!(d.nav.plan(&from, &cut_off), Err(NoPath::Unreachable));
    let elsewhere = Position::new(
        CellId(0x010D_0100),
        Frame::new(Vec3::new(0.0, 0.0, 0.0), Quat::IDENTITY),
    );
    assert!(matches!(
        d.nav.plan(&from, &elsewhere),
        Err(NoPath::Disconnected { .. })
    ));
    let outdoors = Position::new(
        CellId(0x0106_0001),
        Frame::new(Vec3::new(10.0, 10.0, 0.0), Quat::IDENTITY),
    );
    // A dungeon is left by a portal, not on foot.
    assert!(matches!(
        d.nav.plan(&from, &outdoors),
        Err(NoPath::Disconnected { .. })
    ));
}
