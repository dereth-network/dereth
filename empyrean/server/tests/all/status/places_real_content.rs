//! Divergence: V458
//! The places a world publishes, over the real world content: every placed lifestone, portal,
//! vendor and person is listed, read back as the client reads them, and nothing else is.
//! Fixture: `world.pack` (`EMPYREAN_TEST_WORLD_PACK`), with the `real-content` feature.

use std::collections::HashMap;

use dereth_navigation::{EdgeSource, PlaceKind, PlaceStore};
use empyrean_content::WorldDatabase;
use empyrean_server::portal_table;

#[test]
fn the_real_worlds_places_are_its_lifestones_portals_vendors_and_people() {
    if !cfg!(feature = "real-content") {
        return;
    }
    let path = empyrean_common::test_paths::world_pack();
    let content = empyrean_content::PackContent::open(&path).unwrap_or_else(|e| {
        panic!(
            "the real-content tier needs world.pack at {} (EMPYREAN_TEST_WORLD_PACK): {e}",
            path.display()
        )
    });
    let content: &dyn WorldDatabase = &content;
    let started = std::time::Instant::now();
    let text = portal_table::places_document("Dereth", content);
    eprintln!("the places took {:?}", started.elapsed());
    let store = PlaceStore::from_text(&text).expect("the client reads the places");
    let mut kinds: HashMap<PlaceKind, usize> = HashMap::new();
    for p in store.places() {
        *kinds.entry(p.kind).or_default() += 1;
    }
    eprintln!("{kinds:?}, {} bytes", text.len());
    assert!(store
        .places()
        .iter()
        .all(|p| p.source == EdgeSource::Published && p.owner.is_none()));
    assert_eq!(kinds.get(&PlaceKind::House), None, "no house is published");
    // Each portal place that leads somewhere is a line of the portal table, with the same class,
    // place and destination; the table also has the portals hidden from the interface, which are
    // no places.
    let portals = portal_table::document("Dereth", content);
    let table: std::collections::HashSet<(&str, &str, &str)> = portals
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f[0] == "portal").then(|| (f[2], f[4], f[5]))
        })
        .collect();
    let leading: Vec<(&str, &str, &str)> = text
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f[0] == "place" && f[1] == "portal" && f[9] != "-").then(|| (f[3], f[5], f[9]))
        })
        .collect();
    assert!(leading.len() > 3000 && leading.len() <= table.len());
    assert!(leading.iter().all(|p| table.contains(p)));
    let named = |kind: PlaceKind, name: &str, block: u32| {
        store
            .places()
            .iter()
            .filter(|p| p.kind == kind && p.name == name && p.at.cell.0 >> 16 == block)
            .count()
    };
    assert!(
        named(PlaceKind::Lifestone, "Life Stone", 0xA9B4) >= 1,
        "Holtburg's"
    );
    for (kind, count) in [
        (PlaceKind::Lifestone, 100),
        (PlaceKind::Portal, 1000),
        (PlaceKind::Vendor, 300),
        (PlaceKind::Npc, 1000),
    ] {
        assert!(
            kinds.get(&kind).copied().unwrap_or(0) >= count,
            "{kind:?}: {kinds:?}"
        );
    }
}
