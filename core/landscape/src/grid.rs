//! The landscape's grid: how a landblock divides into land cells, and how its terrain vertices sit
//! on it.
//!
//! A landblock is a square of `cells_per_side²` land cells. Each land cell side is cut into
//! `vertices_per_cell` equal steps, so the block has `cells_per_side * vertices_per_cell + 1`
//! vertices along a side, indexed **x major** (`x * vertices_per_side + y`), with x and y running
//! from the block's south-west corner. Every quantity the terrain and the block window derive from
//! the grid comes from the three numbers here, so a world laid out on another grid is a different
//! [`LandGrid`] value, not different code.

/// The shape of the landscape grid. [`LandGrid::DEFAULT`] is the grid this workspace's world uses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandGrid {
    /// A land cell's side, in world units.
    pub cell_length: f32,
    /// Land cells along a landblock side.
    pub cells_per_side: usize,
    /// Vertex steps along a land cell side: 1 puts a vertex only at each cell corner.
    pub vertices_per_cell: usize,
}

impl LandGrid {
    /// The default grid: eight 24-unit land cells to a 192-unit landblock side, a vertex at every
    /// cell corner, so 9 x 9 vertices per block.
    pub const DEFAULT: Self = Self {
        cell_length: 24.0,
        cells_per_side: 8,
        vertices_per_cell: 1,
    };

    /// A landblock side, in world units: `cells_per_side * cell_length`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // a handful of cells per side
    pub const fn block_length(&self) -> f32 {
        self.cells_per_side as f32 * self.cell_length
    }

    /// Vertex steps along a landblock side (the squares a side is cut into).
    #[must_use]
    pub const fn squares_per_side(&self) -> usize {
        self.cells_per_side * self.vertices_per_cell
    }

    /// Vertices along a landblock side: one more than the squares.
    #[must_use]
    pub const fn vertices_per_side(&self) -> usize {
        self.squares_per_side() + 1
    }

    /// Vertices in a landblock: the length of its per-vertex height and terrain arrays.
    #[must_use]
    pub const fn vertex_count(&self) -> usize {
        self.vertices_per_side() * self.vertices_per_side()
    }

    /// The distance between neighboring vertices: `cell_length / vertices_per_cell`.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // a handful of steps per cell
    pub const fn vertex_spacing(&self) -> f32 {
        self.cell_length / self.vertices_per_cell as f32
    }

    /// The index of vertex `(x, y)` in a block's per-vertex arrays, x major.
    #[must_use]
    pub const fn vertex_index(&self, x: usize, y: usize) -> usize {
        x * self.vertices_per_side() + y
    }
}

impl Default for LandGrid {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default grid is the block window's: 192-unit blocks of eight 24-unit cells with a
    /// vertex at each corner, nine to a side, indexed x major.
    ///
    /// Behaviour: world.terrain.the-default-grid-is-nine-vertices-to-a-block-side
    #[test]
    fn the_default_grid_is_nine_vertices_to_a_192_unit_block() {
        let g = LandGrid::DEFAULT;
        assert_eq!(g.block_length(), crate::BLOCK_LENGTH);
        assert_eq!(g.squares_per_side(), 8);
        assert_eq!(g.vertices_per_side(), 9);
        assert_eq!(g.vertex_count(), 81);
        assert_eq!(g.vertex_spacing(), 24.0);
        assert_eq!(
            g.vertex_index(1, 0),
            9,
            "x major: the next x is a whole column on"
        );
        assert_eq!(g.vertex_index(0, 1), 1);
        assert_eq!(LandGrid::default(), g);
    }

    /// A grid that cuts each cell side in two has a vertex in the middle of every cell edge and
    /// cell: twice the squares and 2n + 1 vertices along a side, at half the spacing.
    ///
    /// Behaviour: world.terrain.a-grid-of-another-shape-derives-its-own-block
    #[test]
    fn a_grid_with_two_steps_per_cell_has_a_vertex_mid_cell() {
        let g = LandGrid {
            cell_length: 20.0,
            cells_per_side: 8,
            vertices_per_cell: 2,
        };
        assert_eq!(g.block_length(), 160.0);
        assert_eq!(g.squares_per_side(), 16);
        assert_eq!(g.vertices_per_side(), 17);
        assert_eq!(g.vertex_count(), 289);
        assert_eq!(g.vertex_spacing(), 10.0);
        assert_eq!(g.vertex_index(2, 3), 2 * 17 + 3);
    }
}
