//! The walk over the world: every landblock's scenery, statics, buildings and cells, counted
//! into a [`Tally`].
//!
//! Each landblock is built at full detail, the way the client builds the blocks around the
//! player: its mesh, the scenery the client grows on it and the statics and buildings its
//! landblock information places ([`land_content`]), then its interior cells. A cell reached from a
//! building's outdoor portals through the cells' own portals is that building's interior; every
//! other cell of the block is a dungeon's, and a block's dungeon cells together are counted as
//! one dungeon.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use dereth_assets::Region;
use dereth_client_runtime::world_build::{land_content, read_landblock};
use dereth_dat::RetailDatStore;
use dereth_primitives::direction::BlockDirection;
use dereth_terrain::consts::LAND_HEIGHT_TABLE_LEN;
use dereth_terrain::land::mesh::{generate_landblock_with_table, height_table};
use dereth_world_data::env_cells::{building_cells, cell_statics, EnvCellLoader};
use dereth_world_data::landblock::block_xy;

use crate::tally::{BlockTally, ModelId, Sighting, Tally, Use};

/// Every landblock id the world can hold, `0x0000` to `0xFEFE`.
#[must_use]
pub fn every_landblock() -> Vec<u16> {
    (0..=0xFEu16)
        .flat_map(|x| (0..=0xFEu16).map(move |y| (x << 8) | y))
        .collect()
}

/// The terrain type and scene type a vertex's terrain word holds.
#[must_use]
pub fn terrain_and_scene(word: u16) -> (u8, u8) {
    // LINT-OK: five and five bits.
    let terrain = u8::try_from((word >> 2) & 0x1F).unwrap_or(0);
    let scene = u8::try_from(word >> 11).unwrap_or(0);
    (terrain, scene)
}

/// Counts one landblock into `tally`. A landblock the cell file does not hold counts nothing.
pub fn survey_block(
    store: &RetailDatStore,
    region: &Region,
    table: &[f32; LAND_HEIGHT_TABLE_LEN],
    cells: &mut EnvCellLoader,
    landblock: u16,
    tally: &mut Tally,
) {
    let (bx, by) = block_xy(landblock);
    let Some(lb) = read_landblock(store, bx, by) else {
        return;
    };
    tally.world.landblocks += 1;
    let mesh =
        generate_landblock_with_table(&lb, region, table, bx, by, 1, BlockDirection::InViewerBlock);
    let content = land_content(store, region, &lb, &mesh, bx, by, true);
    let mut block = BlockTally::new(landblock);
    if !content.placed.is_empty() {
        tally.world.scenery_landblocks += 1;
    }
    for p in &content.placed {
        let word = lb.terrain.get(p.vertex).copied().unwrap_or(0);
        block.see(
            ModelId::Object(p.gfxobj),
            Use::Scenery,
            Sighting {
                position: Some(p.frame.origin),
                cell: Some(p.cell.0),
                scale: Some(p.scale),
                terrain: Some(terrain_and_scene(word)),
                interior_cells: 0,
            },
        );
    }

    let before = cells.stats;
    let decoded = cells.load_block(store, landblock);
    let after = cells.stats;
    tally.world.cells_unloaded += (after.missing - before.missing)
        + (after.undecodable - before.undecodable)
        + (after.no_environment - before.no_environment);
    let buildings = content
        .lbi
        .as_ref()
        .map_or(&[][..], |info| info.buildings.as_slice());
    let reach = building_cells(landblock, buildings, &decoded);
    if let Some(info) = &content.lbi {
        tally.world.building_landblocks += u64::from(!info.buildings.is_empty());
        tally.world.static_landblocks += u64::from(!info.objects.is_empty());
        for (b, inside) in info.buildings.iter().zip(&reach) {
            tally.world.buildings += 1;
            block.see(
                ModelId::Object(b.id),
                Use::Building,
                Sighting {
                    position: Some(b.frame.origin),
                    interior_cells: inside.len() as u64,
                    ..Sighting::default()
                },
            );
        }
        for (o, slot) in info.objects.iter().zip(&content.object_slots) {
            if slot.is_none() {
                tally.world.statics_outside_block += 1;
            }
            block.see(
                ModelId::Object(o.id),
                Use::Static,
                Sighting {
                    position: Some(o.frame.origin),
                    cell: slot.map(|(c, _)| c.0),
                    ..Sighting::default()
                },
            );
        }
    }

    let interior: BTreeSet<u32> = reach.iter().flatten().map(|c| c.0).collect();
    let mut dungeon_cells = 0;
    for d in &decoded {
        let inside = interior.contains(&d.id.0);
        let piece = ModelId::Piece {
            environment: d.cell.environment,
            cell_struct: d.cell.cell_struct,
        };
        let (piece_use, static_use) = if inside {
            tally.world.interior_cells += 1;
            (Use::InteriorPiece, Use::InteriorStatic)
        } else {
            dungeon_cells += 1;
            (Use::DungeonPiece, Use::DungeonStatic)
        };
        block.see(
            piece,
            piece_use,
            Sighting {
                position: Some(d.cell.frame.origin),
                cell: Some(d.id.0),
                ..Sighting::default()
            },
        );
        tally.piece_cell(piece, d.id.0, &d.cell.surfaces);
        for s in cell_statics(d) {
            block.see(
                ModelId::Object(s.id),
                static_use,
                Sighting {
                    position: Some(s.frame.origin),
                    cell: Some(d.id.0),
                    ..Sighting::default()
                },
            );
        }
    }
    tally.world.interior_landblocks += u64::from(!interior.is_empty());
    if dungeon_cells > 0 {
        tally.world.dungeons += 1;
        tally.world.dungeon_cells += dungeon_cells;
    }
    tally.fold(block);
}

/// Counts `landblocks` on `threads` threads. The result does not depend on the thread count.
///
/// `progress` is called with the number of landblocks done so far, now and then.
#[must_use]
pub fn walk(
    store: &RetailDatStore,
    region: &Region,
    landblocks: &[u16],
    threads: usize,
    progress: &(dyn Fn(usize) + Sync),
) -> Tally {
    let table = height_table(region);
    let next = AtomicUsize::new(0);
    let tallies: Vec<Tally> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads.max(1))
            .map(|_| {
                s.spawn(|| {
                    let mut t = Tally::default();
                    let mut cells = EnvCellLoader::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&lb) = landblocks.get(i) else {
                            break;
                        };
                        survey_block(store, region, &table, &mut cells, lb, &mut t);
                        if i % 4096 == 4095 {
                            progress(i + 1);
                        }
                    }
                    t
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().expect("a survey thread finishes"))
            .collect()
    });
    let mut total = Tally::default();
    for t in tallies {
        total.merge(t);
    }
    total
}
