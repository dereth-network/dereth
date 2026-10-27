//! Terrain mesh construction.
//!
//! The landblock mesh construction and the four routines it drives: the vertices, the polygons,
//! the UVs and the cell rotation.
//!
//! The terrain record layout and height table are described in `docs/formats/15-region.md`. The
//! mesh preserves the original `x*9 + y` indexing, diagonal split, UVs and per-cell rotation.
//!
//! The triangles built here are **shared with the physics engine**: each land cell hands the same
//! two polygons to rendering and `find_terrain_poly`. The split rule must therefore agree with
//! `dereth_physics::land::split_hash` bit for bit, or the drawn ground and the walked
//! ground disagree.

use dereth_assets::region::Region;
use dereth_assets::world::CellLandblock;
use dereth_primitives::{LandblockId, Vec3};

use crate::consts::{
    LAND_HEIGHT_MAX, LAND_HEIGHT_TABLE_LEN, NE_CORNER, NW_CORNER, SE_CORNER, SW_CORNER,
};
use crate::land::water::{calc_water, WaterType};
use crate::narrow::{i32_of, u16_of, u16_of_i32, u32_of, u8_of};
use crate::Plane;

/// The grid a landblock's terrain is laid on: the landscape's.
pub use dereth_landscape::LandGrid;

/// Which edges of a block are stitched toward the finer ring beside it: the landscape window's
/// type, consumed here by mesh generation.
pub use dereth_landscape::Direction;

/// `Rotation` — `ROT_0`, `ROT_90`, `ROT_180`, `ROT_270`, used as an index into the corner tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum Rotation {
    #[default]
    Rot0 = 0,
    Rot90 = 1,
    Rot180 = 2,
    Rot270 = 3,
}

impl Rotation {
    /// The four rotations in order, so a caller can index the key set.
    pub const ALL: [Self; 4] = [Self::Rot0, Self::Rot90, Self::Rot180, Self::Rot270];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// `rot += ROT_90`, returning `None` past `ROT_270` -- which is exactly how
    /// the terrain alpha search gives up.
    #[must_use]
    pub const fn next(self) -> Option<Self> {
        match self {
            Self::Rot0 => Some(Self::Rot90),
            Self::Rot90 => Some(Self::Rot180),
            Self::Rot180 => Some(Self::Rot270),
            Self::Rot270 => None,
        }
    }
}

/// One terrain triangle: three vertex indices into [`LandblockMesh::vertices`], the plane the
/// winding produces, the surface index the cell resolved to, and the three uv indices into
/// [`crate::consts::LAND_UVS`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandPolygon {
    pub v: [u32; 3],
    pub plane: Plane,
    /// Index into the land-surface array installed for block drawing.
    pub surface: u16,
    /// Positive-surface UV indices filled from the corner tables at the cell's rotation.
    pub uv_indices: [u8; 3],
    /// Stippling: `BOTH_STIPPLING (3)` when the cell is uniform, else 0.
    pub stippling: u8,
}

/// `BOTH_STIPPLING`. The UV construction sets it on both triangles of a uniform cell.
pub const BOTH_STIPPLING: u8 = 3;

/// One landblock's constructed geometry — the renderer's half of the landblock structure.
///
/// Shared in shape with the physics engine's copy: the split flags and the polygon planes must be
/// identical (contracts 3.1, 3.2).
#[derive(Debug, Clone, PartialEq)]
pub struct LandblockMesh {
    pub id: LandblockId,
    /// 8, 4, 2 or 1 — the LOD ring. `side_polygon_count` is the same number in the client.
    pub side_cell_count: u8,
    /// `side_cell_count + 1`.
    pub side_vertex_count: u8,
    /// Direction this block was stitched for; [`Direction::InViewerBlock`] when it was not.
    pub trans_dir: Direction,
    /// `side_vertex_count²` vertices, index `v(a, b) = a * side_vertex_count + b`. **True** z: the
    /// z-fight adjustment is applied at emission, not here.
    pub vertices: Vec<Vec3>,
    /// The SW-to-NE cut, `side_cell_count²` flags indexed `i * side_cell_count + j`.
    pub sw_to_ne_cut: Vec<bool>,
    /// `2 * side_cell_count²` triangles, in emission order: cell `(i, j)` owns `[2n]` and `[2n+1]`
    /// with `n = i * side_cell_count + j`.
    pub polygons: Vec<LandPolygon>,
    /// The merge key and rotation the cell-rotation lookup produced per cell, same indexing as
    /// [`Self::sw_to_ne_cut`]. Kept so the compositor never has to re-derive them.
    pub cell_keys: Vec<(crate::land::merge::MergeKey, Rotation)>,
    /// Per-cell water, `side_cell_count²`. Always `NotWater` unless `side_cell_count == 8`.
    pub cell_water: Vec<WaterType>,
    /// The whole-block summary from the water calculation.
    pub water_type: WaterType,
    /// Per-vertex terrain lighting, filled by [`crate::land::lighting::calc_lighting`]. Empty until
    /// then, which mirrors the client allocating the array before `calc_lighting` fills it.
    pub colours: Vec<[u8; 3]>,
    /// The landblock's height limit: tallest terrain vertex + 200.
    pub max_zval: f32,
    /// The land limits: lowest terrain vertex - 1.
    pub min_zval: f32,
}

impl LandblockMesh {
    /// `v(a, b) = a * side_vertex_count + b`.
    #[inline]
    #[must_use]
    pub fn vertex_index(&self, a: usize, b: usize) -> usize {
        a * self.side_vertex_count as usize + b
    }

    /// The two triangles of cell `(i, j)`.
    #[must_use]
    pub fn cell_polygons(&self, i: usize, j: usize) -> (&LandPolygon, &LandPolygon) {
        let n = i * self.side_cell_count as usize + j;
        (&self.polygons[2 * n], &self.polygons[2 * n + 1])
    }
}

/// Diagonal split derived from a hash of the cell's
/// **global** coordinates.
///
/// ```text
/// v   = (x * 0x0CCAC033 + 0x6C1AC587) * y + x * 0xBDE41C43 + 0xAE6470DB   (32-bit wrapping)
///     = x*y*0x0CCAC033 + y*0x6C1AC587 - x*0x421BE3BD - 0x519B8F25
/// cut = (v as unsigned) * 2.3283064e-10 >= 0.5                             (the top bit of v)
/// ```
///
/// `2.3283064e-10` is 2⁻³², so the float compare is exactly "is the sign bit set" and is done here
/// as a shift. Returns **true** when the shared edge runs SW→NE.
///
/// **Note the convention.** ACE's `LandblockMesh.GetSplitDir` computes the identical 32-bit value
/// but returns `(dw & 0x80000000) == 0` under the name "NW-SE split" — the inverse boolean with the
/// opposite naming, so a naive port flips every terrain triangle pair. ACE's *physics* copy
/// (`Physics/Common/LandblockStruct.cs`) computes a different expression entirely and is simply
/// wrong, so ACE disagrees with itself. The client expression above is the reference.
#[inline]
#[must_use]
pub fn sw_to_ne_cut(global_cell_x: i32, global_cell_y: i32) -> bool {
    // Written in the retail client's own nesting, all 32-bit wrapping.
    // `dereth_physics::land::split_hash` is the same expression.
    let x = global_cell_x as u32;
    let y = global_cell_y as u32;
    let v = x
        .wrapping_mul(0x0CCA_C033)
        .wrapping_add(0x6C1A_C587)
        .wrapping_mul(y)
        .wrapping_add(x.wrapping_mul(0xBDE4_1C43))
        .wrapping_add(0xAE64_70DB);
    v >> 31 != 0
}

/// The terrain's rules: the grid it is built on, which way each square is split, and the highest
/// height the height table accepts. [`TerrainSpec::DEFAULT`] is this workspace's world; another
/// world built by the same algorithms is another value.
#[derive(Debug, Clone, Copy)]
pub struct TerrainSpec {
    /// The grid: block, cell and vertex spacing.
    pub grid: LandGrid,
    /// Whether the square at **global** square coordinates `(x, y)` (block coordinate times the
    /// squares per block side, plus the square within the block) is cut from its south-west to its
    /// north-east corner. It must depend on the global coordinates alone, or neighboring blocks
    /// disagree about a square they share.
    pub split: fn(i32, i32) -> bool,
    /// The highest entry the height table accepts: the sky height less the greatest object height.
    pub height_max: f32,
}

impl TerrainSpec {
    /// The default terrain: [`LandGrid::DEFAULT`], the [`sw_to_ne_cut`] hash and a height table
    /// of at most [`LAND_HEIGHT_MAX`].
    pub const DEFAULT: Self = Self {
        grid: LandGrid::DEFAULT,
        split: sw_to_ne_cut,
        height_max: LAND_HEIGHT_MAX,
    };
}

impl Default for TerrainSpec {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Copies entries until the first one outside `[0, 800]`,
/// then stops — leaving entries *k*..255 at whatever was there before rather than rejecting the
/// whole table. The table is a lookup, **not** `2 * i`. ACE's comment
/// saying it is twice the byte is wrong and misplaces all high terrain.
///
/// `previous` is the table currently installed, which is what the short-circuit leaves behind.
#[must_use]
pub fn set_height_table(
    candidate: &[f32],
    previous: &[f32; LAND_HEIGHT_TABLE_LEN],
) -> [f32; LAND_HEIGHT_TABLE_LEN] {
    set_height_table_within(candidate, previous, LAND_HEIGHT_MAX)
}

/// [`set_height_table`] with the upper bound given: entries are copied until the first one outside
/// `[0, height_max]`, and the rest keep their previous values.
#[must_use]
pub fn set_height_table_within(
    candidate: &[f32],
    previous: &[f32; LAND_HEIGHT_TABLE_LEN],
    height_max: f32,
) -> [f32; LAND_HEIGHT_TABLE_LEN] {
    let mut out = *previous;
    for (k, slot) in out.iter_mut().enumerate() {
        let Some(&v) = candidate.get(k) else { break };
        if !(0.0..=height_max).contains(&v) {
            break;
        }
        *slot = v;
    }
    out
}

/// Pull the 256-entry height table out of a decoded region through [`set_height_table`], starting
/// from an all-zero previous table (which is what a fresh `LandDefs` holds).
#[must_use]
pub fn height_table(region: &Region) -> [f32; LAND_HEIGHT_TABLE_LEN] {
    set_height_table(
        &region.land_defs.land_height_table,
        &[0.0; LAND_HEIGHT_TABLE_LEN],
    )
}

/// Construct a landblock's vertices on `grid`, at a detail of `side_cell_count` land cells a side.
///
/// Vertex `(i, j)` sits at `(i·L, j·L, height_table[height[(i·V + j)·step]])` with
/// `L = block_length / side_polygon_count`, `V` the grid's full-detail vertices per side and
/// `step = cells_per_side / side_cell_count`; `side_polygon_count` is `side_cell_count` times the
/// grid's vertices per cell. On the default grid that is `L = 192 / side_cell_count` and
/// `height[i·step·9 + j·step]`.
///
/// # Panics
/// When `height` holds fewer than the grid's vertex count, or `side_cell_count` is 0.
#[must_use]
pub fn land_vertices(
    grid: &LandGrid,
    height: &[u8],
    side_cell_count: usize,
    height_table: &[f32; LAND_HEIGHT_TABLE_LEN],
) -> Vec<Vec3> {
    let side_polygon_count = side_cell_count * grid.vertices_per_cell;
    #[allow(clippy::cast_precision_loss)] // a few dozen squares a side
    let scale = grid.block_length() / side_polygon_count as f32;
    let step = grid.cells_per_side / side_cell_count;
    let full_side = grid.vertices_per_side();
    let svc = side_polygon_count + 1;
    let mut vertices = Vec::with_capacity(svc * svc);
    for i in 0..svc {
        for j in 0..svc {
            #[allow(clippy::cast_precision_loss)] // i, j are a few dozen at most
            let (x, y) = (i as f32 * scale, j as f32 * scale);
            let z = height_table[height[(i * full_side + j) * step] as usize];
            vertices.push(Vec3::new(x, y, z));
        }
    }
    vertices
}

/// One terrain triangle of [`LandGeometry`]: three vertex indices and the plane they make.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandTriangle {
    pub v: [u32; 3],
    pub plane: Plane,
}

/// A landblock's terrain geometry on any grid: what [`land_geometry`] builds before any texturing.
#[derive(Debug, Clone, PartialEq)]
pub struct LandGeometry {
    /// Squares along a side at this detail (the polygon count along a side).
    pub squares_per_side: usize,
    /// `squares_per_side + 1`.
    pub vertices_per_side: usize,
    /// `vertices_per_side²` vertices, x major, block-local, true heights.
    pub vertices: Vec<Vec3>,
    /// Per square, x major: whether its shared edge runs south-west to north-east.
    pub sw_to_ne_cut: Vec<bool>,
    /// Two triangles per square, x major: square `(x, y)` owns `[2n]` and `[2n + 1]` with
    /// `n = x * squares_per_side + y`, wound as [`cell_triangles`] winds them.
    pub triangles: Vec<LandTriangle>,
    /// Per square, x major: whether the square is a hole (one of its corners is marked). All false
    /// when no hole marks were given.
    pub holes: Vec<bool>,
}

/// Build a landblock's terrain geometry: its vertices ([`land_vertices`]), each square's split
/// (`spec.split` of its global square coordinates), its two triangles and their planes
/// ([`make_plane`]), and which squares are holes.
///
/// `block_x` and `block_y` are the block's coordinates; a square's global coordinates are the
/// block's times the grid's squares per side, plus the square within the block (at reduced detail,
/// the reduced square's index, as the default world has always done). `hole_vertices`, when given,
/// marks full-detail vertices (x major) that open a hole: a square with a marked corner is a hole.
///
/// # Panics
/// As [`land_vertices`]; and when `hole_vertices` holds fewer than the grid's vertex count.
#[must_use]
pub fn land_geometry(
    spec: &TerrainSpec,
    height: &[u8],
    height_table: &[f32; LAND_HEIGHT_TABLE_LEN],
    block_x: i32,
    block_y: i32,
    side_cell_count: usize,
    hole_vertices: Option<&[bool]>,
) -> LandGeometry {
    let grid = &spec.grid;
    let vertices = land_vertices(grid, height, side_cell_count, height_table);
    let squares = side_cell_count * grid.vertices_per_cell;
    let svc = squares + 1;
    let step = grid.cells_per_side / side_cell_count;
    let full_side = grid.vertices_per_side();
    // LINT-OK: a block coordinate times a few dozen squares stays far inside i32.
    let per_block = i32_of(grid.squares_per_side());
    let (gx_base, gy_base) = (block_x * per_block, block_y * per_block);
    let mut sw_to_ne_cut = Vec::with_capacity(squares * squares);
    let mut triangles = Vec::with_capacity(2 * squares * squares);
    let mut holes = Vec::with_capacity(squares * squares);
    for i in 0..squares {
        for j in 0..squares {
            // LINT-OK: index arithmetic bounded by the squares of one block.
            let cut = (spec.split)(gx_base + i32_of(i), gy_base + i32_of(j));
            sw_to_ne_cut.push(cut);
            for tri in cell_triangles(svc, i, j, cut) {
                let [a, b, c] = tri;
                triangles.push(LandTriangle {
                    v: tri,
                    plane: make_plane(
                        vertices[a as usize],
                        vertices[b as usize],
                        vertices[c as usize],
                    ),
                });
            }
            let hole = hole_vertices.is_some_and(|marks| {
                let (x0, y0) = (i * step, j * step);
                [
                    (x0, y0),
                    (x0 + step, y0),
                    (x0, y0 + step),
                    (x0 + step, y0 + step),
                ]
                .iter()
                .any(|&(x, y)| marks[x * full_side + y])
            });
            holes.push(hole);
        }
    }
    LandGeometry {
        squares_per_side: squares,
        vertices_per_side: svc,
        vertices,
        sw_to_ne_cut,
        triangles,
        holes,
    }
}

/// for the three-vertex case.
///
/// The normal is `(v1 - v0) x (v2 - v0)`, **normalised** — the retail client folds the
/// reciprocal square root into all three components. With +x east and +y north the windings
/// used above are counter-clockwise seen from above, so `N.z > 0`.
///
/// `d` is `-(N·v0 + N·v1 + N·v2) / 3`, not `-N·v0`. For an exactly planar triangle the two are the
/// same number mathematically and different floats in practice, and the plane distance feeds
/// Set a plane's height, which places every tree.
pub(crate) fn make_plane(a: Vec3, b: Vec3, c: Vec3) -> Plane {
    let u = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
    let v = Vec3::new(c.x - a.x, c.y - a.y, c.z - a.z);
    let n = Vec3::new(
        u.y * v.z - u.z * v.y,
        u.z * v.x - u.x * v.z,
        u.x * v.y - u.y * v.x,
    );
    let len = n.magnitude();
    // The client divides unconditionally, producing inf/NaN for a degenerate polygon. Land cells
    // have a fixed non-degenerate xy footprint so this branch is unreachable for terrain; it exists
    // so a malformed caller gets a usable plane rather than NaN propagating into the scenery hash.
    let n = if len > 0.0 {
        let inv = 1.0 / len;
        Vec3::new(n.x * inv, n.y * inv, n.z * inv)
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let sum = n.dot(a) + n.dot(b) + n.dot(c);
    Plane {
        normal: n,
        d: -(sum / 3.0),
    }
}

/// Drop a point onto the plane:
/// `z = -(x·N.x + y·N.y + d) / N.z`, which needs `|N.z| > 0.0002`.
#[must_use]
pub fn plane_set_height(p: &Plane, x: f32, y: f32) -> Option<f32> {
    if p.normal.z.abs() <= crate::consts::EPSILON {
        return None;
    }
    Some(-(x * p.normal.x + y * p.normal.y + p.d) / p.normal.z)
}

/// Re-run `make_plane` on every triangle after the vertices moved, which
/// is what `generate` does when the polygon arrays were **not** rebuilt.
pub fn adj_planes(m: &mut LandblockMesh) {
    for p in &mut m.polygons {
        let (a, b, c) = (
            m.vertices[p.v[0] as usize],
            m.vertices[p.v[1] as usize],
            m.vertices[p.v[2] as usize],
        );
        p.plane = make_plane(a, b, c);
    }
}

/// The two triangles of cell `(i, j)` as vertex-index triples, per the retail diagonal-split
/// table.
///
/// | SW-to-NE cut | triangle 0 | triangle 1 |
/// |---|---|---|
/// | 1 | SW, SE, NE | SW, NE, NW |
/// | 0 | SW, SE, NW | NE, NW, SE |
#[must_use]
pub fn cell_triangles(svc: usize, i: usize, j: usize, cut: bool) -> [[u32; 3]; 2] {
    // LINT-OK: index arithmetic; svc <= 9 so the product is far inside u32.
    let v = |a: usize, b: usize| -> u32 { u32_of(a * svc + b) };
    let (sw, se, ne, nw) = (v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1));
    if cut {
        [[sw, se, ne], [sw, ne, nw]]
    } else {
        [[sw, se, nw], [ne, nw, se]]
    }
}

/// The uv-index triples matching [`cell_triangles`], from the corner tables at rotation `r`.
///
/// | SW-to-NE cut | tri 0 | tri 1 |
/// |---|---|---|
/// | 1 | SW, SE, NE | SW, NE, NW |
/// | 0 | SW, SE, NW | NE, NW, SE |
#[must_use]
pub fn cell_uv_indices(cut: bool, r: Rotation) -> [[u8; 3]; 2] {
    let k = r.index();
    let (sw, se, ne, nw) = (SW_CORNER[k], SE_CORNER[k], NE_CORNER[k], NW_CORNER[k]);
    if cut {
        [[sw, se, ne], [sw, ne, nw]]
    } else {
        [[sw, se, nw], [ne, nw, se]]
    }
}

/// Generate a landblock mesh in one-shot form:
/// build the whole mesh from scratch for a given LOD divisor and stitch direction.
///
/// The client's `generate` is incremental — it early-outs when neither the cell count nor the
/// direction changed, and only re-runs the polygon and UV construction when the arrays were
/// reallocated. [`dereth_landscape::LandblockWindow`] reproduces that decision; this function is
/// the "rebuilt" path it calls into.
///
/// `lod_div` is 1, 2, 4 or 8, giving `side_cell_count = 8 / lod_div`.
///
/// # Panics
/// Never: `lod_div` is validated and any other value is treated as 1, matching the client having no
/// other caller than the block-orientation step.
#[must_use]
pub fn generate_landblock(
    lb: &CellLandblock,
    region: &Region,
    block_x: i32,
    block_y: i32,
    lod_div: u8,
    dir: Direction,
) -> LandblockMesh {
    let table = height_table(region);
    generate_landblock_with_table(lb, region, &table, block_x, block_y, lod_div, dir)
}

/// [`generate_landblock`] with the height table hoisted out, so a caller building 65 025 blocks
/// does not re-derive it 65 025 times.
#[must_use]
pub fn generate_landblock_with_table(
    lb: &CellLandblock,
    region: &Region,
    table: &[f32; LAND_HEIGHT_TABLE_LEN],
    block_x: i32,
    block_y: i32,
    lod_div: u8,
    dir: Direction,
) -> LandblockMesh {
    let side_cell_count = match lod_div {
        2 => 4,
        4 => 2,
        8 => 1,
        _ => 8,
    };
    let svc = side_cell_count + 1;
    // The geometry is the shared core's on the default grid, where a land cell is one square, so
    // its squares are the cells below in the same order.
    let geometry = land_geometry(
        &TerrainSpec::DEFAULT,
        &lb.height,
        table,
        block_x,
        block_y,
        side_cell_count,
        None,
    );
    debug_assert_eq!(geometry.vertices_per_side, svc);

    // The polygon construction interleaved with the UV construction and the cell rotation: the
    // client runs them as three passes over the same cells, and the
    // per-cell results are independent, so one pass produces identical output.
    let mut polygons = Vec::with_capacity(2 * side_cell_count * side_cell_count);
    let mut cell_keys = Vec::with_capacity(side_cell_count * side_cell_count);
    for i in 0..side_cell_count {
        for j in 0..side_cell_count {
            let n = i * side_cell_count + j;
            let cut = geometry.sw_to_ne_cut[n];

            let (key, rot, uniform) =
                crate::land::merge::cell_rotation(lb, region, side_cell_count, i, j);
            cell_keys.push((key, rot));

            let uvs = cell_uv_indices(cut, rot);
            // UV construction sets `pos_surface` to the resolved land-surface index.
            // The array is per-region and installed for each block draw; first use allocates the
            // index. This crate carries the merge key instead and lets the compositor own
            // the index, so `surface` here is the cell's slot within the block's key list.
            // LINT-OK: index arithmetic, bounded by the 8x8 cell grid.
            let surface = u16_of(n);
            for (k, uv_indices) in uvs.into_iter().enumerate() {
                let tri = geometry.triangles[2 * n + k];
                polygons.push(LandPolygon {
                    v: tri.v,
                    plane: tri.plane,
                    surface,
                    uv_indices,
                    stippling: if uniform { BOTH_STIPPLING } else { 0 },
                });
            }
        }
    }
    let LandGeometry {
        vertices,
        sw_to_ne_cut: cuts,
        ..
    } = geometry;

    let (cell_water, water_type) = calc_water(&lb.terrain, side_cell_count);

    // The land limits, over the full 9x9 height array regardless of LOD.
    let max_h = lb.height.iter().copied().max().unwrap_or(0);
    let min_h = lb.height.iter().copied().min().unwrap_or(0);

    let mut mesh = LandblockMesh {
        // LINT-OK: a landblock id is two bytes, one per axis, each 0..=0xFE.
        id: LandblockId((u16_of_i32(block_x & 0xFF) << 8) | u16_of_i32(block_y & 0xFF)),
        // LINT-OK: side_cell_count is 1, 2, 4 or 8.
        side_cell_count: u8_of(side_cell_count),
        side_vertex_count: u8_of(svc),
        trans_dir: dir,
        vertices,
        sw_to_ne_cut: cuts,
        polygons,
        cell_keys,
        cell_water,
        water_type,
        colours: Vec::new(),
        max_zval: table[max_h as usize] + crate::consts::MAX_OBJECT_HEIGHT,
        min_zval: table[min_h as usize] - 1.0,
    };

    // The mesh generator calls [`crate::land::stitch::trans_adjust`] here — between vertex
    // construction and the polygon pass, under its own guard — and this is its only caller. It was
    // absent from this function, so every LOD
    // ring boundary in the drawn window was an open seam: measured over Holtburg, a 4-cell ring-2
    // block's edge stood up to **8 m above** the 8-cell ground it abuts at 12 of 108 samples, and
    // a stitched block's outward edge disagreed with its coarser neighbour's by up to **10 m** at
    // 43 of 72. Those errors appear as streaks below the horizon.
    //
    // Retail stitches the vertices *before* building the polygons, so the planes come out of
    // polygon construction already adjusted; [`crate::land::stitch::trans_adjust`] instead ends by
    // recomputing the same planes from the same vertices. The
    // guard inside it is `generate`'s own, so a full-detail or `InViewerBlock` slot is untouched
    // and this is a no-op for every caller that asks for one — physics included, which never calls
    // this function at all.
    crate::land::stitch::trans_adjust(&mut mesh, &lb.height, table, dir);
    mesh
}

#[cfg(test)]
mod tests {
    // Index arithmetic in test fixtures, bounded by the loops that build them.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;

    /// Oracle: an independent Python transcription of the flattened expression
    /// (`v = x*y*0x0CCAC033 + y*0x6C1AC587 - x*0x421BE3BD - 0x519B8F25`), evaluated with
    /// arbitrary-precision integers and masked to 32 bits. The values below were produced by
    /// running that transcription, not by reading this implementation.
    #[test]
    fn split_hash_matches_an_independent_transcription() {
        // (gx, gy, hash, SWtoNEcut)
        const CASES: &[(i32, i32, u32, bool)] = &[
            (0, 0, 0xAE64_70DB, true),
            (1, 0, 0x6C48_8D1E, false),
            (0, 1, 0x1A7F_3662, false),
            (1, 1, 0xE52E_12D8, true),
            (1352, 1440, 0xC15B_D913, true),
            (1353, 1440, 0x73B9_1436, false),
            (1352, 1441, 0xBC3D_ABF2, true),
            (2039, 2039, 0x3846_F0E4, false),
            (100, 200, 0xB051_5CDF, true),
            (7, 7, 0x472B_6724, false),
        ];
        for &(x, y, h, cut) in CASES {
            assert_eq!(
                h >> 31 != 0,
                cut,
                "the case table itself is inconsistent at ({x}, {y})"
            );
            assert_eq!(sw_to_ne_cut(x, y), cut, "({x}, {y})");
        }
    }

    /// Oracle: the hash's two spellings. The nested form is the retail
    /// client's; the flattened form is its algebraic expansion. They must agree everywhere,
    /// over every global cell coordinate the world has (255 blocks x 8 cells = 2040 per axis).
    ///
    /// The true-count is the Python transcription's, run over the same grid.
    #[test]
    fn nested_and_flattened_forms_agree_over_the_whole_world() {
        let mut trues = 0u32;
        for x in 0..2040i32 {
            for y in 0..2040i32 {
                let (xu, yu) = (x as u32, y as u32);
                let flat = xu
                    .wrapping_mul(yu)
                    .wrapping_mul(0x0CCA_C033)
                    .wrapping_add(yu.wrapping_mul(0x6C1A_C587))
                    .wrapping_sub(xu.wrapping_mul(0x421B_E3BD))
                    .wrapping_sub(0x519B_8F25);
                let cut = sw_to_ne_cut(x, y);
                assert_eq!(cut, flat >> 31 != 0, "({x}, {y})");
                trues += u32::from(cut);
            }
        }
        assert_eq!(
            trues, 2_081_396,
            "split ratio over the 2040x2040 world grid"
        );
    }

    /// Oracle: the client's own mechanism-2 continuity rule — the hash is over *global* coordinates,
    /// so the diagonal a cell gets cannot depend on which landblock loaded it. If this ever fails
    /// the hash has picked up a block-local term and every landblock seam will crack.
    #[test]
    fn split_is_a_function_of_global_coordinates_only() {
        for bx in [0i32, 1, 0x7E, 0xA9] {
            for by in [0i32, 1, 0x7E, 0xB4] {
                for i in 0..8 {
                    for j in 0..8 {
                        let (gx, gy) = (bx * 8 + i, by * 8 + j);
                        assert_eq!(sw_to_ne_cut(gx, gy), sw_to_ne_cut(gx, gy));
                    }
                }
            }
        }
        // A cell on the east edge of block (10, 10) and the same global cell reached as the west
        // edge of block (11, 10) are the same cell and must get the same diagonal.
        assert_eq!(
            sw_to_ne_cut(10 * 8 + 8, 10 * 8 + 3),
            sw_to_ne_cut(11 * 8, 10 * 8 + 3)
        );
    }

    /// Oracle: the recovered diagonal-split triangle table and per-cell rotation UV table, read as
    /// data. Both must use the *same* corner order
    /// per triangle, which is the property that breaks if either table is transcribed out of step.
    #[test]
    fn triangle_and_uv_tables_use_the_same_corner_order() {
        let svc = 9;
        // Corner index within a cell, in the order (SW, SE, NE, NW).
        let corners = |i: usize, j: usize| {
            [
                (i * svc + j) as u32,
                ((i + 1) * svc + j) as u32,
                ((i + 1) * svc + j + 1) as u32,
                (i * svc + j + 1) as u32,
            ]
        };
        for cut in [true, false] {
            let tris = cell_triangles(svc, 3, 4, cut);
            let uvs = cell_uv_indices(cut, Rotation::Rot0);
            let c = corners(3, 4);
            let corner_of = |v: u32| c.iter().position(|&x| x == v).expect("vertex is a corner");
            // At ROT_0 the corner tables are the identity (SW_Corner[0] = 0, SE = 1, NE = 2,
            // NW = 3), so the uv index *is* the corner index.
            for t in 0..2 {
                for k in 0..3 {
                    assert_eq!(
                        corner_of(tris[t][k]) as u8,
                        uvs[t][k],
                        "cut={cut} tri={t} vertex={k}"
                    );
                }
            }
        }
    }

    /// Oracle: the four corner tables, as tabulated.
    /// Each rotation must be a permutation of 0..3, and the four tables must be cyclic shifts of
    /// each other — that is what makes one cached texture serve four orientations.
    #[test]
    fn corner_tables_are_cyclic_permutations() {
        for r in 0..4 {
            let mut seen = [false; 4];
            for t in [SW_CORNER, SE_CORNER, NE_CORNER, NW_CORNER] {
                seen[t[r] as usize] = true;
            }
            assert!(seen.iter().all(|&b| b), "rotation {r} is not a permutation");
        }
        for r in 0..4 {
            assert_eq!(SE_CORNER[r], (SW_CORNER[r] + 1) % 4);
            assert_eq!(NE_CORNER[r], (SW_CORNER[r] + 2) % 4);
            assert_eq!(NW_CORNER[r], (SW_CORNER[r] + 3) % 4);
        }
    }

    /// Oracle: the height-table setter's short-circuit. Entry
    /// *k* out of range leaves *k*..255 at their previous values.
    #[test]
    fn height_table_validation_short_circuits_rather_than_rejecting() {
        let previous = [7.0f32; LAND_HEIGHT_TABLE_LEN];
        let mut candidate = vec![0.0f32; LAND_HEIGHT_TABLE_LEN];
        for (i, v) in candidate.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            {
                *v = i as f32;
            }
        }
        candidate[10] = 900.0; // out of [0, 800]
        let out = set_height_table(&candidate, &previous);
        assert_eq!(out[9], 9.0, "entries before the bad one are copied");
        assert_eq!(
            out[10], 7.0,
            "the bad entry and everything after it keeps the old value"
        );
        assert_eq!(out[255], 7.0);
    }

    /// A deterministic height byte for global vertex `(x, y)`: the tests' landscape, the same
    /// whichever block reads it.
    fn global_height(x: i32, y: i32) -> u8 {
        let v = (x as u32)
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add((y as u32).wrapping_mul(0x85EB_CA6B))
            .rotate_left(13)
            .wrapping_mul(0xC2B2_AE35);
        (v >> 24) as u8
    }

    /// A height table that is not the identity, so a wrong index shows.
    fn ramp_table() -> [f32; LAND_HEIGHT_TABLE_LEN] {
        let mut t = [0.0f32; LAND_HEIGHT_TABLE_LEN];
        for (i, v) in t.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            {
                *v = (i * i) as f32 * 0.03 + i as f32 * 0.5;
            }
        }
        t
    }

    /// The heights of block `(bx, by)` on `grid`, read from the global landscape.
    fn block_heights(grid: &LandGrid, bx: i32, by: i32) -> Vec<u8> {
        let n = grid.vertices_per_side();
        let per = grid.squares_per_side() as i32;
        let mut h = Vec::with_capacity(n * n);
        for x in 0..n {
            for y in 0..n {
                h.push(global_height(bx * per + x as i32, by * per + y as i32));
            }
        }
        h
    }

    /// The default spec builds what the default world always built: an independent
    /// transcription of the nine-by-nine rules (`x = i * 192 / n`, the height at
    /// `i*step*9 + j*step`, the split hash of the block's cell coordinates, the triangle table)
    /// agrees with the shared core vertex for vertex and triangle for triangle, at every detail.
    ///
    /// Behaviour: world.terrain.the-default-terrain-spec-builds-the-default-landblock
    #[test]
    fn the_default_terrain_spec_builds_the_default_landblock() {
        let table = ramp_table();
        for (bx, by) in [(0, 0), (0x12, 0x5A), (0xA9, 0xB4), (0xFE, 0xFE)] {
            let height = block_heights(&LandGrid::DEFAULT, bx, by);
            for side in [8usize, 4, 2, 1] {
                let g = land_geometry(&TerrainSpec::DEFAULT, &height, &table, bx, by, side, None);
                let scale = 192.0 / side as f32;
                let step = 8 / side;
                let svc = side + 1;
                assert_eq!(g.vertices_per_side, svc);
                assert_eq!(g.squares_per_side, side);
                for i in 0..svc {
                    for j in 0..svc {
                        let want = Vec3::new(
                            i as f32 * scale,
                            j as f32 * scale,
                            table[height[i * step * 9 + j * step] as usize],
                        );
                        assert_eq!(
                            g.vertices[i * svc + j],
                            want,
                            "({bx},{by}) {side} ({i},{j})"
                        );
                    }
                }
                for i in 0..side {
                    for j in 0..side {
                        let n = i * side + j;
                        let cut = sw_to_ne_cut(bx * 8 + i as i32, by * 8 + j as i32);
                        assert_eq!(g.sw_to_ne_cut[n], cut);
                        let tris = cell_triangles(svc, i, j, cut);
                        for (k, tri) in tris.into_iter().enumerate() {
                            let [a, b, c] = tri;
                            assert_eq!(g.triangles[2 * n + k].v, tri);
                            assert_eq!(
                                g.triangles[2 * n + k].plane,
                                make_plane(
                                    g.vertices[a as usize],
                                    g.vertices[b as usize],
                                    g.vertices[c as usize]
                                )
                            );
                        }
                    }
                }
                assert!(g.holes.iter().all(|&h| !h));
            }
        }
    }

    /// Two-step grid used by the tests below: 160-unit blocks of 20-unit cells, a vertex every 10.
    const TWO_STEP: LandGrid = LandGrid {
        cell_length: 20.0,
        cells_per_side: 8,
        vertices_per_cell: 2,
    };

    /// A split that depends only on its global square coordinates and is easy to predict.
    fn checker_split(x: i32, y: i32) -> bool {
        (x + 3 * y).rem_euclid(5) < 2
    }

    /// On a grid of another shape, a block and its east and north neighbors agree on every vertex
    /// of the edge they share: the same position in the world and the same height, so the
    /// terrain has no crack. Every square is split as the spec's function of its global square
    /// coordinates says, and every plane faces up.
    ///
    /// Behaviour: world.terrain.neighbouring-blocks-share-their-edge-on-any-grid
    #[test]
    fn neighbouring_blocks_share_their_edge_on_a_two_step_grid() {
        let spec = TerrainSpec {
            grid: TWO_STEP,
            split: checker_split,
            height_max: 1800.0,
        };
        let table = ramp_table();
        let n = TWO_STEP.vertices_per_side();
        let len = TWO_STEP.block_length();
        let build = |bx: i32, by: i32| {
            let h = block_heights(&TWO_STEP, bx, by);
            land_geometry(&spec, &h, &table, bx, by, 8, None)
        };
        let (a, east, north) = (build(40, 60), build(41, 60), build(40, 61));
        assert_eq!(a.vertices.len(), 289);
        assert_eq!(a.triangles.len(), 2 * 16 * 16);
        for k in 0..n {
            let (e_a, e_b) = (a.vertices[(n - 1) * n + k], east.vertices[k]);
            assert_eq!(e_a.x, e_b.x + len, "east edge vertex {k}");
            assert_eq!((e_a.y, e_a.z), (e_b.y, e_b.z), "east edge vertex {k}");
            let (n_a, n_b) = (a.vertices[k * n + n - 1], north.vertices[k * n]);
            assert_eq!(n_a.y, n_b.y + len, "north edge vertex {k}");
            assert_eq!((n_a.x, n_a.z), (n_b.x, n_b.z), "north edge vertex {k}");
        }
        for i in 0..16 {
            for j in 0..16 {
                assert_eq!(
                    a.sw_to_ne_cut[i * 16 + j],
                    checker_split(40 * 16 + i as i32, 60 * 16 + j as i32)
                );
                assert_eq!(a.vertices[i * n + j].x, i as f32 * 10.0);
            }
        }
        assert!(a.triangles.iter().all(|t| t.plane.normal.z > 0.0));
    }

    /// A vertex marked as a hole opens every square it is a corner of, and only those.
    ///
    /// Behaviour: world.terrain.a-marked-vertex-opens-a-hole-in-the-squares-it-touches
    #[test]
    fn a_marked_vertex_opens_a_hole_in_the_squares_it_touches() {
        let spec = TerrainSpec {
            grid: TWO_STEP,
            split: checker_split,
            height_max: 1800.0,
        };
        let h = block_heights(&TWO_STEP, 3, 4);
        let mut marks = vec![false; TWO_STEP.vertex_count()];
        marks[TWO_STEP.vertex_index(4, 6)] = true;
        let g = land_geometry(&spec, &h, &ramp_table(), 3, 4, 8, Some(&marks));
        let holes: Vec<(usize, usize)> = (0..16)
            .flat_map(|i| (0..16).map(move |j| (i, j)))
            .filter(|&(i, j)| g.holes[i * 16 + j])
            .collect();
        assert_eq!(holes, vec![(3, 5), (3, 6), (4, 5), (4, 6)]);
        let none = land_geometry(&spec, &h, &ramp_table(), 3, 4, 8, None);
        assert!(none.holes.iter().all(|&h| !h));
        assert_eq!(none.triangles, g.triangles, "a hole changes no geometry");
    }

    /// At reduced detail a two-step grid keeps its two steps per cell and samples the full grid
    /// every `cells_per_side / side_cell_count` vertices: half the cells, a vertex every 20 units.
    ///
    /// Behaviour: world.terrain.a-coarser-detail-samples-the-finer-grid
    #[test]
    fn a_coarser_detail_samples_the_finer_grid() {
        let spec = TerrainSpec {
            grid: TWO_STEP,
            split: checker_split,
            height_max: 1800.0,
        };
        let h = block_heights(&TWO_STEP, 9, 2);
        let table = ramp_table();
        let g = land_geometry(&spec, &h, &table, 9, 2, 4, None);
        assert_eq!(g.squares_per_side, 8);
        assert_eq!(g.vertices_per_side, 9);
        for i in 0..9 {
            for j in 0..9 {
                let v = g.vertices[i * 9 + j];
                assert_eq!((v.x, v.y), (i as f32 * 20.0, j as f32 * 20.0));
                assert_eq!(v.z, table[h[TWO_STEP.vertex_index(2 * i, 2 * j)] as usize]);
            }
        }
    }

    /// The height table's upper bound is a setting: a table bounded at 1800 accepts 1500 and stops
    /// at the first entry above 1800, keeping the previous values from there on, exactly as the
    /// default bound of 800 does.
    ///
    /// Behaviour: world.terrain.the-height-table-bound-is-a-setting
    #[test]
    fn the_height_table_bound_is_a_setting() {
        let previous = [7.0f32; LAND_HEIGHT_TABLE_LEN];
        let mut candidate = vec![0.0f32; LAND_HEIGHT_TABLE_LEN];
        candidate[3] = 1500.0;
        candidate[4] = 1900.0;
        let out = set_height_table_within(&candidate, &previous, 1800.0);
        assert_eq!(out[3], 1500.0);
        assert_eq!(out[4], 7.0);
        assert_eq!(out[255], 7.0);
        let default = set_height_table(&candidate, &previous);
        assert_eq!(default[2], 0.0);
        assert_eq!(default[3], 7.0, "the default bound is 800");
        assert_eq!(
            set_height_table(&candidate, &previous),
            set_height_table_within(&candidate, &previous, TerrainSpec::DEFAULT.height_max)
        );
    }

    /// Oracle: `make_plane`'s winding contract: with +x east and +y north this winding is
    /// counter-clockwise seen from above, giving `N.z > 0`. Terrain that
    /// faces downwards is terrain the back-face test throws away.
    #[test]
    fn every_terrain_plane_faces_upwards_for_both_split_directions() {
        let svc = 9;
        let mut verts = Vec::new();
        for i in 0..svc {
            for j in 0..svc {
                #[allow(clippy::cast_precision_loss)]
                verts.push(Vec3::new(
                    i as f32 * 24.0,
                    j as f32 * 24.0,
                    ((i * 7 + j * 3) % 11) as f32,
                ));
            }
        }
        for cut in [true, false] {
            for [a, b, c] in cell_triangles(svc, 2, 5, cut) {
                let p = make_plane(verts[a as usize], verts[b as usize], verts[c as usize]);
                assert!(p.normal.z > 0.0, "cut={cut} normal={:?}", p.normal);
            }
        }
    }
}
