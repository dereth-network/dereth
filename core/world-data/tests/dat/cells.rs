//! Interior cells as physics holds them, built from the target install's cell file.

use std::fmt::Write;
use std::sync::Arc;

use dereth_physics::source::LandSource;
use dereth_primitives::{CellId, LandblockId};
use dereth_world_data::env_cells::{
    cell_statics, physics_geometry, static_geometry, EnvCellLoader,
};
use dereth_world_data::land_source::DatLandSource;
use dereth_world_data::setup::SetupPartStats;

/// FNV-1a, 64 bits, over everything written to it, and how many bytes that was.
struct Digest(u64, u64);

impl Write for Digest {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for b in s.bytes() {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0100_0000_01B3);
        }
        self.1 += s.len() as u64;
        Ok(())
    }
}

/// Every interior cell of the dungeon column (blocks `0x0100` to `0x01FF`), the training dungeon,
/// four more dungeons and Holtburg's buildings, built as the client and the server build them,
/// with the objects each cell bakes in and the buildings standing on those blocks: their whole
/// collision geometry, every polygon and tree, digests to what it digested when this was
/// recorded. A change to how cells are held that leaves what they hold alone keeps it; a change to
/// what they hold has to say so here.
///
/// Behaviour: physics.cells.interior-cells-build-to-the-recorded-geometry
#[test]
fn interior_cells_build_to_the_recorded_geometry() {
    let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {} -- \
             set DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    }));
    let region = dereth_world_data::landblock::load_region(&store).expect("the region");
    let land = DatLandSource::new(Arc::clone(&store), &region).expect("the land");
    let mut loader = EnvCellLoader::new();
    let mut stats = SetupPartStats::default();
    let mut d = Digest(0xCBF2_9CE4_8422_2325, 0);
    let mut cells = 0;
    let blocks = (0x0100..=0x01FF).chain([0x0007, 0x7F03, 0x8602, 0xA9B4, 0xEA6C]);
    for block in blocks {
        let decoded = loader.load_block(&store, block);
        land.load_block_cells(LandblockId(block));
        for c in &decoded {
            cells += 1;
            write!(d, "{:?}", physics_geometry(c)).unwrap();
            write!(d, "{:?}", land.env_cell(c.id)).unwrap();
            for s in cell_statics(c) {
                write!(d, "{:?}", static_geometry(&store, s.id, &mut stats)).unwrap();
            }
        }
        for k in 1..=64 {
            let id = CellId((u32::from(block) << 16) | k);
            write!(d, "{:?}", land.building(id)).unwrap();
        }
    }
    assert_eq!(
        format!("{cells} cells, {} bytes, digest {:016X}", d.1, d.0),
        "90390 cells, 2775859816 bytes, digest 99BFBFE670EF42C0"
    );
}

/// An interior cell and its collision tree hold one list of the cell's polygons between them:
/// every cell of the training dungeon and Holtburg that has a tree.
///
/// Behaviour: physics.cells.a-cell-and-its-tree-hold-one-list-of-polygons
#[test]
fn a_cell_and_its_tree_hold_one_list_of_polygons() {
    let store = dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {} -- \
             set DERETH_TEST_DAT_DIR",
            dereth_dat::testing::dat_dir().display()
        )
    });
    let mut loader = EnvCellLoader::new();
    let mut trees = 0;
    for block in [0x8602, 0xA9B4] {
        for c in &loader.load_block(&store, block) {
            let g = physics_geometry(c);
            let Some(tree) = &g.physics_bsp else { continue };
            trees += 1;
            assert!(
                Arc::ptr_eq(&g.physics_polygons, &tree.polygons),
                "cell {:08X} and its tree hold lists of their own",
                c.id.0
            );
        }
    }
    assert!(trees > 600, "cells with a tree: {trees}");
}
