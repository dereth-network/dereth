//! Behaviour: none (prints the planner's cost over retail dungeons)
//!
//! For each dungeon: the time to build its region, the places and bytes it holds, the time to plan
//! between the two ends of its largest walkable area, and the time to walk that route through the
//! physics world. Run with `--ignored --nocapture`.

use std::sync::Arc;
use std::time::Instant;

use crate::common::*;

#[test]
#[ignore = "instrument: prints build and plan cost per dungeon"]
fn region_build_and_plan_cost_per_dungeon() {
    println!(
        "| block | cells | places | clear | build ms | bytes | route m | legs | plan ms | walk ms |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for block in [0x010D_u16, 0x01EE, 0x0106, 0x00D1, 0x8602, 0x7E03] {
        let mut d = dungeon(block);
        let first = d.first_cell();
        let t = Instant::now();
        d.nav.region(first).expect("loaded");
        let build = t.elapsed();
        let (cells, places, clear, bytes, from, to) = {
            let region = d.nav.region(first).expect("built");
            let r = region.report();
            let (a, b) = far_pair(region);
            (
                r.cells,
                r.nodes,
                r.clear_nodes,
                region.memory_bytes(),
                at(region, a),
                at(region, b),
            )
        };
        let t = Instant::now();
        let runs = 5;
        let mut path = None;
        for _ in 0..runs {
            path = Some(d.nav.plan(&from, &to).expect("a route"));
        }
        let plan = t.elapsed() / runs;
        let path = path.expect("planned");
        let t = Instant::now();
        let mut w = dereth_navigation::PhysicsWalker::new(
            &mut d.world,
            Arc::clone(&d.player),
            dereth_navigation::WalkerConfig::default(),
        );
        let (_, report) = d.nav.plan_checked(&from, &to, &mut w, 4);
        let walk = t.elapsed();
        assert!(report.walked);
        println!(
            "  from {:08X} {:?} to {:08X} {:?}",
            from.cell.0, from.frame.origin, to.cell.0, to.frame.origin
        );
        println!(
            "| {block:04X} | {cells} | {places} | {clear} | {:.1} | {bytes} | {:.1} | {} | {:.2} | {:.0} |",
            build.as_secs_f64() * 1e3,
            path.length(),
            path.waypoints.len() - 1,
            plan.as_secs_f64() * 1e3,
            walk.as_secs_f64() * 1e3,
        );
    }
}

/// A point on the ground of `block`.
fn ground(d: &Dungeon, block: u16, x: f32, y: f32) -> dereth_primitives::Position {
    let z = d
        .nav
        .source()
        .land
        .ground_height(dereth_primitives::LandblockId(block), x, y)
        .expect("terrain");
    let mut c = dereth_primitives::CellId((u32::from(block) << 16) | 1);
    let mut o = dereth_primitives::Vec3::new(x, y, z);
    dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o);
    dereth_primitives::Position::new(c, dereth_primitives::Frame::new(o, Default::default()))
}

#[test]
#[ignore = "instrument: prints build and plan cost per outdoor window"]
fn region_build_and_plan_cost_per_outdoor_window() {
    println!(
        "| scene | landblocks | places | clear | indoor places | build ms | bytes | route m | legs | plan ms |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    type Scene = (&'static str, u16, (f32, f32), u16, (f32, f32));
    let scenes: [Scene; 4] = [
        ("Holtburg", 0xA9B4, (20.0, 20.0), 0xA9B4, (170.0, 170.0)),
        (
            "two landblocks",
            0xA9B4,
            (150.0, 60.0),
            0xA9B3,
            (150.0, 120.0),
        ),
        ("forest", 0xAEB7, (10.0, 96.0), 0xAEB7, (182.0, 96.0)),
        ("hills", 0xB5BF, (20.0, 96.0), 0xB5BF, (170.0, 96.0)),
    ];
    for (name, a, (ax, ay), b, (bx, by)) in scenes {
        let mut d = outdoors(a, 1);
        let from = ground(&d, a, ax, ay);
        let to = ground(&d, b, bx, by);
        let t = Instant::now();
        let (r, bytes) = {
            let region = d.nav.region_between(&from, &to).expect("built");
            (region.report().clone(), region.memory_bytes())
        };
        let build = t.elapsed();
        let t = Instant::now();
        let path = d.nav.plan(&from, &to);
        let plan = t.elapsed();
        let (length, legs) = path
            .as_ref()
            .map_or((0.0, 0), |p| (p.length(), p.waypoints.len() - 1));
        println!(
            "| {name} | {} | {} | {} | {} | {:.0} | {bytes} | {length:.1} | {legs} | {:.1} |",
            r.landblocks,
            r.ground + r.nodes,
            r.clear_ground + r.clear_nodes,
            r.nodes,
            build.as_secs_f64() * 1e3,
            plan.as_secs_f64() * 1e3,
        );
    }
}

#[test]
#[ignore = "instrument: prints journey planning cost over a world's worth of portals"]
fn journey_planning_cost_over_a_world_of_portals() {
    use dereth_navigation::{
        land_realm, plan_journey, EdgeSource, JourneyConfig, PortalEdge, PortalStore, Realm,
    };
    use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};
    let at = |cell: u32, x: f32, y: f32| {
        Position::new(
            CellId(cell),
            Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY),
        )
    };
    // As many portals as a full world places, standing on land across the map and sending into
    // dungeons, one in ten of them a way back out of a dungeon.
    let mut seed = 0x2545_f491_u32;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    let mut edges = Vec::new();
    for i in 0..4500_u32 {
        let block = ((next() % 0xFE) << 24) | ((next() % 0xFE) << 16);
        let dungeon = ((0x0100 + i % 1500) << 16) | 0x0100;
        let (from, to) = if i % 10 == 0 {
            (at(dungeon, 10.0, 10.0), at(block | 0x0011, 50.0, 50.0))
        } else {
            (at(block | 0x0011, 40.0, 40.0), at(dungeon, 0.0, 0.0))
        };
        edges.push(PortalEdge {
            source: EdgeSource::Hint,
            wcid: Some(i),
            name: format!("Portal {i}"),
            from,
            to: Some(to),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
        });
    }
    let mut store = PortalStore::new("synthetic");
    store.merge(edges);
    let realm = |p: &Position| {
        land_realm(p.cell).or_else(|| u16::try_from(p.cell.0 >> 16).ok().map(Realm::Dungeon))
    };
    let start = at(0xA9B4_0011, 50.0, 50.0);
    let reached = store.portals()[1].to.expect("a destination");
    for (what, goal) in [
        ("a dungeon a portal reaches", reached),
        ("a dungeon nothing reaches", at(0xFFF0_0100, 0.0, 0.0)),
    ] {
        let t = Instant::now();
        let routable = store.routable(None);
        let pick = t.elapsed();
        let t = Instant::now();
        let j = plan_journey(
            &start,
            &goal,
            &routable,
            &[],
            &realm,
            &JourneyConfig::default(),
        );
        let plan = t.elapsed();
        println!(
            "{what}: {} portal edges, choosing {:.1} ms, planning {:.1} ms, {}",
            routable.len(),
            pick.as_secs_f64() * 1e3,
            plan.as_secs_f64() * 1e3,
            j.map_or_else(
                || String::from("no journey"),
                |j| format!("{} legs", j.legs.len())
            )
        );
    }
}
