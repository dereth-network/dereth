//! The navigation mesh over retail dungeons and outdoors. A dungeon of many levels, its stairs and
//! ramps, sampled into standing places and made into convex polygons, its longest walk searched a
//! polygon at a time and walked by a human body through the physics world; the mesh joining
//! exactly the places a body steps between; and outdoor windows put together from landblock
//! tiles, a walk that goes on into the next window building only the landblocks new to it, and the
//! tiles kept between sessions read back rather than built again.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use dereth_navigation::TileStore;

use crate::common::*;

/// Behaviour: plugins.nav.routes-are-searched-a-polygon-of-the-navigation-mesh-at-a-time
#[test]
fn a_dungeons_places_make_polygons_a_hundredth_as_many_and_its_longest_walk_is_walked() {
    let mut d = dungeon(0x01EE);
    let first = d.first_cell();
    let (from, to, polygons, clear) = {
        let region = d.nav.region(first).expect("the first cell is loaded");
        let (a, b) = far_pair(region);
        let r = region.report();
        (at(region, a), at(region, b), r.polygons, r.clear_nodes)
    };
    assert!(
        polygons > 0 && polygons * 100 <= clear,
        "{polygons} polygons for {clear} clear places"
    );
    let mut walker = dereth_navigation::PhysicsWalker::new(
        &mut d.world,
        std::sync::Arc::clone(&d.player),
        dereth_navigation::WalkerConfig::default(),
    );
    let (path, report) = d.nav.plan_checked(&from, &to, &mut walker, 4);
    let path = path.unwrap_or_else(|e| panic!("no route: {e} ({report:?})"));
    assert!(report.walked, "the body did not walk the route: {report:?}");
    // About 990 m, stepping across the narrow gaps between floors on the way.
    assert!(
        path.length() > 900.0,
        "across the dungeon: {} m",
        path.length()
    );
}

/// Behaviour: plugins.nav.routes-are-searched-a-polygon-of-the-navigation-mesh-at-a-time
#[test]
fn the_navigation_mesh_parts_a_regions_places_exactly_as_their_steps_do() {
    for block in [0x0106_u16, 0x01EE, 0x8602, 0x00D1] {
        let mut d = dungeon(block);
        let first = d.first_cell();
        let region = d.nav.region(first).expect("the first cell is loaded");
        let (mesh, places) = region.mesh_and_place_parts();
        assert_eq!(
            mesh, places,
            "{block:04X}: the mesh parts the places into {mesh}, their steps into {places}"
        );
    }
    // Outdoors too: Holtburg's window, and the one round the stream at A7B4.
    for block in [0xA9B4_u16, 0xA7B4] {
        let mut d = outdoors(block, 1);
        let (a, b) = (
            ground(&d, block, 40.0, 96.0),
            ground(&d, block, 150.0, 96.0),
        );
        let region = d.nav.region_between(&a, &b).expect("the window");
        let (mesh, places) = region.mesh_and_place_parts();
        assert_eq!(
            mesh, places,
            "{block:04X}: the mesh parts the places into {mesh}, their steps into {places}"
        );
    }
}

/// A store of tiles in memory, shared by the planners given a copy.
#[derive(Debug, Default, Clone)]
pub struct Kept(pub Arc<Mutex<HashMap<String, Vec<u8>>>>);

impl Kept {
    pub fn len(&self) -> usize {
        self.0.lock().expect("the store").len()
    }

    pub fn holds(&self, prefix: &str) -> bool {
        self.0
            .lock()
            .expect("the store")
            .keys()
            .any(|k| k.starts_with(prefix))
    }
}

impl TileStore for Kept {
    fn load(&self, key: &str) -> Option<Vec<u8>> {
        self.0.lock().expect("the store").get(key).cloned()
    }
    fn save(&self, key: &str, bytes: &[u8]) {
        self.0
            .lock()
            .expect("the store")
            .insert(key.to_owned(), bytes.to_vec());
    }
    fn forget(&self, key: &str) {
        self.0.lock().expect("the store").remove(key);
    }
}

/// Behaviour: plugins.nav.outdoor-windows-are-put-together-from-landblock-tiles-built-once
#[test]
fn a_window_moved_one_landblock_on_builds_only_the_landblocks_new_to_it() {
    let mut d = outdoors(0xA9B4, 1);
    let (a, b) = (
        ground(&d, 0xA9B4, 40.0, 96.0),
        ground(&d, 0xA9B4, 150.0, 96.0),
    );
    d.nav.plan(&a, &b).expect("a route across Holtburg");
    let first = d.nav.tiles_built();
    assert_eq!(first, 9, "Holtburg's landblock and the eight round it");
    let (c, e) = (
        ground(&d, 0xAAB4, 40.0, 96.0),
        ground(&d, 0xAAB4, 150.0, 96.0),
    );
    d.nav.plan(&c, &e).expect("a route a landblock east");
    assert_eq!(
        d.nav.tiles_built() - first,
        3,
        "only the column of landblocks new to the window"
    );
    // Back again: nothing new to build.
    let before = d.nav.tiles_built();
    d.nav.plan(&a, &b).expect("the route across Holtburg again");
    assert_eq!(d.nav.tiles_built(), before);
}

/// Behaviour: plugins.nav.landblock-tiles-kept-between-sessions-are-read-back-not-built-again
#[test]
fn landblock_tiles_kept_between_sessions_are_read_back_and_plan_the_same_route() {
    let kept = Kept::default();
    let mut d = outdoors(0xA9B4, 1);
    d.nav.set_tile_store(Box::new(kept.clone()));
    let (a, b) = (
        ground(&d, 0xA9B4, 20.0, 20.0),
        ground(&d, 0xA9B4, 170.0, 170.0),
    );
    let t = std::time::Instant::now();
    let first = d.nav.plan(&a, &b).expect("a route across Holtburg");
    let built = t.elapsed();
    assert_eq!(kept.len(), 9, "each landblock's tile kept");

    let mut again = outdoors(0xA9B4, 1);
    again.nav.set_tile_store(Box::new(kept.clone()));
    let t = std::time::Instant::now();
    let second = again
        .nav
        .plan(&a, &b)
        .expect("the same route, next session");
    let read = t.elapsed();
    assert_eq!(
        again.nav.tiles_built(),
        0,
        "every tile read back, none built"
    );
    assert_eq!(first.waypoints, second.waypoints, "the same route");
    println!(
        "built and planned in {:.0} ms; read back and planned in {:.0} ms",
        built.as_secs_f64() * 1e3,
        read.as_secs_f64() * 1e3
    );

    // A dungeon's region the same way: kept under the cell its route started from.
    let mut d = dungeon(0x0106);
    d.nav.set_tile_store(Box::new(kept.clone()));
    let first = d.first_cell();
    let (a, b) = {
        let region = d.nav.region(first).expect("the first cell is loaded");
        let (a, b) = far_pair(region);
        (at(region, a), at(region, b))
    };
    let once = d.nav.plan(&a, &b).expect("a route across the dungeon");
    assert!(kept.holds("dungeon-"), "the dungeon's region kept");
    let mut again = dungeon(0x0106);
    again.nav.set_tile_store(Box::new(kept.clone()));
    let t = std::time::Instant::now();
    let twice = again
        .nav
        .plan(&a, &b)
        .expect("the same route, next session");
    println!(
        "the dungeon read back and planned in {:.0} ms",
        t.elapsed().as_secs_f64() * 1e3
    );
    assert_eq!(once.waypoints, twice.waypoints, "the dungeon's same route");
}

/// Behaviour: plugins.nav.a-dungeons-drops-are-links-of-its-mesh-marked-and-never-walked
#[test]
fn a_dungeons_drops_are_links_of_its_mesh_kept_with_it_and_never_walked() {
    let kept = Kept::default();
    let mut d = dungeon(0x010A);
    d.nav.set_tile_store(Box::new(kept.clone()));
    let first = d.first_cell();
    let count = d
        .nav
        .region(first)
        .expect("the first cell is loaded")
        .drop_link_count();
    assert!(
        count > 0,
        "the Black Dominion's ledges are drops of its mesh"
    );
    let mut again = dungeon(0x010A);
    again.nav.set_tile_store(Box::new(kept.clone()));
    let read = again
        .nav
        .region(first)
        .expect("read back")
        .drop_link_count();
    assert_eq!(read, count, "the drops kept with the dungeon's region");
}

/// Behaviour: plugins.nav.landblock-tiles-kept-between-sessions-are-read-back-not-built-again
#[test]
fn a_kept_tile_garbled_on_disk_is_let_go_of_and_built_again() {
    let kept = Kept::default();
    let mut d = outdoors(0xA9B4, 1);
    d.nav.set_tile_store(Box::new(kept.clone()));
    let (a, b) = (
        ground(&d, 0xA9B4, 20.0, 20.0),
        ground(&d, 0xA9B4, 170.0, 170.0),
    );
    let first = d.nav.plan(&a, &b).expect("a route across Holtburg");
    // Holtburg's own tile garbled halfway along, as a disk might leave it.
    {
        let mut m = kept.0.lock().expect("the store");
        let t = m.get_mut("tile-a9b4").expect("Holtburg's tile kept");
        let mid = t.len() / 2;
        t[mid] ^= 0x5A;
    }
    let mut again = outdoors(0xA9B4, 1);
    again.nav.set_tile_store(Box::new(kept.clone()));
    let second = again
        .nav
        .plan(&a, &b)
        .expect("the same route, next session");
    assert_eq!(
        again.nav.tiles_built(),
        1,
        "only the garbled tile built again"
    );
    assert_eq!(first.waypoints, second.waypoints, "the same route");
    let mut third = outdoors(0xA9B4, 1);
    third.nav.set_tile_store(Box::new(kept.clone()));
    third.nav.plan(&a, &b).expect("the route a third time");
    assert_eq!(third.nav.tiles_built(), 0, "the tile kept whole again");
}
