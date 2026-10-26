//! A retail dungeon of many levels, its stairs and ramps, sampled into standing places and merged
//! into patches, and the longest walk across it searched a patch at a time and walked by a human
//! body through the physics world.

use crate::common::*;

/// Behaviour: plugins.nav.routes-are-searched-a-patch-of-places-at-a-time
#[test]
fn a_dungeons_places_merge_into_patches_a_fiftieth_as_many_and_its_longest_walk_is_walked() {
    let mut d = dungeon(0x01EE);
    let first = d.first_cell();
    let (from, to, patches, clear) = {
        let region = d.nav.region(first).expect("the first cell is loaded");
        let (a, b) = far_pair(region);
        let r = region.report();
        (at(region, a), at(region, b), r.patches, r.clear_nodes)
    };
    assert!(
        patches > 0 && patches * 50 <= clear,
        "{patches} patches for {clear} clear places"
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
