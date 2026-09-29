//! The landblock window, its scroll and its four LOD rings.
//!
//! The landscape's mid-radius setter, its block update, its viewpoint update, its draw-order
//! calculation and its frame calculation.
//!
//! The window scroll and four LOD rings preserve the original landblock generation behavior.
//!
//! The window is `mid_width² = (2·mid_radius+1)²` landblocks centred on the viewer, indexed
//! `mid_width * xi + yi`. When the viewer moves one block the array is **scrolled**,
//! not rebuilt; that is the part with a live-entry hazard, and the property test below is what
//! keeps it honest.

use crate::{block_orient, side_cell_count, Direction, BLOCK_LENGTH, MID_RADIUS_PRESETS};

/// A small window index as `i32`. Window sides are at most 31 blocks, so this never saturates.
fn i32_of(v: usize) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// One window slot: the block's id and its currently generated geometry, `M`, which is whatever
/// the drawing side builds for a block.
#[derive(Debug, Clone)]
pub struct WindowSlot<M> {
    /// `block_coord = (blockX*8, blockY*8)`, stored on each block by `update_block`.
    pub block_x: i32,
    pub block_y: i32,
    pub mesh: Option<M>,
    /// The LOD divisor this slot was last generated at, so `update_block` can tell whether
    /// `notify_change_size` is needed.
    pub lod_div: u8,
    pub trans_dir: Direction,
}

/// What `update_block` decided to do with one slot this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotAction {
    /// The slot left the window and releases all owned block data.
    Released,
    /// A newly exposed edge slot: fetch `blockId | 0xFFFF` and generate.
    Fetched {
        block_x: i32,
        block_y: i32,
        lod_div: u8,
        dir: Direction,
    },
    /// The slot stayed, at the same detail and direction: `generate` returns 0 and does nothing.
    Unchanged,
    /// The slot stayed but its ring changed: `notify_change_size` then a full `generate`.
    Resized { lod_div: u8, dir: Direction },
    /// The slot stayed at the same size but a new stitch direction: `ConstructVertices`,
    /// `TransAdjust`, the plane adjustment, `CalcWater` — the "rebuilt = 0" tail of `generate`.
    Restitched { dir: Direction },
}

/// The landscape's block window.
#[derive(Debug)]
pub struct LandblockWindow<M> {
    mid_radius: u32,
    /// `mid_width²` slots, indexed `mid_width * xi + yi`. `None` is an unloaded slot.
    slots: Vec<Option<WindowSlot<M>>>,
    /// The block the viewer is standing in.
    viewer_block: Option<(i32, i32)>,
}

impl<M> LandblockWindow<M> {
    /// Set the middle radius `r`; `mid_width = 2r + 1`.
    ///
    /// The client only lets this succeed when the block array has been released, which is why the
    /// caller resets the cell manager first; the equivalent here is
    /// that the constructor is the only way to set it.
    #[must_use]
    pub fn new(mid_radius: u32) -> Self {
        let w = (2 * mid_radius + 1) as usize;
        Self {
            mid_radius,
            slots: (0..w * w).map(|_| None).collect(),
            viewer_block: None,
        }
    }

    /// The overall graphics-quality presets, quality 1..=5 → 3, 5, 8, 11, 15.
    #[must_use]
    pub fn for_quality(quality: usize) -> Self {
        Self::new(MID_RADIUS_PRESETS[quality.clamp(1, 5) - 1])
    }

    #[must_use]
    pub const fn mid_radius(&self) -> u32 {
        self.mid_radius
    }

    #[must_use]
    pub const fn mid_width(&self) -> u32 {
        2 * self.mid_radius + 1
    }

    #[must_use]
    pub fn slot(&self, xi: u32, yi: u32) -> Option<&WindowSlot<M>> {
        self.slots
            .get((self.mid_width() * xi + yi) as usize)?
            .as_ref()
    }

    /// The window index of `(xi, yi)`.
    #[must_use]
    pub fn index(&self, xi: u32, yi: u32) -> usize {
        (self.mid_width() * xi + yi) as usize
    }

    /// The viewer's block, once [`Self::update_block`] has been called.
    #[must_use]
    pub const fn viewer_block(&self) -> Option<(i32, i32)> {
        self.viewer_block
    }

    /// Block frame origin:
    /// `((xi - viewer_b_xoff)*192, (yi - viewer_b_yoff)*192, 0)`. The landscape is rendered in a
    /// **viewer-block-relative** space, so vertex positions stay block-local (0..192).
    #[must_use]
    pub fn block_frame_origin(&self, xi: u32, yi: u32) -> (f32, f32) {
        // LINT-OK: integer index arithmetic, not a float conversion; mid_radius is at most 15.
        let r = i32_of(self.mid_radius as usize);
        // LINT-OK: as above; the `as f32` widening below is the only float conversion here.
        let (dx, dy) = (i32_of(xi as usize) - r, i32_of(yi as usize) - r);
        #[allow(clippy::cast_precision_loss)] // window offsets are at most 15
        {
            (dx as f32 * BLOCK_LENGTH, dy as f32 * BLOCK_LENGTH)
        }
    }

    /// The landscape's block update.
    ///
    /// Returns one [`SlotAction`] per window slot in `mid_width * xi + yi` order, so a caller can
    /// drive the fetch/generate work and a test can assert what happened without a dat.
    ///
    /// * If the array does not exist, or the shift is >= `mid_width` in either axis, everything is
    ///   released and the whole window is re-fetched.
    /// * Otherwise the array is **scrolled** by the block shift, in the direction that avoids
    ///   overwriting live entries.
    /// * Then, for **every** block in the window, `get_block_orient` decides the ring and the
    ///   stitch direction, and `generate` is invoked accordingly.
    pub fn update_block(&mut self, viewer_block: (i32, i32)) -> Vec<SlotAction> {
        let w = self.mid_width() as i32;
        let shift = self
            .viewer_block
            .map(|(x, y)| (viewer_block.0 - x, viewer_block.1 - y));
        let full_reload = match shift {
            None => true,
            Some((dx, dy)) => dx.abs() >= w || dy.abs() >= w,
        };
        let mut released = Vec::new();
        if full_reload {
            for s in &mut self.slots {
                if s.take().is_some() {
                    released.push(());
                }
            }
        } else if let Some((dx, dy)) = shift {
            self.scroll(dx, dy);
        }
        self.viewer_block = Some(viewer_block);

        let r = self.mid_radius as i32;
        let mut actions = Vec::with_capacity(self.slots.len());
        for xi in 0..w {
            for yi in 0..w {
                let (bx, by) = (viewer_block.0 + xi - r, viewer_block.1 + yi - r);
                let (lod_div, dir) = block_orient(xi - r, yi - r);
                // LINT-OK: index arithmetic bounded by mid_width <= 31.
                let idx = (w * xi + yi) as usize;
                let action = match &mut self.slots[idx] {
                    None => SlotAction::Fetched {
                        block_x: bx,
                        block_y: by,
                        lod_div,
                        dir,
                    },
                    Some(s) => {
                        if s.lod_div != lod_div {
                            SlotAction::Resized { lod_div, dir }
                        } else if s.trans_dir != dir {
                            SlotAction::Restitched { dir }
                        } else {
                            SlotAction::Unchanged
                        }
                    }
                };
                match (&mut self.slots[idx], action) {
                    (
                        slot @ None,
                        SlotAction::Fetched {
                            block_x,
                            block_y,
                            lod_div,
                            dir,
                        },
                    ) => {
                        *slot = Some(WindowSlot {
                            block_x,
                            block_y,
                            mesh: None,
                            lod_div,
                            trans_dir: dir,
                        });
                    }
                    (Some(s), SlotAction::Resized { lod_div, dir }) => {
                        s.lod_div = lod_div;
                        s.trans_dir = dir;
                        s.mesh = None; // Destroy() then InitPVArrays()
                    }
                    (Some(s), SlotAction::Restitched { dir }) => s.trans_dir = dir,
                    _ => {}
                }
                actions.push(action);
            }
        }
        actions
    }

    /// The scroll half of `update_block`. Entries that leave the window are released, entries that
    /// stay are moved, and the newly exposed edge is left `None` for the caller to fetch.
    ///
    /// The client picks one of four iteration orders (dx<0/>=0 x dy<0/>=0) so a live entry is never
    /// overwritten before it has been moved. This transcribes the same rule, and
    /// [`Self::scroll`]'s property test below checks the *outcome* over random walks.
    ///
    /// **The order is inert here: it is not what prevents the overwrite.** Two mutations —
    /// reversing the `xs` order and reversing the `ys` order — both SURVIVED, against the pixel
    /// test and against `the_scroll_path_never_overwrites_a_live_entry` respectively. That is not a
    /// weak test. It is structural, and provable rather than measured: every read is
    /// `self.slots[sx][sy].take()` and every write is into `moved`, **a second buffer**, so no
    /// destination can alias a source that has not been read yet. Each destination is visited once
    /// and `(x, y) -> (x+dx, y+dy)` is injective, so the result is a pure function of `dx`, `dy`
    /// and the input, in any order. The client shifts **in place**, which is why it needs the four
    /// orders and why this needs none.
    ///
    /// The four orders are **kept, not deleted**: they are the faithful transcription, and they
    /// become load-bearing the moment anyone makes this an in-place shift. But nothing in this
    /// workspace can falsify them, and saying so here is what stops the next reader taking a
    /// surviving mutation as evidence about the tests. Correct but locally unfalsifiable code is
    /// kept with that limitation stated explicitly.
    fn scroll(&mut self, dx: i32, dy: i32) {
        let w = self.mid_width() as i32;
        let take =
            |slots: &mut Vec<Option<WindowSlot<M>>>, x: i32, y: i32| -> Option<WindowSlot<M>> {
                if (0..w).contains(&x) && (0..w).contains(&y) {
                    // LINT-OK: index arithmetic bounded by mid_width <= 31.
                    slots[(w * x + y) as usize].take()
                } else {
                    None
                }
            };
        // Four iteration orders: ascending when the source is above the
        // destination, descending when it is below. **Inert in this formulation** -- see the
        // doc comment; `moved` is a second buffer, so nothing can be clobbered whatever the order.
        let xs: Vec<i32> = if dx >= 0 {
            (0..w).collect()
        } else {
            (0..w).rev().collect()
        };
        let ys: Vec<i32> = if dy >= 0 {
            (0..w).collect()
        } else {
            (0..w).rev().collect()
        };
        let mut moved: Vec<Option<WindowSlot<M>>> = (0..self.slots.len()).map(|_| None).collect();
        for &x in &xs {
            for &y in &ys {
                let (sx, sy) = (x + dx, y + dy);
                let v = take(&mut self.slots, sx, sy);
                // LINT-OK: index arithmetic bounded by mid_width <= 31.
                moved[(w * x + y) as usize] = v;
            }
        }
        // Anything still in `slots` left the window: release_all.
        self.slots = moved;
    }

    /// Install a generated mesh into a slot — the tail of `update_block`'s per-block work
    /// (`init_lcell_ptrs`, `calc_lighting`, `get_land_limits`).
    pub fn set_mesh(&mut self, xi: u32, yi: u32, mesh: M) {
        let idx = self.index(xi, yi);
        if let Some(Some(s)) = self.slots.get_mut(idx) {
            s.mesh = Some(mesh);
        }
    }

    /// `side_cell_count` of each slot, as [`block_orient`] assigns it. Handy for the ring test.
    #[must_use]
    pub fn ring_map(&self) -> Vec<u8> {
        let w = self.mid_width() as i32;
        let r = self.mid_radius as i32;
        let mut out = Vec::with_capacity((w * w) as usize);
        for xi in 0..w {
            for yi in 0..w {
                out.push(side_cell_count(block_orient(xi - r, yi - r).0));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the LOD-ring table and the five
    /// presets. The ring assignment is a pure function of the Chebyshev distance, checked for every
    /// slot of every preset window.
    #[test]
    fn ring_assignment_matches_the_table_for_all_five_presets() {
        for (q, &r) in MID_RADIUS_PRESETS.iter().enumerate() {
            let win = LandblockWindow::<()>::for_quality(q + 1);
            assert_eq!(win.mid_radius(), r);
            let w = win.mid_width() as i32;
            let map = win.ring_map();
            assert_eq!(map.len(), (w * w) as usize);
            for xi in 0..w {
                for yi in 0..w {
                    let m = (xi - r as i32).abs().max((yi - r as i32).abs());
                    let expect = match m {
                        0 | 1 => 8u8,
                        2 => 4,
                        3 | 4 => 2,
                        _ => 1,
                    };
                    assert_eq!(
                        map[(w * xi + yi) as usize],
                        expect,
                        "r={r} ({xi},{yi}) m={m}"
                    );
                }
            }
        }
    }

    /// Oracle: the landblock array is **scrolled**; entries that leave the
    /// window are released, entries that stay are moved, and the newly exposed edge is fetched.
    /// The hazard is a move overwriting a live entry before it has been read.
    ///
    /// A random walk of single-block steps,
    /// asserting after every step that every surviving slot still holds the block it should and
    /// that no block appears twice.
    #[test]
    fn the_scroll_path_never_overwrites_a_live_entry() {
        // A small deterministic walk; the generator is a plain LCG so the test is reproducible and
        // does not reach for a PRNG whose draw order is contract elsewhere.
        let mut seed = 0x1234_5678u32;
        let mut next = move || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            ((seed >> 16) % 5) as i32 - 2 // a step in -2..=2
        };
        for mid_radius in [1u32, 3, 5] {
            let mut win = LandblockWindow::<()>::new(mid_radius);
            let mut pos = (100i32, 100i32);
            win.update_block(pos);
            // Fill every slot so a lost entry is detectable.
            let w = win.mid_width();
            let r = mid_radius as i32;
            for xi in 0..w {
                for yi in 0..w {
                    let idx = win.index(xi, yi);
                    if let Some(Some(s)) = win.slots.get_mut(idx) {
                        s.block_x = pos.0 + xi as i32 - r;
                        s.block_y = pos.1 + yi as i32 - r;
                    }
                }
            }
            for step in 0..200 {
                let d = (next(), next());
                pos = (pos.0 + d.0, pos.1 + d.1);
                win.update_block(pos);
                let mut seen = std::collections::BTreeSet::new();
                for xi in 0..w {
                    for yi in 0..w {
                        let want = (pos.0 + xi as i32 - r, pos.1 + yi as i32 - r);
                        if let Some(s) = win.slot(xi, yi) {
                            if s.mesh.is_none() && s.block_x == 0 && s.block_y == 0 {
                                continue; // a freshly fetched slot, not yet filled
                            }
                            assert!(
                                seen.insert((s.block_x, s.block_y)),
                                "r={mid_radius} step {step}: block ({}, {}) is in two slots",
                                s.block_x,
                                s.block_y
                            );
                            // A slot that survived the scroll must hold the block its position
                            // names; a slot that was just fetched holds `want` by construction.
                            assert!(
                                (s.block_x, s.block_y) == want
                                    || (s.block_x == 0 && s.block_y == 0),
                                "r={mid_radius} step {step}: slot ({xi},{yi}) holds ({}, {}), wants {want:?}",
                                s.block_x,
                                s.block_y
                            );
                        }
                    }
                }
                // Fill the newly exposed slots so the next step has something to lose.
                for xi in 0..w {
                    for yi in 0..w {
                        let want = (pos.0 + xi as i32 - r, pos.1 + yi as i32 - r);
                        let idx = win.index(xi, yi);
                        if let Some(Some(s)) = win.slots.get_mut(idx) {
                            s.block_x = want.0;
                            s.block_y = want.1;
                        }
                    }
                }
            }
        }
    }

    /// Oracle: if the array does not exist, or the shift is >=
    /// `mid_width` in either axis, everything is released and the whole window is re-fetched.
    #[test]
    fn a_teleport_further_than_the_window_reloads_everything() {
        let mut win = LandblockWindow::<()>::new(3);
        let first = win.update_block((50, 50));
        assert!(first
            .iter()
            .all(|a| matches!(a, SlotAction::Fetched { .. })));
        // One block: everything but the newly exposed edge survives.
        let step = win.update_block((51, 50));
        let fetched = step
            .iter()
            .filter(|a| matches!(a, SlotAction::Fetched { .. }))
            .count();
        assert_eq!(
            fetched, 7,
            "a one-block step exposes one column of a 7-wide window"
        );
        // A jump of mid_width (7) blocks: everything is refetched.
        let jump = win.update_block((58, 50));
        assert!(
            jump.iter().all(|a| matches!(a, SlotAction::Fetched { .. })),
            "a shift of mid_width must reload the whole window"
        );
    }

    /// Oracle: if the block currently has 8 cells per side and the new
    /// when `8/lodDiv != 8`, notify the size change, then generate. Walking one block moves
    /// blocks between rings, so some slots resize and some only restitch.
    #[test]
    fn moving_one_block_resizes_the_slots_that_changed_ring() {
        let mut win = LandblockWindow::<()>::new(5);
        win.update_block((100, 100));
        let actions = win.update_block((101, 100));
        let resized = actions
            .iter()
            .filter(|a| matches!(a, SlotAction::Resized { .. }))
            .count();
        let restitched = actions
            .iter()
            .filter(|a| matches!(a, SlotAction::Restitched { .. }))
            .count();
        assert!(
            resized > 0,
            "a one-block step must move some blocks between rings"
        );
        assert!(
            restitched > 0,
            "and change some blocks' stitch direction without resizing"
        );
    }

    /// Oracle: the block frame origin is the block offset from
    /// the viewer's block times 192, so the viewer's own block sits at the origin and vertex
    /// positions stay block-local.
    #[test]
    fn block_frames_are_viewer_block_relative() {
        let win = LandblockWindow::<()>::new(5);
        assert_eq!(win.block_frame_origin(5, 5), (0.0, 0.0));
        assert_eq!(win.block_frame_origin(6, 5), (192.0, 0.0));
        assert_eq!(win.block_frame_origin(5, 3), (0.0, -384.0));
    }
}
