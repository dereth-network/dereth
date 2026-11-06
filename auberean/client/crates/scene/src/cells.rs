//! The world's cells about the player, got through the client's data cache: what the client loads
//! when the player enters another landblock or another interior cell, a ring of landblocks at a
//! time, before the land and the cells about it are made.
//!
//! - **Asked for.** When the player's cell changes (outdoors, only when it changes landblock), and
//!   no ask is under way, the client asks the data cache for that landblock (outdoors) or that
//!   interior cell, with the records each needs, under one request ([`CellAsks::update`]).
//!   A record the files lack is asked of the server; a refused one ends its part of the request.
//! - **A ring at a time.** Each time the request finishes, it is grown by the next ring
//!   ([`CellRings`]): the cell's visible cells, its landblock (when it sees the outdoors) and its
//!   landblock's info, the first time; then the landblocks one block out with all they need; then
//!   ring after ring of landblocks alone out to past the far landscape; then, for the landblocks
//!   about the player's, the interior cells their infos see from outside and the scenes their
//!   vertices grow.
//! - **Made once in.** Only when the whole request has finished is the land made about the
//!   player's cell ([`crate::near`]): the window of landblocks follows it, and the player's
//!   interior cell is taken.
//! - **Blocking.** A placement by the server (entering the world, a teleport) asks the next update
//!   to block: while a block's request is under way, the world stands still (no physics, no
//!   animation, the camera not placed), as the client freezes its world until the cells it was
//!   sent to are in.

use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;

use auberean_assets::landblock::{Cell, Landblock, LandblockInfo};
use auberean_assets::requests::{Batch, Listener, OnFinished, Outcome, GATHER_NONE};
use auberean_assets::Database;
use auberean_client_sdk::runtime::download::DataDownload;
use auberean_dat::{QualifiedId, QualifiedIds, RecordType};
use auberean_world::records::RegionSet;

/// The number the cells' requests are told by (the data cache's listeners' table).
pub const CELLS: u32 = auberean_client_sdk::runtime::download::listeners::CELLS;
/// The gathering that asks for the records each record needs.
pub const GATHER_ALL: u32 = 0x4000_0009;
/// Landblocks about the player's each way that are kept with their objects.
pub const NEAR_RADIUS: i32 = 1;
/// The far landscape's reach before the preferences say.
pub const FAR_RADIUS: i32 = 4;
/// The lowest interior cell's low word.
const FIRST_INTERIOR: u32 = 0x100;

fn q(id: u32, kind: RecordType) -> QualifiedId {
    QualifiedId { id, kind: kind.0 }
}

/// The landblock id of the block of `id`.
fn landblock_of(id: u32) -> u32 {
    (id & 0xFFFF_0000) | 0xFFFF
}

/// Whether `cell` is a land cell (not an interior one).
fn is_land(cell: u32) -> bool {
    cell & 0xFFFF < FIRST_INTERIOR
}

/// How the cells' request is grown each time it finishes: the asker the data cache calls the
/// moment the request does, before it counts as done.
pub struct CellRings {
    near: i32,
    far: Arc<AtomicI32>,
    /// The world's size in landblocks: wide (east) and long (north).
    size: (i32, i32),
    /// The region, for the scenes the landblocks' vertices grow.
    region: Arc<RegionSet>,
}

impl core::fmt::Debug for CellRings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CellRings")
            .field("near", &self.near)
            .field("far", &self.far.load(Ordering::Relaxed))
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl CellRings {
    /// Rings for a world of `region`, out to the far landscape `far` says.
    #[must_use]
    pub fn new(region: Arc<RegionSet>, far: Arc<AtomicI32>) -> Self {
        let size = (
            i32::try_from(region.region.blocks_wide).unwrap_or(0),
            i32::try_from(region.region.blocks_long).unwrap_or(0),
        );
        Self {
            near: NEAR_RADIUS,
            far,
            size,
            region,
        }
    }

    /// The landblock `(bx + dx, by + dy)`, when it is in the world.
    fn block(&self, (bx, by): (i32, i32), (dx, dy): (i32, i32)) -> Option<u32> {
        let (x, y) = (bx + dx, by + dy);
        if !(0..self.size.0).contains(&x) || !(0..self.size.1).contains(&y) {
            return None;
        }
        Some((u32::try_from(x).ok()? << 24) | (u32::try_from(y).ok()? << 16) | 0xFFFF)
    }

    /// The landblocks `ring` blocks out from the block of `id`, each its own corner of the ring.
    // D2-10763
    fn ring(&self, id: u32, ring: i32, out: &mut QualifiedIds) {
        let at = (
            i32::try_from(id >> 24).unwrap_or(0),
            i32::try_from((id >> 16) & 0xFF).unwrap_or(0),
        );
        for i in -ring..=ring {
            for j in -ring..=ring {
                if i != -ring && i != ring && j != -ring && j != ring {
                    continue;
                }
                if let Some(b) = self.block(at, (i, j)) {
                    out.add_typed(b, RecordType::LANDBLOCK.0);
                }
            }
        }
    }

    /// What the landblocks `ring` blocks out need loaded about the player: the interior cells
    /// their infos see from outside, and the scenes their vertices grow.
    // D2-10764, D2-03738, D2-03711
    fn scene_objects(&self, db: &Database, id: u32, ring: i32, out: &mut QualifiedIds) {
        let mut blocks = QualifiedIds::new();
        self.ring(id, ring, &mut blocks);
        for b in blocks.iter() {
            let Some(lb) = db.get_typed::<Landblock>(b.id, RecordType::LANDBLOCK.0) else {
                continue;
            };
            if lb.info.0 != 0 {
                if let Some(info) =
                    db.get_typed::<LandblockInfo>(lb.info.0, RecordType::LANDBLOCK_INFO.0)
                {
                    for &c in &info.outside_visible {
                        out.add_typed(c, RecordType::CELL.0);
                    }
                }
            }
            let r = &self.region;
            let Some(scenes) = r.scenes.as_ref() else {
                continue;
            };
            let at = (b.id >> 24, (b.id >> 16) & 0xFF);
            let grown = auberean_world::scenery::vertex_scenes(
                &r.land,
                &r.terrain,
                scenes,
                at,
                &lb.vertices,
            );
            for s in grown.into_iter().flatten() {
                out.add_typed(s.0, RecordType::SCENE.0);
            }
        }
    }
}

impl OnFinished for CellRings {
    /// The cells' request finished for the `times`th time: an interior cell's adds, the first
    /// time, the cells it sees, its landblock when it sees the outdoors, and its landblock's info
    /// (one that does not see the outdoors adds no more); a landblock's (or an outdoors-seeing
    /// cell's) adds, by how many times it has finished, the landblocks one block further out (all
    /// they need while within the near radius, themselves alone out to one past the far
    /// landscape), then what the landblocks about it need loaded.
    // D2-10768
    fn finished(
        &mut self,
        db: &Database,
        record: QualifiedId,
        _outcome: Outcome,
        _user: u32,
        times: u32,
    ) -> Vec<(u32, Vec<QualifiedId>)> {
        let mut all = QualifiedIds::new();
        let mut alone = QualifiedIds::new();
        let done = |alone: QualifiedIds, all: QualifiedIds| {
            [(GATHER_NONE, alone), (GATHER_ALL, all)]
                .into_iter()
                .filter(|(_, ids)| !ids.is_empty())
                .map(|(g, ids)| (g, ids.iter().collect()))
                .collect()
        };
        let id = record.id;
        if record.kind == RecordType::CELL.0 {
            let Some(cell) = db.get_typed::<Cell>(id, RecordType::CELL.0) else {
                return done(alone, all);
            };
            let outside = cell.flags & 1 != 0;
            if times == 1 {
                // D2-03529
                for &v in &cell.visible {
                    all.add_typed((id & 0xFFFF_0000) | u32::from(v), RecordType::CELL.0);
                }
                if outside {
                    all.add_typed(landblock_of(id), RecordType::LANDBLOCK.0);
                }
                all.add_typed((id & 0xFFFF_0000) | 0xFFFE, RecordType::LANDBLOCK_INFO.0);
            }
            if !outside {
                return done(alone, all);
            }
        } else if record.kind != RecordType::LANDBLOCK.0 {
            return done(alone, all);
        }
        let near = self.near;
        let total = self.far.load(Ordering::Relaxed) + 1 + near;
        let side = usize::try_from(total * 2 + 1).unwrap_or(0);
        db.set_free_list_ideal(RecordType::LANDBLOCK.0, side * side * 3 / 2);
        drop(db.get_typed::<Landblock>(landblock_of(id), RecordType::LANDBLOCK.0));
        let r = i32::try_from(times).unwrap_or(i32::MAX) - 1;
        let ring = i32::try_from(times).unwrap_or(i32::MAX);
        if r < near {
            self.ring(id, ring, &mut all);
        } else if r < total {
            self.ring(id, ring, &mut alone);
        } else if r == total {
            for k in 0..=near {
                self.scene_objects(db, id, k, &mut all);
            }
        }
        done(alone, all)
    }
}

/// The client's cells about the player and the ask for them.
#[derive(Debug)]
pub struct CellAsks {
    /// The cell the last ask was for (0: none).
    asked_for: u32,
    /// The ask under way (none when there is none).
    batch: Batch,
    /// The world stands still until the ask is in.
    pub blocking: bool,
    /// An ask is under way.
    pub pending: bool,
    /// An ask has come in since the cells were last made.
    fresh: bool,
    /// The far landscape's reach, shared with the rings.
    pub far: Arc<AtomicI32>,
}

impl Default for CellAsks {
    // D2-10761
    fn default() -> Self {
        Self {
            asked_for: 0,
            batch: Batch::NONE,
            blocking: false,
            pending: false,
            fresh: false,
            far: Arc::new(AtomicI32::new(FAR_RADIUS)),
        }
    }
}

/// What an update of the visible cells did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visible {
    /// No cell: everything let go of.
    Released,
    /// The ask is under way: nothing to make yet.
    Waiting,
    /// The ask is in (or there was none): make the land and the cells about the player's cell.
    Finish,
}

impl CellAsks {
    /// The far landscape reaches `blocks` about the viewer's.
    // D2-10758, D2-10771
    pub fn set_far_radius(&self, blocks: i32) {
        self.far.store(blocks, Ordering::Relaxed);
    }

    /// Ask the data cache for the cells of `cell`: its landblock (outdoors) or itself (inside),
    /// with what each needs, under one request grown ring by ring.
    // D2-10767
    fn async_get_cells(&mut self, download: &mut DataDownload, cell: u32) {
        if cell == 0 {
            return;
        }
        let first = if is_land(cell) {
            q(landblock_of(cell), RecordType::LANDBLOCK)
        } else {
            q(cell, RecordType::CELL)
        };
        self.pending = true;
        self.batch = download.get_many(GATHER_ALL, &[first], Some(Listener { to: CELLS, user: 0 }));
    }

    /// The player stands in `cell` (0: nowhere); `block` asks the world to stand still while the
    /// cells are asked for. A new landblock outdoors, or a new interior cell, is asked for when
    /// no ask is under way; once none is, the cells are made about the player's.
    // D2-10769, D2-10803
    pub fn update(&mut self, download: &mut DataDownload, cell: u32, block: bool) -> Visible {
        if cell == 0 {
            self.release(download);
            return Visible::Released;
        }
        let block = self.blocking || block;
        if cell != self.asked_for {
            let same_block = is_land(cell) && self.asked_for >> 16 == cell >> 16;
            if !same_block && self.batch == Batch::NONE {
                self.async_get_cells(download, cell);
                self.asked_for = cell;
            }
        }
        if self.pending {
            self.blocking = block;
            return Visible::Waiting;
        }
        self.blocking = false;
        if self.batch != Batch::NONE {
            download.release(self.batch);
            self.batch = Batch::NONE;
        }
        Visible::Finish
    }

    /// The data cache says a batch of the cells' asks is done: this one's ask is in; any other is
    /// let go.
    // D2-10756
    pub fn on_batch_done(&mut self, download: &mut DataDownload, batch: Batch) {
        if batch == self.batch {
            self.pending = false;
            self.fresh = true;
        } else {
            download.release(batch);
        }
    }

    /// Whether an ask has come in since this was last asked.
    pub fn take_fresh(&mut self) -> bool {
        std::mem::take(&mut self.fresh)
    }

    /// A new session's data cache: what was asked of the last one is forgotten (its asks went
    /// with it).
    pub fn forget(&mut self) {
        let far = self.far.clone();
        *self = Self {
            far,
            ..Self::default()
        };
    }

    /// Nothing asked for and nothing kept.
    // D2-10757
    pub fn release(&mut self, download: &mut DataDownload) {
        if self.batch != Batch::NONE {
            download.release(self.batch);
            self.batch = Batch::NONE;
        }
        self.asked_for = 0;
        self.pending = false;
        self.blocking = false;
    }
}
