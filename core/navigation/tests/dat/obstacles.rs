//! Behaviour: none (checks the planner's obstacles against the physics walk over retail dungeon geometry)
//!
//! The static objects baked into a dungeon's cells are what a route goes around: a planner told
//! to ignore them routes straight through pillars and furniture, and the body walking that route
//! is stopped by them. Doors and other placed objects gate the places they stand on: a closed door
//! is passed with an instruction to open it, a locked one and a solid object are not passed. A low
//! object is stepped over where that is shortest, and a creature standing in the way is walked
//! round once the route knows of it, up a shop's stairs past the people standing in it too.
//! Fixtures: the rooms of pillars and furniture of block `0x0106` and other dungeons, Holtburg's
//! streets (`0xA9B4`) and its scriveners' shop (`0xA9B3`), from the retail dats; the shop's people
//! where the server's world database stands them.

use std::sync::Arc;

use dereth_navigation::{
    DoorState, NavConfig, NoPath, Obstacle, ObstacleKind, SegmentCheck, WaypointAction,
};
use dereth_physics::{CylSphere, SetupGeometry};
use dereth_primitives::{Frame, ObjectId, Position, Quat, Vec3};

use crate::common::*;
use crate::drive::drive;

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

/// A portal standing in the middle of a long straight leg on Holtburg's streets, which the walk is
/// not taking, is walked round with a metre more to spare than a solid post of the same size:
/// touching it would take the body away.
#[test]
fn a_portal_the_walk_is_not_taking_is_walked_round_further_off_than_a_solid() {
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
    let route = d.nav.plan(&from, &to).expect("a route");
    let (a, b) = route
        .waypoints
        .windows(2)
        .map(|w| (w[0].position, w[1].position))
        .find(|(a, b)| dereth_navigation::global_distance(a, b) > 12.0)
        .expect("a long straight leg");
    let (ga, gb) = (dereth_navigation::global(&a), dereth_navigation::global(&b));
    let middle = ground(
        &d,
        (ga.x + gb.x) * 0.5 - f32::from(0xA9_u8) * 192.0,
        (ga.y + gb.y) * 0.5 - f32::from(0xB4_u8) * 192.0,
    );
    // A body a metre round, as a portal's touch reaches.
    let setup = Arc::new(SetupGeometry {
        cyl_spheres: vec![CylSphere {
            low_pt: Vec3::new(0.0, 0.0, 0.0),
            radius: 1.0,
            height: 2.0,
        }],
        ..SetupGeometry::default()
    });
    let nearest = |d: &mut Dungeon, kind: ObstacleKind| {
        let id = ObjectId(0x7A00_0042);
        d.nav.set_obstacle(Obstacle {
            id,
            position: middle,
            setup: Arc::clone(&setup),
            scale: 1.0,
            kind,
        });
        let round = d.nav.plan(&a, &b).expect("a way round");
        d.nav.remove_obstacle(id);
        let m = dereth_navigation::global(&middle);
        round
            .legs
            .iter()
            .flatten()
            .map(|q| {
                let g = dereth_navigation::global(q);
                dereth_primitives::num::math::hypotf(g.x - m.x, g.y - m.y)
            })
            .fold(f32::MAX, f32::min)
    };
    let solid = nearest(&mut d, ObstacleKind::Solid);
    let portal = nearest(&mut d, ObstacleKind::Portal);
    assert!(solid > 1.0, "the solid post is walked round: {solid:.2} m");
    assert!(
        portal >= solid + 0.75,
        "the portal is kept a metre further off: {portal:.2} m against {solid:.2} m"
    );
}

/// Holtburg's scriveners' shop, upstairs and down, with its twelve people standing where the
/// world database puts them: cell, place, heading (a rotation about the vertical) and body.
const SCRIVENERS: [(u32, f32, f32, f32, f32, f32, u32); 12] = [
    (
        0xA9B3_0100,
        182.336,
        87.006,
        116.005,
        -0.0014,
        -1.0,
        0x0200_004E,
    ),
    (
        0xA9B3_0101,
        183.749,
        79.945,
        116.005,
        -0.9303,
        -0.3667,
        0x0200_0001,
    ),
    (
        0xA9B3_0102,
        184.705,
        85.662,
        116.005,
        -0.6406,
        -0.7679,
        0x0200_0001,
    ),
    (
        0xA9B3_0103,
        178.117,
        85.978,
        116.005,
        0.3427,
        -0.9394,
        0x0200_004E,
    ),
    (
        0xA9B3_0103,
        177.444,
        79.747,
        116.005,
        0.8127,
        -0.5827,
        0x0200_196F,
    ),
    (
        0xA9B3_0103,
        175.884,
        81.608,
        116.005,
        0.6811,
        -0.7322,
        0x0200_0001,
    ),
    (
        0xA9B3_0107,
        176.794,
        82.194,
        119.005,
        -0.5143,
        0.8576,
        0x0200_004E,
    ),
    (
        0xA9B3_0107,
        177.083,
        80.469,
        119.005,
        0.9110,
        -0.4125,
        0x0200_004E,
    ),
    (
        0xA9B3_0107,
        178.599,
        79.030,
        119.005,
        0.9890,
        -0.1482,
        0x0200_0001,
    ),
    (
        0xA9B3_0108,
        184.705,
        85.161,
        119.005,
        0.8444,
        0.5358,
        0x0200_0001,
    ),
    (
        0xA9B3_0108,
        184.464,
        86.882,
        119.005,
        -0.5614,
        -0.8276,
        0x0200_004E,
    ),
    (
        0xA9B3_0108,
        182.096,
        88.145,
        119.005,
        -0.2871,
        -0.9579,
        0x0200_004E,
    ),
];

/// Up the scriveners' stairs from the street, by the south door and by the east, with the shop's
/// twelve people standing about them, downstairs and up: the route goes round them, and the body,
/// driven by keys at 60, 20, 10 and 5 frames a second, walks it to the chest upstairs without once
/// being stuck. A planner not told of them, or not minding them, walks the body into them.
#[test]
fn a_shop_s_stairs_are_climbed_round_the_people_standing_in_it_at_every_frame_rate() {
    let mut d = load(&[0xA9B3], true, NavConfig::default());
    for (k, &(cell, x, y, z, w, turn, body)) in SCRIVENERS.iter().enumerate() {
        let id = ObjectId(0x7A9B_3000 + u32::try_from(k).expect("small"));
        let standing = Position::new(
            dereth_primitives::CellId(cell),
            Frame::new(Vec3::new(x, y, z), Quat::new(w, 0.0, 0.0, turn)),
        );
        let setup = setup_of(&store(), body);
        let h = d.world.create(id, Arc::clone(&setup), true);
        assert!(d.world.set_position(h, &standing), "{id:?} stands there");
        d.nav.set_obstacle(Obstacle {
            id,
            position: standing,
            setup,
            scale: 1.0,
            kind: ObstacleKind::Creature,
        });
    }
    let chest = Position::new(
        dereth_primitives::CellId(0xA9B3_0107),
        Frame::new(Vec3::new(178.525, 78.895, 119.055), Quat::IDENTITY),
    );
    for (door, x, y) in [("south", 180.0, 74.0), ("east", 190.0, 84.0)] {
        let street = ground(&d, 0xA9B3, x, y);
        let path = d
            .nav
            .plan_near(&street, &chest, 2.0)
            .unwrap_or_else(|e| panic!("by the {door} door: no route: {e}"));
        for fps in [60.0, 20.0, 10.0, 5.0] {
            let player = Arc::clone(&d.player);
            let Dungeon { world, nav, .. } = &mut d;
            nav.unblock();
            let mut replan = |p: &Position, _: &Position| nav.plan_near(p, &chest, 2.0).ok();
            let r = drive(world, &player, path.clone(), fps, &mut replan);
            let end = r.end.expect("the body");
            assert!(
                r.arrived && r.stuck == 0,
                "by the {door} door at {fps} frames a second: {r:?}"
            );
            assert!(
                end.frame.origin.z > 118.5,
                "by the {door} door at {fps} frames a second: not upstairs, {end:?}"
            );
        }
    }
}
