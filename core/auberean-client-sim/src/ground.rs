//! The world's physics about the player: what every body moves against.
//!
//! - **The table.** One table holds every body in the world (the player's, everyone else's, the
//!   objects with a shape and the scenery) and the cells' collision lists; it takes the physics
//!   types' bits from the install's property list and knows the physics materials bodies touch.
//! - **The cells follow the player** ([`Ground::follow`]): a new landblock outdoors, or a new
//!   interior cell, is asked of the client's data cache ([`crate::cells`]); once the ask is in, the
//!   interior cells of the player's block and those beside it are loaded and the near window of
//!   landblocks ([`crate::near`]) follows the cell (inside, the land cell under it when its cell
//!   sees the outdoors). A client whose cache holds no cell file reads the world's own file as it
//!   is, and the cells are in at once.
//! - **What is let go of** ([`Ground::cells_let_go`]): the land cells of each landblock the window
//!   let go of, and the interior cells no longer about the player's block, for the objects standing
//!   in them to leave the world.

use std::collections::{BTreeMap, HashSet};

use auberean_client_runtime::cache::DataCache;
use auberean_client_runtime::download::DataDownload;
use auberean_client_runtime::Client;
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;
use auberean_physics::world::World;
use dereth_primitives::{CellId, LandblockId, Position};

use crate::cells::{self, CellAsks, Visible};
use crate::near::{self, NearBlocks};
use crate::{WorldData, FIRST_INTERIOR};

/// The world's physics about the player.
pub struct Ground {
    pub land: Land,
    pub trees: Trees,
    /// The world every body moves against: the player's, everyone else's, the objects with a
    /// shape and the scenery, and the cells' collision lists.
    pub table: World,
    /// The blocks whose interior cells were loaded.
    pub blocks: HashSet<(u8, u8)>,
    /// The landblocks kept about the player.
    pub near: NearBlocks,
    /// The cells about the player, asked for through the data cache.
    pub cells: CellAsks,
    /// The session's data cache the world's records read the cell file through (none before a
    /// session with data files).
    cache: Option<DataCache>,
    /// The block the interior cells loaded are about (the player's), when there is a player.
    interiors_about: Option<(u8, u8)>,
}

impl core::fmt::Debug for Ground {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Ground")
            .field("near", &self.near)
            .field("blocks", &self.blocks.len())
            .finish_non_exhaustive()
    }
}

/// The cells let go of since last asked ([`Ground::cells_let_go`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LetGo {
    /// Every land cell of each landblock let go of, then the interior cells no longer about the
    /// player's block.
    pub cells: Vec<u32>,
    /// The landblocks the near window let go of, and those it was handed.
    pub released: Vec<(u8, u8)>,
    pub handed_out: Vec<(u8, u8)>,
}

/// The landblock a cell is in.
fn block_of(cell: u32) -> (u8, u8) {
    (
        u8::try_from(cell >> 24).unwrap_or(0),
        u8::try_from((cell >> 16) & 0xFF).unwrap_or(0),
    )
}

/// Whether block `b` is within the near window about block `about`.
fn about(b: (u8, u8), about: Option<(u8, u8)>) -> bool {
    about.is_some_and(|a| {
        let r = u8::try_from(cells::NEAR_RADIUS).unwrap_or(1);
        b.0.abs_diff(a.0) <= r && b.1.abs_diff(a.1) <= r
    })
}

impl Ground {
    /// The ground of `world`, with nothing loaded yet: its table takes the physics types' bits of
    /// the install's property list and the physics materials what bodies touch may name.
    #[must_use]
    pub fn new(world: &WorldData) -> Self {
        let mut table = World::new();
        if let Some(list) = world
            .records
            .get::<auberean_assets::property::PropertyList>(0x3400_0000)
        {
            table.set_property_list(list);
        }
        // D2-06766
        for id in world.records.portal_ids(0x3700_0000, 0x37FF_FFFF) {
            if let Some(m) = world.records.get(id) {
                table.add_material(id, m);
            }
        }
        Self {
            land: Land::new(),
            trees: Trees::default(),
            table,
            blocks: HashSet::new(),
            near: NearBlocks::default(),
            cells: CellAsks::default(),
            cache: None,
            interiors_about: None,
        }
    }

    /// Whether the client's data cache holds a cell file: the cells about the player are asked
    /// of it then; a client given none reads the world's own file as it is.
    #[must_use]
    pub fn cache_has_cells(client: &Client) -> bool {
        let cache = client.data_download().cache();
        !cache.is_empty()
            && cache.stores().by_slot[auberean_dat::disk::Slot::Cell as usize].is_some()
    }

    /// The client's data cache for this session: the world's records read the cell file through
    /// it from now on (a record the server sent is read as the file's own), and the cells' asks
    /// are grown by its rings. A client whose cache holds no cell file leaves the world's file read
    /// as it is.
    pub fn attach_data_cache(&mut self, client: &mut Client, world: &WorldData) {
        if !Self::cache_has_cells(client) {
            if self.cache.take().is_some() {
                world.records.read_cells_through(None);
            }
            return;
        }
        let d = client.data_download_mut();
        let cache = d.cache().clone();
        if self.cache.as_ref().is_some_and(|c| c.is(&cache)) {
            return;
        }
        let through = cache.clone();
        world
            .records
            .read_cells_through(Some(std::sync::Arc::new(move |id, kind| {
                through
                    .stores()
                    .load_data(auberean_dat::QualifiedId { id, kind })
            })));
        d.requests.set_hook(
            cells::CELLS,
            Some(Box::new(cells::CellRings::new(
                std::sync::Arc::new(world.region.clone()),
                self.cells.far.clone(),
            ))),
        );
        self.cells.forget();
        self.cache = Some(cache);
    }

    /// What the data cache told the cells' asks: an ask in, or one to let go.
    pub fn hear_cells(&mut self, client: &mut Client) {
        let d: &mut DataDownload = client.data_download_mut();
        let mut done = Vec::new();
        d.told.retain(|e| match e {
            auberean_assets::requests::Event::BatchDone { to, batch, .. }
                if *to == cells::CELLS =>
            {
                done.push(*batch);
                false
            }
            auberean_assets::requests::Event::Finished { to, .. } => *to != cells::CELLS,
            _ => true,
        });
        for b in done {
            self.cells.on_batch_done(d, b);
        }
    }

    /// The interior cells of the landblocks about `block` (its own and those beside it), each
    /// block's once its info is in; when `again`, a block some of whose cells were missing when
    /// it was loaded is loaded again (they may have come since).
    pub fn load_interiors(&mut self, world: &WorldData, block: LandblockId, again: bool) {
        let records = &world.records;
        for dx in -1i32..=1 {
            for dy in -1i32..=1 {
                let (Ok(x), Ok(y)) = (
                    u8::try_from(i32::from(block.x()) + dx),
                    u8::try_from(i32::from(block.y()) + dy),
                ) else {
                    continue;
                };
                let (bx, by) = (i32::from(x), i32::from(y));
                let Some(info) = records.landblock_info(bx, by) else {
                    continue;
                };
                let complete = || {
                    (0..info.cell_count).all(|k| {
                        let low = u32::from(x) << 24 | u32::from(y) << 16 | (FIRST_INTERIOR + k);
                        self.land.cell(CellId(low)).is_some()
                    })
                };
                if self.blocks.insert((x, y)) || (again && !complete()) {
                    auberean_physics::interior::add_block_cells(
                        &self.land,
                        records,
                        &mut self.trees,
                        x,
                        y,
                    );
                }
            }
        }
    }

    /// The cells about the player follow the cell its body stands in (`at`; none without a
    /// player): a new landblock outdoors, or a new interior cell, is asked of the data cache (with
    /// what it needs, a ring of landblocks at a time); once the ask is in, the interior cells about
    /// the player are loaded and the near window of landblocks follows the cell (inside, the land
    /// cell under it when its cell sees the outdoors). `placed`: the server just put the player
    /// there, and the world stands still until its cells are in. `now` is the host's running time,
    /// seconds, which the blocks handed to the window are stamped with. Answers where the cells
    /// stand; whatever it answers, the cells let go of are then the caller's to take
    /// ([`Self::cells_let_go`]).
    // D2-10803, D2-10766
    pub fn follow(
        &mut self,
        client: &mut Client,
        world: &WorldData,
        at: Option<Position>,
        placed: bool,
        now: f64,
    ) -> Visible {
        let cell = at.map_or(0, |p| p.cell.0);
        let visible = if Self::cache_has_cells(client) {
            self.cells.update(client.data_download_mut(), cell, placed)
        } else if cell == 0 {
            Visible::Released
        } else {
            Visible::Finish
        };
        match visible {
            Visible::Released => {
                self.near.release_cells(&self.land);
                return visible;
            }
            Visible::Waiting => return visible,
            Visible::Finish => {}
        }
        let fresh = self.cells.take_fresh();
        if let Some(p) = at {
            self.load_interiors(world, p.cell.landblock(), fresh);
        }
        let (cell, outside) = match at {
            Some(pos) => {
                let inside = pos.cell.0 & 0xFFFF >= FIRST_INTERIOR;
                let outside = inside
                    .then(|| self.land.cell(pos.cell))
                    .flatten()
                    .filter(|g| g.seen_outside)
                    .map(|_| auberean_physics::geom::position::outside_cell(&pos).0)
                    .filter(|c| *c != 0);
                (pos.cell.0, outside)
            }
            None => (0, None),
        };
        let mut m = near::Makers {
            land: &self.land,
            records: &world.records,
            region: &world.region,
            trees: &mut self.trees,
            interiors: &mut self.blocks,
            now,
        };
        self.near.update_visible_cells(&mut m, cell, outside);
        visible
    }

    /// The cells let go of since last asked: every land cell of each landblock the land let go
    /// of, and the interior cells (those the client was told a body stands in, `reported`) of the
    /// blocks no longer about the player's (`player`, the player's cell; none without one).
    // D2-03638, D2-03871
    pub fn cells_let_go(&mut self, player: Option<u32>, reported: &BTreeMap<u64, u32>) -> LetGo {
        let mut out = LetGo::default();
        for (x, y) in self.near.take_released() {
            let base = u32::from(x) << 24 | u32::from(y) << 16;
            out.cells.extend((1..=64).map(|i| base | i));
            out.released.push((x, y));
        }
        out.handed_out = self.near.take_handed_out();
        let now = player.map(block_of);
        if now != self.interiors_about {
            let was = core::mem::replace(&mut self.interiors_about, now);
            let mut inside: Vec<u32> = reported
                .values()
                .copied()
                .filter(|c| c & 0xFFFF >= FIRST_INTERIOR)
                .filter(|c| about(block_of(*c), was) && !about(block_of(*c), now))
                .collect();
            inside.sort_unstable();
            inside.dedup();
            out.cells.extend(inside);
        }
        out
    }

    /// The world left: the cells' asks let go of, the land let go of (the objects standing in it
    /// with it, which [`Self::cells_let_go`] then tells), and the world entered next starts with
    /// fresh free blocks.
    // D2-10660, D2-10765, D2-10807
    pub fn reset_world_state(&mut self, client: &mut Client) {
        self.cells.release(client.data_download_mut());
        self.near.reset_world_state(&self.land);
    }
}
