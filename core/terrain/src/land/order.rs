//! Block and cell draw order.
//!
//! The landscape's block order and draw order, the landscape draw, and a landblock's own
//! square and cell draw orders, plus the direction helper.
//!
//! Block and cell submission preserve the far-to-near order described below.
//!
//! The whole point of this file: **draw far to near, including *within* a cell block.**
//! Blocks go outermost ring first with the viewer's block last; within a block,
//! [`cell_draw_order`] fills `draw_array` **backwards** from the closest cell so `draw_array[0]`
//! is the farthest, and the block draw iterates ascending. It is easy to get the block ordering right
//! and the cell ordering backwards, and the result looks almost correct.

use crate::land::mesh::Direction;
use crate::narrow::u16_of_i32;

/// Convert a signed block delta to a `Direction`.
///
/// Note the client's own branch shape: `dx == 0` with `dy == 0` is [`Direction::InViewerBlock`],
/// and the positive-x branch starts at `dx >= 1`, so there is no "same column" case for `dx > 0`.
#[must_use]
pub fn get_dir(dx: i32, dy: i32) -> Direction {
    if dx < 0 {
        if dy < 0 {
            return Direction::SouthWest;
        }
        return if dy > 0 {
            Direction::NorthWest
        } else {
            Direction::West
        };
    }
    if dx < 1 {
        if dy < 0 {
            return Direction::South;
        }
        return if dy > 0 {
            Direction::North
        } else {
            Direction::InViewerBlock
        };
    }
    if dy < 0 {
        return Direction::SouthEast;
    }
    if dy > 0 {
        Direction::NorthEast
    } else {
        Direction::East
    }
}

/// The ring calculation and the cell count it implies, shared with the landscape window.
pub use dereth_landscape::{block_orient, side_cell_count};

/// The eight-offset ring generator shared by block-list construction and block orientation, for
/// one `(ring, k)` pair:
///
/// ```text
/// (-k, ring)  (-ring, -k)  (k, -ring)  (ring, k)
/// (k+1, ring) (-ring, k+1) (-k-1, -ring) (ring, -k-1)
/// ```
#[must_use]
pub fn ring_offsets(ring: i32, k: i32) -> [(i32, i32); 8] {
    [
        (-k, ring),
        (-ring, -k),
        (k, -ring),
        (ring, k),
        (k + 1, ring),
        (-ring, k + 1),
        (-k - 1, -ring),
        (ring, -k - 1),
    ]
}

/// Block visit list, **nearest first**:
/// `block_draw_list[0]` is the viewer's own block, then ring 1, ring 2, … Entries outside the
/// window are skipped.
///
/// Each entry is the window index `mid_width * xi + yi`.
#[must_use]
pub fn block_draw_list(mid_width: u32, viewer: (i32, i32)) -> Vec<u32> {
    let w = mid_width as i32;
    let mid_radius = (w - 1) / 2;
    let mut out = Vec::new();
    // LINT-OK: index arithmetic bounded by mid_width <= 31.
    let push = |out: &mut Vec<u32>, x: i32, y: i32| {
        if (0..w).contains(&x) && (0..w).contains(&y) {
            out.push((w * x + y) as u32);
        }
    };
    push(&mut out, viewer.0, viewer.1);
    for ring in 1..=mid_radius {
        for k in 0..ring {
            for (dx, dy) in ring_offsets(ring, k) {
                push(&mut out, viewer.0 + dx, viewer.1 + dy);
            }
        }
    }
    out
}

/// Block rendering walks `block_draw_list` **backwards** (`i = mid_width² - 1 … 0`), so
/// blocks are drawn outermost ring first and the viewer's block last — far to near.
///
/// The viewer sits at the centre of the window, which is what the draw-order setup arranges by
/// dropping the whole list when the viewer's block offset escapes `[0, mid_width)`.
#[must_use]
pub fn block_draw_order(mid_width: u32) -> Vec<u32> {
    let r = (mid_width as i32 - 1) / 2;
    let mut v = block_draw_list(mid_width, (r, r));
    v.reverse();
    v
}

/// The landblock's closest-cell table for the square draw order.
///
/// `cell_xy` is the viewer's cell within its **own** block, already divided by `8 / N`.
#[must_use]
pub fn closest_cell(n: u8, dir: Direction, cell_xy: (u8, u8)) -> (u8, u8) {
    let last = n.saturating_sub(1);
    match dir {
        Direction::InViewerBlock => (cell_xy.0, cell_xy.1),
        Direction::North => (cell_xy.0, 0),
        Direction::South => (cell_xy.0, last),
        Direction::East => (0, cell_xy.1),
        Direction::West => (last, cell_xy.1),
        Direction::NorthWest => (last, 0),
        Direction::SouthWest => (last, last),
        Direction::NorthEast => (0, 0),
        Direction::SouthEast => (0, last),
    }
}

/// Per-block cell order.
///
/// `draw_array` is filled **backwards** from index `N² - 1` (the closest cell) using the same
/// eight-offset ring generator, so `draw_array[0]` is the **farthest** cell and
/// Ascending iteration draws far to near within the block.
///
/// Each entry is the cell index `i * N + j`.
#[must_use]
pub fn cell_draw_order(side_cell_count: u8, dir: Direction, viewer_cell: (u8, u8)) -> Vec<u16> {
    let n = i32::from(side_cell_count);
    let total = (n * n) as usize;
    let mut out = vec![0u16; total];
    let mut next = total; // fills backwards from total-1
    let (cx, cy) = closest_cell(side_cell_count, dir, viewer_cell);
    let (cx, cy) = (i32::from(cx), i32::from(cy));
    let mut put = |x: i32, y: i32, next: &mut usize| {
        if (0..n).contains(&x) && (0..n).contains(&y) && *next > 0 {
            *next -= 1;
            // LINT-OK: index arithmetic, bounded by the 8x8 cell grid.
            out[*next] = u16_of_i32(x * n + y);
        }
    };
    put(cx, cy, &mut next);
    for ring in 1..n {
        for k in 0..ring {
            for (dx, dy) in ring_offsets(ring, k) {
                put(cx + dx, cy + dy, &mut next);
            }
        }
    }
    // A block whose closest cell is not the centre reaches fewer cells per ring, so the low
    // indices can stay unwritten; the client leaves them at whatever the array held and skips
    // them by count. Trimming the unwritten prefix is the same visible set in the same order.
    out.drain(..next);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the client's block order starts at the viewer's block and the landscape draw walks
    /// the list backwards, so the emitted order is outermost ring first and the viewer's block last.
    #[test]
    fn blocks_draw_outermost_ring_first_and_the_viewer_block_last() {
        for mid_width in [3u32, 7, 11] {
            let w = mid_width as i32;
            let r = (w - 1) / 2;
            let order = block_draw_order(mid_width);
            assert_eq!(
                order.len(),
                (mid_width * mid_width) as usize,
                "every block is visited once"
            );
            let mut seen = std::collections::BTreeSet::new();
            for &e in &order {
                assert!(seen.insert(e), "block {e} visited twice");
            }
            // Chebyshev ring of each entry, in emission order, must be non-increasing.
            let ring_of = |e: u32| {
                let (x, y) = ((e as i32) / w, (e as i32) % w);
                (x - r).abs().max((y - r).abs())
            };
            let rings: Vec<i32> = order.iter().map(|&e| ring_of(e)).collect();
            assert!(
                rings.windows(2).all(|p| p[0] >= p[1]),
                "mid_width {mid_width}: rings must be non-increasing, got {rings:?}"
            );
            assert_eq!(
                *rings.first().expect("non-empty"),
                r,
                "the first block is outermost"
            );
            // LINT-OK: index arithmetic.
            assert_eq!(
                *order.last().expect("non-empty"),
                (w * r + r) as u32,
                "viewer block last"
            );
        }
    }

    /// Oracle: `draw_array` is filled backwards from the
    /// closest cell, so index 0 is the **farthest** and ascending iteration is far to near. This is
    /// the assertion that fails if someone "fixes" the fill direction.
    #[test]
    fn cells_draw_far_to_near_within_the_block() {
        let order = cell_draw_order(8, Direction::InViewerBlock, (3, 4));
        assert_eq!(order.len(), 64, "every cell exactly once");
        let mut seen = std::collections::BTreeSet::new();
        for &e in &order {
            assert!(seen.insert(e), "cell {e} twice");
        }
        // The last entry is the closest cell.
        assert_eq!(*order.last().expect("non-empty"), 3 * 8 + 4);
        // Chebyshev distance from the closest cell must be non-increasing.
        let d = |e: u16| {
            let (x, y) = (i32::from(e) / 8, i32::from(e) % 8);
            (x - 3).abs().max((y - 4).abs())
        };
        let ds: Vec<i32> = order.iter().map(|&e| d(e)).collect();
        assert!(ds.windows(2).all(|p| p[0] >= p[1]), "{ds:?}");
        assert_eq!(ds[0], 4, "the farthest cell of an 8x8 block from (3,4)");
    }

    /// Oracle: the closest-cell table has one row per direction. A block to the
    /// NORTH of the viewer is approached from its south edge, so its closest cell has `y = 0`.
    #[test]
    fn the_closest_cell_table_matches_the_document() {
        let n = 8u8;
        let v = (3u8, 5u8);
        assert_eq!(closest_cell(n, Direction::InViewerBlock, v), (3, 5));
        assert_eq!(closest_cell(n, Direction::North, v), (3, 0));
        assert_eq!(closest_cell(n, Direction::South, v), (3, 7));
        assert_eq!(closest_cell(n, Direction::East, v), (0, 5));
        assert_eq!(closest_cell(n, Direction::West, v), (7, 5));
        assert_eq!(closest_cell(n, Direction::NorthWest, v), (7, 0));
        assert_eq!(closest_cell(n, Direction::SouthWest, v), (7, 7));
        assert_eq!(closest_cell(n, Direction::NorthEast, v), (0, 0));
        assert_eq!(closest_cell(n, Direction::SouthEast, v), (0, 7));
    }

    /// Oracle: [`block_orient`]'s branch structure, exhaustively over a small delta
    /// range. Every non-zero delta must name a direction and only `(0, 0)` may be
    /// [`Direction::InViewerBlock`].
    #[test]
    fn get_dir_names_every_non_zero_delta() {
        for dx in -3..=3i32 {
            for dy in -3..=3i32 {
                let d = get_dir(dx, dy);
                assert_eq!(
                    d == Direction::InViewerBlock,
                    dx == 0 && dy == 0,
                    "({dx}, {dy}) -> {d:?}"
                );
            }
        }
        assert_eq!(get_dir(0, 1), Direction::North);
        assert_eq!(get_dir(0, -1), Direction::South);
        assert_eq!(get_dir(1, 0), Direction::East);
        assert_eq!(get_dir(-1, 0), Direction::West);
        assert_eq!(get_dir(2, 3), Direction::NorthEast);
        assert_eq!(get_dir(-2, -3), Direction::SouthWest);
    }

    /// Oracle: a reduced-detail block still orders its cells far to near, and a 1x1 block has
    /// exactly one cell — the degenerate case the ring generator must not spin on.
    #[test]
    fn lod_blocks_order_their_cells_too() {
        for n in [1u8, 2, 4] {
            let order = cell_draw_order(n, Direction::NorthEast, (0, 0));
            assert_eq!(order.len(), usize::from(n) * usize::from(n), "n={n}");
            assert_eq!(
                *order.last().expect("non-empty"),
                0,
                "NE approaches from cell (0,0)"
            );
        }
    }
}
