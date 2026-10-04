//! The block water calculation, the per-cell one and the water-depth
//! helper.
//!
//! Water classification follows the terrain-cell behavior described below.
//!
//! Landscape water is **not** a separate render pass: water cells are ordinary terrain triangles
//! textured with the water `TerrainTex` entries. What lives here is the classification, which the
//! footstep and swim code read and which the renderer only uses to pick surfaces.
//!
//! The block water calculation produces a result **only** when `side_cell_count == 8`. LOD blocks carry no
//! water classification at all, and reproducing that is part of what makes distant terrain right.

use crate::consts::{SIDE_VERTEX_COUNT, VERTEX_COUNT};

/// A terrain type's surface character has exactly two values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfChar {
    Solid = 0,
    Water = 1,
}

/// The 32-entry per-terrain-type surface-character table.
/// Terrain types 16-20 are water — ACE's `WaterRunning (0x10)`, `WaterStandingFresh (0x11)`,
/// `WaterShallowSea (0x12)`, `WaterShallowStillSea (0x13)` and `WaterDeepSea (0x14)`.
pub const TERRAIN_SURF_CHAR: [SurfChar; 32] = {
    let mut t = [SurfChar::Solid; 32];
    t[16] = SurfChar::Water;
    t[17] = SurfChar::Water;
    t[18] = SurfChar::Water;
    t[19] = SurfChar::Water;
    t[20] = SurfChar::Water;
    t
};

/// `WaterType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterType {
    #[default]
    NotWater = 0,
    PartiallyWater = 1,
    EntirelyWater = 2,
}

/// Bits 0-1 of a terrain word: non-zero means this vertex is on a road.
#[inline]
#[must_use]
pub const fn road_bits(word: u16) -> u16 {
    word & 3
}

/// Bits 2-6 of a terrain word select the terrain texture type, `0..=31`. The terrain lookup masks
/// `0x7C` and shifts; bits `0x0780` are zero everywhere in the shipped data.
#[inline]
#[must_use]
pub const fn terrain_type(word: u16) -> usize {
    ((word >> 2) & 0x1F) as usize
}

/// Bits 11-15: the scene type index shifts out.
#[inline]
#[must_use]
pub const fn scene_type(word: u16) -> u16 {
    word >> 11
}

/// The `SURFCHAR` of a terrain word.
#[inline]
#[must_use]
pub fn surf_char(word: u16) -> SurfChar {
    TERRAIN_SURF_CHAR[terrain_type(word)]
}

/// Compute `(any water, all water)` over the four corner terrain words of cell
/// `(i, j)`, sampled from the full-detail 9x9 grid.
#[must_use]
pub fn calc_cell_water(terrain: &[u16; VERTEX_COUNT], i: usize, j: usize) -> (bool, bool) {
    let at = |a: usize, b: usize| surf_char(terrain[a * SIDE_VERTEX_COUNT + b]) == SurfChar::Water;
    let corners = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
    (corners.iter().any(|&w| w), corners.iter().all(|&w| w))
}

/// The block water calculation.
///
/// Returns `(per_cell, block)`. **Only** `side_cell_count == 8` produces a result: anything else is
/// a reduced-detail block and the client leaves the whole block `NOT_WATER` with an empty per-cell
/// array.
///
/// The block summary is `NOT_WATER` when no cell had water, else `PARTIALLY_WATER + (every cell
/// entirely water ? 1 : 0)`.
#[must_use]
pub fn calc_water(
    terrain: &[u16; VERTEX_COUNT],
    side_cell_count: usize,
) -> (Vec<WaterType>, WaterType) {
    if side_cell_count != 8 {
        return (Vec::new(), WaterType::NotWater);
    }
    let mut cells = Vec::with_capacity(side_cell_count * side_cell_count);
    let mut any_block_water = false;
    let mut all_block_water = true;
    for i in 0..side_cell_count {
        for j in 0..side_cell_count {
            let (any, all) = calc_cell_water(terrain, i, j);
            let w = if any {
                any_block_water = true;
                if all {
                    WaterType::EntirelyWater
                } else {
                    all_block_water = false;
                    WaterType::PartiallyWater
                }
            } else {
                all_block_water = false;
                WaterType::NotWater
            };
            cells.push(w);
        }
    }
    let block = if !any_block_water {
        WaterType::NotWater
    } else if all_block_water {
        WaterType::EntirelyWater
    } else {
        WaterType::PartiallyWater
    };
    (cells, block)
}

/// Water depth: `0.45` over a water corner, `0.1` over a solid corner, `0` otherwise.
///
/// The corner is picked by comparing the point against the cell centre `((2i+1)·12, (2j+1)·12)`.
/// This is a physics/footstep quantity, not a rendering one; it lives here because it reads the
/// same terrain table.
#[must_use]
pub fn calc_water_depth(
    terrain: &[u16; VERTEX_COUNT],
    cell_water: WaterType,
    i: usize,
    j: usize,
    x: f32,
    y: f32,
) -> f32 {
    match cell_water {
        WaterType::NotWater => 0.0,
        WaterType::EntirelyWater => 0.45,
        WaterType::PartiallyWater => {
            #[allow(clippy::cast_precision_loss)] // i, j <= 7
            let (cx, cy) = ((2 * i + 1) as f32 * 12.0, (2 * j + 1) as f32 * 12.0);
            let (a, b) = (
                if x < cx { i } else { i + 1 },
                if y < cy { j } else { j + 1 },
            );
            if surf_char(terrain[a * SIDE_VERTEX_COUNT + b]) == SurfChar::Water {
                0.45
            } else {
                0.1
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Index arithmetic in test fixtures, bounded by the loops that build them.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;

    /// Oracle: exactly terrain types 16..=20 are water.
    #[test]
    fn only_terrain_types_16_to_20_are_water() {
        for t in 0..32usize {
            let word = (t as u16) << 2;
            let expect = (16..=20).contains(&t);
            assert_eq!(
                surf_char(word) == SurfChar::Water,
                expect,
                "terrain type {t}"
            );
        }
    }

    /// Oracle: water classification produces a result only at full detail.
    #[test]
    fn lod_blocks_get_no_water_classification_at_all() {
        let terrain = [16u16 << 2; VERTEX_COUNT]; // every vertex is running water
        for scc in [1usize, 2, 4] {
            let (cells, block) = calc_water(&terrain, scc);
            assert!(cells.is_empty(), "scc={scc}");
            assert_eq!(block, WaterType::NotWater, "scc={scc}");
        }
        let (cells, block) = calc_water(&terrain, 8);
        assert_eq!(cells.len(), 64);
        assert_eq!(block, WaterType::EntirelyWater);
    }

    /// Oracle: the three-way water rule, exercised on a block whose water covers exactly one
    /// corner of the grid, so some cells are entirely water, some partial and some dry.
    #[test]
    fn block_summary_is_partially_water_when_cells_disagree() {
        let mut terrain = [0u16; VERTEX_COUNT];
        for i in 0..3 {
            for j in 0..3 {
                terrain[i * SIDE_VERTEX_COUNT + j] = 18 << 2; // shallow sea
            }
        }
        let (cells, block) = calc_water(&terrain, 8);
        assert_eq!(block, WaterType::PartiallyWater);
        assert_eq!(
            cells[0],
            WaterType::EntirelyWater,
            "cell (0,0) has four water corners"
        );
        assert_eq!(
            cells[2 * 8 + 2],
            WaterType::PartiallyWater,
            "cell (2,2) has one water corner"
        );
        assert_eq!(cells[7 * 8 + 7], WaterType::NotWater);
    }

    /// Oracle: [`calc_water`]'s three return values and corner selection.
    #[test]
    fn water_depth_picks_the_corner_by_the_cell_centre() {
        let mut terrain = [0u16; VERTEX_COUNT];
        terrain[0] = 16 << 2; // SW corner of cell (0,0) is water; the other three are solid
        let (cells, _) = calc_water(&terrain, 8);
        assert_eq!(cells[0], WaterType::PartiallyWater);
        // Cell (0,0)'s centre is (12, 12): below it in both axes selects the SW corner.
        assert_eq!(calc_water_depth(&terrain, cells[0], 0, 0, 1.0, 1.0), 0.45);
        assert_eq!(calc_water_depth(&terrain, cells[0], 0, 0, 20.0, 20.0), 0.1);
        assert_eq!(
            calc_water_depth(&terrain, WaterType::NotWater, 0, 0, 1.0, 1.0),
            0.0
        );
        assert_eq!(
            calc_water_depth(&terrain, WaterType::EntirelyWater, 0, 0, 20.0, 20.0),
            0.45
        );
    }
}
