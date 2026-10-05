//! `DatLandSource`: the shared physics `LandSource` over the cell dat.
//!
//! Adapted from the shared adapter `dereth_world_data::land_source` (`DatLandSource`,
//! `load_block_cells`, `release_visible_cells`, `shell`) and its block loader
//! `dereth_world_data::env_cells::EnvCellLoader::load_block`, whose conversions it calls through
//! `dereth_world_data`. Nothing here is ported from ACE.
//!
//! The shared source itself is not used because it reads a `RetailDatStore` directly: this one
//! reads through the server's [`DatManager`] (its decoded-object cache, and a `FakeDats` in the
//! unit tier), and has the server's residency model rather than the client's streaming window.
//! `empyrean/crates/dat/tests/all/dat/real_dats.rs` checks both build the same terrain, cells and buildings.
//!
//! The server's residency model replaces the client's streaming window: a landblock's terrain is
//! built on first touch and kept, while its interior cells and buildings are loaded by
//! [`DatLandSource::load_landblock`] (the landblock manager calls it when it loads a landblock)
//! and dropped by [`DatLandSource::unload_landblock`]. A block counts as resident from load to
//! unload; physics does not resolve cells in a block that is not resident.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use dereth_physics::globals::LAND_HEIGHT_TABLE_LEN;
use dereth_physics::land::LandblockCollision;
use dereth_physics::source::{BuildingGeometry, EnvCellGeometry, LandSource, PhysicsPart};
use dereth_primitives::{CellId, LandblockId, Position};

use super::convert::{
    drawing_sphere, env_cell_geometry, first_degrade_mode, gfx_bound_box, gfxobj_physics_bsp,
};
use crate::file_types::{CellLandblock, EnvCell, Environment, GfxObj, LandblockInfo};
use crate::DatManager;

pub use dereth_world_data::env_cells::FIRST_ENV_CELL;

/// Why a `DatLandSource` could not be made.
#[derive(Debug, thiserror::Error)]
pub enum LandSourceError {
    #[error("the portal dat has no RegionDesc")]
    NoRegion,
    #[error("the region's land height table is not valid")]
    BadHeightTable,
}

/// A `LandSource` over a [`DatManager`]'s cell dat.
pub struct DatLandSource {
    dats: Arc<DatManager>,
    table: Box<[f32; LAND_HEIGHT_TABLE_LEN]>,
    blocks: Mutex<BTreeMap<u16, Option<Arc<LandblockCollision>>>>,
    loaded: Mutex<BTreeSet<u16>>,
    cells: Mutex<BTreeMap<u32, Arc<EnvCellGeometry>>>,
    building_cells: Mutex<BTreeMap<u32, Vec<CellId>>>,
    buildings: Mutex<BTreeMap<u32, Arc<BuildingGeometry>>>,
}

impl std::fmt::Debug for DatLandSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatLandSource")
            .field("loaded", &self.loaded_landblocks().len())
            .finish_non_exhaustive()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl DatLandSource {
    /// A source using the region's land height table (`RegionDesc`, `0x13000000`).
    ///
    /// # Errors
    ///
    /// The portal dat has no region, or its height table is not valid.
    pub fn new(dats: Arc<DatManager>) -> Result<Self, LandSourceError> {
        let region = dats
            .portal_dat()
            .try_region_desc()
            .ok_or(LandSourceError::NoRegion)?;
        let table =
            dereth_physics::landdefs::validate_height_table(&region.land_defs.land_height_table)
                .ok_or(LandSourceError::BadHeightTable)?;
        Ok(Self::with_height_table(dats, table))
    }

    /// A source with an explicit height table, for dats without a region (a `FakeDats`).
    #[must_use]
    pub fn with_height_table(dats: Arc<DatManager>, table: [f32; LAND_HEIGHT_TABLE_LEN]) -> Self {
        Self {
            dats,
            table: Box::new(table),
            blocks: Mutex::new(BTreeMap::new()),
            loaded: Mutex::new(BTreeSet::new()),
            cells: Mutex::new(BTreeMap::new()),
            building_cells: Mutex::new(BTreeMap::new()),
            buildings: Mutex::new(BTreeMap::new()),
        }
    }

    /// The landblocks loaded and not yet unloaded, ascending.
    #[must_use]
    pub fn loaded_landblocks(&self) -> Vec<LandblockId> {
        lock(&self.loaded).iter().map(|b| LandblockId(*b)).collect()
    }

    /// Load a landblock for physics: its terrain, its interior cells (from its `LandblockInfo`'s
    /// cell count) and its buildings. Idempotent. Answers whether the cell dat has the block.
    pub fn load_landblock(&self, block: LandblockId) -> bool {
        if !lock(&self.loaded).insert(block.0) {
            return self.landblock(block).is_some();
        }
        let exists = self.landblock(block).is_some();
        let Some(lbi) = self
            .dats
            .cell_dat()
            .read_from_dat::<LandblockInfo>(block.info_id().0)
        else {
            return exists;
        };

        let mut cells = Vec::new();
        for i in 0..lbi.num_cells {
            let id = CellId((u32::from(block.0) << 16) | (FIRST_ENV_CELL + i));
            let Some(cell) = self.dats.cell_dat().read_from_dat::<EnvCell>(id.0) else {
                continue;
            };
            if cell.environment.0 & 0xFF00_0000 != 0x0D00_0000 {
                continue;
            }
            let Some(env) = self
                .dats
                .portal_dat()
                .read_from_dat::<Environment>(cell.environment.0)
            else {
                continue;
            };
            let Some(structure) = env.cells.get(cell.cell_struct as usize) else {
                continue;
            };
            cells.push(Arc::new(env_cell_geometry(id, &cell, structure)));
        }
        {
            let mut map = lock(&self.cells);
            for c in cells {
                map.insert(c.id.0, c);
            }
        }

        let mut shells: Vec<(CellId, BuildingGeometry)> = Vec::new();
        {
            let mut owners = lock(&self.building_cells);
            for b in &lbi.buildings {
                let mut cell = block.cell(1);
                let mut origin = b.frame.origin;
                let inside = dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin);
                let entry = owners.entry(cell.0).or_default();
                for p in &b.portals {
                    let id = CellId((u32::from(block.0) << 16) | u32::from(p.other_cell_id));
                    if !entry.contains(&id) {
                        entry.push(id);
                    }
                }
                if !inside || b.id.0 >> 24 != 0x01 {
                    continue;
                }
                if let Some(g) = self.shell(b, cell) {
                    shells.push((cell, g));
                }
            }
        }
        let mut buildings = lock(&self.buildings);
        for (cell, g) in shells {
            // A land cell keeps the first building offered.
            buildings.entry(cell.0).or_insert_with(|| Arc::new(g));
        }
        exists
    }

    /// Drop a landblock's interior cells, buildings and terrain and make it non-resident. A caller
    /// still holding any of them keeps a valid `Arc`.
    pub fn unload_landblock(&self, block: LandblockId) {
        let base = u32::from(block.0) << 16;
        let mine = |k: &u32| k & 0xFFFF_0000 == base;
        lock(&self.loaded).remove(&block.0);
        lock(&self.cells).retain(|k, _| !mine(k));
        lock(&self.building_cells).retain(|k, _| !mine(k));
        lock(&self.buildings).retain(|k, _| !mine(k));
        lock(&self.blocks).remove(&block.0);
    }

    fn shell(&self, b: &dereth_assets::world::BuildInfo, cell: CellId) -> Option<BuildingGeometry> {
        let g = self.dats.portal_dat().read_from_dat::<GfxObj>(b.id.0)?;
        Some(BuildingGeometry {
            parts: vec![PhysicsPart {
                pos: Position::new(cell, b.frame),
                gfxobj_scale: 1.0,
                physics_bsp: gfxobj_physics_bsp(&g).map(Arc::new),
                bound_box: Some(gfx_bound_box(&g)),
                drawing_sphere: drawing_sphere(&g),
                first_degrade_mode: first_degrade_mode(&self.dats, &g),
            }],
        })
    }

    fn build(&self, id: LandblockId) -> Option<Arc<LandblockCollision>> {
        let lb = self
            .dats
            .cell_dat()
            .read_from_dat::<CellLandblock>(id.terrain_id().0)?;
        LandblockCollision::build(
            id,
            Box::new(lb.height),
            Box::new(lb.terrain),
            lb.lbi_exists != 0,
            8,
            &self.table,
        )
        .ok()
        .map(Arc::new)
    }
}

impl LandSource for DatLandSource {
    fn landblock(&self, id: LandblockId) -> Option<Arc<LandblockCollision>> {
        if let Some(hit) = lock(&self.blocks).get(&id.0) {
            return hit.clone();
        }
        let built = self.build(id);
        lock(&self.blocks).entry(id.0).or_insert(built).clone()
    }

    fn landblock_resident(&self, id: LandblockId) -> bool {
        lock(&self.loaded).contains(&id.0)
    }

    fn height_table(&self) -> &[f32; LAND_HEIGHT_TABLE_LEN] {
        &self.table
    }

    fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>> {
        lock(&self.cells).get(&cell.0).cloned()
    }

    fn building_cells(&self, cell: CellId) -> Vec<CellId> {
        lock(&self.building_cells)
            .get(&cell.0)
            .cloned()
            .unwrap_or_default()
    }

    fn building(&self, cell: CellId) -> Option<Arc<BuildingGeometry>> {
        lock(&self.buildings).get(&cell.0).cloned()
    }
}
