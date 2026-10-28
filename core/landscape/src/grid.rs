//! The landscape's grid: how a landblock divides into land cells, and how its terrain vertices sit
//! on it.
//!
//! A landblock is a square of `cells_per_side²` land cells. Each land cell side is cut into
//! `vertices_per_cell` equal steps, so the block has `cells_per_side * vertices_per_cell + 1`
//! vertices along a side, indexed **x major** (`x * vertices_per_side + y`), with x and y running
//! from the block's south-west corner. Every quantity the terrain and the block window derive from
//! the grid comes from the three numbers here, so a world laid out on another grid is a different
//! [`LandGrid`] value, not different code.
//!
//! [`WorldGrid`] is the whole world on that grid: its landblocks along a side, and every vertex by
//! its world grid point, which a world builder's brushes, map and terrain edits work in.

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

/// The world's grid of vertices: `blocks_per_side²` landblocks of one [`LandGrid`], every vertex
/// named by its world grid point `(gx, gy)`, counted in vertex steps from the world's south-west
/// corner. A block's east and north edges are the next block's west and south ones, so a grid
/// point on a block's edge stands in two blocks (four at a corner), each with its own copy of the
/// vertex. What a world builder's terrain brushes cover, where its map draws each vertex and which
/// blocks an edit of one vertex reaches are all worked out here, so a world on another grid is a
/// different [`WorldGrid`] value, not different code.
///
/// A landblock is named `0xXXYY`, its column in the high byte and its row in the low.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldGrid {
    /// How each landblock is cut.
    pub land: LandGrid,
    /// Landblocks along each side of the world.
    pub blocks_per_side: u32,
}

impl WorldGrid {
    /// The default world: 255 landblocks a side on [`LandGrid::DEFAULT`], so 2,041 vertices a
    /// side, 24 units apart.
    pub const DEFAULT: Self = Self {
        land: LandGrid::DEFAULT,
        blocks_per_side: 255,
    };

    /// The distance between neighboring vertices, in world units.
    #[must_use]
    pub const fn spacing(&self) -> f32 {
        self.land.vertex_spacing()
    }

    /// Vertex steps along a landblock side.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // a handful of steps per block
    pub const fn steps_per_block(&self) -> u32 {
        self.land.squares_per_side() as u32
    }

    /// The highest grid point on either axis: the world's far edge.
    #[must_use]
    pub const fn last_point(&self) -> u32 {
        self.blocks_per_side * self.steps_per_block()
    }

    /// The name of the landblock in column `bx` and row `by`.
    #[must_use]
    pub fn block_id(bx: u32, by: u32) -> u16 {
        u16::try_from((bx << 8) | by).unwrap_or(u16::MAX)
    }

    /// The grid point nearest world units `(x, y)`, in world units.
    #[must_use]
    pub fn nearest_vertex(&self, x: f32, y: f32) -> (f32, f32) {
        let g = self.spacing();
        #[allow(clippy::cast_precision_loss)] // the far edge, a few thousand steps
        let max = self.last_point() as f32;
        let snap = |v: f32| (v / g).round().clamp(0.0, max) * g;
        (snap(x), snap(y))
    }

    /// Every grid point within `radius` world units of `(x, y)`, nearest the center first (ties
    /// in grid order).
    #[must_use]
    pub fn within(&self, x: f32, y: f32, radius: f32) -> Vec<(u32, u32)> {
        let g = self.spacing();
        let r = radius.max(0.0);
        let lo = |v: f32| {
            u32::try_from(dereth_primitives::num::floor_to_i32(((v - r) / g).ceil()).max(0))
                .unwrap_or(0)
        };
        let hi = |v: f32| {
            u32::try_from(dereth_primitives::num::floor_to_i32((v + r) / g).max(0))
                .unwrap_or(0)
                .min(self.last_point())
        };
        let mut out = Vec::new();
        for gx in lo(x)..=hi(x) {
            for gy in lo(y)..=hi(y) {
                #[allow(clippy::cast_precision_loss)] // a grid point, a few thousand at most
                let (dx, dy) = (gx as f32 * g - x, gy as f32 * g - y);
                let d = dx * dx + dy * dy;
                if d <= r * r {
                    out.push((d, gx, gy));
                }
            }
        }
        out.sort_by(|a, b| a.0.total_cmp(&b.0).then((a.1, a.2).cmp(&(b.1, b.2))));
        out.into_iter().map(|(_, gx, gy)| (gx, gy)).collect()
    }

    /// The landblock a grid point is written in, and its local coordinates there: local `0` to
    /// one short of a block's steps, so a block's east or north edge belongs to the next block;
    /// the world's own far edge to the last.
    #[must_use]
    pub fn owner(&self, gx: u32, gy: u32) -> (u16, u8, u8) {
        let n = self.steps_per_block();
        let last = self.blocks_per_side.saturating_sub(1);
        let bx = (gx / n).min(last);
        let by = (gy / n).min(last);
        (
            Self::block_id(bx, by),
            u8::try_from(gx - bx * n).unwrap_or(u8::MAX),
            u8::try_from(gy - by * n).unwrap_or(u8::MAX),
        )
    }

    /// Every landblock that holds a copy of grid point `(gx, gy)`, with the vertex's local
    /// coordinates in each.
    #[must_use]
    pub fn copies(&self, gx: u32, gy: u32) -> Vec<(u16, usize, usize)> {
        let n = self.steps_per_block();
        let blocks = |g: u32| -> Vec<u32> {
            if g.is_multiple_of(n) && g > 0 {
                vec![g / n - 1, g / n]
            } else {
                vec![g / n]
            }
        };
        let mut out = Vec::new();
        for bx in blocks(gx) {
            for by in blocks(gy) {
                if bx >= self.blocks_per_side || by >= self.blocks_per_side {
                    continue;
                }
                out.push((
                    Self::block_id(bx, by),
                    (gx - bx * n) as usize,
                    (gy - by * n) as usize,
                ));
            }
        }
        out
    }

    /// Block `(bx, by)`'s south-west corner, in world units.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // a block, a few hundred at most
    pub fn block_origin(&self, (bx, by): (i32, i32)) -> (f32, f32) {
        let l = self.land.block_length();
        (bx as f32 * l, by as f32 * l)
    }

    /// The landblock under world units `(x, y)` and the units there; `None` outside the world.
    #[must_use]
    pub fn landblock_at(&self, x: f32, y: f32) -> Option<(u16, f32, f32)> {
        let l = self.land.block_length();
        let bx = dereth_primitives::num::floor_to_i32(x / l);
        let by = dereth_primitives::num::floor_to_i32(y / l);
        let blocks = i32::try_from(self.blocks_per_side).ok()?;
        if !(0..blocks).contains(&bx) || !(0..blocks).contains(&by) {
            return None;
        }
        let lb = u16::try_from((bx << 8) | by).ok()?;
        let (ox, oy) = self.block_origin((bx, by));
        Some((lb, x - ox, y - oy))
    }

    /// The side of a square map with a pixel for every grid point, in pixels: the next power of
    /// two.
    #[must_use]
    pub const fn map_side(&self) -> usize {
        (self.last_point() as usize + 1).next_power_of_two()
    }

    /// The map pixel (column, row) of world units `(x, y)`, north up.
    #[must_use]
    pub fn map_pixel(&self, x: f32, y: f32) -> (f32, f32) {
        let g = self.spacing();
        #[allow(clippy::cast_precision_loss)] // the far edge, a few thousand steps
        let top = self.last_point() as f32;
        (x / g, top - y / g)
    }

    /// The world units of map pixel (column, row): [`Self::map_pixel`] undone.
    #[must_use]
    pub fn map_world(&self, px: f32, py: f32) -> (f32, f32) {
        let g = self.spacing();
        #[allow(clippy::cast_precision_loss)] // the far edge, a few thousand steps
        let top = self.last_point() as f32;
        (px * g, (top - py) * g)
    }

    /// The map's top-left pixel (column, row) of landblock `lb`'s squares.
    #[must_use]
    pub fn map_block_corner(&self, lb: u16) -> (usize, usize) {
        let n = self.steps_per_block() as usize;
        let (bx, by) = (usize::from(lb >> 8), usize::from(lb & 0xFF));
        (bx * n, self.last_point() as usize - (by * n + n - 1))
    }
}

/// One field of a terrain vertex as a world builder edits it: its name, as sources and items write
/// it, and the highest value it takes (from 0). A game's terrain vertices are a table of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexField {
    pub name: &'static str,
    pub max: u16,
}

impl VertexField {
    /// The field of `table` called `name`, with its place in the table.
    #[must_use]
    pub fn find(table: &[Self], name: &str) -> Option<(usize, Self)> {
        table
            .iter()
            .copied()
            .enumerate()
            .find(|(_, f)| f.name == name)
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

#[cfg(test)]
mod world_tests {
    //! Behaviour: none (tooling: the world builders' grid of vertices across the world)
    use super::*;

    #[test]
    fn the_default_world_is_255_blocks_of_nine_vertices_and_its_map_is_2048_pixels() {
        let w = WorldGrid::DEFAULT;
        assert_eq!(w.spacing(), 24.0);
        assert_eq!(w.steps_per_block(), 8);
        assert_eq!(w.last_point(), 2040);
        assert_eq!(w.map_side(), 2048);
        assert_eq!(w.nearest_vertex(35.0, -5.0), (24.0, 0.0));
        assert_eq!(w.nearest_vertex(1e9, 13.0), (2040.0 * 24.0, 24.0));
        assert_eq!(w.block_origin((2, 3)), (384.0, 576.0));
        assert_eq!(w.landblock_at(400.0, 580.0), Some((0x0203, 16.0, 4.0)));
        assert_eq!(w.landblock_at(-1.0, 0.0), None);
        assert_eq!(
            w.landblock_at(255.0 * 192.0, 0.0),
            None,
            "past the last block"
        );
    }

    #[test]
    fn a_vertex_on_a_block_edge_has_a_copy_in_each_block_and_is_written_in_the_east_or_north_one() {
        let w = WorldGrid::DEFAULT;
        assert_eq!(w.copies(3, 4), vec![(0x0000, 3, 4)]);
        assert_eq!(
            w.copies(8, 16),
            vec![
                (0x0001, 8, 8),
                (0x0002, 8, 0),
                (0x0101, 0, 8),
                (0x0102, 0, 0)
            ]
        );
        assert_eq!(w.owner(8, 16), (0x0102, 0, 0));
        assert_eq!(
            w.owner(2040, 2040),
            (0xFEFE, 8, 8),
            "the far edge is the last block's"
        );
        assert_eq!(w.copies(2040, 0), vec![(0xFE00, 8, 0)]);
    }

    #[test]
    fn a_brush_covers_the_points_within_its_radius_nearest_first() {
        let w = WorldGrid::DEFAULT;
        assert_eq!(w.within(48.0, 48.0, 0.0), vec![(2, 2)]);
        let ring = w.within(48.0, 48.0, 24.0);
        assert_eq!(ring[0], (2, 2));
        assert_eq!(ring[1..], [(1, 2), (2, 1), (2, 3), (3, 2)]);
        assert_eq!(w.within(0.0, 0.0, 30.0), vec![(0, 0), (0, 1), (1, 0)]);
        assert_eq!(w.within(0.0, 0.0, 34.0).len(), 4, "the diagonal at 33.9");
    }

    #[test]
    fn a_vertex_field_is_found_in_its_table_by_name() {
        const TABLE: [VertexField; 2] = [
            VertexField {
                name: "height",
                max: 255,
            },
            VertexField {
                name: "road",
                max: 3,
            },
        ];
        assert_eq!(VertexField::find(&TABLE, "road"), Some((1, TABLE[1])));
        assert_eq!(VertexField::find(&TABLE, "Road"), None);
    }

    #[test]
    fn a_world_on_another_grid_numbers_its_vertices_and_map_by_that_grid() {
        let w = WorldGrid {
            land: LandGrid {
                cell_length: 20.0,
                cells_per_side: 8,
                vertices_per_cell: 2,
            },
            blocks_per_side: 256,
        };
        assert_eq!(w.spacing(), 10.0);
        assert_eq!(w.steps_per_block(), 16);
        assert_eq!(w.last_point(), 4096);
        assert_eq!(w.map_side(), 8192);
        assert_eq!(w.copies(16, 5), vec![(0x0000, 16, 5), (0x0100, 0, 5)]);
        assert_eq!(w.owner(4096, 17), (0xFF01, 16, 1));
        assert_eq!(w.landblock_at(170.0, 5.0), Some((0x0100, 10.0, 5.0)));
        assert_eq!(w.map_pixel(100.0, 0.0), (10.0, 4096.0));
        assert_eq!(w.map_world(10.0, 4096.0), (100.0, 0.0));
        assert_eq!(w.map_block_corner(0x0001), (0, 4096 - 31));
    }
}
