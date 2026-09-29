//! The landscape's block window: which landblocks are resident around the viewer, at what level of
//! detail, and which way each one stitches to its neighbours.
//!
//! **Depends on** no other workspace crate. **Used by** the client runtime's world streamer
//! (`dereth-client-runtime`), which keeps the window resident as the viewer moves; world drawing
//! (`dereth-world-render`), which builds each block's mesh at the detail and stitch direction the
//! window assigned; and the SDK (`dereth-client-sdk`).
//!
//! **Must never** draw, read a file or know about a device: it is index arithmetic over the
//! window's own slots, and a slot's geometry is the caller's own type.
//!
//! The window is `mid_width² = (2·mid_radius+1)²` landblocks centred on the viewer's block. Each
//! block is kept at one of four levels of detail chosen by its ring (its Chebyshev distance from
//! the viewer's block), and the blocks on the rings where the detail halves are stitched toward the
//! finer ring so the two meshes meet without a crack.

#![forbid(unsafe_code)]

pub mod window;

pub use window::{LandblockWindow, SlotAction, WindowSlot};

/// A landblock side, in world units.
pub const BLOCK_LENGTH: f32 = 192.0;

/// The window's `mid_radius` presets for graphics quality 1..=5.
pub const MID_RADIUS_PRESETS: [u32; 5] = [3, 5, 8, 11, 15];

/// Which edges of a block are stitched toward the finer ring beside it. Returned by the ring
/// calculation ([`block_orient`]) and consumed by landblock mesh generation. The discriminants are
/// the client's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Direction {
    InViewerBlock = 0,
    North = 1,
    South = 2,
    East = 3,
    West = 4,
    NorthWest = 5,
    SouthWest = 6,
    NorthEast = 7,
    SouthEast = 8,
}

impl Direction {
    /// True for N, NW and NE — the three directions whose "outward" edge is the block's max-y edge.
    #[must_use]
    pub const fn has_north(self) -> bool {
        matches!(self, Self::North | Self::NorthWest | Self::NorthEast)
    }
    /// True for S, SW and SE.
    #[must_use]
    pub const fn has_south(self) -> bool {
        matches!(self, Self::South | Self::SouthWest | Self::SouthEast)
    }
    /// True for E, NE and SE.
    #[must_use]
    pub const fn has_east(self) -> bool {
        matches!(self, Self::East | Self::NorthEast | Self::SouthEast)
    }
    /// True for W, NW and SW.
    #[must_use]
    pub const fn has_west(self) -> bool {
        matches!(self, Self::West | Self::NorthWest | Self::SouthWest)
    }
    /// True for the four cardinals, which is the guard on the inward crack fix of stitching.
    #[must_use]
    pub const fn is_cardinal(self) -> bool {
        matches!(self, Self::North | Self::South | Self::East | Self::West)
    }
}

/// Compute the four LOD rings and stitch direction.
///
/// With `m = max(|dx|, |dy|)`:
///
/// | `m` | `lod_div` | `side_cell_count` | `q` (band threshold) |
/// |---|---|---|---|
/// | 0, 1 | 1 | 8 | 1 |
/// | 2 | 2 | 4 | 2 |
/// | 3, 4 | 4 | 2 | 4 |
/// | >= 5 | 8 | 1 | — ([`Direction::InViewerBlock`]) |
///
/// Because `q` is a **band** threshold rather than `m`, rings 0 and 3 always yield
/// [`Direction::InViewerBlock`] — no `|dx|` or `|dy|` equals `q` there — so combined with
/// generation's guard (`trans_dir != InViewerBlock && 1 < side_cell_count < 8`) the stitching pass
/// runs **only on ring 2 and ring 4**.
#[must_use]
pub fn block_orient(dx: i32, dy: i32) -> (u8, Direction) {
    let m = dx.abs().max(dy.abs());
    let (lod_div, q) = match m {
        0 | 1 => (1u8, 1i32),
        2 => (2, 2),
        3 | 4 => (4, 4),
        _ => return (8, Direction::InViewerBlock),
    };
    let dir = if dx == q {
        if dy == q {
            Direction::NorthEast
        } else if dy == -q {
            Direction::SouthEast
        } else {
            Direction::East
        }
    } else if dx == -q {
        if dy == q {
            Direction::NorthWest
        } else if dy == -q {
            Direction::SouthWest
        } else {
            Direction::West
        }
    } else if dy == q {
        Direction::North
    } else if dy == -q {
        Direction::South
    } else {
        Direction::InViewerBlock
    };
    (lod_div, dir)
}

/// `8 / lod_div`.
#[must_use]
pub const fn side_cell_count(lod_div: u8) -> u8 {
    8 / lod_div
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the LOD-ring table, read row by row, plus the rule that rings 0 and 3
    /// yield [`Direction::InViewerBlock`] — the property that makes stitching run on exactly two
    /// rings.
    #[test]
    fn lod_rings_match_the_table_and_only_rings_2_and_4_stitch() {
        for m in 0..8i32 {
            let (lod, _) = block_orient(m, 0);
            let expect = match m {
                0 | 1 => 1,
                2 => 2,
                3 | 4 => 4,
                _ => 8,
            };
            assert_eq!(lod, expect, "m={m}");
        }
        // Sweep the whole window and count which rings ever produce a stitch direction.
        let mut stitching_rings = std::collections::BTreeSet::new();
        for dx in -15..=15i32 {
            for dy in -15..=15i32 {
                let (lod, dir) = block_orient(dx, dy);
                let n = side_cell_count(lod);
                if dir != Direction::InViewerBlock && n > 1 && n < 8 {
                    stitching_rings.insert(dx.abs().max(dy.abs()));
                }
            }
        }
        assert_eq!(
            stitching_rings.into_iter().collect::<Vec<_>>(),
            vec![2, 4],
            "stitching must run on exactly rings 2 and 4"
        );
    }

    /// Oracle: the stitch-direction derivation, case by case, at ring 2 where `q == 2`.
    #[test]
    fn stitch_direction_follows_the_band_threshold() {
        assert_eq!(block_orient(2, 2).1, Direction::NorthEast);
        assert_eq!(block_orient(2, -2).1, Direction::SouthEast);
        assert_eq!(block_orient(2, 0).1, Direction::East);
        assert_eq!(block_orient(-2, 2).1, Direction::NorthWest);
        assert_eq!(block_orient(-2, -2).1, Direction::SouthWest);
        assert_eq!(block_orient(-2, 1).1, Direction::West);
        assert_eq!(block_orient(1, 2).1, Direction::North);
        assert_eq!(block_orient(0, -2).1, Direction::South);
        // Ring 1: q is 1 and |dx| or |dy| is 1, so ring 1 *does* get a direction -- but its
        // side_cell_count is 8, which generation's guard excludes.
        assert_eq!(block_orient(1, 1).1, Direction::NorthEast);
        assert_eq!(side_cell_count(block_orient(1, 1).0), 8);
        // Ring 3: q is 4, and nothing on ring 3 reaches 4.
        assert_eq!(block_orient(3, 2).1, Direction::InViewerBlock);
        assert_eq!(block_orient(3, 3).1, Direction::InViewerBlock);
    }
}
