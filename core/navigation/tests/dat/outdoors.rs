//! Behaviour: none (checks the planner's outdoor routes against the physics walk over retail landblocks)
//!
//! Outdoor routes over a window of landblocks: terrain, buildings, scenery and landblock statics,
//! and the interiors the buildings open onto, planned and then walked by a human body through the
//! physics world holding the same landblocks and every outdoor object as a body. Scenes: Holtburg
//! (`0xA9B4`), its street and its buildings' insides; a crossing into the next landblock; hills
//! with slopes too steep to stand on (`0xB5BF`); the stream west of Holtburg (`0xA7B4`); and the
//! forest at `0xAEB7`.

use dereth_navigation::{svg, CheckReport, NavConfig, NavPath, NoPath, Place, Region};
use dereth_primitives::{CellId, Frame, Position};

use crate::common::*;

/// Plan from `from` to `to`, walk it, keep a picture as `name`, and return the route, what the
/// walk did and where the body stood.
fn walk(
    d: &mut Dungeon,
    from: &Position,
    to: &Position,
    name: &str,
) -> (Result<NavPath, NoPath>, CheckReport, Vec<Position>) {
    let mut w = dereth_navigation::PhysicsWalker::new(
        &mut d.world,
        std::sync::Arc::clone(&d.player),
        dereth_navigation::WalkerConfig::default(),
    );
    w.keep_trail = true;
    let (route, report) = d.nav.plan_checked(from, to, &mut w, 6);
    let trail = std::mem::take(&mut w.trail);
    drop(w);
    if let Ok(region) = d.nav.region_between(from, to) {
        std::fs::write(
            out_dir().join(format!("{name}.svg")),
            svg(region, route.as_ref().ok(), &trail),
        )
        .expect("the picture is written");
    }
    if let Ok(r) = &route {
        println!(
            "{name}: {:.1} m in {} legs over {} cells; {report:?}",
            r.length(),
            r.waypoints.len() - 1,
            r.cells().len()
        );
    }
    (route, report, trail)
}

/// The terrain place of a grid column, if the column has terrain.
fn ground_place(region: &Region, ix: i32, iy: i32) -> Option<(u32, Place)> {
    region
        .column(ix, iy)
        .map(|i| (i, region.place(i)))
        .find(|(_, p)| p.ground)
}

/// Straight lines across terrain that `what` says is in the way: a run of such places along one
/// grid axis, at least two metres of it and as wide again either side, with clear terrain at both
/// ends within `reach` places. At most one per 32 m square of `block`.
fn crossings(
    region: &Region,
    block: u16,
    what: impl Fn(&Place) -> bool,
    want: usize,
    reach: i32,
) -> Vec<(Position, Position)> {
    let s = region.spacing();
    let per = dereth_primitives::num::floor_to_i32(192.0 / s);
    // The block's columns in region space.
    let o = region.to_region(&Position::new(
        CellId((u32::from(block) << 16) | 1),
        Frame::default(),
    ));
    let (x0, y0) = (
        dereth_primitives::num::floor_to_i32(o.x / s),
        dereth_primitives::num::floor_to_i32(o.y / s),
    );
    let mut out = Vec::new();
    let mut used = std::collections::HashSet::new();
    for iy in (y0..y0 + per).step_by(4) {
        for ix in (x0..x0 + per).step_by(4) {
            if out.len() >= want || used.contains(&(ix / 128, iy / 128)) {
                continue;
            }
            let Some((_, here)) = ground_place(region, ix, iy) else {
                continue;
            };
            if !what(&here) {
                continue;
            }
            for (ax, ay) in [(1, 0), (0, 1)] {
                let deep = [-8, 8].iter().all(|k| {
                    ground_place(region, ix + ay * k, iy + ax * k).is_some_and(|(_, p)| what(&p))
                });
                if !deep {
                    continue;
                }
                let run = |dir: i32| -> Option<(u32, i32)> {
                    for k in 1..reach {
                        let (i, p) = ground_place(region, ix + dir * k * ax, iy + dir * k * ay)?;
                        if p.clear {
                            return Some((i, k));
                        }
                        if !what(&p) && !p.blocked_by_static {
                            return None;
                        }
                    }
                    None
                };
                let (Some((a, ka)), Some((b, kb))) = (run(-1), run(1)) else {
                    continue;
                };
                if ka + kb < 8 {
                    continue;
                }
                used.insert((ix / 128, iy / 128));
                out.push((region.position(a), region.position(b)));
                break;
            }
        }
    }
    out
}

/// How far across, at most, the trail strays from the straight line `a`..`b`.
fn deviation(trail: &[Position], a: &Position, b: &Position) -> f32 {
    let (a, b) = (dereth_navigation::global(a), dereth_navigation::global(b));
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l = dereth_primitives::num::math::hypotf(dx, dy).max(1e-6);
    trail
        .iter()
        .map(|p| {
            let q = dereth_navigation::global(p);
            ((q.x - a.x) * dy - (q.y - a.y) * dx).abs() / l
        })
        .fold(0.0, f32::max)
}

#[test]
fn a_route_across_holtburg_threads_the_streets_round_the_buildings_and_is_walked() {
    let mut d = outdoors(0xA9B4, 1);
    let from = ground(&d, 0xA9B4, 20.0, 20.0);
    let to = ground(&d, 0xA9B4, 170.0, 170.0);
    let (route, report, _) = walk(&mut d, &from, &to, "holtburg");
    let route = route.expect("a route across town");
    assert!(report.walked, "{report:?}");
    assert!(
        route.waypoints.len() > 2,
        "the route bends round the buildings"
    );
    let region = d.nav.region_between(&from, &to).expect("built");
    let r = region.report();
    assert!(r.buildings >= 10, "{r:?}");
    assert!(
        region.memory_bytes() < 16 << 20,
        "{} bytes",
        region.memory_bytes()
    );
}

#[test]
fn a_route_from_the_street_into_a_building_and_from_inside_back_out_is_walked() {
    let mut d = outdoors(0xA9B4, 1);
    let street = ground(&d, 0xA9B4, 96.0, 40.0);
    // Clear places inside the town's buildings, on the ground floor, farthest first.
    let insides: Vec<Position> = {
        let region = d.nav.region(street.cell).expect("built");
        let terrain = region.to_region(&street).z;
        let mut v: Vec<Position> = region
            .places()
            .take(region.report().nodes)
            .map(|i| (i, region.place(i)))
            .filter(|(_, p)| p.clear && p.cell.0 >> 16 == 0xA9B4 && p.cell.0 & 0xFFFF >= 0x0100)
            .filter(|(_, p)| (p.z - terrain).abs() < 3.0)
            .map(|(i, _)| region.position(i))
            .filter(|p| dereth_navigation::global_distance(p, &street) < 80.0)
            .collect();
        v.sort_by(|a, b| {
            let da = dereth_navigation::global_distance(a, &street);
            let db = dereth_navigation::global_distance(b, &street);
            db.total_cmp(&da)
        });
        v
    };
    assert!(!insides.is_empty(), "Holtburg has buildings to walk into");
    // The farthest one a route reaches (a room may be shut off, or upstairs).
    let inside = insides
        .iter()
        .step_by(insides.len() / 20 + 1)
        .find(|p| d.nav.plan(&street, p).is_ok_and(|r| r.length() > 20.0))
        .copied()
        .expect("a building a route goes into");
    let (route, report, _) = walk(&mut d, &street, &inside, "holtburg-in");
    let route = route.expect("a route indoors");
    assert!(report.walked, "{report:?}");
    let cells = route.cells();
    assert!(
        cells.iter().any(|c| c.0 & 0xFFFF < 0x0100),
        "starts outside"
    );
    assert!(cells.iter().any(|c| c.0 & 0xFFFF >= 0x0100), "ends inside");
    let out = ground(&d, 0xA9B4, 150.0, 20.0);
    let (route, report, _) = walk(&mut d, &inside, &out, "holtburg-out");
    assert!(route.is_ok() && report.walked, "{report:?}");
}

#[test]
fn a_route_into_the_next_landblock_crosses_the_edge_and_is_walked() {
    let mut d = outdoors(0xA9B4, 1);
    let from = ground(&d, 0xA9B4, 150.0, 60.0);
    let to = ground(&d, 0xA9B3, 150.0, 120.0);
    let (route, report, trail) = walk(&mut d, &from, &to, "edge");
    let route = route.expect("a route across the edge");
    assert!(report.walked, "{report:?}");
    let blocks: std::collections::BTreeSet<u32> = route.cells().iter().map(|c| c.0 >> 16).collect();
    assert!(
        blocks.contains(&0xA9B4) && blocks.contains(&0xA9B3),
        "{blocks:X?}"
    );
    assert!(
        trail.iter().any(|p| p.cell.0 >> 16 == 0xA9B3),
        "the body crossed"
    );
}

#[test]
fn slopes_too_steep_to_stand_on_are_walked_round() {
    let mut d = outdoors(0xB5BF, 1);
    let pairs = {
        let region = d.nav.region(CellId(0xB5BF_0001)).expect("built");
        assert!(region.report().steep_ground > 0, "{:?}", region.report());
        crossings(region, 0xB5BF, |p| p.steep, 4, 160)
    };
    assert!(
        !pairs.is_empty(),
        "the hills have a steep slope with flat ground either side"
    );
    let mut detoured = 0;
    let mut balked = 0;
    for (k, (from, to)) in pairs.iter().enumerate() {
        // Straight at the slope: the body does not get across, or is turned aside.
        let mut w = d.walker();
        w.keep_trail = true;
        assert!(dereth_navigation::SegmentCheck::place(&mut w, from));
        w.trail.clear();
        let reached = dereth_navigation::SegmentCheck::walk(&mut w, to).reached;
        let off = deviation(&w.trail, from, to);
        drop(w);
        println!("steep-{k}: straight walk reached {reached}, pushed {off:.2} m off the line");
        if !reached || off > 0.5 {
            balked += 1;
        }
        let (route, report, _) = walk(&mut d, from, to, &format!("steep-{k}"));
        let Ok(route) = route else { continue };
        assert!(report.walked, "{report:?}");
        let straight = dereth_navigation::global_distance(from, to);
        if route.length() > straight + 1.0 {
            detoured += 1;
        }
    }
    assert!(detoured >= 1, "no route went round a slope");
    assert!(
        balked >= 1,
        "no slope stopped a body walking straight at it"
    );
}

#[test]
fn water_is_waded_only_where_that_is_clearly_shorter_than_the_way_round() {
    let dry_config = NavConfig {
        wade: false,
        ..NavConfig::default()
    };
    let mut dry = outdoors_with(0xA7B4, 1, dry_config);
    let pairs = {
        let region = dry.nav.region(CellId(0xA7B4_0001)).expect("built");
        assert!(region.report().water_ground > 0, "{:?}", region.report());
        crossings(region, 0xA7B4, |p| p.water, 8, 160)
    };
    assert!(!pairs.is_empty(), "the stream has dry ground either side");
    let mut d = outdoors(0xA7B4, 1);
    // How far a route goes through water.
    let waded = |p: &NavPath, n: &mut Dungeon, from: &Position, to: &Position| -> f32 {
        let region = n.nav.region_between(from, to).expect("built");
        let wet = p
            .legs
            .iter()
            .flatten()
            .filter(|q| {
                let r = region.to_region(q);
                let (ix, iy) = (
                    dereth_primitives::num::to_i32((r.x / region.spacing()).round()),
                    dereth_primitives::num::to_i32((r.y / region.spacing()).round()),
                );
                ground_place(region, ix, iy).is_some_and(|(_, g)| g.water)
            })
            .count();
        f32::from(u16::try_from(wet).unwrap_or(u16::MAX)) * region.spacing()
    };
    let (mut waded_routes, mut dry_routes) = (0, 0);
    for (k, (from, to)) in pairs.iter().enumerate() {
        let round = dry.nav.plan(from, to).ok().map(|p| p.length());
        let route = d.nav.plan(from, to).expect("wading, the way is open");
        let wet = waded(&route, &mut d, from, to);
        println!(
            "water-{k}: chosen {:.1} m, {wet:.1} m of it wading; round {round:?}",
            route.length()
        );
        // Never longer than the dry way round, and through the water only when that is
        // shorter by more than the metres waded.
        if let Some(round) = round {
            assert!(
                route.length() <= round + 0.5,
                "{} vs {round}",
                route.length()
            );
            if wet > 0.0 {
                assert!(
                    route.length() + wet <= round + 1.0,
                    "wading was not clearly shorter"
                );
            }
        }
        if wet > 0.0 {
            waded_routes += 1;
        } else {
            dry_routes += 1;
        }
        let (walked, report, _) = walk(&mut d, from, to, &format!("water-{k}"));
        assert!(walked.is_ok() && report.walked, "{report:?}");
    }
    assert!(waded_routes >= 1, "no route waded");
    println!("{waded_routes} routes waded, {dry_routes} went round");
}

#[test]
fn routes_through_the_forest_go_round_the_trees_that_stop_a_body_walking_straight() {
    let mut d = outdoors(0xAEB7, 1);
    let pairs = {
        let region = d.nav.region(CellId(0xAEB7_0001)).expect("built");
        assert!(region.report().statics > 500, "{:?}", region.report());
        crossings(region, 0xAEB7, |p| p.blocked_by_static, 4, 80)
    };
    assert!(
        pairs.len() >= 3,
        "the forest has trees standing free across a line"
    );
    let mut blind = outdoors_with(
        0xAEB7,
        1,
        NavConfig {
            statics: false,
            ..NavConfig::default()
        },
    );
    let mut collided = 0;
    for (k, (from, to)) in pairs.iter().enumerate() {
        // Blind to the trees: straight through, and the body meets the trunk.
        let line = blind
            .nav
            .plan(from, to)
            .expect("open ground without the trees");
        assert_eq!(line.waypoints.len(), 2, "a straight line");
        let mut w = blind.walker();
        w.keep_trail = true;
        assert!(dereth_navigation::SegmentCheck::place(&mut w, from));
        w.trail.clear();
        let reached = dereth_navigation::SegmentCheck::walk(&mut w, to).reached;
        let off = deviation(&w.trail, from, to);
        drop(w);
        println!("forest-{k}: blind walk reached {reached}, pushed {off:.2} m off the line");
        if !reached || off > 0.25 {
            collided += 1;
        }
        // Seeing them: round the tree, walked end to end.
        let (route, report, _) = walk(&mut d, from, to, &format!("forest-{k}"));
        let route = route.expect("a way round the tree");
        assert!(report.walked, "{report:?}");
        assert!(route.waypoints.len() > 2, "the route bends round the tree");
    }
    assert_eq!(
        collided,
        pairs.len(),
        "every tree stops the body or pushes it aside"
    );
}

#[test]
fn a_route_seven_landblocks_long_is_walked_one_window_at_a_time() {
    let mut blocks = Vec::new();
    for x in 0xA8_u16..=0xB1 {
        for y in 0xB3_u16..=0xB5 {
            blocks.push((x << 8) | y);
        }
    }
    blocks.sort_by_key(|b| u16::from(*b != 0xA9B4));
    let mut d = load(&blocks, true, NavConfig::default());
    let from = ground(&d, 0xA9B4, 96.0, 96.0);
    let to = ground(&d, 0xB0B4, 96.0, 96.0);
    let corridor = d
        .nav
        .corridor((0xA9, 0xB4), (0xB0, 0xB4))
        .expect("the land joins them");
    println!("corridor: {} landblocks", corridor.len());
    let mut here = from;
    let mut walked = 0.0;
    let mut hops = 0;
    loop {
        hops += 1;
        assert!(hops <= 8, "too many windows");
        let (path, last) = d
            .nav
            .plan_toward(&here, &to)
            .expect("a route toward the goal");
        let end = path.waypoints.last().expect("an end").position;
        let (route, report, trail) = walk(&mut d, &here, &end, &format!("long-{hops}"));
        assert!(route.is_ok() && report.walked, "hop {hops}: {report:?}");
        walked += report.distance;
        here = *trail.last().expect("the body moved");
        if last {
            break;
        }
    }
    let gap = dereth_navigation::global_distance(&here, &to);
    println!("long: {hops} windows, {walked:.0} m walked, ends {gap:.2} m from the goal");
    assert!(hops >= 2, "it took more than one window");
    assert!(gap < 1.0, "the body reached the goal");
}

/// A route planned a second time is planned as the first was. The first, too long for one window,
/// builds a window along the way and ends at its far edge; that window covers the goal (a room in
/// a Holtburg shop) only with its edge, so the second is not searched for in it, where the way to
/// the room runs outside it, but planned along the way again. (From where the Holtburg Redoubt's
/// Surface portal sends a character, to Shopkeeper Renald the Elder's shop.)
#[test]
fn a_route_planned_again_from_the_same_place_is_not_cut_short_by_the_first_ones_window() {
    let mut d = outdoors(0xAAB2, 2);
    let landing = Position::new(
        CellId(0xAAB1_0020),
        Frame::new(
            dereth_primitives::Vec3::new(95.6, 174.5, 123.97),
            dereth_primitives::Quat::IDENTITY,
        ),
    );
    let shop = Position::new(
        CellId(0xA9B4_0141),
        Frame::new(
            dereth_primitives::Vec3::new(139.2, 18.3, 94.0),
            dereth_primitives::Quat::IDENTITY,
        ),
    );
    let (first, whole) = d.nav.plan_toward(&landing, &shop).expect("toward the shop");
    assert!(!whole, "too far for one window: to its edge first");
    let (second, _) = d
        .nav
        .plan_toward(&landing, &shop)
        .expect("planned again, toward the shop again");
    assert_eq!(
        second.waypoints.last().map(|w| w.position.cell),
        first.waypoints.last().map(|w| w.position.cell),
        "the same way as the first"
    );
}
