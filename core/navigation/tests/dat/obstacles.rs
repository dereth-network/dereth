//! Behaviour: none (checks the planner's obstacles against the physics walk over retail dungeon geometry)
//!
//! The static objects baked into a dungeon's cells are what a route goes around: a planner told
//! to ignore them routes straight through pillars and furniture, and the body walking that route
//! is stopped by them. Doors and other placed objects gate the places they stand on: a closed door
//! is passed with an instruction to open it, a locked one and a solid object are not passed. A low
//! object is stepped over where that is shortest, and a creature standing in the way is walked
//! round once the route knows of it. Fixtures: the rooms of pillars and furniture of block
//! `0x0106` and other dungeons, and Holtburg's streets (`0xA9B4`), from the retail dats.

use std::sync::Arc;

use dereth_navigation::{
    DoorState, NavConfig, NoPath, Obstacle, ObstacleKind, SegmentCheck, WaypointAction,
};
use dereth_physics::{CylSphere, SetupGeometry};
use dereth_primitives::{Frame, ObjectId, Position, Quat, Vec3};

use crate::common::*;

/// Which objects a crossing is over.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Over {
    /// Too tall to step onto: blocked whatever the step allowance.
    Tall,
    /// Low enough to step over or onto: blocked when the body may only step 0.3 m, and walkable
    /// with its whole step height.
    Low,
}

/// The configuration that only steps 0.3 m and climbs 0.45 m between places.
fn short_steps() -> NavConfig {
    NavConfig {
        step_allowance: 0.3,
        max_rise: 0.45,
        max_drop: 0.45,
        ..NavConfig::default()
    }
}

/// Places of the region blocked by a static object (as `over` says), on one grid row, with clear
/// floor of the largest walkable set either side: the object stands across the straight line
/// between the two ends.
fn crossings(block: u16, want: usize, over: Over) -> Vec<(Position, Position)> {
    let mut d = dungeon(block);
    let mut short = dungeon_with(block, short_steps());
    let first = d.first_cell();
    let region = d.nav.region(first).expect("loaded");
    let (label, biggest) = largest_component(region);
    // The same floors sampled the same way: the two regions' places correspond one to one.
    let low = short.nav.region(first).expect("loaded");
    assert_eq!(low.place_count(), region.place_count());
    let solid = |j: u32| match over {
        Over::Tall => region.place(j).blocked_by_static,
        Over::Low => low.place(j).blocked_by_static,
    };
    let end = |j: u32| {
        region.place(j).clear
            && label.get(&j) == Some(&biggest)
            && (over == Over::Tall || low.place(j).clear)
    };
    let near = |ix: i32, iy: i32, z: f32| -> Option<u32> {
        region
            .column(ix, iy)
            .find(|j| (region.place(*j).z - z).abs() < 0.3)
    };
    let mut out = Vec::new();
    let mut used = std::collections::HashSet::new();
    for i in region.places() {
        let n = region.place(i);
        if out.len() >= want || !solid(i) || used.contains(&(n.ix / 24, n.iy / 24)) {
            continue;
        }
        // Along each grid axis: the line runs through the object's middle, not past a corner
        // (half a metre either side of it is object too: a sphere passing a corner by less than
        // its radius slides by), and walking either way off the object reaches the ends' floor.
        for (ax, ay) in [(1, 0), (0, 1)] {
            let deep = [-2, 2]
                .iter()
                .all(|k| near(n.ix + ay * k, n.iy + ax * k, n.z).is_some_and(solid));
            if !deep {
                continue;
            }
            let run = |dir: i32| -> Option<(u32, i32)> {
                let mut k = 1;
                loop {
                    let j = near(n.ix + dir * k * ax, n.iy + dir * k * ay, n.z)?;
                    if end(j) {
                        return Some((j, k));
                    }
                    if !solid(j) || k > 24 {
                        return None;
                    }
                    k += 1;
                }
            };
            let (Some((a, kw)), Some((b, ke))) = (run(-1), run(1)) else {
                continue;
            };
            // At least a metre of object on the line.
            if kw + ke < 5 {
                continue;
            }
            used.insert((n.ix / 24, n.iy / 24));
            out.push((at(region, a), at(region, b)));
            break;
        }
    }
    out
}

#[test]
fn a_low_object_across_the_way_is_stepped_over_when_that_is_shortest_and_the_body_walks_it() {
    let mut stepped = 0;
    for block in [0x0106, 0x8602, 0x00D1, 0x01EE, 0x7E03, 0x8704] {
        let pairs = crossings(block, 8, Over::Low);
        let mut d = dungeon(block);
        let mut short = dungeon_with(block, short_steps());
        for (from, to) in &pairs {
            // With its whole step height: straight over the object, and the body takes it.
            let mut w = dereth_navigation::PhysicsWalker::new(
                &mut d.world,
                Arc::clone(&d.player),
                dereth_navigation::WalkerConfig::default(),
            );
            w.keep_trail = true;
            let (route, report) = d.nav.plan_checked(from, to, &mut w, 4);
            let trail = std::mem::take(&mut w.trail);
            drop(w);
            let route = route.expect("a route over the object");
            assert!(report.walked, "{report:?}");
            // Stepping 0.3 m at most: the long way round, or none.
            let round = short.nav.plan(from, to).map(|p| p.length());
            println!(
                "{block:04X}: over {:.1} m in {} legs ({report:?}); round {round:?}",
                route.length(),
                route.waypoints.len() - 1
            );
            // Up onto the object: the body stood higher than the floor at both ends.
            let floor = from.frame.origin.z.max(to.frame.origin.z);
            let top = trail
                .iter()
                .map(|p| p.frame.origin.z)
                .fold(f32::MIN, f32::max);
            println!("  climbs {:.2} m", top - floor);
            if report.failed_legs == 0
                && top - floor > 0.2
                && round
                    .as_ref()
                    .ok()
                    .is_none_or(|r| *r > route.length() + 0.1)
            {
                stepped += 1;
            }
        }
    }
    assert!(stepped >= 3, "only {stepped} low objects were stepped over");
}

#[test]
fn without_the_static_objects_a_route_runs_through_them_and_the_body_collides() {
    let mut crossed = 0;
    let mut collided = 0;
    for block in [0x0106, 0x8602, 0x00D1] {
        let pairs = crossings(block, 3, Over::Tall);
        let mut blind = dungeon_with(
            block,
            NavConfig {
                statics: false,
                ..NavConfig::default()
            },
        );
        let mut seeing = dungeon(block);
        for (from, to) in &pairs {
            crossed += 1;
            // Blind to the objects: one straight leg through the object, which the body walking
            // it collides with: stopped, or pushed off the line.
            let path = blind
                .nav
                .plan(from, to)
                .expect("the floor is open without the objects");
            assert_eq!(path.waypoints.len(), 2, "a straight line across open floor");
            let mut w = blind.walker();
            w.keep_trail = true;
            assert!(w.place(from));
            w.trail.clear();
            let reached = w.walk(to).reached;
            let off = deviation(&w.trail, from.frame.origin, to.frame.origin);
            println!("{block:04X}: blind walk reached {reached}, pushed {off:.2} m off the line");
            if !reached || off > 0.25 {
                collided += 1;
            }
            drop(w);
            // Seeing them: a route around, walked end to end.
            let mut w = dereth_navigation::PhysicsWalker::new(
                &mut seeing.world,
                Arc::clone(&seeing.player),
                dereth_navigation::WalkerConfig::default(),
            );
            let (route, report) = seeing.nav.plan_checked(from, to, &mut w, 4);
            let route = route.expect("a way around");
            assert!(report.walked, "{report:?}");
            assert!(
                route.waypoints.len() > 2,
                "the route bends around the object"
            );
        }
    }
    assert!(
        crossed >= 5,
        "only {crossed} tall objects stand free across a line"
    );
    assert_eq!(
        collided, crossed,
        "every object stops the body or pushes it aside"
    );
}

/// How far across, at most, the body strayed from the straight line `a`..`b`.
fn deviation(trail: &[Position], a: Vec3, b: Vec3) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l = dereth_primitives::num::math::hypotf(dx, dy).max(1e-6);
    trail
        .iter()
        .map(|p| ((p.frame.origin.x - a.x) * dy - (p.frame.origin.y - a.y) * dx).abs() / l)
        .fold(0.0, f32::max)
}

/// A closed, locked, open or solid object standing across the middle of a long route.
fn gate(kind: ObstacleKind) -> (Result<dereth_navigation::NavPath, NoPath>, Vec3) {
    let mut d = dungeon(0x0106);
    let first = d.first_cell();
    let (from, to) = {
        let region = d.nav.region(first).expect("loaded");
        let (a, b) = far_pair(region);
        (at(region, a), at(region, b))
    };
    let open = d
        .nav
        .plan(&from, &to)
        .expect("a route with nothing in the way");
    let places: Vec<Position> = open.legs.iter().flatten().copied().collect();
    let middle = places[places.len() / 2];
    // A post as wide as any corridor here, standing on the route's halfway place.
    let setup = Arc::new(SetupGeometry {
        cyl_spheres: vec![CylSphere {
            low_pt: Vec3::new(0.0, 0.0, 0.0),
            radius: 3.0,
            height: 3.0,
        }],
        ..SetupGeometry::default()
    });
    d.nav.set_obstacle(Obstacle {
        id: ObjectId(0x7A00_0001),
        position: Position::new(middle.cell, Frame::new(middle.frame.origin, Quat::IDENTITY)),
        setup,
        scale: 1.0,
        kind,
    });
    (d.nav.plan(&from, &to), middle.frame.origin)
}

fn passes(path: &dereth_navigation::NavPath, p: Vec3, within: f32) -> bool {
    path.legs.iter().flatten().any(|q| {
        dereth_primitives::num::math::hypotf(q.frame.origin.x - p.x, q.frame.origin.y - p.y)
            < within
    })
}

#[test]
fn a_closed_door_is_passed_with_an_instruction_to_open_it() {
    let (path, door) = gate(ObstacleKind::Door(DoorState::Closed));
    let path = path.expect("a route through the door");
    let opens: Vec<_> = path.waypoints.iter().filter_map(|w| w.action).collect();
    assert_eq!(opens, vec![WaypointAction::OpenDoor(ObjectId(0x7A00_0001))]);
    assert!(passes(&path, door, 1.0));
}

#[test]
fn an_open_door_is_walked_through_with_nothing_to_do() {
    let (path, door) = gate(ObstacleKind::Door(DoorState::Open));
    let path = path.expect("a route through the door");
    assert!(path.waypoints.iter().all(|w| w.action.is_none()));
    assert!(passes(&path, door, 1.0));
}

#[test]
fn a_locked_door_and_a_solid_object_are_never_passed() {
    for kind in [ObstacleKind::Door(DoorState::Locked), ObstacleKind::Solid] {
        let (path, door) = gate(kind);
        match path {
            // No walk past it; a drop elsewhere is no route either.
            Err(NoPath::Unreachable | NoPath::OnlyByDrop { .. }) => {}
            Ok(p) => assert!(!passes(&p, door, 3.0), "{kind:?}: the route goes through"),
            Err(e) => panic!("{kind:?}: {e}"),
        }
    }
}

/// Creatures standing on the straight line between two places: a planner that does not know
/// them sends the body into them; told of them, it goes round, and the body walks it.
#[test]
fn a_creature_standing_in_the_way_is_walked_round_when_the_route_knows_it() {
    // Holtburg's open streets: long straight legs.
    let mut d = outdoors(0xA9B4, 1);
    let ground = |d: &Dungeon, x: f32, y: f32| {
        let z = d
            .nav
            .source()
            .land
            .ground_height(dereth_primitives::LandblockId(0xA9B4), x, y)
            .expect("terrain");
        let mut c = dereth_primitives::CellId(0xA9B4_0001);
        let mut o = Vec3::new(x, y, z);
        assert!(dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o));
        Position::new(c, Frame::new(o, Quat::IDENTITY))
    };
    let (from, to) = (ground(&d, 20.0, 20.0), ground(&d, 170.0, 170.0));
    // The long straight legs of the route, each with a creature standing at its middle.
    let route = d.nav.plan(&from, &to).expect("a route");
    let legs: Vec<(Position, Position)> = route
        .waypoints
        .windows(2)
        .map(|w| (w[0].position, w[1].position))
        .filter(|(a, b)| dereth_navigation::global_distance(a, b) > 8.0)
        .take(4)
        .collect();
    assert!(legs.len() >= 3, "the route has long straight legs");
    let mut bumped = 0;
    for (k, (a, b)) in legs.iter().enumerate() {
        let id = ObjectId(0x8000_1000 + u32::try_from(k).expect("small"));
        // Halfway along the leg, on the ground.
        let (ga, gb) = (dereth_navigation::global(a), dereth_navigation::global(b));
        let standing = ground(
            &d,
            (ga.x + gb.x) * 0.5 - f32::from(0xA9_u8) * 192.0,
            (ga.y + gb.y) * 0.5 - f32::from(0xB4_u8) * 192.0,
        );
        // The creature's body in the world: a human, standing still.
        let creature = d.world.create(id, Arc::clone(&d.player), true);
        assert!(
            d.world.set_position(creature, &standing),
            "the creature stands there"
        );
        // Not told: straight at it.
        let mut w = d.walker();
        assert!(w.place(a));
        let straight = w.walk(b);
        drop(w);
        let blocked =
            !straight.reached || straight.distance > dereth_navigation::global_distance(a, b) + 0.5;
        // Told: round it.
        d.nav.set_obstacle(Obstacle {
            id,
            position: standing,
            setup: Arc::clone(&d.player),
            scale: 1.0,
            kind: ObstacleKind::Creature,
        });
        let mut w = dereth_navigation::PhysicsWalker::new(
            &mut d.world,
            Arc::clone(&d.player),
            dereth_navigation::WalkerConfig::default(),
        );
        let (round, report) = d.nav.plan_checked(a, b, &mut w, 4);
        drop(w);
        let round = round.expect("a way round the creature");
        println!(
            "leg {k}: straight reached {} in {:.1} m; round {:.1} m in {} legs, {report:?}",
            straight.reached,
            straight.distance,
            round.length(),
            round.waypoints.len() - 1
        );
        assert!(report.walked && report.failed_legs == 0, "{report:?}");
        let clear_of = round.legs.iter().flatten().all(|q| {
            let (g, m) = (
                dereth_navigation::global(q),
                dereth_navigation::global(&standing),
            );
            dereth_primitives::num::math::hypotf(g.x - m.x, g.y - m.y) > 0.9
        });
        assert!(clear_of, "the route keeps clear of the creature");
        if blocked {
            bumped += 1;
        }
        d.nav.remove_obstacle(id);
        d.world.destroy(creature);
    }
    assert_eq!(
        bumped,
        legs.len(),
        "every creature is in the way of a body walking straight"
    );
}
