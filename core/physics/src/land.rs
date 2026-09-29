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
//! **UNVERIFIED:** the LOD seam vertex
//! rewrite between a reduced-detail block and its full-detail neighbours is **not implemented**.
//! The rule was never read out of the client, and it only ever runs on reduced-detail blocks,
//! which physics never sees: landblock-mesh lookup returns NULL for them and
//! [`LandblockCollision::build`] refuses them. It is primarily a *rendering* problem, because it
//! changes distant silhouettes.

use dereth_primitives::{CellId, LandblockId, Vec3};

use crate::geom::polygon::{PolySide, Polygon};
use crate::globals::{
    BLOCK_LENGTH, HALF_SQUARE_LENGTH, LAND_HEIGHT_TABLE_LEN, MAX_OBJECT_HEIGHT, ROAD_WIDTH,
    ROAD_WIDTH_FAR, WATER_DEPTH_ENTIRELY, WATER_DEPTH_NONE, WATER_DEPTH_PARTIAL_SOLID_VERTEX,
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
#[derive(Debug, Clone, PartialEq)]
pub struct LandblockCollision {
    pub id: LandblockId,
    /// 9 at full detail.
    pub side_vertex_count: u32,
    /// 8; anything else is a degenerate block and is excluded from physics.
    pub side_cell_count: u32,
    /// 8.
    pub side_polygon_count: u32,
    /// Height-table indices, `height[x * 9 + y]`.
    pub height: Box<[u8; VERTEX_COUNT]>,
    /// Terrain words, `terrain[x * 9 + y]`.
    pub terrain: Box<[u16; VERTEX_COUNT]>,
    /// A `LandblockInfo` record exists for this block (buildings / env cells).
    pub has_info: bool,

    /// `side_vertex_count^2` vertices, `x * 9 + y`.
    pub vertices: Vec<Vec3>,
    /// `SWtoNEcut`, `side_polygon_count^2` flags indexed `side_polygon_count * i + j`.
    pub split: Vec<bool>,
    /// `2 * side_polygon_count^2` triangles; cell `(i, j)` owns `[2k]` and `[2k + 1]` where
    /// `k = side_polygon_count * i + j`.
    pub polygons: Vec<Polygon>,
    /// Per-cell water, `side_cell_count^2`, same indexing as [`Self::split`].
    pub cell_water: Vec<WaterType>,
    /// The whole-block summary from the water calculation.
    pub water_type: WaterType,
    /// The landblock's height limit.
    pub max_zval: f32,
    /// As above.
    pub min_zval: f32,
}

impl LandblockCollision {
    /// Build the collision geometry for one full-detail landblock.
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
        let side_vertex_count = side_cell_count + 1;
        let side_polygon_count = side_cell_count;

        // step = 8 / side_cell_count (1 at full detail),
        // scale = block_length / side_polygon_count = 24.
        #[allow(clippy::cast_precision_loss)]
        let scale = BLOCK_LENGTH / side_polygon_count as f32;
        let step = 8 / side_cell_count as usize;
        let svc = side_vertex_count as usize;
        let mut vertices = Vec::with_capacity(svc * svc);
        for i in 0..svc {
            for j in 0..svc {
                #[allow(clippy::cast_precision_loss)]
                let (x, y) = (i as f32 * scale, j as f32 * scale);
                let z = height_table[height[i * step * SIDE_VERTEX_COUNT + j * step] as usize];
                vertices.push(Vec3::new(x, y, z));
            }
        }

        // The hash is over global cell coordinates.
        let lbx8 = u32::from(id.x()) * 8;
        let lby8 = u32::from(id.y()) * 8;
        let spc = side_polygon_count as usize;
        let mut split = Vec::with_capacity(spc * spc);
        let mut polygons = Vec::with_capacity(2 * spc * spc);
        for i in 0..spc {
            for j in 0..spc {
                #[allow(clippy::cast_possible_truncation)]
                let (gx, gy) = (lbx8 + i as u32, lby8 + j as u32);
                let sw_to_ne = split_hash(gx, gy);
                split.push(sw_to_ne);
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
                let (any, all) = calc_cell_water(&terrain, i, j);
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
        let max_zval = height_table[max_h as usize] + MAX_OBJECT_HEIGHT;
        let min_zval = height_table[min_h as usize] - 1.0;

        Ok(Self {
            id,
            side_vertex_count,
            side_cell_count,
            side_polygon_count,
            height,
            terrain,
            has_info,
            vertices,
            split,
            polygons,
            cell_water,
            water_type,
            max_zval,
            min_zval,
        })
    }

    /// The two terrain triangles of one land cell, by cell index `1 ..= 0x40`.
    #[must_use]
    pub fn cell_polygons(&self, cell_index: u16) -> Option<(&Polygon, &Polygon)> {
        if cell_index == 0 || cell_index > 0x40 {
            return None;
        }
        let (cx, cy) = landdefs::cell_index_to_xy(cell_index);
        let k = (self.side_polygon_count * cx + cy) as usize;
        Some((&self.polygons[2 * k], &self.polygons[2 * k + 1]))
    }

    /// The first of the land cell's two triangles that contains the point in 2-D. Every
    /// land cell has exactly two terrain polygons; several routines hard-code that bound.
    #[must_use]
    pub fn find_terrain_poly(&self, cell_index: u16, local: Vec3) -> Option<&Polygon> {
        let (a, b) = self.cell_polygons(cell_index)?;
        if a.point_in_poly2d(local, PolySide::Positive) {
            Some(a)
        } else if b.point_in_poly2d(local, PolySide::Positive) {
            Some(b)
        } else {
            None
        }
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

    /// The shared "which of the cell's four corner vertices is nearest" rule, used identically by
    /// the terrain lookup and the water-depth step.
    fn nearest_vertex_word(&self, cx: i32, cy: i32, local: Vec3) -> u16 {
        #[allow(clippy::cast_precision_loss)]
        let mid_x = (2 * cx + 1) as f32 * HALF_SQUARE_LENGTH;
        #[allow(clippy::cast_precision_loss)]
        let mid_y = (2 * cy + 1) as f32 * HALF_SQUARE_LENGTH;
        #[allow(clippy::cast_sign_loss)]
        let (cx, cy) = (cx as usize, cy as usize);
        let i = cy + cx * SIDE_VERTEX_COUNT;
        if local.x <= mid_x {
            if local.y <= mid_y {
                self.terrain[i]
            } else {
                self.terrain[i + 1]
            }
        } else if local.y <= mid_y {
            self.terrain[cy + (cx + 1) * SIDE_VERTEX_COUNT]
        } else {
            self.terrain[i + SIDE_VERTEX_COUNT + 1]
        }
    }

    /// The depth inside a partially-water cell,
    /// from the nearest of the cell's four corner vertices.
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

    /// The water depth for a land cell of this block. The four depths are fixed at
    /// `0.9 / 0.45 / 0.1 / 0.0`.
    #[must_use]
    pub fn get_water_depth(&self, cell: CellId, local: Vec3) -> f32 {
        match self.cell_water_type(cell.index()) {
            None | Some(WaterType::NotWater) => WATER_DEPTH_NONE,
            Some(WaterType::EntirelyWater) => WATER_DEPTH_ENTIRELY,
            Some(WaterType::PartiallyWater) => self.calc_water_depth(cell, local),
        }
    }

    /// The water type of one land cell by index `1 ..= 0x40`.
    #[must_use]
    pub fn cell_water_type(&self, cell_index: u16) -> Option<WaterType> {
        if cell_index == 0 || cell_index > 0x40 {
            return None;
        }
        let (cx, cy) = landdefs::cell_index_to_xy(cell_index);
        Some(self.cell_water[(self.side_cell_count * cx + cy) as usize])
    }

    /// The 16-case road-edge geometry, with a 5 m half width
    /// and its `24 - 5 = 19` mirror.
    ///
    /// **The `1100` case is `fx < 5` and the `1010` case is `fy < 5`**, easy to swap. The
    /// geometry says so too: `1100` is `r00 | r01`, i.e. both road vertices sit on the `x = cx`
    /// edge, so the road runs along Y at low X and the on-road band is `fx < 5`.
    ///
    /// Roads do not change collision — `on_road` is consulted only by scenery placement and by
    /// the renderer's road blend, so a road on a 50-degree slope is still not walkable.
    #[must_use]
    pub fn on_road(&self, local: Vec3) -> bool {
        let cx = landdefs::cell_of(local.x);
        let cy = landdefs::cell_of(local.y);
        #[allow(clippy::cast_sign_loss)]
        let i = (cx * 9 + cy) as usize;
        if i + 10 >= VERTEX_COUNT {
            return false;
        }
        let r00 = road_bits(self.terrain[i]) != 0;
        let r01 = road_bits(self.terrain[i + 1]) != 0;
        let r10 = road_bits(self.terrain[i + 9]) != 0;
        let r11 = road_bits(self.terrain[i + 10]) != 0;
        if !(r00 || r01 || r10 || r11) {
            return false;
        }
        #[allow(clippy::cast_precision_loss)]
        let fx = local.x - cx as f32 * crate::globals::CELL_SIZE;
        #[allow(clippy::cast_precision_loss)]
        let fy = local.y - cy as f32 * crate::globals::CELL_SIZE;
        let w = ROAD_WIDTH;
        let f = ROAD_WIDTH_FAR;
        match (r00, r01, r10, r11) {
            (true, true, true, true) => true,
            (true, true, true, false) => fx < w || fy < w,
            (true, true, false, true) => fx < w || fy > f,
            (true, true, false, false) => fx < w,
            (true, false, true, true) => fx > f || fy < w,
            (true, false, true, false) => fy < w,
            (true, false, false, true) => (fx - fy).abs() < w,
            (true, false, false, false) => fy + fx < w,
            (false, true, true, true) => fx > f || fy > f,
            (false, true, true, false) => ((fy + fx) - 24.0).abs() < w,
            (false, true, false, true) => fy > f,
            (false, true, false, false) => (fx + 24.0) - fy < w,
            (false, false, true, true) => fx > f,
            (false, false, true, false) => (24.0 - fx) + fy < w,
            (false, false, false, true) => (48.0 - fx) - fy < w,
            (false, false, false, false) => false,
        }
    }
}

/// Look at the cell's four corner vertices.
/// Returns `(any_water, all_water)`.
#[must_use]
pub fn calc_cell_water(terrain: &[u16; VERTEX_COUNT], i: usize, j: usize) -> (bool, bool) {
    let mut any = false;
    let mut all = true;
    for a in i..=i + 1 {
        for b in j..=j + 1 {
            if surf_char(terrain[b + a * SIDE_VERTEX_COUNT]) == SurfChar::Water {
                any = true;
            } else {
                all = false;
            }
        }
    }
    (any, all)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the client's own landblock-structure and landblock code --
    // the vertex and polygon construction, the per-cell water, the block water, the water depth
    // and the terrain lookup,
    // on_road. The retail-data comparison lives in tests/retail_land.rs.

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
}
