//! The world's land grid: how a landblock divides into land cells, and how its terrain vertices sit
//! on it.
//!
//! A landblock is a square of `cells_per_side²` land cells. Each land cell side is cut into
//! `vertices_per_cell` equal steps, so the block has `cells_per_side * vertices_per_cell + 1`
//! vertices along a side, indexed **x major** (`x * vertices_per_side + y`), with x and y running
//! from the block's south-west corner.
//!
//! A world has one grid, read once from its region record when its files open, and every piece
//! that works on the land's vertices (the records, collision, the terrain mesh, the map, the
//! server) takes its shape from that value rather than deciding it for itself. A world laid out on
//! another grid is a different [`LandGrid`] value, not different code. [`LandGrid::DEFAULT`] is the
//! grid of every retail world.
//!
//! Every grid a world has is made by [`LandGrid::with_steps`] from its region record, so its cells
//! and blocks are always the default grid's; only the vertex steps and the road width vary. The
//! cell fields say what the vertex arithmetic is in terms of, and collision refuses a grid whose
//! cells are not the default grid's.

/// The shape of a world's land grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandGrid {
    /// A land cell's side, in world units.
    pub cell_length: f32,
    /// Land cells along a landblock side.
    pub cells_per_side: usize,
    /// Vertex steps along a land cell side: 1 puts a vertex only at each cell corner.
    pub vertices_per_cell: usize,
    /// A road's half width, measured in from a road vertex's square edges.
    pub road_width: f32,
}

impl LandGrid {
    /// The default grid: eight 24-unit land cells to a 192-unit landblock side, a vertex at every
    /// cell corner, so 9 x 9 vertices per block, and 5-unit roads.
    pub const DEFAULT: Self = Self {
        cell_length: 24.0,
        cells_per_side: 8,
        vertices_per_cell: 1,
        road_width: 5.0,
    };

    /// The default grid's cells cut into `vertices_per_cell` steps, with roads `road_width` wide: a
    /// world that keeps the default world's cells and blocks and lays more vertices in them. One
    /// step is the default grid itself, whatever road width is asked for.
    #[must_use]
    pub fn with_steps(vertices_per_cell: usize, road_width: f32) -> Self {
        if vertices_per_cell <= 1 {
            Self::DEFAULT
        } else {
            Self {
                vertices_per_cell,
                road_width,
                ..Self::DEFAULT
            }
        }
    }

    /// Whether this is the default grid, the grid of every retail world.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::DEFAULT
    }

    /// Whether the grid keeps the default grid's cells and blocks (only its vertex steps and road
    /// width may differ): every metric constant of the default world then holds.
    #[must_use]
    pub fn has_default_cells(&self) -> bool {
        self.cell_length == Self::DEFAULT.cell_length
            && self.cells_per_side == Self::DEFAULT.cells_per_side
    }

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

    /// The index of the vertex at land cell corner `(cx, cy)` (`0 ..= cells_per_side` each).
    #[must_use]
    pub const fn corner_index(&self, cx: usize, cy: usize) -> usize {
        self.vertex_index(cx * self.vertices_per_cell, cy * self.vertices_per_cell)
    }
}

impl Default for LandGrid {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Whether any and whether all of the vertices of a square `n` vertex steps a side are water, by
/// `wet` of each vertex's offset `(0..=n, 0..=n)` from the square's south-west corner: the four
/// corners of one of a grid's squares at one step, and every vertex of a land cell at the grid's
/// steps a cell. Returns `(any, all)`.
#[must_use]
pub fn water_of(n: usize, wet: impl Fn(usize, usize) -> bool) -> (bool, bool) {
    let (mut any, mut all) = (false, true);
    for a in 0..=n {
        for b in 0..=n {
            let w = wet(a, b);
            any |= w;
            all &= w;
        }
    }
    (any, all)
}

/// Whether block-local `(x, y)` is on a road, on a block of `side` vertices a side whose squares are
/// `square` units a side and whose roads are `width` wide: [`on_road_square`] over the square the
/// point lies in, by its corners' road bits (bits 0-1 of each terrain word in `words`, x major).
/// Off the block, no road.
#[must_use]
pub fn on_road(words: &[u16], side: usize, square: f32, width: f32, x: f32, y: f32) -> bool {
    let squares = i32::try_from(side.saturating_sub(1)).unwrap_or(0);
    let (sx, sy) = (
        crate::num::floor_to_i32(x / square),
        crate::num::floor_to_i32(y / square),
    );
    if !(0..squares).contains(&sx) || !(0..squares).contains(&sy) {
        return false;
    }
    // LINT-OK: 0..squares, checked above.
    #[allow(clippy::cast_sign_loss)]
    let base = sx as usize * side + sy as usize;
    let road = |k: usize| words.get(k).is_some_and(|w| w & 3 != 0);
    // LINT-OK: a square's place in its block, a few dozen.
    #[allow(clippy::cast_precision_loss)]
    let (dx, dy) = (x - square * sx as f32, y - square * sy as f32);
    on_road_square(
        road(base),
        road(base + 1),
        road(base + side),
        road(base + side + 1),
        dx,
        dy,
        square,
        width,
    )
}

/// The 16-case table for a square of side `span` and a road half-width of `width`: `W` below is
/// `width` and `F` is `span - width`, so the default world's table is this one at 24 and 5. The corners are `A` south-west,
/// `B` north-west, `C` south-east and `D` north-east; `(dx, dy)` is the point from `A`.
///
/// | corners with road | on-road test |
/// |---|---|
/// | none | false |
/// | all four | true |
/// | A only (SW) | `dx + dy < W` |
/// | B only (NW) | `dy - dx > F` |
/// | C only (SE) | `dy - dx < -F` |
/// | D only (NE) | `dx + dy > span + F` |
/// | A, B / A, C / C, D / B, D | `dx < W` / `dy < W` / `dx > F` / `dy > F` |
/// | A, D / B, C | `\|dx - dy\| < W` / `\|dx + dy - span\| < W` |
/// | three corners | the two edge tests of the missing corner's neighbors, either |
#[must_use]
#[allow(clippy::too_many_arguments)] // four corners, a point and the square's two lengths
pub fn on_road_square(
    a: bool,
    b: bool,
    c: bool,
    d: bool,
    dx: f32,
    dy: f32,
    span: f32,
    width: f32,
) -> bool {
    let w = width;
    let f = span - width;
    match (a, b, c, d) {
        (false, false, false, false) => false,
        (true, true, true, true) => true,
        // one corner
        (true, false, false, false) => dx + dy < w,
        (false, true, false, false) => dy - dx > f,
        (false, false, true, false) => dy - dx < -f,
        (false, false, false, true) => dx + dy > span + f,
        // two corners: the four edges
        (true, true, false, false) => dx < w,
        (true, false, true, false) => dy < w,
        (false, false, true, true) => dx > f,
        (false, true, false, true) => dy > f,
        // two corners: the two diagonals
        (true, false, false, true) => (dx - dy).abs() < w,
        (false, true, true, false) => (dx + dy - span).abs() < w,
        // three corners
        (true, true, true, false) => dx < w || dy < w,
        (true, true, false, true) => dx < w || dy > f,
        (true, false, true, true) => dx > f || dy < w,
        (false, true, true, true) => dx > f || dy > f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_grid_is_nine_by_nine_on_24_unit_cells() {
        let g = LandGrid::DEFAULT;
        assert_eq!((g.vertices_per_side(), g.vertex_count()), (9, 81));
        assert_eq!((g.block_length(), g.vertex_spacing()), (192.0, 24.0));
        assert_eq!(g.corner_index(3, 5), 3 * 9 + 5);
        assert!(g.is_default());
    }

    #[test]
    fn three_steps_a_cell_lay_25_vertices_a_side_8_units_apart() {
        let g = LandGrid::with_steps(3, 4.0);
        assert!(g.has_default_cells());
        assert!(!g.is_default());
        assert_eq!((g.vertices_per_side(), g.vertex_count()), (25, 625));
        assert_eq!((g.block_length(), g.vertex_spacing()), (192.0, 8.0));
        assert_eq!(g.corner_index(1, 2), 3 * 25 + 6);
    }

    #[test]
    fn water_of_a_square_is_any_and_all_of_its_vertices() {
        assert_eq!(water_of(1, |_, _| true), (true, true));
        assert_eq!(water_of(1, |_, _| false), (false, false));
        assert_eq!(water_of(3, |a, b| (a, b) == (3, 3)), (true, false));
        assert_eq!(water_of(3, |a, b| (a, b) != (2, 1)), (true, false));
    }

    #[test]
    fn one_step_is_the_default_grid_whatever_road_width_is_asked() {
        assert_eq!(LandGrid::with_steps(1, 9.0), LandGrid::DEFAULT);
        assert_eq!(LandGrid::with_steps(0, 9.0), LandGrid::DEFAULT);
    }
}
