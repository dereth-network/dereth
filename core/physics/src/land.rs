//! The outdoor collision surface built from landblock geometry.
//!
//! Transcribed against the client's own landblock-structure and landblock code.
//!
//! Outdoor collision is entirely triangle-based: each 24 m land cell carries exactly two terrain
//! triangles built from a 9x9 grid of height samples. The same triangles are used by the
//! renderer, so the diagonal split rule must match exactly or the visible ground and the walked
//! ground disagree — see the split-hash note on [`split_hash`].
//!
//! Degenerate blocks (`side_cell_count != 8`) are excluded from physics entirely:
//! Landblock-mesh lookup returns NULL for them and forces
//! `NOT_WATER`. [`LandblockCollision::build`] refuses them for the same reason.
//!
//! the LOD seam vertex
//! rewrite between a reduced-detail block and its full-detail neighbours is **not implemented**.
//! The rule was never read out of the client, and it only ever runs on reduced-detail blocks,
//! which physics never sees: landblock-mesh lookup returns NULL for them and
//! [`LandblockCollision::build`] refuses them. It is primarily a *rendering* problem, because it
//! changes distant silhouettes.

use dereth_primitives::land_grid::LandGrid;
use dereth_primitives::num::floor_to_i32;
use dereth_primitives::{CellId, LandblockId, Vec3};

use crate::geom::polygon::{PolySide, Polygon};
use crate::globals::{
    BLOCK_LENGTH, HALF_SQUARE_LENGTH, LAND_HEIGHT_TABLE_LEN, MAX_OBJECT_HEIGHT,
    WATER_DEPTH_ENTIRELY, WATER_DEPTH_NONE, WATER_DEPTH_PARTIAL_SOLID_VERTEX,
    WATER_DEPTH_PARTIAL_WATER_VERTEX,
};
use crate::landdefs;
use crate::PhysicsError;

/// Nine vertices to a side at full detail, indexed `x * 9 + y` — **x major**.
/// Transposing it mirrors the world, and the mirror is symmetric enough to look plausible.
pub const SIDE_VERTEX_COUNT: usize = 9;
/// `9 * 9`.
pub const VERTEX_COUNT: usize = SIDE_VERTEX_COUNT * SIDE_VERTEX_COUNT;
/// `8 * 8` land cells to a block.
pub const SIDE_CELL_COUNT: usize = 8;
/// `64`.
pub const CELL_COUNT: usize = SIDE_CELL_COUNT * SIDE_CELL_COUNT;

/// `SURFCHAR` has exactly two values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfChar {
    Solid = 0,
    Water = 1,
}

/// `TERRAIN_SURF_CHAR`, the retail 32-entry table.
/// Terrain types 16-20 are water; everything else is solid.
///
/// Those five are ACE's `WaterRunning (0x10)`, `WaterStandingFresh (0x11)`,
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

/// Bits 2-6 of a terrain word: the terrain type, `0..=31`.
#[inline]
#[must_use]
pub const fn terrain_type(word: u16) -> usize {
    ((word >> 2) & 0x1F) as usize
}

/// The `SURFCHAR` of a terrain word.
#[inline]
#[must_use]
pub fn surf_char(word: u16) -> SurfChar {
    TERRAIN_SURF_CHAR[terrain_type(word)]
}

/// Whether a terrain word is wading water on a finer land grid: the shallow sea's two kinds,
/// `WaterShallowSea (0x12)` and `WaterShallowStillSea (0x13)`.
#[inline]
#[must_use]
pub const fn is_wading_water(word: u16) -> bool {
    matches!(terrain_type(word), 18 | 19)
}

/// Bits 11-15: the scene type index. Physics never reads it; scenery placement does.
#[inline]
#[must_use]
pub const fn scene_type(word: u16) -> u16 {
    word >> 11
}

/// The diagonal split for one land cell, from a hash of its **global** cell coordinates.
///
/// `true` means the shared edge runs **SW to NE**, matching `SWtoNEcut` in the client. Note that
/// ACE's `LandblockMesh.GetSplitDir` returns the inverse boolean under the opposite name, so a
/// naive port flips every triangle pair; and `ACE.Server/Physics/Common/LandblockStruct.cs`
/// computes a *different* expression that divides part of the sum by 2147483648 and is simply
/// wrong. See `docs/CORRECTIONS.md`.
///
/// The client converts the full 32-bit value to an unsigned float, multiplies by `2^-32` and
/// compares against `0.5`, which is exactly "is the sign bit set". Because the hash is over global
/// coordinates, the diagonal a cell gets does not depend on which landblock loaded it — one of
/// the things that keep block edges continuous.
#[inline]
#[must_use]
pub fn split_hash(global_cell_x: u32, global_cell_y: u32) -> bool {
    // Written in the original's nesting: (x * A + B) * y + x * C + D, all 32-bit wrapping.
    let h = global_cell_x
        .wrapping_mul(0x0CCA_C033)
        .wrapping_add(0x6C1A_C587)
        .wrapping_mul(global_cell_y)
        .wrapping_add(global_cell_x.wrapping_mul(0xBDE4_1C43))
        .wrapping_add(0xAE64_70DB);
    h >> 31 != 0
}

/// One landblock's collision geometry: the decoded record plus everything landblock generation
/// derives from it.
///
/// The block is `side_polygon_count²` squares, each cut into two triangles: one square per land
/// cell on the retail grid, `vertex_steps²` of them per cell on a finer one.
#[derive(Debug, Clone, PartialEq)]
pub struct LandblockCollision {
    pub id: LandblockId,
    /// 9 at full detail on the retail grid; `8 * vertex_steps + 1`.
    pub side_vertex_count: u32,
    /// 8; anything else is a degenerate block and is excluded from physics.
    pub side_cell_count: u32,
    /// Squares along a block side: `side_cell_count * vertex_steps`.
    pub side_polygon_count: u32,
    /// Vertex steps along a land cell side: 1 on the retail grid.
    pub vertex_steps: u32,
    /// The road's half width.
    pub road_width: f32,
    /// Height-table indices, `height[x * side_vertex_count + y]`.
    pub height: Box<[u8]>,
    /// Terrain words, same indexing.
    pub terrain: Box<[u16]>,
    /// A `LandblockInfo` record exists for this block (buildings / env cells).
    pub has_info: bool,

    /// `side_vertex_count^2` vertices, `x * side_vertex_count + y`.
    pub vertices: Vec<Vec3>,
    /// `SWtoNEcut`, `side_polygon_count^2` flags indexed `side_polygon_count * i + j`.
    pub split: Vec<bool>,
    /// `2 * side_polygon_count^2` triangles; square `(i, j)` owns `[2k]` and `[2k + 1]` where
    /// `k = side_polygon_count * i + j`.
    pub polygons: Vec<Polygon>,
    /// Per-cell water, `side_cell_count^2`, indexed `side_cell_count * i + j`.
    pub cell_water: Vec<WaterType>,
    /// The whole-block summary from the water calculation.
    pub water_type: WaterType,
    /// The landblock's height limit.
    pub max_zval: f32,
    /// As above.
    pub min_zval: f32,
}

impl LandblockCollision {
    /// Build the collision geometry for one full-detail landblock of the retail grid.
    ///
    /// Returns [`PhysicsError::DegenerateLandblock`] for `side_cell_count != 8`, which is the
    /// same answer the client's landblock-mesh lookup gives by returning NULL.
    pub fn build(
        id: LandblockId,
        height: Box<[u8; VERTEX_COUNT]>,
        terrain: Box<[u16; VERTEX_COUNT]>,
        has_info: bool,
        side_cell_count: u32,
        height_table: &[f32; LAND_HEIGHT_TABLE_LEN],
    ) -> Result<Self, PhysicsError> {
        if side_cell_count != 8 {
            return Err(PhysicsError::DegenerateLandblock(id));
        }
        Self::build_in(
            &LandGrid::DEFAULT,
            id,
            height,
            None,
            terrain,
            |_, _| None,
            has_info,
            height_table,
        )
    }

    /// Build the collision geometry for one full-detail landblock of a world laid on `grid`.
    ///
    /// `height` and `terrain` hold the grid's whole set of vertices, x major, and `fine` a finer
    /// grid's fine height bytes ([`dereth_primitives::land_grid::fine_height`]); without them each
    /// vertex stands at its height-table entry. `cut` gives square
    /// `(i, j)`'s stored cut, `true` when it runs south-west to north-east; where it gives none,
    /// the square's cut comes from [`split_hash`] over its global square coordinates, as on the
    /// retail grid.
    ///
    /// The block is at full detail, eight land cells a side.
    ///
    /// Returns [`PhysicsError::DegenerateLandblock`] for a grid whose cells are not the default
    /// grid's 24 m (the land-coordinate rules are those cells'), and for vertices that are not the
    /// grid's size.
    #[allow(clippy::too_many_arguments)]
    pub fn build_in(
        grid: &LandGrid,
        id: LandblockId,
        height: Box<[u8]>,
        fine: Option<&[u8]>,
        terrain: Box<[u16]>,
        cut: impl Fn(usize, usize) -> Option<bool>,
        has_info: bool,
        height_table: &[f32; LAND_HEIGHT_TABLE_LEN],
    ) -> Result<Self, PhysicsError> {
        let side_cell_count = 8_u32;
        if !grid.has_default_cells() {
            return Err(PhysicsError::DegenerateLandblock(id));
        }
        let steps = u32::try_from(grid.vertices_per_cell.max(1))
            .map_err(|_| PhysicsError::DegenerateLandblock(id))?;
        let side_polygon_count = side_cell_count * steps;
        let side_vertex_count = side_polygon_count + 1;
        let svc = side_vertex_count as usize;
        let spc = side_polygon_count as usize;
        if height.len() != svc * svc
            || terrain.len() != svc * svc
            || fine.is_some_and(|f| f.len() != svc * svc)
        {
            return Err(PhysicsError::DegenerateLandblock(id));
        }

        // scale = block_length / side_polygon_count: 24 on the retail grid.
        #[allow(clippy::cast_precision_loss)]
        let scale = BLOCK_LENGTH / side_polygon_count as f32;
        let mut vertices = Vec::with_capacity(svc * svc);
        for i in 0..svc {
            for j in 0..svc {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (i as f32 * scale, j as f32 * scale);
                let k = i * svc + j;
                let z = match fine {
                    Some(f) => {
                        dereth_primitives::land_grid::fine_height(height_table, height[k], f[k])
                    }
                    None => height_table[height[k] as usize],
                };
                vertices.push(Vec3::new(x, y, z));
            }
        }

        // The hash is over global square coordinates (cells, on the retail grid).
        let lbx = u32::from(id.x()) * side_polygon_count;
        let lby = u32::from(id.y()) * side_polygon_count;
        let mut cuts = Vec::with_capacity(spc * spc);
        let mut polygons = Vec::with_capacity(2 * spc * spc);
        for i in 0..spc {
            for j in 0..spc {
                #[allow(clippy::cast_possible_truncation)]
                let sw_to_ne =
                    cut(i, j).unwrap_or_else(|| split_hash(lbx + i as u32, lby + j as u32));
                cuts.push(sw_to_ne);
                let v00 = vertices[i * svc + j]; // SW
                let v01 = vertices[i * svc + j + 1]; // NW (+Y)
                let v10 = vertices[(i + 1) * svc + j]; // SE (+X)
                let v11 = vertices[(i + 1) * svc + j + 1]; // NE
                let (a, b) = if sw_to_ne {
                    (vec![v00, v10, v11], vec![v00, v11, v01])
                } else {
                    (vec![v00, v10, v01], vec![v11, v01, v10])
                };
                for verts in [a, b] {
                    // pos_surface is 0 when all three vertices have z == 0
                    // exactly, otherwise 1.
                    let flat = verts.iter().all(|v| v.z == 0.0);
                    let mut p = Polygon::new(verts);
                    p.pos_surface = u16::from(!flat);
                    polygons.push(p);
                }
            }
        }

        // The block water calculation over the per-cell one.
        let scc = side_cell_count as usize;
        let mut cell_water = Vec::with_capacity(scc * scc);
        let mut all_block_water = true;
        let mut any_block_water = false;
        for i in 0..scc {
            for j in 0..scc {
                let (any, all) = calc_cell_water_in(&terrain, steps as usize, i, j);
                let w = if any {
                    if all {
                        any_block_water = true;
                        WaterType::EntirelyWater
                    } else {
                        all_block_water = false;
                        any_block_water = true;
                        WaterType::PartiallyWater
                    }
                } else {
                    all_block_water = false;
                    WaterType::NotWater
                };
                cell_water.push(w);
            }
        }
        let water_type = if any_block_water {
            if all_block_water {
                WaterType::EntirelyWater
            } else {
                WaterType::PartiallyWater
            }
        } else {
            WaterType::NotWater
        };

        // The landblock's height limits.
        let max_h = height.iter().copied().max().unwrap_or(0);
        let min_h = height.iter().copied().min().unwrap_or(0);
        // A finer grid's limits over its vertices' fine heights.
        let (top, bottom) = match fine {
            Some(_) => vertices.iter().fold((f32::MIN, f32::MAX), |(hi, lo), v| {
                (hi.max(v.z), lo.min(v.z))
            }),
            None => (height_table[max_h as usize], height_table[min_h as usize]),
        };
        let max_zval = top + MAX_OBJECT_HEIGHT;
        let min_zval = bottom - 1.0;

        Ok(Self {
            id,
            side_vertex_count,
            side_cell_count,
            side_polygon_count,
            vertex_steps: steps,
            road_width: grid.road_width,
            height,
            terrain,
            has_info,
            vertices,
            split: cuts,
            polygons,
            cell_water,
            water_type,
            max_zval,
            min_zval,
        })
    }

    /// A square's side: the cell length on the retail grid.
    #[must_use]
    pub fn square_length(&self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let n = self.side_polygon_count as f32;
        BLOCK_LENGTH / n
    }

    /// The two terrain triangles of square `(sx, sy)` of the block.
    #[must_use]
    pub fn square_polygons(&self, sx: u32, sy: u32) -> Option<(&Polygon, &Polygon)> {
        if sx >= self.side_polygon_count || sy >= self.side_polygon_count {
            return None;
        }
        let k = (self.side_polygon_count * sx + sy) as usize;
        Some((&self.polygons[2 * k], &self.polygons[2 * k + 1]))
    }

    /// The two terrain triangles of one land cell's south-west square, by cell index
    /// `1 ..= 0x40`: the cell's only square on the retail grid.
    #[must_use]
    pub fn cell_polygons(&self, cell_index: u16) -> Option<(&Polygon, &Polygon)> {
        if cell_index == 0 || cell_index > 0x40 {
            return None;
        }
        let (cx, cy) = landdefs::cell_index_to_xy(cell_index);
        self.square_polygons(cx * self.vertex_steps, cy * self.vertex_steps)
    }

    /// The square of land cell `(cx, cy)` that a block-local point falls in, its coordinates
    /// clamped into the cell: the cell itself on the retail grid.
    fn square_in_cell(&self, cx: u32, cy: u32, local: Vec3) -> (u32, u32) {
        let s = self.vertex_steps;
        if s == 1 {
            return (cx, cy);
        }
        let len = self.square_length();
        let pick = |v: f32, c: u32| -> u32 {
            let lo = c * s;
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_wrap)]
            let at = floor_to_i32(v / len).clamp(lo as i32, (lo + s - 1) as i32) as u32;
            at
        };
        (pick(local.x, cx), pick(local.y, cy))
    }

    /// The first of the land cell's terrain triangles that contains the point in 2-D: the two of
    /// the square the point falls in, then those of the cell's other squares. On the retail grid
    /// every land cell has exactly two terrain polygons; several routines hard-code that bound.
    #[must_use]
    pub fn find_terrain_poly(&self, cell_index: u16, local: Vec3) -> Option<&Polygon> {
        if cell_index == 0 || cell_index > 0x40 {
            return None;
        }
        let (cx, cy) = landdefs::cell_index_to_xy(cell_index);
        let (sx, sy) = self.square_in_cell(cx, cy, local);
        let hit = |sx: u32, sy: u32| -> Option<&Polygon> {
            let (a, b) = self.square_polygons(sx, sy)?;
            if a.point_in_poly2d(local, PolySide::Positive) {
                Some(a)
            } else if b.point_in_poly2d(local, PolySide::Positive) {
                Some(b)
            } else {
                None
            }
        };
        if let Some(p) = hit(sx, sy) {
            return Some(p);
        }
        let s = self.vertex_steps;
        (0..s * s)
            .map(|k| (cx * s + k / s, cy * s + k % s))
            .filter(|&q| q != (sx, sy))
            .find_map(|(qx, qy)| hit(qx, qy))
    }

    /// The 5-bit terrain type of the vertex nearest the
    /// point.
    #[must_use]
    pub fn get_terrain(&self, cell: CellId, local: Vec3) -> Option<usize> {
        let (gx, gy) = landdefs::gid_to_lcoord(cell)?;
        // The original computes a positive modulo 8 with the `& 0x80000007` sign fix-up.
        let (cx, cy) = (gx.rem_euclid(8), gy.rem_euclid(8));
        Some(terrain_type(self.nearest_vertex_word(cx, cy, local)))
    }

    /// The shared "which of the square's four corner vertices is nearest" rule, used identically
    /// by the terrain lookup and the water-depth step. On the retail grid the square is the cell.
    fn nearest_vertex_word(&self, cx: i32, cy: i32, local: Vec3) -> u16 {
        #[allow(clippy::cast_sign_loss)]
        let (sx, sy) = self.square_in_cell(cx as u32, cy as u32, local);
        #[allow(clippy::cast_precision_loss)]
        let half = HALF_SQUARE_LENGTH / self.vertex_steps as f32;
        #[allow(clippy::cast_precision_loss)]
        let mid_x = (2 * sx + 1) as f32 * half;
        #[allow(clippy::cast_precision_loss)]
        let mid_y = (2 * sy + 1) as f32 * half;
        let (sx, sy) = (sx as usize, sy as usize);
        let svc = self.side_vertex_count as usize;
        let i = sy + sx * svc;
        if local.x <= mid_x {
            if local.y <= mid_y {
                self.terrain[i]
            } else {
                self.terrain[i + 1]
            }
        } else if local.y <= mid_y {
            self.terrain[sy + (sx + 1) * svc]
        } else {
            self.terrain[i + svc + 1]
        }
    }

    /// The depth inside a partially-water square,
    /// from the nearest of the square's four corner vertices.
    ///
    /// The client reads garbage indices when the cell id is not a valid outdoor id; this returns
    /// `0.0` instead, and every caller in the crate has already established the id.
    #[must_use]
    pub fn calc_water_depth(&self, cell: CellId, local: Vec3) -> f32 {
        if !landdefs::inbound_valid_cellid(cell) || !landdefs::is_outdoors(cell) {
            return WATER_DEPTH_NONE;
        }
        let (cx, cy) = landdefs::cell_index_to_xy(cell.index());
        #[allow(clippy::cast_possible_wrap)]
        let word = self.nearest_vertex_word(cx as i32, cy as i32, local);
        match surf_char(word) {
            SurfChar::Solid => WATER_DEPTH_PARTIAL_SOLID_VERTEX,
            SurfChar::Water => WATER_DEPTH_PARTIAL_WATER_VERTEX,
        }
    }

    /// The water depth at a block-local point of a land cell of this block, by the water of the
    /// square the point lies in ([`Self::water_type_at`]): the cell's own on the retail grid,
    /// whose square is the cell. The four depths are fixed at `0.9 / 0.45 / 0.1 / 0.0`.
    ///
    /// On a finer land grid a body stands in a square of water alone as at a shore, `0.45`, where
    /// the vertex nearest it is wading water ([`is_wading_water`]): such a grid's water says how
    /// deep it is vertex by vertex, where the retail grid's only says where the shore is.
    #[must_use]
    pub fn get_water_depth(&self, cell: CellId, local: Vec3) -> f32 {
        match self.water_type_at(cell.index(), local) {
            None | Some(WaterType::NotWater) => WATER_DEPTH_NONE,
            Some(WaterType::EntirelyWater) if self.vertex_steps > 1 => {
                let (cx, cy) = landdefs::cell_index_to_xy(cell.index());
                #[allow(clippy::cast_possible_wrap)]
                let word = self.nearest_vertex_word(cx as i32, cy as i32, local);
                if is_wading_water(word) {
                    WATER_DEPTH_PARTIAL_WATER_VERTEX
                } else {
                    WATER_DEPTH_ENTIRELY
                }
            }
            Some(WaterType::EntirelyWater) => WATER_DEPTH_ENTIRELY,
            Some(WaterType::PartiallyWater) => self.calc_water_depth(cell, local),
        }
    }

    /// The water type of one land cell by index `1 ..= 0x40`: a summary of its squares', water
    /// where any of them has a water corner and entirely water where all of them are.
    #[must_use]
    pub fn cell_water_type(&self, cell_index: u16) -> Option<WaterType> {
        if cell_index == 0 || cell_index > 0x40 {
            return None;
        }
        let (cx, cy) = landdefs::cell_index_to_xy(cell_index);
        Some(self.cell_water[(self.side_cell_count * cx + cy) as usize])
    }

    /// The water type of square `(sx, sy)` of the block, by its four corner vertices: on the
    /// retail grid the square is the land cell, and this is the cell's own.
    #[must_use]
    pub fn square_water_type(&self, sx: u32, sy: u32) -> Option<WaterType> {
        if sx >= self.side_polygon_count || sy >= self.side_polygon_count {
            return None;
        }
        let svc = self.side_vertex_count as usize;
        let (x, y) = (sx as usize, sy as usize);
        let (any, all) = dereth_primitives::land_grid::water_of(1, |a, b| {
            surf_char(self.terrain[(x + a) * svc + y + b]) == SurfChar::Water
        });
        Some(water_type_of(any, all))
    }

    /// The water type where block-local point `local` of land cell `1 ..= 0x40` lies: that of the
    /// cell's square it falls in ([`Self::square_water_type`]).
    #[must_use]
    pub fn water_type_at(&self, cell_index: u16, local: Vec3) -> Option<WaterType> {
        if cell_index == 0 || cell_index > 0x40 {
            return None;
        }
        let (cx, cy) = landdefs::cell_index_to_xy(cell_index);
        let (sx, sy) = self.square_in_cell(cx, cy, local);
        self.square_water_type(sx, sy)
    }

    /// The 16-case road-edge geometry ([`dereth_primitives::land_grid::on_road_square`]) over the
    /// square the point falls in, with the road's half
    /// width and its `square - width` mirror: 5 m and 19 m on the retail grid, whose square is
    /// the 24 m cell.
    ///
    /// **The `1100` case is `fx < 5` and the `1010` case is `fy < 5`**, easy to swap. The
    /// geometry says so too: `1100` is `r00 | r01`, i.e. both road vertices sit on the `x = cx`
    /// edge, so the road runs along Y at low X and the on-road band is `fx < 5`.
    ///
    /// Roads do not change collision — `on_road` is consulted only by scenery placement and by
    /// the renderer's road blend, so a road on a 50-degree slope is still not walkable.
    #[must_use]
    pub fn on_road(&self, local: Vec3) -> bool {
        dereth_primitives::land_grid::on_road(
            &self.terrain,
            self.side_vertex_count as usize,
            self.square_length(),
            self.road_width,
            local.x,
            local.y,
        )
    }
}

/// Look at the cell's four corner vertices.
/// Returns `(any_water, all_water)`.
#[must_use]
pub fn calc_cell_water(terrain: &[u16; VERTEX_COUNT], i: usize, j: usize) -> (bool, bool) {
    calc_cell_water_in(terrain, 1, i, j)
}

/// The water type `(any_water, all_water)` makes.
const fn water_type_of(any: bool, all: bool) -> WaterType {
    match (any, all) {
        (false, _) => WaterType::NotWater,
        (true, false) => WaterType::PartiallyWater,
        (true, true) => WaterType::EntirelyWater,
    }
}

/// Look at every vertex of land cell `(i, j)` on a grid of `vertex_steps` steps per cell: its
/// four corners on the retail grid. Returns `(any_water, all_water)`, which are also whether any
/// of the cell's squares has a water corner and whether all of them are wholly water.
#[must_use]
pub fn calc_cell_water_in(
    terrain: &[u16],
    vertex_steps: usize,
    i: usize,
    j: usize,
) -> (bool, bool) {
    let side = SIDE_CELL_COUNT * vertex_steps + 1;
    let (x, y) = (i * vertex_steps, j * vertex_steps);
    dereth_primitives::land_grid::water_of(vertex_steps, |a, b| {
        surf_char(terrain[(x + a) * side + y + b]) == SurfChar::Water
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the client's own landblock-structure and landblock code --
    // the vertex and polygon construction, the per-cell water, the block water, the water depth
    // and the terrain lookup,
    // on_road. The retail-data comparison lives in tests/retail_land.rs.

    /// **A finer grid's collision stands at its fine heights**: each vertex its table entry moved by
    /// its fine byte, and the block's limits over those heights.
    #[test]
    fn a_finer_grids_collision_stands_at_its_fine_heights() {
        let grid = LandGrid::with_steps(3, 4.0);
        let n = 25 * 25;
        let mut table = [0f32; LAND_HEIGHT_TABLE_LEN];
        for (i, v) in table.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            {
                *v = i as f32 * 2.0;
            }
        }
        let height: Box<[u8]> = vec![10u8; n].into();
        let mut fine = vec![0u8; n];
        fine[0] = 64; // half a step up: 21 m
        fine[1] = (-64i8).to_ne_bytes()[0]; // half a step down: 19 m
        let b = LandblockCollision::build_in(
            &grid,
            LandblockId::new(1, 1),
            height,
            Some(&fine),
            vec![0u16; n].into(),
            |_, _| None,
            false,
            &table,
        )
        .unwrap();
        assert_eq!(b.vertices[0].z, 21.0);
        assert_eq!(b.vertices[1].z, 19.0);
        assert_eq!(b.vertices[2].z, 20.0);
        assert_eq!(b.max_zval, 21.0 + MAX_OBJECT_HEIGHT);
        assert_eq!(b.min_zval, 19.0 - 1.0);
    }

    fn flat_table() -> [f32; LAND_HEIGHT_TABLE_LEN] {
        let mut t = [0.0_f32; LAND_HEIGHT_TABLE_LEN];
        for (i, v) in t.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            {
                *v = i as f32 * 2.0;
            }
        }
        t
    }

    fn block(height: [u8; VERTEX_COUNT], terrain: [u16; VERTEX_COUNT]) -> LandblockCollision {
        LandblockCollision::build(
            LandblockId::new(0xA9, 0xB4),
            Box::new(height),
            Box::new(terrain),
            false,
            8,
            &flat_table(),
        )
        .expect("full-detail block")
    }

    #[test]
    fn degenerate_blocks_are_refused() {
        for scc in [1_u32, 2, 4, 16] {
            let e = LandblockCollision::build(
                LandblockId::new(0, 0),
                Box::new([0; VERTEX_COUNT]),
                Box::new([0; VERTEX_COUNT]),
                false,
                scc,
                &flat_table(),
            );
            assert!(
                matches!(e, Err(PhysicsError::DegenerateLandblock(_))),
                "scc = {scc}"
            );
        }
    }

    #[test]
    fn vertices_are_x_major_at_24_metre_spacing_with_table_looked_up_z() {
        let mut h = [0_u8; VERTEX_COUNT];
        h[3 * 9 + 5] = 100; // vertex (x = 3, y = 5)
        let b = block(h, [0; VERTEX_COUNT]);
        assert_eq!(b.vertices.len(), 81);
        let v = b.vertices[3 * 9 + 5];
        assert_eq!(v.x, 72.0, "x = i * 24");
        assert_eq!(v.y, 120.0, "y = j * 24");
        assert_eq!(v.z, 200.0, "z is a table lookup, not the byte");
        // and a neighbour is untouched
        assert_eq!(b.vertices[3 * 9 + 6].z, 0.0);
        // corners
        assert_eq!(b.vertices[0], Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(b.vertices[80], Vec3::new(192.0, 192.0, 0.0));
    }

    /// The split hash is the single most important constant in this module. The value is checked
    /// against the expression the knowledge base and ACE's `LandblockMesh.cs:167` agree on,
    /// computed independently here in the flattened form.
    #[test]
    fn split_hash_matches_the_flattened_expression_over_the_whole_grid() {
        let mut trues = 0_u32;
        let mut n = 0_u32;
        let mut x = 0_u32;
        while x < 2040 {
            let mut y = 0_u32;
            while y < 2040 {
                let flat = x
                    .wrapping_mul(y)
                    .wrapping_mul(0x0CCA_C033)
                    .wrapping_add(y.wrapping_mul(0x6C1A_C587))
                    .wrapping_sub(x.wrapping_mul(0x421B_E3BD))
                    .wrapping_sub(0x519B_8F25);
                assert_eq!(split_hash(x, y), flat >> 31 != 0, "({x}, {y})");
                if split_hash(x, y) {
                    trues += 1;
                }
                n += 1;
                y += 7;
            }
            x += 7;
        }
        // A hash whose sign bit is stuck would be a silent catastrophe; assert both arms occur
        // at roughly even rates.
        assert!(n > 80_000, "{n} samples");
        let ratio = f64::from(trues) / f64::from(n);
        assert!(
            (0.45..0.55).contains(&ratio),
            "split ratio {ratio} looks biased"
        );
    }

    #[test]
    fn split_hash_is_a_function_of_global_coordinates_so_neighbouring_blocks_agree() {
        // Cell (7, 3) of block (10, 20) and cell (7, 3) of block (10, 20) reached from the block
        // to its east are the same global cell and must get the same diagonal.
        let global = (10 * 8 + 7, 20 * 8 + 3);
        assert_eq!(split_hash(global.0, global.1), split_hash(87, 163));
        // and the same local index in a different block generally does NOT agree, which is the
        // whole reason the hash is global
        assert_ne!(
            (0..64)
                .map(|k| split_hash(10 * 8 + k / 8, 20 * 8 + k % 8))
                .collect::<Vec<_>>(),
            (0..64)
                .map(|k| split_hash(11 * 8 + k / 8, 20 * 8 + k % 8))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn triangle_winding_follows_the_split_flag() {
        // A block whose four corners of cell (0,0) are at distinct heights, so the two
        // triangulations are visibly different.
        let mut h = [0_u8; VERTEX_COUNT];
        h[0] = 1; // (0,0)
        h[1] = 2; // (0,1) NW
        h[9] = 3; // (1,0) SE
        h[10] = 4; // (1,1) NE
        let b = block(h, [0; VERTEX_COUNT]);
        let sw_to_ne = b.split[0];
        let (p0, p1) = b.cell_polygons(1).expect("cell 1 exists");
        let v00 = b.vertices[0];
        let v01 = b.vertices[1];
        let v10 = b.vertices[9];
        let v11 = b.vertices[10];
        if sw_to_ne {
            assert_eq!(p0.vertices, vec![v00, v10, v11]);
            assert_eq!(p1.vertices, vec![v00, v11, v01]);
        } else {
            assert_eq!(p0.vertices, vec![v00, v10, v01]);
            assert_eq!(p1.vertices, vec![v11, v01, v10]);
        }
        // Whichever way it went, both triangles must have an upward normal.
        assert!(p0.plane.normal.z > 0.0, "{:?}", p0.plane.normal);
        assert!(p1.plane.normal.z > 0.0, "{:?}", p1.plane.normal);
    }

    #[test]
    fn every_point_in_a_cell_lands_in_exactly_one_of_its_two_triangles() {
        let mut h = [0_u8; VERTEX_COUNT];
        for (k, v) in h.iter_mut().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            {
                *v = (k % 17) as u8;
            }
        }
        let b = block(h, [0; VERTEX_COUNT]);
        let mut found = 0;
        for cx in 0..8_u32 {
            for cy in 0..8_u32 {
                let index = u16::try_from(cx * 8 + cy + 1).unwrap();
                for a in 1..24_u32 {
                    for c in 1..24_u32 {
                        #[allow(clippy::cast_precision_loss)]
                        let p = Vec3::new(
                            cx as f32 * 24.0 + a as f32,
                            cy as f32 * 24.0 + c as f32,
                            0.0,
                        );
                        assert!(
                            b.find_terrain_poly(index, p).is_some(),
                            "cell {index} point {p:?} fell through both triangles"
                        );
                        found += 1;
                    }
                }
            }
        }
        assert_eq!(found, 64 * 23 * 23);
    }

    #[test]
    fn terrain_surf_char_marks_exactly_types_16_to_20_as_water() {
        for (i, s) in TERRAIN_SURF_CHAR.iter().enumerate() {
            let expect = (16..=20).contains(&i);
            assert_eq!(*s == SurfChar::Water, expect, "terrain type {i}");
        }
    }

    #[test]
    fn cell_water_comes_from_the_four_corner_vertices() {
        let water = 16_u16 << 2; // terrain type 16 = WaterRunning
        let mut t = [0_u16; VERTEX_COUNT];
        // Make cell (0,0) entirely water: vertices (0,0) (0,1) (1,0) (1,1).
        for (a, bb) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            t[a * 9 + bb] = water;
        }
        // Make cell (2,2) partially water: one corner only.
        t[2 * 9 + 2] = water;
        let b = block([0; VERTEX_COUNT], t);
        assert_eq!(b.cell_water[0], WaterType::EntirelyWater);
        assert_eq!(b.cell_water[8 * 2 + 2], WaterType::PartiallyWater);
        assert_eq!(b.cell_water[8 * 5 + 5], WaterType::NotWater);
        // A block with any water and not all water is PARTIALLY_WATER.
        assert_eq!(b.water_type, WaterType::PartiallyWater);
    }

    #[test]
    fn an_all_water_block_is_entirely_water_and_a_dry_one_is_not_water() {
        let water = 18_u16 << 2; // WaterShallowSea
        assert_eq!(
            block([0; VERTEX_COUNT], [water; VERTEX_COUNT]).water_type,
            WaterType::EntirelyWater
        );
        assert_eq!(
            block([0; VERTEX_COUNT], [0; VERTEX_COUNT]).water_type,
            WaterType::NotWater
        );
    }

    #[test]
    fn water_depths_are_the_four_fixed_values() {
        let water = 16_u16 << 2;
        let mut t = [0_u16; VERTEX_COUNT];
        for (a, bb) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            t[a * 9 + bb] = water;
        }
        // cell (2,2) partially: only its SW corner is water
        t[2 * 9 + 2] = water;
        let b = block([0; VERTEX_COUNT], t);
        let lb = LandblockId::new(0xA9, 0xB4);

        // entirely water cell -> 0.9
        assert_eq!(
            b.get_water_depth(lb.cell(1), Vec3::new(12.0, 12.0, 0.0)),
            0.9
        );
        // dry cell -> 0.0
        assert_eq!(
            b.get_water_depth(lb.cell(8 * 5 + 5 + 1), Vec3::new(0.0, 0.0, 0.0)),
            0.0
        );
        // partially-water cell (2,2) spans x in [48,72), y in [48,72); the nearest vertex to
        // (50, 50) is the water one at (2,2) -> 0.45
        let idx = u16::try_from(2 * 8 + 2 + 1).unwrap();
        assert_eq!(
            b.get_water_depth(lb.cell(idx), Vec3::new(50.0, 50.0, 0.0)),
            0.45
        );
        // the nearest vertex to (70, 70) is the solid one at (3,3) -> 0.1
        assert_eq!(
            b.get_water_depth(lb.cell(idx), Vec3::new(70.0, 70.0, 0.0)),
            0.1
        );
    }

    #[test]
    fn get_terrain_picks_the_nearest_vertex() {
        let mut t = [0_u16; VERTEX_COUNT];
        t[9 + 1] = 7 << 2;
        t[2 * 9 + 2] = 9 << 2;
        let b = block([0; VERTEX_COUNT], t);
        let lb = LandblockId::new(0xA9, 0xB4);
        let cell = lb.cell(8 + 1 + 1); // cell (1,1): x, y in [24, 48)
        assert_eq!(b.get_terrain(cell, Vec3::new(25.0, 25.0, 0.0)), Some(7));
        assert_eq!(b.get_terrain(cell, Vec3::new(47.0, 47.0, 0.0)), Some(9));
    }

    /// The 16-case road table, one assertion per case, with the geometry each case describes.
    /// Cases `1100` and `1010` are the two the knowledge base had swapped.
    #[test]
    fn on_road_reproduces_all_sixteen_cases() {
        let r = 1_u16; // road bits
        type RoadCase = ((bool, bool, bool, bool), &'static [(f32, f32, bool)]);
        let cases: &[RoadCase] = &[
            (
                (true, true, true, true),
                &[(1.0, 1.0, true), (12.0, 12.0, true), (23.0, 23.0, true)],
            ),
            (
                (true, true, true, false),
                &[(1.0, 20.0, true), (20.0, 1.0, true), (20.0, 20.0, false)],
            ),
            (
                (true, true, false, true),
                &[(1.0, 1.0, true), (20.0, 23.0, true), (20.0, 1.0, false)],
            ),
            // r00 | r01: both on the x = cx edge, so the band is fx < 5, NOT fy < 5.
            (
                (true, true, false, false),
                &[(1.0, 12.0, true), (12.0, 1.0, false), (23.0, 12.0, false)],
            ),
            (
                (true, false, true, true),
                &[(23.0, 12.0, true), (12.0, 1.0, true), (12.0, 12.0, false)],
            ),
            // r00 | r10: both on the y = cy edge, so the band is fy < 5, NOT fx < 5.
            (
                (true, false, true, false),
                &[(12.0, 1.0, true), (1.0, 12.0, false), (12.0, 23.0, false)],
            ),
            (
                (true, false, false, true),
                &[(12.0, 12.0, true), (1.0, 20.0, false)],
            ),
            (
                (true, false, false, false),
                &[(1.0, 1.0, true), (12.0, 12.0, false)],
            ),
            (
                (false, true, true, true),
                &[(23.0, 12.0, true), (12.0, 23.0, true), (12.0, 12.0, false)],
            ),
            (
                (false, true, true, false),
                &[(12.0, 12.0, true), (1.0, 1.0, false)],
            ),
            (
                (false, true, false, true),
                &[(12.0, 23.0, true), (12.0, 1.0, false)],
            ),
            (
                (false, true, false, false),
                &[(1.0, 23.0, true), (12.0, 12.0, false)],
            ),
            (
                (false, false, true, true),
                &[(23.0, 12.0, true), (1.0, 12.0, false)],
            ),
            (
                (false, false, true, false),
                &[(23.0, 1.0, true), (12.0, 12.0, false)],
            ),
            (
                (false, false, false, true),
                &[(23.0, 23.0, true), (12.0, 12.0, false)],
            ),
            (
                (false, false, false, false),
                &[(12.0, 12.0, false), (1.0, 1.0, false)],
            ),
        ];
        for &((r00, r01, r10, r11), points) in cases {
            let mut t = [0_u16; VERTEX_COUNT];
            // cell (0, 0): vertices (0,0) (0,1) (1,0) (1,1) at indices 0, 1, 9, 10.
            if r00 {
                t[0] = r;
            }
            if r01 {
                t[1] = r;
            }
            if r10 {
                t[9] = r;
            }
            if r11 {
                t[10] = r;
            }
            let b = block([0; VERTEX_COUNT], t);
            for &(x, y, expect) in points {
                assert_eq!(
                    b.on_road(Vec3::new(x, y, 0.0)),
                    expect,
                    "case {r00}{r01}{r10}{r11} at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn land_limits_bracket_the_height_table_entries_in_use() {
        let mut h = [10_u8; VERTEX_COUNT];
        h[40] = 200;
        h[41] = 1;
        let b = block(h, [0; VERTEX_COUNT]);
        assert_eq!(b.max_zval, 400.0 + 200.0);
        assert_eq!(b.min_zval, 2.0 - 1.0);
    }

    #[test]
    fn pos_surface_is_zero_only_for_a_wholly_flat_zero_triangle() {
        let mut h = [0_u8; VERTEX_COUNT];
        let flat = block(h, [0; VERTEX_COUNT]);
        assert!(flat.polygons.iter().all(|p| p.pos_surface == 0));
        h[0] = 1;
        let bumpy = block(h, [0; VERTEX_COUNT]);
        assert!(bumpy.polygons.iter().any(|p| p.pos_surface == 1));
    }

    // A world whose land cell sides are cut into two vertex steps: 17 x 17 vertices 12 m apart,
    // 16 x 16 squares, the cells still 24 m.

    use crate::geom::plane::PlaneExt;

    fn two() -> LandGrid {
        LandGrid::with_steps(2, 4.0)
    }
    const DENSE: usize = 17 * 17;

    fn dense(
        height: &[u8],
        terrain: &[u16],
        cut: impl Fn(usize, usize) -> Option<bool>,
    ) -> LandblockCollision {
        LandblockCollision::build_in(
            &two(),
            LandblockId::new(0x5B, 0x79),
            height.into(),
            None,
            terrain.into(),
            cut,
            false,
            &flat_table(),
        )
        .expect("two-step block")
    }

    /// The point at block-local `(x, y)`, its land cell index and the ground height there.
    fn ground(b: &LandblockCollision, x: f32, y: f32) -> Option<f32> {
        let p = Vec3::new(x, y, 0.0);
        let index = 1 + (floor_to_i32(x / 24.0) * 8 + floor_to_i32(y / 24.0));
        let poly = b.find_terrain_poly(u16::try_from(index).ok()?, p)?;
        let mut q = p;
        poly.plane.set_height(&mut q).then_some(q.z)
    }

    #[test]
    fn the_retail_form_builds_the_same_block_as_the_nine_by_nine_builder() {
        let mut h = [0_u8; VERTEX_COUNT];
        let mut t = [0_u16; VERTEX_COUNT];
        for i in 0..VERTEX_COUNT {
            h[i] = u8::try_from((i * 37) % 251).unwrap();
            t[i] = u16::try_from((i * 0x0105) % 0xFFFF).unwrap();
        }
        let a = block(h, t);
        let b = LandblockCollision::build_in(
            &LandGrid::with_steps(1, 9.0),
            LandblockId::new(0xA9, 0xB4),
            Box::new(h),
            None,
            Box::new(t),
            |_, _| None,
            false,
            &flat_table(),
        )
        .unwrap();
        assert_eq!(a, b);
        assert_eq!(b.road_width, 5.0, "one step is the retail grid's road too");
    }

    #[test]
    fn a_two_step_block_has_seventeen_vertices_a_side_twelve_metres_apart() {
        let mut h = vec![0_u8; DENSE];
        h[3 * 17 + 5] = 100;
        let b = dense(&h, &[0; DENSE], |_, _| None);
        assert_eq!((b.vertices.len(), b.polygons.len()), (289, 512));
        let v = b.vertices[3 * 17 + 5];
        assert_eq!((v.x, v.y, v.z), (36.0, 60.0, 200.0));
        assert_eq!(b.square_length(), 12.0);
    }

    #[test]
    fn a_vertex_inside_a_land_cell_shapes_the_ground_there() {
        // The middle vertex of cell (2, 3) raised: on the retail grid there is no vertex there.
        let mut h = vec![0_u8; DENSE];
        h[5 * 17 + 7] = 50;
        let b = dense(&h, &[0; DENSE], |_, _| None);
        let top = ground(&b, 60.0, 84.0).unwrap();
        assert!((top - 100.0).abs() < 1e-3, "on the raised vertex: {top}");
        let corner = ground(&b, 48.0, 72.0).unwrap();
        assert!(corner.abs() < 1e-3, "the cell's corner stays low: {corner}");
        let mid = ground(&b, 57.0, 81.0).unwrap();
        assert!(mid > 0.0 && mid < 100.0, "half way up: {mid}");
    }

    #[test]
    fn every_point_of_a_two_step_cell_lands_on_a_triangle_of_its_own_square() {
        let mut h = vec![0_u8; DENSE];
        for (i, v) in h.iter_mut().enumerate() {
            *v = u8::try_from((i * 13) % 97).unwrap();
        }
        let b = dense(&h, &[0; DENSE], |_, _| None);
        for k in 0..48 {
            for m in 0..48 {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (24.0 + k as f32 * 0.5 + 0.25, 48.0 + m as f32 * 0.5 + 0.25);
                let p = b
                    .find_terrain_poly(1 + 8 + 2, Vec3::new(x, y, 0.0))
                    .unwrap();
                let (sx, sy) = (floor_to_i32(x / 12.0), floor_to_i32(y / 12.0));
                let (a, c) = b.square_polygons(sx as u32, sy as u32).unwrap();
                assert!(std::ptr::eq(p, a) || std::ptr::eq(p, c), "({x}, {y})");
            }
        }
    }

    #[test]
    fn a_stored_split_bit_cuts_its_square_south_west_to_north_east() {
        let k = 3 * 16 + 9; // square (3, 9)
        let b = dense(&vec![0; DENSE], &[0; DENSE], |i, j| Some((i, j) == (3, 9)));
        assert!(b.split[k]);
        assert_eq!(b.split.iter().filter(|&&s| s).count(), 1);
        let (a, _) = b.square_polygons(3, 9).unwrap();
        let ne = Vec3::new(48.0, 120.0, 0.0);
        assert!(a.vertices.contains(&Vec3::new(36.0, 108.0, 0.0)) && a.vertices.contains(&ne));
    }

    #[test]
    fn without_stored_splits_the_hash_runs_over_global_square_coordinates() {
        let b = dense(&vec![0; DENSE], &[0; DENSE], |_, _| None);
        for i in 0..16_u32 {
            for j in 0..16_u32 {
                let expect = split_hash(0x5B * 16 + i, 0x79 * 16 + j);
                assert_eq!(b.split[(i * 16 + j) as usize], expect);
            }
        }
    }

    #[test]
    fn a_two_step_cell_is_partly_water_when_only_its_middle_vertex_is_water() {
        let water = 17 << 2;
        let mut t = vec![0_u16; DENSE];
        t[17 + 1] = water; // vertex (1, 1): the middle of cell (0, 0)
        let b = dense(&vec![0; DENSE], &t, |_, _| None);
        assert_eq!(b.cell_water_type(1), Some(WaterType::PartiallyWater));
        assert_eq!(b.cell_water_type(2), Some(WaterType::NotWater));
        assert_eq!(b.water_type, WaterType::PartiallyWater);
        // The nearest vertex is the square's: in the cell's south-west square, near its
        // north-east corner, that is the middle vertex.
        let cell = CellId(0x5B79_0001);
        assert_eq!(
            b.get_water_depth(cell, Vec3::new(10.0, 10.0, 0.0)),
            WATER_DEPTH_PARTIAL_WATER_VERTEX
        );
        assert_eq!(
            b.get_water_depth(cell, Vec3::new(2.0, 2.0, 0.0)),
            WATER_DEPTH_PARTIAL_SOLID_VERTEX
        );
    }

    #[test]
    fn a_two_step_road_runs_along_its_squares_at_the_forms_width() {
        // Road along the x = 12 vertex column, from y = 0 to y = 24.
        let mut t = vec![0_u16; DENSE];
        for y in 0..=2 {
            t[17 + y] = 1;
        }
        let b = dense(&vec![0; DENSE], &t, |_, _| None);
        // Square (1, 0) has its west edge on the road: on road within 4 m of x = 12.
        assert!(b.on_road(Vec3::new(15.0, 5.0, 0.0)));
        assert!(!b.on_road(Vec3::new(16.5, 5.0, 0.0)));
        // Square (0, 0) has its east edge on it: on road from 12 - 4 = 8 m.
        assert!(b.on_road(Vec3::new(9.0, 5.0, 0.0)));
        assert!(!b.on_road(Vec3::new(7.0, 5.0, 0.0)));
    }

    #[test]
    fn a_block_whose_vertices_are_not_the_grids_size_is_refused() {
        let e = LandblockCollision::build_in(
            &two(),
            LandblockId::new(1, 1),
            vec![0_u8; VERTEX_COUNT].into(),
            None,
            vec![0_u16; VERTEX_COUNT].into(),
            |_, _| None,
            false,
            &flat_table(),
        );
        assert!(matches!(e, Err(PhysicsError::DegenerateLandblock(_))));
    }

    #[test]
    fn a_grid_of_other_than_24_metre_cells_is_refused() {
        let grid = LandGrid {
            cell_length: 20.0,
            ..two()
        };
        let e = LandblockCollision::build_in(
            &grid,
            LandblockId::new(1, 1),
            vec![0_u8; DENSE].into(),
            None,
            vec![0_u16; DENSE].into(),
            |_, _| None,
            false,
            &flat_table(),
        );
        assert!(matches!(e, Err(PhysicsError::DegenerateLandblock(_))));
    }

    #[test]
    fn on_a_finer_grid_a_body_wades_in_a_cell_of_water_where_the_nearest_vertex_is_shallow_sea() {
        // A three-step block all of deep sea but cell (0, 0)'s middle vertex (1, 1), at (8, 8),
        // which is shallow sea; and cell (0, 1)'s middle vertex (1, 4), at (8, 32), shallow still
        // sea.
        let n = 25 * 25;
        let mut t = vec![20_u16 << 2; n];
        t[25 + 1] = 18 << 2;
        t[25 + 4] = 19 << 2;
        let b = LandblockCollision::build_in(
            &LandGrid::with_steps(3, 4.0),
            LandblockId::new(0x10, 0x20),
            vec![0_u8; n].into(),
            None,
            t.into(),
            |_, _| None,
            false,
            &flat_table(),
        )
        .unwrap();
        let lb = LandblockId::new(0x10, 0x20);
        assert_eq!(b.cell_water_type(1), Some(WaterType::EntirelyWater));
        // Nearest the shallow vertex: as at a shore.
        assert_eq!(
            b.get_water_depth(lb.cell(1), Vec3::new(9.0, 7.0, 0.0)),
            WATER_DEPTH_PARTIAL_WATER_VERTEX
        );
        assert_eq!(
            b.get_water_depth(lb.cell(2), Vec3::new(7.0, 33.0, 0.0)),
            WATER_DEPTH_PARTIAL_WATER_VERTEX
        );
        // Nearest a deep vertex, in the same cell and elsewhere: deep.
        assert_eq!(
            b.get_water_depth(lb.cell(1), Vec3::new(1.0, 1.0, 0.0)),
            WATER_DEPTH_ENTIRELY
        );
        assert_eq!(
            b.get_water_depth(lb.cell(10), Vec3::new(36.0, 36.0, 0.0)),
            WATER_DEPTH_ENTIRELY
        );
    }

    /// A three-step block whose cell (0, 0) has deep sea at the four vertices of its south-west
    /// square alone, `x, y <= 8`, and is dry land elsewhere.
    fn a_shore_cell_on_three_steps() -> LandblockCollision {
        let n = 25 * 25;
        let mut t = vec![0_u16; n];
        for (x, y) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            t[x * 25 + y] = 20 << 2;
        }
        LandblockCollision::build_in(
            &LandGrid::with_steps(3, 4.0),
            LandblockId::new(0x10, 0x20),
            vec![0_u8; n].into(),
            None,
            t.into(),
            |_, _| None,
            false,
            &flat_table(),
        )
        .unwrap()
    }

    #[test]
    fn on_a_finer_grid_a_body_stands_by_the_water_of_its_own_square() {
        let b = a_shore_cell_on_three_steps();
        let cell = LandblockId::new(0x10, 0x20).cell(1);
        assert_eq!(b.cell_water_type(1), Some(WaterType::PartiallyWater));
        // The south-west square is all water: deep, where by its cell it would be a shore.
        assert_eq!(
            b.water_type_at(1, Vec3::new(4.0, 4.0, 0.0)),
            Some(WaterType::EntirelyWater)
        );
        assert_eq!(
            b.get_water_depth(cell, Vec3::new(4.0, 4.0, 0.0)),
            WATER_DEPTH_ENTIRELY
        );
        // A square with no water corner is dry, where by its cell it would be a shore's 0.1.
        assert_eq!(
            b.water_type_at(1, Vec3::new(20.0, 20.0, 0.0)),
            Some(WaterType::NotWater)
        );
        assert_eq!(
            b.get_water_depth(cell, Vec3::new(20.0, 20.0, 0.0)),
            WATER_DEPTH_NONE
        );
        // A square with some water corners is a shore, by the corner nearest.
        assert_eq!(
            b.water_type_at(1, Vec3::new(9.0, 1.0, 0.0)),
            Some(WaterType::PartiallyWater)
        );
        assert_eq!(
            b.get_water_depth(cell, Vec3::new(9.0, 1.0, 0.0)),
            WATER_DEPTH_PARTIAL_WATER_VERTEX
        );
        assert_eq!(
            b.get_water_depth(cell, Vec3::new(15.0, 7.0, 0.0)),
            WATER_DEPTH_PARTIAL_SOLID_VERTEX
        );
    }

    #[test]
    fn a_cells_water_is_a_summary_of_its_squares_and_on_the_retail_grid_its_one_square() {
        for steps in [1_usize, 2, 3] {
            let side = 8 * steps + 1;
            let t: Vec<u16> = (0..side * side)
                .map(|i| {
                    if (i * 7 + i / 5) % 3 == 0 {
                        17 << 2
                    } else {
                        3 << 2
                    }
                })
                .collect();
            let b = LandblockCollision::build_in(
                &LandGrid::with_steps(steps, 4.0),
                LandblockId::new(0x10, 0x20),
                vec![0_u8; side * side].into(),
                None,
                t.into(),
                |_, _| None,
                false,
                &flat_table(),
            )
            .unwrap();
            let s = u32::try_from(steps).unwrap();
            for index in 1..=0x40_u16 {
                let (cx, cy) = landdefs::cell_index_to_xy(index);
                let squares: Vec<WaterType> = (0..s * s)
                    .map(|k| b.square_water_type(cx * s + k / s, cy * s + k % s).unwrap())
                    .collect();
                let any = squares.iter().any(|&w| w != WaterType::NotWater);
                let all = squares.iter().all(|&w| w == WaterType::EntirelyWater);
                assert_eq!(
                    b.cell_water_type(index),
                    Some(water_type_of(any, all)),
                    "{steps} steps, cell {index}"
                );
                if steps == 1 {
                    #[allow(clippy::cast_precision_loss)]
                    let p = Vec3::new(cx as f32 * 24.0 + 5.0, cy as f32 * 24.0 + 19.0, 0.0);
                    assert_eq!(b.water_type_at(index, p), b.cell_water_type(index));
                }
            }
        }
    }

    #[test]
    fn on_the_retail_grid_a_cell_of_shallow_sea_alone_sinks_a_body_deep() {
        let t = [18_u16 << 2; VERTEX_COUNT];
        let b = block([0; VERTEX_COUNT], t);
        let lb = LandblockId::new(0xA9, 0xB4);
        assert_eq!(b.cell_water_type(1), Some(WaterType::EntirelyWater));
        assert_eq!(
            b.get_water_depth(lb.cell(1), Vec3::new(2.0, 2.0, 0.0)),
            WATER_DEPTH_ENTIRELY
        );
    }

    #[test]
    fn a_three_step_block_lays_25_vertices_a_side_8_metres_apart_and_finds_its_squares() {
        let n = 25 * 25;
        let mut h = vec![0_u8; n];
        h[4 * 25 + 7] = 30; // inside cell (1, 2), at (32, 56)
        let b = LandblockCollision::build_in(
            &LandGrid::with_steps(3, 4.0),
            LandblockId::new(0x10, 0x20),
            h.into(),
            None,
            vec![0_u16; n].into(),
            |_, _| None,
            false,
            &flat_table(),
        )
        .unwrap();
        assert_eq!(
            (b.vertices.len(), b.polygons.len(), b.square_length()),
            (625, 1152, 8.0)
        );
        let top = ground(&b, 32.0, 56.0).unwrap();
        assert!((top - 60.0).abs() < 1e-3, "on the raised vertex: {top}");
        for k in 0..24 {
            for m in 0..24 {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (24.0 + k as f32 + 0.5, 48.0 + m as f32 + 0.5);
                assert!(ground(&b, x, y).is_some(), "({x}, {y})");
            }
        }
    }
}
