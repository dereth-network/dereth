//! The clearance the September 2013 world's placed objects ask of the scenery about them.

use std::collections::HashMap;

use auberean_primitives::DataId;
use auberean_world::place::{PlaceRecords, DRAWN_KINDS};
use auberean_world::scenery::{block_scenery, KeptClear, SceneryInput};
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
