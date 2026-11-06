//! The clearance the September 2013 world's placed objects ask of the scenery about them.

use std::collections::HashMap;

use auberean_primitives::DataId;
use auberean_world::place::{PlaceRecords, DRAWN_KINDS};
use auberean_world::scenery::{barren_field, block_scenery, KeptClear, SceneryInput};
use auberean_world::terrain::build_block;

use crate::common::{landblocks, need_world};

/// Every block's placed objects read their clearance properties: some ask for boxes or upright
/// cylinders to be kept clear, and in the blocks where they do, scenery that stood in them as
/// points kept clear does not stand there now (the blocks keep the rest of their scenery).
///
/// Behaviour: world.scenery.the-worlds-objects-keep-their-clearance
#[test]
fn the_worlds_objects_keep_their_clearance() {
    let w = need_world!();
    let land = landblocks();
    let lookup = |id: u32| PlaceRecords::entity(&w.records, DataId(id));
    let (Some(scenes), terrain) = (w.region.scenes.as_ref(), &w.region.terrain) else {
        panic!("the region's scenery");
    };
    let (mut objects, mut asking, mut boxes, mut cylinders) = (0usize, 0usize, 0usize, 0usize);
    let (mut blocks, mut before, mut after) = (0usize, 0usize, 0usize);
    let mut changed = HashMap::new();
    for (&(x, y), lb) in land {
        let Some(info) = w.records.landblock_info(x, y) else {
            continue;
        };
        let full: Vec<KeptClear> = info
            .entities
            .entities
            .iter()
            .filter(|e| e.kind.is_some_and(|k| DRAWN_KINDS.contains(&k)))
            .filter_map(|e| {
                let f = e.offset.as_ref()?.frame;
                let total = e.total_properties(&lookup);
                Some(KeptClear::of_entity(f.origin, f.rotation, Some(&total)))
            })
            .collect();
        objects += full.len();
        let shaped = full
            .iter()
            .filter(|c| !c.boxes.is_empty() || !c.cylinders.is_empty())
            .count();
        if shaped == 0 {
            continue;
        }
        asking += shaped;
        boxes += full.iter().map(|c| c.boxes.len()).sum::<usize>();
        cylinders += full.iter().map(|c| c.cylinders.len()).sum::<usize>();
        let points: Vec<KeptClear> = full
            .iter()
            .map(|c| KeptClear::of_entity(c.origin, c.rotation, None))
            .collect();
        let tb = build_block(&w.region.land, w.region.surfaces(), land, x, y).expect("the block");
        let scenery = |obstacles: &[KeptClear]| {
            let input = SceneryInput {
                land: &w.region.land,
                terrain,
                scenes,
                vertices: &lb.vertices,
                block: &tb,
                obstacles,
                degrade: 1.0,
                water: None,
            };
            block_scenery(&input, &w.records)
                .into_iter()
                .filter(|o| !o.frill)
                .count()
        };
        blocks += 1;
        let (b, a) = (scenery(&points), scenery(&full));
        before += b;
        after += a;
        if a != b {
            changed.insert((x, y), (b, a));
        }
    }
    eprintln!(
        "{objects} placed objects; {asking} ask for clearance ({boxes} boxes, {cylinders} \
         cylinders) in {blocks} blocks; scenery there {before} -> {after}; {} blocks changed",
        changed.len()
    );
    let mut most: Vec<_> = changed.into_iter().collect();
    most.sort_by_key(|(k, (b, a))| (std::cmp::Reverse(b - a.min(b)), *k));
    eprintln!("most changed: {:x?}", &most[..most.len().min(6)]);
    assert!(
        asking > 0 && boxes + cylinders > 0,
        "some objects ask for clearance"
    );
    assert!(after < before, "they keep scenery out: {before} -> {after}");
}

/// **Every terrain kind has a scene field under which it grows nothing, so a block whose vertices all
/// hold it has no scenery, frills included, while its other fields are as they were.**
///
/// Behaviour: world.scenery.every-terrain-kind-has-a-barren-field
#[test]
fn a_block_of_barren_vertices_grows_nothing_at_all() {
    let w = need_world!();
    let land = landblocks();
    let (Some(scenes), terrain) = (w.region.scenes.as_ref(), &w.region.terrain) else {
        panic!("the region's scenery");
    };
    assert!(terrain.terrains.len() > 60);
    for t in &terrain.terrains {
        assert!(
            barren_field(terrain, scenes, &w.records, t.index).is_some(),
            "terrain kind {}",
            t.index
        );
    }
    let word = w.region.land.word;
    let mut keys: Vec<_> = land.keys().copied().collect();
    keys.sort_unstable();
    let mut blocks = 0usize;
    for (x, y) in keys.into_iter().step_by(101) {
        let lb = &land[&(x, y)];
        let Some(tb) = build_block(&w.region.land, w.region.surfaces(), land, x, y) else {
            continue;
        };
        let count = |vertices: &[u32]| {
            let input = SceneryInput {
                land: &w.region.land,
                terrain,
                scenes,
                vertices,
                block: &tb,
                obstacles: &[],
                degrade: 1.0,
                water: None,
            };
            block_scenery(&input, &w.records).len()
        };
        if count(&lb.vertices) == 0 {
            continue;
        }
        let barren: Vec<u32> = lb
            .vertices
            .iter()
            .map(|v| {
                let kind = word.terrain.of(*v);
                let field = barren_field(terrain, scenes, &w.records, kind).expect("a field");
                let new = word.scene.with(*v, field);
                assert_eq!(
                    new ^ *v,
                    (new ^ *v) & (((1 << word.scene.bits) - 1) << word.scene.offset)
                );
                new
            })
            .collect();
        assert_eq!(count(&barren), 0, "{x:02X}{y:02X}");
        blocks += 1;
    }
    assert!(blocks > 100, "only {blocks} blocks grew anything");
}
