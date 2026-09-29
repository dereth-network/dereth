//! A [`dereth_physics::LandSource`] over the retail cell dat.
//!
//! **This module wires; it does not implement.** `dereth-physics` declares the shapes it needs and
//! a source trait. The application connects `dereth-assets` through free functions, avoiding an
//! orphan-rule problem by keeping conversion outside either library. This
//! file is that wiring, and nothing more: `LandblockCollision::build` does all the geometry and
//! its own gate covers all 65,025 retail landblocks.
//!
//! Two behaviours are deliberate rather than incidental:
//!
//! * **A block loads on demand and is cached until that block is released.**
//!   A land-block lookup returns NULL for a block that is not resident, so physics silently stops
//!   at that edge rather than stalling on disk I/O. Loading it here on first touch is the closest
//!   a synchronous implementation gets to the client's asynchronous cache fetch.
//!
//!   **Releasing matters as much as loading; keeping blocks for ever leaves a session-long
//!   floating portal.** Retail's array is *finite and emptied* —
//!   the cell manager releases every landscape block on teleport, while a window update
//!   releases each departing landblock edge —
//!   so the landblock lookup answers **NULL** for a block the client has left, and every caller
//!   above it takes the "there is no such cell" arm. The same departed-block release drops this
//!   map's exact collision entry too; a retained `Arc` reader remains valid and a revisit
//!   rebuilds from the
//!   DAT. `LandSource::landblock_resident` is the separated residency question, kept
//!   in `DatLandSource::prefetched` — set by `DatLandSource::load_block_cells`
//!   (cell prefetch), cleared by
//!   `DatLandSource::release_visible_cells` — and `ObjectPhysics::place` is its one consumer.
//! * **`LandSource::env_cell` never loads.** It performs a visible-cell lookup and
//!   reports only what is resident, which is why physics stops at
//!   the edge of the loaded window instead of stalling on I/O. The table it reads is filled
//!   from `DatLandSource::load_block_cells`, which the landblock loading path calls — the
//!   equivalent of the cell-prefetch pass.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use dereth_assets::world::CellLandblock;
use dereth_assets::{Decode, Region};
use dereth_dat::{DbType, RetailDatStore};
use dereth_physics::globals::LAND_HEIGHT_TABLE_LEN;
use dereth_physics::land::LandblockCollision;
use dereth_physics::source::{BuildingGeometry, EnvCellGeometry, LandSource, PhysicsPart};
use dereth_physics::PlaneExt;
use dereth_primitives::{CellId, LandblockId, Position};

use crate::env_cells::{gfxobj_physics_bsp, physics_geometry, CellLoadStats, EnvCellLoader};

/// What the building half of `DatLandSource::load_block_cells` did. Counted, and **asserted on**
/// by the tests.
///
/// Every field but `blocks` and `registered` records a building whose shell is *missing* from the
/// world — the exact failure that is invisible from inside the client, because you walk through the
/// wall and the interior cell catches you. Such failures must be measured and asserted on, which is why
/// each one is a counter rather than a silent `continue`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuildingLoadStats {
    /// Blocks whose landblock information carried at least one `BuildInfo`.
    pub blocks: u64,
    /// `BuildInfo` entries seen.
    pub buildings: u64,
    /// Shells accepted by their owning sorted land cell.
    pub registered: u64,
    /// A second building whose origin falls in a land cell that already has one.
    /// `add_building` keeps the **first** and drops this on the floor.
    pub second_in_a_cell: u64,
    /// `BuildInfo::id` was not a `0x01000000` `GfxObj` id. See `DatLandSource::load_block_cells`:
    /// refused rather than placed at a frame that would be wrong.
    pub not_a_gfxobj: u64,
    /// The `GfxObj` record is missing or would not decode.
    pub undecodable: u64,
    /// The `GfxObj` decoded and carries no physics BSP, so its shell is intangible. The building
    /// is registered anyway, exactly as the client registers it; the counter is what makes the
    /// hole visible, and object-collision discovery counts the other half in
    /// `CollisionCounters::building_parts_without_bsp`.
    pub without_bsp: u64,
    /// Outdoor-coordinate adjustment refused the origin, so there is no land cell to
    /// add the building to.
    pub outside_the_block: u64,
    /// Shells handed back with their block by
    /// `DatLandSource::release_visible_cells`. Every other field here is cumulative, so this one
    /// is too: [`DatLandSource::resident_buildings`] is `registered - released`, and the tests
    /// assert that identity rather than reading either number alone.
    pub released: u64,
}

/// Anything that stops the land source from coming up.
#[derive(Debug, thiserror::Error)]
pub enum LandSourceError {
    /// Height-table installation rejects a table with a negative entry or one above
    /// 800. The retail region passes; a corrupt one is fatal, exactly as it is in the client.
    #[error("the region's land height table is not valid")]
    BadHeightTable,
}

/// The resident landblock set, read from `client_cell_1.dat` on demand.
pub struct DatLandSource {
    store: Arc<RetailDatStore>,
    table: Box<[f32; LAND_HEIGHT_TABLE_LEN]>,
    /// `None` is cached too: the world edge is asked about on every step near it.
    blocks: Mutex<BTreeMap<u16, Option<Arc<LandblockCollision>>>>,
    /// The interior cells of every block that has been prefetched,
    /// keyed by the full cell id.
    cells: Mutex<BTreeMap<u32, Arc<EnvCellGeometry>>>,
    /// Which land cell of a block owns which building's interior cells: from the land cell's
    /// building entry to each portal's other-cell id.
    building_cells: Mutex<BTreeMap<u32, Vec<CellId>>>,
    /// The building **shell** standing in a land cell, keyed by that cell.
    /// At most one per cell.
    buildings: Mutex<BTreeMap<u32, Arc<BuildingGeometry>>>,
    /// The building half's counters, behind their own lock.
    building_stats: Mutex<BuildingLoadStats>,
    /// The decoder and its counters. Behind the same lock as the tables it fills.
    loader: Mutex<EnvCellLoader>,
    /// Blocks whose cells have already been pulled, so a second scroll over the same block is free.
    prefetched: Mutex<std::collections::BTreeSet<u16>>,
    /// **The run-time cache miss.** Cell-dat records this source asked for and the
    /// store did not have, as `(DbType, DataID)`.
    ///
    /// Native asynchronous-cache lookup tries memory first, then -- when disk loads are allowed
    /// and the record is on disk -- the worker read, and only when *neither* answers does it
    /// ask other sources, when network requests are allowed, which is the client's
    /// one `0xF7E3` producer. This queue is the "neither" arm; `dereth_client_runtime::ddd::drain_cache_misses`
    /// is the other-sources half and owns the coalescing rule.
    missing: Mutex<std::collections::BTreeSet<(u32, u32)>>,
    /// The store a DDD patch reopened, once one has. [`Self::resupply`] installs it.
    ///
    /// The field it shadows is an `Arc` this source took at world entry; `RetailDatStore` caches
    /// each file's whole directory at open, so after a patch that `Arc` is not merely missing the
    /// new record -- its entries can name chains that are now on the free list. Replacing the
    /// handle is the only fix, and `App::invalidate_after_ddd` is where the replacement is built.
    patched: Mutex<Option<Arc<RetailDatStore>>>,
}

impl std::fmt::Debug for DatLandSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatLandSource")
            .field("resident", &self.resident())
            .finish_non_exhaustive()
    }
}

impl DatLandSource {
    /// # Errors
    /// [`LandSourceError::BadHeightTable`] when the region's table fails `set_height_table`'s
    /// validation.
    pub fn new(store: Arc<RetailDatStore>, region: &Region) -> Result<Self, LandSourceError> {
        let table =
            dereth_physics::landdefs::validate_height_table(&region.land_defs.land_height_table)
                .ok_or(LandSourceError::BadHeightTable)?;
        Ok(Self {
            store,
            table: Box::new(table),
            blocks: Mutex::new(BTreeMap::new()),
            cells: Mutex::new(BTreeMap::new()),
            building_cells: Mutex::new(BTreeMap::new()),
            buildings: Mutex::new(BTreeMap::new()),
            building_stats: Mutex::new(BuildingLoadStats::default()),
            loader: Mutex::new(EnvCellLoader::new()),
            prefetched: Mutex::new(std::collections::BTreeSet::new()),
            missing: Mutex::new(std::collections::BTreeSet::new()),
            patched: Mutex::new(None),
        })
    }

    /// The store to read through: the one a DDD patch installed, if one has, else the one this
    /// source was built with.
    fn store(&self) -> Arc<RetailDatStore> {
        match self.patched.lock() {
            Ok(p) => p
                .as_ref()
                .map_or_else(|| Arc::clone(&self.store), Arc::clone),
            Err(_) => Arc::clone(&self.store),
        }
    }

    /// Note that a cell-dat record this source needed is not in the store.
    ///
    /// A `BTreeSet` rather than a queue: the same block is asked about on every step near it, and
    /// the client's pending-get table also coalesces by `QualifiedDataID`.
    fn note_missing(&self, kind: DbType, id: dereth_primitives::DataId) {
        if let Ok(mut m) = self.missing.lock() {
            m.insert((kind as u32, id.raw()));
        }
    }

    /// Take the cell-dat records this source has found missing since the last drain, as
    /// `(DbType, DataID)`.
    #[must_use]
    pub fn take_missing(&self) -> Vec<(u32, dereth_primitives::DataId)> {
        let Ok(mut m) = self.missing.lock() else {
            return Vec::new();
        };
        std::mem::take(&mut *m)
            .into_iter()
            .map(|(t, i)| (t, dereth_primitives::DataId(i)))
            .collect()
    }

    /// A DDD patch landed: read through `store` from now on, and forget the negative cache entry
    /// for every landblock the patch named.
    ///
    /// [`LandSource::landblock`] caches `None` -- the world edge is asked about on every step near
    /// it, so it has to -- and without this the block the server has just sent would go on
    /// answering `None` for the life of the session.
    pub fn resupply(&self, store: Arc<RetailDatStore>, ids: &[dereth_primitives::DataId]) {
        if let Ok(mut p) = self.patched.lock() {
            *p = Some(store);
        }
        self.forget_blocks(ids);
    }

    /// Drop the cached answer -- including a cached `None` -- for every landblock these ids name.
    ///
    /// Two callers, and they are the two halves of one rule. [`Self::resupply`] uses it when a
    /// record arrives, because otherwise the block the server has just sent would go on answering
    /// `None` for the life of the session. `App` uses it on a `0xF7E4`, because
    /// asynchronous-get failure taking the request out of the pending-gets list
    /// only permits a new get -- something still has to *ask*, and here the asker is a cache that
    /// remembers the miss.
    pub fn forget_blocks(&self, ids: &[dereth_primitives::DataId]) {
        if let Ok(mut blocks) = self.blocks.lock() {
            for id in ids {
                #[allow(clippy::cast_possible_truncation)]
                let block = (id.raw() >> 16) as u16;
                blocks.remove(&block);
            }
        }
    }

    /// How many collision blocks this source currently owns, for the startup line and the tests.
    #[must_use]
    pub fn resident(&self) -> usize {
        self.blocks
            .lock()
            .map_or(0, |b| b.values().filter(|v| v.is_some()).count())
    }

    /// Prefetch the block's cells and wire its buildings to those cells.
    ///
    /// **This is the only thing that reads an env cell off disk**, and it is called from the
    /// landblock loading path, never from `LandSource::env_cell` — see the module comment. It is
    /// idempotent: a block already pulled costs a set lookup.
    ///
    /// Building initialization also decides which *land* cell owns each building, because a
    /// building is added to exactly one sorted land cell — the one outdoor-coordinate adjustment
    /// puts its frame origin in. That is what lets an object standing outdoors find its way in
    /// through the
    /// doorway (transit-cell discovery).
    pub fn load_block_cells(&self, block: LandblockId) {
        if let Ok(mut done) = self.prefetched.lock() {
            if !done.insert(block.0) {
                return;
            }
        }
        let decoded = match self.loader.lock() {
            Ok(mut l) => l.load_block(&self.store(), block.0),
            Err(_) => return,
        };
        if let Ok(mut cells) = self.cells.lock() {
            for d in &decoded {
                cells.insert(d.id.0, Arc::new(physics_geometry(d)));
            }
        }
        // Each building portal's other-cell id is the interior cell behind the
        // building's own portal polygon; building initialization puts the building in the land
        // cell its origin falls in, so that is where the transit test has to find it.
        let lbi_id = crate::landblock::lbi_did(block.0);
        let Ok(bytes) = self.store().read_typed(DbType::Lbi, lbi_id) else {
            return;
        };
        let Ok(lbi) = dereth_assets::world::LandblockInfo::decode_payload(lbi_id, &bytes) else {
            return;
        };
        let mut stats = BuildingLoadStats::default();
        if !lbi.buildings.is_empty() {
            stats.blocks = 1;
        }
        // The shells, built in the same pass and inserted after it so that no two of this type's
        // locks are ever held at once.
        let mut shells: Vec<(CellId, BuildingGeometry)> = Vec::new();
        {
            let Ok(mut owners) = self.building_cells.lock() else {
                return;
            };
            for b in &lbi.buildings {
                stats.buildings += 1;
                let mut cell = block.cell(1);
                let mut origin = b.frame.origin;
                let inside = dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut origin);
                let entry = owners.entry(cell.0).or_default();
                for p in &b.portals {
                    // A building portal's `other_cell_id` is the low 16 bits, like every other cell
                    // reference in the cell dat.
                    let id = CellId((u32::from(block.0) << 16) | u32::from(p.other_cell_id));
                    if !entry.contains(&id) {
                        entry.push(id);
                    }
                }
                if !inside {
                    stats.outside_the_block += 1;
                    continue;
                }
                if let Some(g) = self.shell(block, b, cell, &mut stats) {
                    shells.push((cell, g));
                }
            }
        }
        if let Ok(mut buildings) = self.buildings.lock() {
            for (cell, g) in shells {
                // **the first one wins**. A land cell holds
                // one building object and a second offer is dropped, not stacked.
                match buildings.entry(cell.0) {
                    std::collections::btree_map::Entry::Vacant(v) => {
                        v.insert(Arc::new(g));
                        stats.registered += 1;
                    }
                    std::collections::btree_map::Entry::Occupied(_) => stats.second_in_a_cell += 1,
                }
            }
        }
        if let Ok(mut s) = self.building_stats.lock() {
            s.blocks += stats.blocks;
            s.buildings += stats.buildings;
            s.registered += stats.registered;
            s.second_in_a_cell += stats.second_in_a_cell;
            s.not_a_gfxobj += stats.not_a_gfxobj;
            s.undecodable += stats.undecodable;
            s.without_bsp += stats.without_bsp;
            s.outside_the_block += stats.outside_the_block;
        }
    }

    /// Release one collision block, every interior cell
    /// it made visible, and the building wiring `load_block_cells` put in beside them.
    ///
    /// The client releases a block's cells from exactly one place, whose three
    /// steps release the objects, release the visible-cell stab list, and release the
    /// block itself. Window scrolling calls that release for each block pushed off an edge
    /// (four sites, one per edge). The **stab list** is the block's own cell ids, which is what
    /// makes this a prefix scan here: a cell id is `blockId << 16 | index`.
    ///
    /// **Two things the client does that this deliberately does not, and the reason for each.**
    ///
    /// * Releasing a visible cell moves it into a flush table rather than freeing
    ///   it. The cell flush at the end of the cell manager's position change frees it.
    ///   The deferral lasts exactly one position change: adding a visible cell looks in the
    ///   flush table before performing a database lookup, so a cell released by the window scroll and re-grabbed
    ///   by the new viewer cell in the same position change survives untouched. **That window is
    ///   not reachable from this seam**: the block window is centred on the viewer, so a block the
    ///   scroll released is by construction not the block the viewer just entered, and a flush
    ///   table here would be an empty table with a name. It is described rather than transcribed
    ///   for that reason, and this is the note that says so.
    /// * It does **not** clear the decode memos. `EnvCellLoader::environments` belongs to the
    ///   database cache rather than a visible cell; see [`crate::env_cells::CellStaticObjects::release_block`].
    ///
    /// Returns `(cells, buildings)` handed back.
    pub fn release_visible_cells(&self, block: LandblockId) -> (u64, u64) {
        let base = u32::from(block.0) << 16;
        let mine = |k: &u32| k & 0xFFFF_0000 == base;
        // Taken one lock at a time, in the same discipline `load_block_cells` states: no two of
        // this type's locks are ever held at once.
        if let Ok(mut done) = self.prefetched.lock() {
            done.remove(&block.0);
        }
        let mut cells_released = 0u64;
        if let Ok(mut cells) = self.cells.lock() {
            let dead: Vec<u32> = cells.keys().copied().filter(mine).collect();
            for k in dead {
                cells.remove(&k);
                cells_released += 1;
            }
        }
        if let Ok(mut owners) = self.building_cells.lock() {
            owners.retain(|k, _| !mine(k));
        }
        let mut buildings_released = 0u64;
        if let Ok(mut buildings) = self.buildings.lock() {
            let dead: Vec<u32> = buildings.keys().copied().filter(mine).collect();
            for k in dead {
                buildings.remove(&k);
                buildings_released += 1;
            }
        }
        if let Ok(mut l) = self.loader.lock() {
            l.stats.released += cells_released;
            l.stats.blocks_released += 1;
        }
        if let Ok(mut s) = self.building_stats.lock() {
            s.released += buildings_released;
        }
        // Landblock teardown releases the block's own DB object after its cells. The original
        // client's database cache may retain freed storage in a bounded free list, but it does
        // not keep one live owner per block visited. Rust removes only this key:
        // an outstanding physics reader keeps its Arc valid, while the next visit rebuilds from DAT.
        if let Ok(mut blocks) = self.blocks.lock() {
            blocks.remove(&block.0);
        }
        (cells_released, buildings_released)
    }

    /// One `BuildInfo` as landblock building initialization creates it: the building
    /// `GfxObj` record's physics BSP, hung on part 0 of a one-part setup.
    ///
    /// Three details are the whole of the placement, and each is the client's:
    ///
    /// * **`pos` is the `BuildInfo`'s own frame, un-adjusted and block-local.**
    ///   Outdoor coordinate adjustment runs above only to pick the owning sorted land cell; it
    ///   moves the origin into cell-local space, which is *not* the part-placement coordinate space.
    /// * **`gfxobj_scale` is 1.** Simple-setup creation leaves `default_scale` NULL,
    ///   so part initialization never overwrites the physics part's default value.
    /// * **Part 0's placement frame is the identity**, which is the only reason `pos` can be the
    ///   building's own frame with no placement transform. That holds because every retail
    ///   `BuildInfo::id` is a `0x01000000` `GfxObj` id. A `0x02000000` `Setup` id would go through
    ///   the setup path instead, where part 0 carries the frame
    ///   the resting placement installs — `placement_frames[0x65]`, or `[0]` when
    ///   the setup has no `0x65` entry — and the composition is real — so such an id is **refused
    ///   and counted** rather than placed at a frame that would silently be wrong.
    fn shell(
        &self,
        block: LandblockId,
        b: &dereth_assets::world::BuildInfo,
        cell: CellId,
        stats: &mut BuildingLoadStats,
    ) -> Option<BuildingGeometry> {
        if b.id.0 >> 24 != 0x01 {
            stats.not_a_gfxobj += 1;
            tracing::warn!(
                "landblock {:04X} building {:#010X} is not a GfxObj id, so its \
                 part 0 frame is not the identity; the shell is NOT registered \
                 while combining frames",
                block.0,
                b.id.0
            );
            return None;
        }
        let Ok(bytes) = self.store().read_typed(DbType::GfxObj, b.id) else {
            stats.undecodable += 1;
            return None;
        };
        let Ok(g) = dereth_assets::GfxObj::decode_payload(b.id, &bytes) else {
            stats.undecodable += 1;
            return None;
        };
        let physics_bsp = gfxobj_physics_bsp(&g).map(Arc::new);
        if physics_bsp.is_none() {
            // Registered anyway: the client's building shell exists whether or not its `GfxObj`
            // carries a tree, and collision discovery answers `OK_TS` for it.
            stats.without_bsp += 1;
        }
        Some(BuildingGeometry {
            parts: vec![PhysicsPart {
                pos: Position::new(cell, b.frame),
                gfxobj_scale: 1.0,
                physics_bsp,
                // A building shell is never an argument to bounding-box cell collection --
                // it is the *cell's* geometry, not an object
                // registered in one -- so nothing reads these two, and they are filled from the
                // same decode anyway rather than left as a lie about the GfxObj.
                bound_box: Some(crate::setup::gfx_bound_box(&g)),
                drawing_sphere: crate::setup::drawing_sphere(&g),
                // The always-2D property is read only while discovering
                // outside land cells over the parts of an object registered in a cell.
                // A building shell is the cell's own geometry and is never
                // handed to that, so this is unread here; it is filled from the same decode as
                // the two fields above rather than left as a lie about the `GfxObj`.
                first_degrade_mode: crate::setup::first_degrade_mode(&self.store(), &g),
            }],
        })
    }

    /// What the building half of [`Self::load_block_cells`] did. Asserted on by the tests.
    #[must_use]
    pub fn building_stats(&self) -> BuildingLoadStats {
        self.building_stats.lock().map(|s| *s).unwrap_or_default()
    }

    /// How many land cells hold a building shell, for the log line and the tests.
    #[must_use]
    pub fn resident_buildings(&self) -> usize {
        self.buildings.lock().map_or(0, |b| b.len())
    }

    /// How many interior cells are resident, and what the decode did. Asserted on by the tests.
    #[must_use]
    pub fn cell_stats(&self) -> CellLoadStats {
        self.loader.lock().map(|l| l.stats).unwrap_or_default()
    }

    /// How many interior cells are loaded, for the log line and the tests.
    #[must_use]
    pub fn resident_cells(&self) -> usize {
        self.cells.lock().map_or(0, |c| c.len())
    }

    /// Every row of the collision-block cache, **cached negatives included**. Instrumentation.
    ///
    /// [`Self::resident`] filters to `Some`, which is the right number for "how much collision
    /// geometry is held" and the wrong one for "is this map growing": a `None` row is a world-edge
    /// or never-authored block and costs a key for ever if nothing removes it.
    /// [`Self::release_visible_cells`] removes the departed key, present or negative; this is the
    /// gauge that says so over many departures rather than one.
    #[must_use]
    pub fn cached_block_count(&self) -> usize {
        self.blocks.lock().map_or(0, |b| b.len())
    }

    /// Land cells that own at least one building's interior cells — `building_cells`, the other
    /// half of what [`Self::release_visible_cells`] hands back. Instrumentation.
    #[must_use]
    pub fn building_cell_count(&self) -> usize {
        self.building_cells.lock().map_or(0, |b| b.len())
    }

    /// The terrain height under a block-local `(x, y)`, from the collision polygon the point falls
    /// in.
    ///
    /// This is the same surface used by terrain collision to stand an object on:
    /// `LandblockCollision::find_terrain_poly` picks the cell's triangle by the split flag and
    /// the plane-height calculation solves for z. It is used to *place* the character,
    /// never to move it — moving belongs to the physics object's update and nothing here second-guesses
    /// it.
    #[must_use]
    pub fn ground_height(&self, id: LandblockId, x: f32, y: f32) -> Option<f32> {
        let block = self.landblock(id)?;
        let mut cell = id.cell(1);
        let mut local = dereth_primitives::Vec3::new(x, y, 0.0);
        dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut local);
        let poly = block.find_terrain_poly(cell.index(), local)?;
        let mut p = dereth_primitives::Vec3::new(x, y, 0.0);
        poly.plane.set_height(&mut p).then_some(p.z)
    }

    /// Read and build one block. `None` for a block outside the 255x255 world, one the cell dat
    /// does not carry, or one `LandblockCollision::build` refuses as degenerate.
    ///
    /// **This is the run-time cache miss.** The native cache-miss path reaches the
    /// server request producer -- the client's one `0xF7E3`
    /// producer -- only when the object is neither in memory nor on disk, and this read is the
    /// on-disk half for a `LandBlock` record. `DatError::NotFound` is exactly "the disk controller
    /// does not have it"; every other error means the record *is* there and is bad, which the
    /// server cannot fix by sending it again, so only `NotFound` is reported.
    fn build(&self, id: LandblockId) -> Option<Arc<LandblockCollision>> {
        let did = crate::landblock::landblock_did(id.0);
        let bytes = match self.store().read_typed(DbType::LandBlock, did) {
            Ok(bytes) => bytes,
            Err(dereth_dat::DatError::NotFound(_)) => {
                self.note_missing(DbType::LandBlock, did);
                return None;
            }
            Err(_) => return None,
        };
        let lb = CellLandblock::decode_payload(did, &bytes).ok()?;
        // `side_cell_count` is 8 for every physics landblock: the LOD rings are a *rendering*
        // reduction, and `LandblockCollision::build` refuses anything else because a degenerate
        // block carries no physics geometry at all.
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
        if let Ok(c) = self.blocks.lock() {
            if let Some(hit) = c.get(&id.0) {
                return hit.clone();
            }
        }
        let built = self.build(id);
        if let Ok(mut c) = self.blocks.lock() {
            c.insert(id.0, built.clone());
        }
        built
    }

    /// Check whether a landblock is resident.
    ///
    /// Residency is a different answer, not just a hitch: after a teleport the original client
    /// has freed the departed blocks, so landblock lookup returns NULL. When cell visibility
    /// returns NULL, object-position adjustment leaves `cell == NULL`, and the placement-failure
    /// path queues the object for destruction 25 s later. A block that answered for ever would
    /// let an object that arrived for a landblock the window had already left **place
    /// successfully**, keeping its cell, its draw and its table entry for the rest of the session.
    ///
    /// [`Self::prefetched`] is the load state: [`Self::load_block_cells`]
    /// sets it and [`Self::release_visible_cells`]
    /// (the second step of land-block release-all) clears
    /// it, matching the original loaded-state test.
    fn landblock_resident(&self, id: LandblockId) -> bool {
        self.prefetched.lock().is_ok_and(|p| p.contains(&id.0))
    }

    fn height_table(&self) -> &[f32; LAND_HEIGHT_TABLE_LEN] {
        &self.table
    }

    fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>> {
        // This lookup never loads: it looks in the visible-cell table and
        // returns NULL for anything not there. `load_block_cells` is what fills it.
        self.cells.lock().ok()?.get(&cell.0).cloned()
    }

    fn building_cells(&self, cell: CellId) -> Vec<CellId> {
        self.building_cells
            .lock()
            .ok()
            .and_then(|m| m.get(&cell.0).cloned())
            .unwrap_or_default()
    }

    fn building(&self, cell: CellId) -> Option<Arc<BuildingGeometry>> {
        // Sort-cell collision discovery checks this field for null. Like `env_cell`
        // it never loads: `load_block_cells` is what fills the table.
        self.buildings.lock().ok()?.get(&cell.0).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The retail land source, or **fail**.
    ///
    /// In a checkout with no retail dats a skipping helper would let both landblock tests report
    /// `... ok` having loaded no block.
    fn source() -> DatLandSource {
        let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are these tests' oracle and they are not under {} -- \
                 set DERETH_TEST_DAT_DIR",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let region = crate::landblock::load_region(&store).expect("the region decodes");
        DatLandSource::new(store, &region).expect("the retail height table is valid")
    }

    // Oracle: the retail cell dat, through `LandblockCollision::build`, whose own tests cover
    // all 65,025 landblocks. What this adds is that the *wiring* addresses the right record
    // -- `blockId | 0xFFFF`, with (x, y) in the order LandDefs indexes by (contract 9.10). A
    // transposed index still produces a self-consistent world, so the check is that the seams
    // agree with the neighbours the id arithmetic picks out.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn holtburg_and_its_neighbours_load_and_the_world_edge_does_not() {
        let s = source();
        let holtburg = LandblockId::new(0xA9, 0xB4);
        let b = s.landblock(holtburg).expect("Holtburg is in the cell dat");
        assert_eq!(b.id, holtburg);
        assert_eq!(
            b.side_cell_count, 8,
            "every physics landblock is full detail"
        );
        assert_eq!(b.vertices.len(), 81);
        assert_eq!(b.polygons.len(), 128, "two triangles per cell");
        // The height table is the region's, so the vertices carry real heights, not indices.
        assert!(b.vertices.iter().all(|v| v.z >= 0.0 && v.z <= 800.0));
        // Cached: the same allocation comes back.
        assert!(Arc::ptr_eq(&b, &s.landblock(holtburg).expect("cached")));

        // The row and column at index 8 are the same physical vertices as index 0 of the
        // neighbour, and the dat is authored so they agree.
        let east = s
            .landblock(LandblockId::new(0xAA, 0xB4))
            .expect("the block to the east");
        for j in 0..9 {
            assert_eq!(
                b.vertices[8 * 9 + j].z,
                east.vertices[j].z,
                "east seam at j={j}"
            );
        }
        let north = s
            .landblock(LandblockId::new(0xA9, 0xB5))
            .expect("the block to the north");
        for i in 0..9 {
            assert_eq!(
                b.vertices[i * 9 + 8].z,
                north.vertices[i * 9].z,
                "north seam at i={i}"
            );
        }

        // 0xFF is outside the 255x255 world: returns NULL and so does this.
        assert!(s.landblock(LandblockId::new(0xFF, 0xFF)).is_none());
        // `GetVisible` never loads, so an interior cell nobody prefetched is not there.
        assert!(
            s.env_cell(CellId(0xA9B4_0100)).is_none(),
            "env_cell loaded from disk; it is GetVisible, not "
        );
    }

    /// **A departed block gives up the land source's collision-cache ownership.**
    ///
    /// Releasing a landblock releases its database object after its
    /// objects and visible cells. Retail's database release may retain the allocation in the cache's
    /// bounded free list; that is not permission for this unbounded per-world map to retain one
    /// strong reference for every block visited. Existing users may still own the collision object,
    /// exactly as an `Arc` permits, while the next visit rebuilds identical geometry from the DAT.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn released_landblock_drops_cache_ownership_while_readers_survive_and_reload_matches() {
        let s = source();
        let home = LandblockId(0xA9B4);
        let neighbour = LandblockId(0xAAB4);
        let missing = LandblockId(0xFFFF);

        let held = s.landblock(home).expect("Holtburg collision geometry");
        let adjacent = s
            .landblock(neighbour)
            .expect("east-neighbour collision geometry");
        assert!(
            s.landblock(missing).is_none(),
            "the world edge is a cached negative entry"
        );
        let initial_entries = s.blocks.lock().expect("block cache").len();
        assert_eq!(
            initial_entries, 3,
            "the fixture owns two blocks and one negative entry"
        );
        assert_eq!(
            Arc::strong_count(&held),
            2,
            "the caller and cache are the only owners"
        );

        s.release_visible_cells(home);
        {
            let cache = s.blocks.lock().expect("block cache");
            assert!(
                !cache.contains_key(&home.0),
                "the departed collision block remained cached"
            );
            assert!(
                cache
                    .get(&neighbour.0)
                    .and_then(Option::as_ref)
                    .is_some_and(|b| Arc::ptr_eq(b, &adjacent)),
                "an exact release disturbed the neighbouring block"
            );
            assert!(
                matches!(cache.get(&missing.0), Some(None)),
                "an unrelated negative changed"
            );
        }

        assert_eq!(
            Arc::strong_count(&held),
            1,
            "the cache retained a strong departed owner"
        );
        assert_eq!(
            held.id, home,
            "a retained reader stopped being usable after cache release"
        );
        assert_eq!(held.vertices.len(), 81);
        let reloaded = s.landblock(home).expect("a revisited block reloads");
        assert!(
            !Arc::ptr_eq(&held, &reloaded),
            "the released cache owner returned the old Arc"
        );
        assert_eq!(reloaded.id, held.id);
        assert_eq!(reloaded.side_vertex_count, held.side_vertex_count);
        assert_eq!(reloaded.side_cell_count, held.side_cell_count);
        assert_eq!(reloaded.side_polygon_count, held.side_polygon_count);
        assert_eq!(
            reloaded.height, held.height,
            "reload changed the authored height indices"
        );
        assert_eq!(
            reloaded.terrain, held.terrain,
            "reload changed the authored terrain words"
        );
        assert_eq!(reloaded.has_info, held.has_info);
        assert_eq!(
            reloaded.vertices, held.vertices,
            "reload changed collision vertices"
        );
        assert_eq!(
            reloaded.split, held.split,
            "reload changed authored triangle splits"
        );
        assert_eq!(
            reloaded.polygons, held.polygons,
            "reload changed collision triangles"
        );
        assert_eq!(reloaded.cell_water, held.cell_water);
        assert_eq!(reloaded.water_type, held.water_type);
        assert_eq!(reloaded.max_zval, held.max_zval);
        assert_eq!(reloaded.min_zval, held.min_zval);
        assert_eq!(
            s.blocks.lock().expect("block cache").len(),
            initial_entries,
            "a release/revisit cycle grew the exact cache instead of recycling its entry"
        );
    }

    // Oracle: `client_cell_1.dat` and its cell-id encoding — a cell id is
    // `blockId << 16 | index`, so a block owns exactly the ids sharing its **top 16 bits**.
    //
    // This is the literal pin for that shift, and it is load-bearing: the release scan
    // is a prefix test, and a mask of `0xFF00_0000` (the *data-id space* mask, which is the
    // plausible typo, and which this file's `environment()` genuinely uses) would take three
    // neighbouring blocks with every release while every count in the differential still balanced.
    // The ids below are written out in full rather than composed from `LandblockId`, because a
    // composition that agreed with the code under test would prove nothing.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn releasing_one_block_takes_its_own_cells_and_none_of_its_neighbours() {
        let s = source();
        let resident = |base: u32| {
            (0..64u32)
                .filter(|i| s.env_cell(CellId(base | (0x0100 + i))).is_some())
                .count()
        };
        // Holtburg, plus the nearest block with an interior that differs from it in the **high**
        // byte alone and the nearest that differs in the **low** byte alone. Both are needed and
        // neither on its own would do: a `0xFF00_0000` mask keeps the high byte and would take the
        // second, a `0x00FF_0000` mask would take the first. Holtburg's immediate neighbours are
        // fields, so the two are searched for rather than guessed, and the search asserts it found
        // them.
        s.load_block_cells(LandblockId(0xA9B4));
        let home = resident(0xA9B4_0000);
        assert!(home > 0, "Holtburg has buildings, so it has interior cells");
        let find = |ids: Vec<u16>| -> Option<(u16, usize)> {
            ids.into_iter().find_map(|id| {
                s.load_block_cells(LandblockId(id));
                let n = resident(u32::from(id) << 16);
                (n > 0).then_some((id, n))
            })
        };
        // Same low byte (0xB4), walking the high byte outwards; then the same the other way.
        let across: Vec<u16> = (1..=8u16)
            .flat_map(|d| [0xA9B4 + (d << 8), 0xA9B4 - (d << 8)])
            .collect();
        let along: Vec<u16> = (1..=8u16).flat_map(|d| [0xA9B4 + d, 0xA9B4 - d]).collect();
        let (hi_id, east) = find(across).expect("a block with an interior at Holtburg's latitude");
        let (lo_id, north) = find(along).expect("a block with an interior at Holtburg's longitude");
        assert_eq!(
            hi_id & 0x00FF,
            0x00B4,
            "the high-byte neighbour changed the low byte too"
        );
        assert_eq!(
            lo_id & 0xFF00,
            0xA900,
            "the low-byte neighbour changed the high byte too"
        );
        let (hi_base, lo_base) = (u32::from(hi_id) << 16, u32::from(lo_id) << 16);
        let before = s.resident_cells();
        assert_eq!(s.cell_stats().released, 0, "nothing has been released yet");

        let (cells, _shells) = s.release_visible_cells(LandblockId(0xA9B4));
        assert!(cells > 0, "releasing Holtburg released no cell");
        assert_eq!(
            resident(0xA9B4_0000),
            0,
            "Holtburg kept {} cells",
            resident(0xA9B4_0000)
        );
        assert_eq!(
            resident(hi_base),
            east,
            "block {hi_id:#06X} lost cells with Holtburg"
        );
        assert_eq!(
            resident(lo_base),
            north,
            "block {lo_id:#06X} lost cells with Holtburg"
        );
        assert_eq!(
            s.resident_cells(),
            before - usize::try_from(cells).expect("small"),
            "the residency does not match what the release said it took"
        );
        assert_eq!(s.cell_stats().released, cells);
        assert_eq!(s.cell_stats().blocks_released, 1);
        assert_eq!(
            s.resident_cells() as u64,
            s.cell_stats().loaded - s.cell_stats().released,
            "resident is not loaded minus released"
        );

        // Adding a visible cell can put it back: the prefetch mark went with the
        // release, so the second `load_block_cells` is a load and not the free set-lookup the
        // first one's idempotence gives.
        s.load_block_cells(LandblockId(0xA9B4));
        assert_eq!(
            resident(0xA9B4_0000),
            home,
            "the released block did not come back whole"
        );
        assert_eq!(
            s.resident_cells(),
            before,
            "the residency did not come back"
        );
    }
}
