//! Every landblock's terrain, built from the September 2013 install.

use std::collections::HashMap;

use auberean_world::terrain::{build_block, SurfacePick, TerrainBlock};

use crate::common::{landblocks, need_world};

/// Every landblock's terrain, built once.
fn built() -> &'static HashMap<(i32, i32), TerrainBlock> {
    static ALL: std::sync::OnceLock<HashMap<(i32, i32), TerrainBlock>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| {
        let Some(w) = crate::common::world() else {
            return HashMap::new();
        };
        let land = landblocks();
        land.keys()
            .map(|&(x, y)| {
                let b = build_block(&w.region.land, w.region.surfaces(), land, x, y)
                    .expect("the block is here");
                ((x, y), b)
            })
            .collect()
    })
}

/// Every landblock meets its east and north neighbors at the shared edge with nothing between
/// them: each of the 17 shared vertices is at the same place (one block length apart in the
/// blocks' own frames) and the same height, and has the same normal to the last bit and the same
/// ground, except at a block corner beside a block the world lacks, whose height reads as 0 from
/// one side only. Checked over every pair of neighboring blocks of the 2013 world.
///
/// Behaviour: world.terrain.neighbouring-landblocks-meet-without-a-crack
#[test]
fn every_landblock_meets_its_neighbours_without_a_crack() {
    let w = need_world!();
    let all = built();
    let n = w.region.land.grid().vertices_per_side();
    let len = w.region.land.grid().block_length();
    let (mut pairs, mut bad) = (0usize, Vec::new());
    let mut kinds = HashMap::new();
    let mut beside_missing = 0usize;
    for (&(x, y), a) in all {
        for (dx, dy) in [(1, 0), (0, 1)] {
            let Some(b) = all.get(&(x + dx, y + dy)) else {
                continue;
            };
            pairs += 1;
            for k in 0..n {
                let (ia, ib) = if dx == 1 {
                    (a.vertex(n - 1, k), b.vertex(0, k))
                } else {
                    (a.vertex(k, n - 1), b.vertex(k, 0))
                };
                let (pa, pb) = (a.geometry.vertices[ia], b.geometry.vertices[ib]);
                let same_place = if dx == 1 {
                    pa.x == pb.x + len && pa.y == pb.y
                } else {
                    pa.y == pb.y + len && pa.x == pb.x
                };
                let same = same_place
                    && pa.z == pb.z
                    && a.normals[ia] == b.normals[ib]
                    && a.terrain[ia] == b.terrain[ib]
                    && a.road[ia] == b.road[ib];
                // A corner where one of the four blocks around it is missing reads height 0 for
                // that block from one side only, so its normal (and the ground that follows from
                // its slope) differs by design; heights still agree.
                let touches_missing = (k == 0 || k == n - 1) && {
                    let o = if k == 0 { -1 } else { 1 };
                    let around = if dx == 1 {
                        [(x, y + o), (x + 1, y + o)]
                    } else {
                        [(x + o, y), (x + o, y + 1)]
                    };
                    around.iter().any(|b| !all.contains_key(b))
                };
                if !same && touches_missing && pa.z == pb.z {
                    beside_missing += 1;
                    continue;
                }
                if !same {
                    let why = (
                        pa.z != pb.z,
                        a.normals[ia] != b.normals[ib],
                        a.terrain[ia] != b.terrain[ib],
                        a.road[ia] != b.road[ib],
                    );
                    *kinds.entry(why).or_insert(0usize) += 1;
                    if bad.len() < 10 {
                        bad.push(((x, y), (dx, dy), k, pa.z, pb.z));
                    }
                }
            }
        }
    }
    eprintln!("{} blocks, {pairs} neighboring pairs; disagreements (height, normal, ground, road): {kinds:?}; corners beside a missing block: {beside_missing}", all.len());
    assert!(all.len() > 65_000, "the 2013 world has 65,018 landblocks");
    assert!(pairs > 128_000);
    assert!(
        bad.is_empty(),
        "edges that disagree (block, direction, vertex, heights): {bad:?}"
    );
}

/// The built terrain's heights are the record's: at every vertex of every 101st landblock, the
/// vertex's height is the height table's entry for the byte at offset `0x10 + 17x + y` of the
/// landblock record's own bytes, read with no decoder.
///
/// Behaviour: world.terrain.terrain-heights-are-the-records-heights
#[test]
fn terrain_heights_are_the_records_heights() {
    let w = need_world!();
    let all = built();
    let mut keys: Vec<_> = all.keys().copied().collect();
    keys.sort_unstable();
    let mut checked = 0;
    for &(x, y) in keys.iter().step_by(101) {
        let id = auberean_world::records::block_id(x, y).expect("a block") | 0xFFFF;
        let raw = auberean_dat::DatFile::open(
            &auberean_dat::locate::DatName::Cell(1)
                .in_dir(&auberean_dat::testing::require_target_install()),
        )
        .expect("the cell file")
        .read(id)
        .expect("the record");
        let block = &all[&(x, y)];
        for i in 0..17 {
            for j in 0..17 {
                let byte = raw[0x10 + 17 * i + j];
                let want = w.region.land.heights[usize::from(byte)];
                assert_eq!(
                    block.geometry.vertices[block.vertex(i, j)].z,
                    want,
                    "block ({x}, {y}) vertex ({i}, {j})"
                );
                checked += 1;
            }
        }
        if checked > 289 * 40 {
            break;
        }
    }
    assert!(checked >= 289 * 40, "checked {checked} vertices");
}

/// Every vertex of every landblock that is not a hole, whose terrain kind names a surface the
/// region has, finds a ground to show: the surface has a material group for the vertex's slope,
/// and the group gives it a base, a corner mask and a side mask. (One block of the 2013 world is
/// all of a kind whose surface the region lacks, and shows no ground.)
///
/// Behaviour: world.terrain.every-vertex-finds-its-ground
#[test]
fn every_vertex_finds_its_ground() {
    let w = need_world!();
    let all = built();
    let mut missing = HashMap::<u32, usize>::new();
    let mut undescribed = HashMap::<(u32, i32, i32), usize>::new();
    // A kind whose surface word names no surface of the region (the 2013 world's "Marsh", kind 48,
    // on one block) gets the default: no ground.
    let described = |kind: u32| w.region.surfaces().terrain_surface(kind).is_some();
    let mut total = 0usize;
    let land = landblocks();
    for (&(x, y), block) in all {
        let lb = &land[&(x, y)];
        for (k, pick) in block.terrain.iter().enumerate() {
            total += 1;
            let kind = w.region.land.word.terrain.of(lb.vertices[k]);
            if kind != auberean_world::land::HOLE_TERRAIN && *pick == SurfacePick::NONE {
                if described(kind) {
                    *missing.entry(kind).or_default() += 1;
                } else {
                    *undescribed.entry((kind, x, y)).or_default() += 1;
                }
            }
        }
    }
    eprintln!("{total} vertices; without a ground, by terrain kind: {missing:?}; kinds naming no surface (kind, block): {undescribed:?}");
    assert!(
        missing.is_empty(),
        "vertices without a ground, by terrain kind: {missing:?}"
    );
    assert!(
        undescribed.values().sum::<usize>() <= 289,
        "more than one block names no surface: {undescribed:?}"
    );
}
