//! The landscape's ambient sound points of every landblock of the install, against the region's
//! ambient sound table.

use std::collections::{BTreeMap, BTreeSet};

use auberean_assets::region::SoundDesc;
use auberean_world::sound::block_sound_points;

use crate::common::{landblocks, need_world};

/// Behaviour: world.sound.every-points-kind-is-described
#[test]
fn every_landblocks_points_name_kinds_the_regions_ambient_table_describes() {
    let w = need_world!();
    let scenes = w
        .region
        .scenes
        .as_ref()
        .expect("the region describes its scenery");
    let table_id = w
        .region
        .region
        .sounds
        .first()
        .expect("an ambient table")
        .record
        .0;
    let table: SoundDesc = w.records.get(table_id).expect("the ambient table decodes");
    let described: BTreeSet<u32> = table.sounds.iter().map(|s| s.kind).collect();
    let mut kinds: BTreeMap<u32, usize> = BTreeMap::new();
    let (mut blocks, mut points) = (0, 0);
    for b in landblocks().values() {
        let p = block_sound_points(&w.region.land, &w.region.terrain, scenes, b);
        if !p.is_empty() {
            blocks += 1;
        }
        points += p.len();
        for q in p {
            *kinds.entry(q.kind).or_default() += 1;
        }
    }
    let missing: Vec<String> = kinds
        .keys()
        .filter(|k| !described.contains(k))
        .map(|k| format!("{k:08X}"))
        .collect();
    eprintln!(
        "{} landblocks, {blocks} with points, {points} points of {} kinds; the table describes {}",
        landblocks().len(),
        kinds.len(),
        described.len()
    );
    assert!(blocks > 0, "the landscape names ambient kinds");
    assert_eq!(points, 289 * landblocks().len(), "every vertex names one");
    assert_eq!(kinds.len(), 12);
    assert!(missing.is_empty(), "kinds the table lacks: {missing:?}");
}
