//! The landscape's constants: the grid, the height table, the vertex word's fields and the road
//! width, all read from the region's landscape record, and the terrain rules built from them.

use auberean_assets::region::Landscape;
use dereth_terrain::land::mesh::{set_height_table_within, LandGrid, TerrainSpec};

use crate::random::sw_to_ne_cut;

/// Entries in the height table.
pub const HEIGHT_TABLE_LEN: usize = 256;

/// The terrain kind that opens a hole in the ground: a square with this kind at any corner has no
/// land.
pub const HOLE_TERRAIN: u32 = 0x1E;

/// One field of the vertex word: its lowest bit and its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub offset: u32,
    pub bits: u32,
}

impl Field {
    /// The field's value in `word`.
    // D2-03707, D2-03708, D2-03709, D2-03710
    #[must_use]
    pub fn of(self, word: u32) -> u32 {
        if self.bits == 0 || self.offset + self.bits > 32 {
            return 0;
        }
        let mask = if self.bits == 32 {
            u32::MAX
        } else {
            (1u32 << self.bits) - 1
        };
        (word >> self.offset) & mask
    }
}

/// The six fields of a vertex word, lowest first: terrain, scene, encounter, color, road, replace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VertexWord {
    pub terrain: Field,
    pub scene: Field,
    pub encounter: Field,
    pub color: Field,
    pub road: Field,
    pub replace: Field,
}

/// Everything the terrain needs from the landscape record.
#[derive(Clone, Debug)]
pub struct LandConstants {
    /// The grid, the split rule and the height bound: what the shared terrain core builds with.
    pub spec: TerrainSpec,
    /// The height of each height byte.
    pub heights: [f32; HEIGHT_TABLE_LEN],
    pub word: VertexWord,
    /// The road's half width: a point this close to a road vertex's edges is on the road.
    pub road_width: f32,
    pub max_object_height: f32,
    pub sky_height: f32,
}

/// Why a landscape record cannot be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LandError {
    /// The cell side, the cells per block or the vertex steps are out of the accepted ranges.
    Grid,
    /// The vertex word's fields need more than 32 bits.
    Fields,
}

impl LandConstants {
    /// Read the constants from a landscape record, with the checks the world applies to them.
    ///
    /// # Errors
    /// When the grid or the field widths are out of range.
    // D2-13207, D2-13208, D2-13209
    pub fn from_record(l: &Landscape) -> Result<Self, LandError> {
        let cells = usize::try_from(l.cells_per_block_side).map_err(|_| LandError::Grid)?;
        let steps = usize::try_from(l.pixels_per_cell_side).map_err(|_| LandError::Grid)?;
        if !(0.1..1000.0).contains(&l.cell_side)
            || ![2, 4, 8, 16].contains(&cells)
            || !(1..=64).contains(&steps)
        {
            return Err(LandError::Grid);
        }
        let f = &l.vertex_fields;
        let widths = [f.terrain, f.scene, f.encounter, f.color, f.road, f.replace];
        if widths.iter().sum::<u32>() > 32 {
            return Err(LandError::Fields);
        }
        let mut offset = 0;
        let mut next = |bits: u32| {
            let field = Field { offset, bits };
            offset += bits;
            field
        };
        let word = VertexWord {
            terrain: next(f.terrain),
            scene: next(f.scene),
            encounter: next(f.encounter),
            color: next(f.color),
            road: next(f.road),
            replace: next(f.replace),
        };
        let height_max = l.sky_height - l.max_object_height;
        let heights = set_height_table_within(&l.heights, &[0.0; HEIGHT_TABLE_LEN], height_max);
        Ok(Self {
            spec: TerrainSpec {
                grid: LandGrid {
                    cell_length: l.cell_side,
                    cells_per_side: cells,
                    vertices_per_cell: steps,
                },
                split: sw_to_ne_cut,
                height_max,
            },
            heights,
            word,
            road_width: l.road_width,
            max_object_height: l.max_object_height,
            sky_height: l.sky_height,
        })
    }

    /// The grid.
    #[must_use]
    pub fn grid(&self) -> &LandGrid {
        &self.spec.grid
    }
}
