//! The land the client keeps about the player: the near window of landblocks, three blocks wide,
//! centered on the block of the land cell the player stands in, as the world's physics finds it.
//!
//! - **The window follows the player's cell.** Each frame the player's cell is looked at; when it
//!   changes, the window is worked out again: outdoors around the cell's block; inside, around the
//!   block of the land cell under the player when its cell sees the outdoors, else no window at all
//!   (the land is let go of while the player is shut in).
//! - **A shift keeps what stays.** When the window moves by less than its width, the blocks it still
//!   covers stay; the blocks it leaves are let go of, and the blocks it comes to are made.
//! - **Blocks let go of are kept a while.** A block the window leaves is no longer found, but its
//!   land is kept in a list of free blocks, twice as many as the window holds, the oldest given up
//!   first: a block the window comes back to while it is on that list is found again as it was;
//!   otherwise the oldest free block is given up and the new one made.
//! - **A block's objects fade in.** Each block handed to the window, made or found again, is
//!   stamped with the time; for the second after, its objects are drawn at the time since as their
//!   opacity ([`NearBlocks::fade`]).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use auberean_physics::land::{Block, Land};
use auberean_physics::mesh::Trees;
use auberean_world::records::{RegionSet, WorldRecords};

/// How many blocks the window reaches each way from its center, and its width.
pub(crate) const NEAR_RADIUS: i32 = 1;
pub(crate) const NEAR_WIDTH: i32 = NEAR_RADIUS * 2 + 1;
/// How many blocks the world has each way.
const LAND_SIDE: i32 = 255;

/// One of the free blocks: never used yet, or a block let go of with its land.
enum Free {
    Empty,
    Kept(Arc<Block>),
}

/// What the near window needs of the world to make a block.
pub(crate) struct Makers<'a> {
    pub land: &'a Land,
    pub records: &'a WorldRecords,
    pub region: &'a RegionSet,
    pub trees: &'a mut Trees,
    /// The blocks whose interior cells were loaded already.
    pub interiors: &'a mut HashSet<(u8, u8)>,
    /// The client's running time, seconds: a block handed out is stamped with it.
    pub now: f64,
}

/// The near window and the free blocks.
pub(crate) struct NearBlocks {
    /// The window's blocks, a row of `NEAR_WIDTH` for each x from the west, each row from the south:
    /// `None` past the world's edge or where no block could be made.
    window: Vec<Option<(u8, u8)>>,
    /// The land cell the window was worked out for (0: none).
    center: u32,
    free: VecDeque<Free>,
    /// The cell the player stood in when the window was last looked at.
    last_cell: u32,
    /// The blocks let go of since last taken.
    released: Vec<(u8, u8)>,
    /// The blocks handed out to the window since last asked.
    handed_out: Vec<(u8, u8)>,
    /// When each block of the window was handed out (made, or found again among the free
    /// blocks), by block.
    stamps: HashMap<(u8, u8), f64>,
}

impl std::fmt::Debug for NearBlocks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NearBlocks")
            .field("window", &self.window)
            .field("center", &format_args!("{:#010x}", self.center))
            .field("free", &self.free.len())
            .finish_non_exhaustive()
    }
}

impl Default for NearBlocks {
    fn default() -> Self {
        let mut n = Self {
            window: Vec::new(),
            center: 0,
            free: VecDeque::new(),
            last_cell: 0,
            released: Vec::new(),
            handed_out: Vec::new(),
            stamps: HashMap::new(),
        };
        n.init_free_blocks();
        n
    }
}

fn block_of(cell: u32) -> (i32, i32) {
    (
        i32::try_from(cell >> 24).unwrap_or(0),
        i32::try_from((cell >> 16) & 0xFF).unwrap_or(0),
    )
}

impl NearBlocks {
    /// The free blocks made up to twice as many as the window holds.
    // D2-10778
    fn init_free_blocks(&mut self) {
        let want = usize::try_from(NEAR_WIDTH * NEAR_WIDTH * 2).unwrap_or(18);
        while self.free.len() < want {
            self.free.push_front(Free::Empty);
        }
    }

    /// Every free block given up.
    // D2-10779
    fn cleanup_free_blocks(&mut self) {
        self.free.clear();
    }

    /// The world left: every block let go of and every free block given up.
    // D2-10780
    pub(crate) fn reset_world_state(&mut self, land: &Land) {
        self.release_cells(land);
        self.cleanup_free_blocks();
        self.last_cell = 0;
    }

    /// A new region: the free blocks made afresh.
    // D2-10781, D2-10759
    pub(crate) fn notify_change_region(&mut self) {
        self.cleanup_free_blocks();
        self.init_free_blocks();
    }

    /// The blocks let go of since last asked.
    pub(crate) fn take_released(&mut self) -> Vec<(u8, u8)> {
        std::mem::take(&mut self.released)
    }

    /// The blocks handed out to the window since last asked.
    pub(crate) fn take_handed_out(&mut self) -> Vec<(u8, u8)> {
        std::mem::take(&mut self.handed_out)
    }

    /// The blocks the window holds.
    pub(crate) fn blocks(&self) -> impl Iterator<Item = (u8, u8)> + '_ {
        self.window.iter().flatten().copied()
    }

    /// How many blocks east and north the window's center moves from cell `from` to cell `to`;
    /// `None` with no center.
    // D2-10770
    fn block_shift(from: u32, to: u32) -> Option<(i32, i32)> {
        if from == 0 {
            return None;
        }
        let (a, b) = (block_of(to), block_of(from));
        Some((a.0 - b.0, a.1 - b.1))
    }

    /// A block let go of: no longer found, its land kept at the end of the free blocks.
    // D2-10785
    // D2-03634
    fn release_block(&mut self, land: &Land, block: (u8, u8)) {
        self.released.push(block);
        self.stamps.remove(&block);
        if let Some(b) = land.release_block(block.0, block.1) {
            self.free.push_back(Free::Kept(b));
        } else {
            self.free.push_back(Free::Empty);
        }
    }

    /// Block `(x, y)` for the window: found again among the free blocks as it was, else made in
    /// place of the oldest free block (its land given up); `None` with no free block or no such
    /// block in the world.
    // D2-10790
    fn get_free_block(&mut self, m: &mut Makers<'_>, x: u8, y: u8) -> Option<(u8, u8)> {
        let kept = self
            .free
            .iter()
            .position(|f| matches!(f, Free::Kept(b) if (b.x, b.y) == (x, y)));
        if let Some(i) = kept {
            if let Some(Free::Kept(b)) = self.free.remove(i) {
                // D2-03633
                m.land.recover_block(b);
                self.stamps.insert((x, y), m.now);
                self.handed_out.push((x, y));
                return Some((x, y));
            }
        }
        let oldest = self.free.pop_front()?;
        drop(oldest);
        if !m.land.add_block(m.records, m.region, m.trees, x, y) {
            self.free.push_front(Free::Empty);
            return None;
        }
        if m.interiors.insert((x, y)) {
            auberean_physics::interior::add_block_cells(m.land, m.records, m.trees, x, y);
        }
        self.stamps.insert((x, y), m.now);
        self.handed_out.push((x, y));
        Some((x, y))
    }

    /// The blocks whose objects are drawn at less than full opacity at time `now`, and the
    /// opacity: the time since the block was handed out, while that is more than 0 and less than a
    /// second (at its very handing out, and after, they are drawn whole).
    // D2-03691
    pub(crate) fn fade(&self, now: f64) -> Vec<((u8, u8), f32)> {
        let mut out: Vec<((u8, u8), f32)> = self
            .blocks()
            .filter_map(|b| {
                let since = now - self.stamps.get(&b)?;
                #[allow(clippy::cast_possible_truncation)]
                (since > 0.0 && since < 1.0).then_some((b, since as f32))
            })
            .collect();
        out.sort_by_key(|(b, _)| *b);
        out
    }

    /// When block `block` was handed out to the window; `None` for a block it does not hold.
    pub(crate) fn stamp(&self, block: (u8, u8)) -> Option<f64> {
        self.stamps.get(&block).copied()
    }

    /// Every block of the window let go of; no window.
    // D2-10788
    pub(crate) fn release_cells(&mut self, land: &Land) {
        let held: Vec<(u8, u8)> = self.blocks().collect();
        for b in held {
            self.release_block(land, b);
        }
        self.window.clear();
        self.center = 0;
    }

    /// The window for land cell `cell`: anew when there was none, when asked to (`reload`), or
    /// when it moves its width or more; else shifted by `shift` blocks, keeping what it still
    /// covers.
    // D2-10795
    fn update_blocks(&mut self, m: &mut Makers<'_>, cell: u32, shift: (i32, i32), reload: bool) {
        let (cx, cy) = block_of(cell);
        let w = NEAR_WIDTH;
        let anew = self.window.is_empty() || reload || shift.0.abs() >= w || shift.1.abs() >= w;
        let wanted: Vec<Option<(u8, u8)>> = (0..w)
            .flat_map(|i| (0..w).map(move |j| (cx - NEAR_RADIUS + i, cy - NEAR_RADIUS + j)))
            .map(|(x, y)| {
                ((0..LAND_SIDE).contains(&x) && (0..LAND_SIDE).contains(&y))
                    .then(|| (u8::try_from(x).unwrap_or(0), u8::try_from(y).unwrap_or(0)))
            })
            .collect();
        if anew {
            self.release_cells(m.land);
        } else {
            let leaving: Vec<(u8, u8)> = self
                .blocks()
                .filter(|b| !wanted.contains(&Some(*b)))
                .collect();
            for b in leaving {
                self.release_block(m.land, b);
            }
        }
        let held: Vec<(u8, u8)> = self.blocks().collect();
        let mut window = Vec::with_capacity(wanted.len());
        for slot in wanted {
            window.push(match slot {
                Some(b) if held.contains(&b) => Some(b),
                Some((x, y)) => self.get_free_block(m, x, y),
                None => None,
            });
        }
        self.window = window;
        self.center = cell;
    }

    /// The window for land cell `cell` (0: none): worked out again when the window would move or
    /// there is none yet.
    // D2-10798
    fn update_visible_land(&mut self, m: &mut Makers<'_>, cell: u32) {
        if cell == 0 {
            self.release_cells(m.land);
            return;
        }
        let shift = Self::block_shift(self.center, cell);
        let reload = shift.is_none();
        let (dx, dy) = shift.unwrap_or((0, 0));
        if dx != 0 || dy != 0 || self.window.is_empty() || reload {
            self.update_blocks(m, cell, (dx, dy), reload);
        }
        self.center = cell;
    }

    /// The player stands in cell `cell` (0: nowhere), at `outside` the land cell under it when that
    /// cell is inside and sees the outdoors (`None` when it does not): when the cell changed, the
    /// window follows it; inside a cell shut off from the outdoors there is none.
    pub(crate) fn update_visible_cells(
        &mut self,
        m: &mut Makers<'_>,
        cell: u32,
        outside: Option<u32>,
    ) {
        if cell == 0 {
            self.release_cells(m.land);
            self.last_cell = 0;
            return;
        }
        if cell == self.last_cell {
            return;
        }
        if cell & 0xFFFF < 0x100 {
            self.update_visible_land(m, cell);
        } else {
            match outside {
                Some(land) => self.update_visible_land(m, land),
                None => self.release_cells(m.land),
            }
        }
        self.last_cell = cell;
    }
}
