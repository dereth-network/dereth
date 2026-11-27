//! The landblock's LOD transition adjustment.
//!
//! This module applies the rendering-side LOD stitching rule.
//!
//! Edge stitching uses the vertex-rewrite rule implemented below.
//!
//! Physics (`dereth_physics::land`) deliberately does **not** implement the transition adjustment at all: it only
//! ever runs on reduced-detail blocks, and returns NULL for
//! those, so physics never sees a stitched block. The two implementations therefore cannot diverge
//! There is only one — and wrong-but-identical is far less harmful than two different
//! rules. If physics ever grows LOD collision, it must call
//! into this rule rather than restate it.
//!
//! The pass runs on **exactly** rings 2 and 4, because `q` is a band threshold rather than
//! `m` (see [`crate::land::order::block_orient`]). Fix (a) runs for any direction whenever
//! `1 < side_cell_count < 8`; fix (b) exists **only** for `side_cell_count == 4` with a cardinal
//! direction.
//!
//! **Fix (a) guards the ring-2/ring-3 and ring-4/ring-5 seams.** It is tempting to read the rule
//! as having "deliberately no guard" there, so that a crack is faithful behaviour.
//! That is wrong: **fix (a) *is* that guard, and it closes both boundaries exactly.** A ring-2
//! block's outward edge, once its odd knots are the averages of their neighbours, runs straight
//! through grid 0, 4 and 8 — which is precisely the polyline the 2-cell ring-3 neighbour draws; one
//! ring out, a ring-4 block's outward edge becomes the single span grid 0 to 8, which is precisely
//! the 1-cell ring-5 neighbour. Both agree to the float, measured over Holtburg in
//! the client's terrain-seam tests. Taken with fix (b) on the ring-1/ring-2 boundary and
//! the fact that rings 3 and 4 share a detail level, **the pass leaves no open seam anywhere in the
//! window** — and the ring-2 and ring-4 *diagonals*, which get no inward clamp, need none, because
//! their inward neighbours are in the same ring at the same detail.
//!
//! Fix (b) is the one that merely *hides* rather than closes: it drives the coarse edge **below**
//! the fine one so the nearer full-detail block stands in front of the gap. That is directional —
//! correct from the viewer, who is always on the fine side — and it is why the test for it asserts
//! an inequality where the test for fix (a) asserts equality.

use crate::consts::LAND_HEIGHT_TABLE_LEN;
use crate::land::mesh::{adj_planes, Direction, LandblockMesh};

/// `generate`'s guard: `trans_dir != InViewerBlock && 1 < side_cell_count < 8`.
#[must_use]
pub fn should_trans_adjust(side_cell_count: u8, dir: Direction) -> bool {
    dir != Direction::InViewerBlock && side_cell_count > 1 && side_cell_count < 8
}

/// The landblock's transition adjustment.
///
/// The client runs this **between** vertex construction and polygon construction, so the polygon
/// planes are built from the adjusted vertices. Running it after construction and re-running
/// plane adjustment — which is what generation does on the non-rebuilt path — produces
/// the same planes from the same vertices, so this function does that and the caller needs no
/// special ordering.
///
/// `heights` is the raw height-index array from the landblock record (9x9 on the default grid, the
/// block's whole grid on a finer one), `fine` a finer grid's fine height bytes; `table` is the
/// land height table. Fix (b) reads them directly rather than through the LOD-sampled vertices.
pub fn trans_adjust(
    m: &mut LandblockMesh,
    heights: &[u8],
    fine: Option<&[u8]>,
    table: &[f32; LAND_HEIGHT_TABLE_LEN],
    dir: Direction,
) {
    if !should_trans_adjust(m.side_cell_count, dir) {
        return;
    }
    m.trans_dir = dir;
    outward_edge_average(m, dir);
    if m.side_cell_count == 4 && dir.is_cardinal() {
        inward_crack_guard(m, heights, fine, table, dir);
    }
    adj_planes(m);
}

/// Fix (a): the **outward** edge, for every direction, whenever `1 < side_cell_count < 8`.
///
/// Every *odd-indexed* vertex on the edge named by `trans_dir` is replaced by the average of its
/// two neighbours along that edge, so the edge becomes the polyline the coarser neighbour — which
/// has half the vertex count — will draw.
///
/// | `trans_dir` contains | edge fixed | vertices |
/// |---|---|---|
/// | NORTH (N, NW, NE) | max y (`j = spc`) | `v(i, spc).z = (v(i-1, spc).z + v(i+1, spc).z)/2` |
/// | WEST (W, NW, SW) | `i = 0` | `v(0, j).z = (v(0, j-1).z + v(0, j+1).z)/2` |
/// | SOUTH (S, SW, SE) | `j = 0` | `v(i, 0).z = (v(i-1, 0).z + v(i+1, 0).z)/2` |
/// | EAST (E, NE, SE) | max x (`i = spc`) | `v(spc, j).z = (v(spc, j-1).z + v(spc, j+1).z)/2` |
fn outward_edge_average(m: &mut LandblockMesh, dir: Direction) {
    let spc = m.squares_per_side();
    let idx = |a: usize, b: usize| a * (spc + 1) + b;
    let mut odd: Vec<usize> = (1..spc).step_by(2).collect();
    odd.retain(|&i| i < spc);
    if dir.has_north() {
        for &i in &odd {
            m.vertices[idx(i, spc)].z =
                (m.vertices[idx(i - 1, spc)].z + m.vertices[idx(i + 1, spc)].z) / 2.0;
        }
    }
    if dir.has_west() {
        for &j in &odd {
            m.vertices[idx(0, j)].z =
                (m.vertices[idx(0, j - 1)].z + m.vertices[idx(0, j + 1)].z) / 2.0;
        }
    }
    if dir.has_south() {
        for &i in &odd {
            m.vertices[idx(i, 0)].z =
                (m.vertices[idx(i - 1, 0)].z + m.vertices[idx(i + 1, 0)].z) / 2.0;
        }
    }
    if dir.has_east() {
        for &j in &odd {
            m.vertices[idx(spc, j)].z =
                (m.vertices[idx(spc, j - 1)].z + m.vertices[idx(spc, j + 1)].z) / 2.0;
        }
    }
}

/// Fix (b): the **inward** edge crack guard, only for `side_cell_count == 4` with a cardinal
/// direction.
///
/// The edge that faces the viewer's full-detail (8-cell) ring keeps its own z but is clamped
/// downwards so the coarse edge can never poke above the fine one, using the raw height array (on
/// a grid of `s` steps a cell the same rule runs over every odd coarse vertex `n < 4s`, the grid
/// index `2n` and the edge at `8s`):
///
/// ```text
/// NorthOfViewer   : for i = 1, 3:  v = vertex(i, 0)   // south edge, grid x = 2i
///                   z = min(z, 2*H(2i-1, 0) - H(2i-2, 0), 2*H(2i+1, 0) - H(2i+2, 0))
/// SouthOfViewer   : for i = 1, 3:  v = vertex(i, 4)   // north edge, grid y = 8
/// EastOfViewer    : for j = 1, 3:  v = vertex(0, j)   // west edge, grid x = 0
/// WestOfViewer    : for j = 1, 3:  v = vertex(4, j)   // east edge, grid x = 8
/// ```
///
/// The clamp makes the straight coarse segment pass at or below the fine terrain at the
/// intermediate vertex, hiding the seam behind the nearer block.
fn inward_crack_guard(
    m: &mut LandblockMesh,
    heights: &[u8],
    fine: Option<&[u8]>,
    table: &[f32; LAND_HEIGHT_TABLE_LEN],
    dir: Direction,
) {
    // At side_cell_count 4 the block has 4 * steps squares a side; the full grid twice that.
    let sq = m.squares_per_side();
    let full = 2 * sq;
    let h = |a: usize, b: usize| {
        let k = a * (full + 1) + b;
        match fine {
            Some(f) => dereth_primitives::land_grid::fine_height(table, heights[k], f[k]),
            None => table[heights[k] as usize],
        }
    };
    let idx = |a: usize, b: usize| a * (sq + 1) + b; // 5 x 5 at side_cell_count 4 on the default grid
    for n in (1..sq).step_by(2) {
        let (vi, along) = match dir {
            Direction::North => (idx(n, 0), Along::X { fixed: 0 }),
            Direction::South => (idx(n, sq), Along::X { fixed: full }),
            Direction::East => (idx(0, n), Along::Y { fixed: 0 }),
            Direction::West => (idx(sq, n), Along::Y { fixed: full }),
            _ => return,
        };
        let g = 2 * n; // the grid index of this coarse vertex along the varying axis
        let sample = |k: usize| match along {
            Along::X { fixed } => h(k, fixed),
            Along::Y { fixed } => h(fixed, k),
        };
        let lo = 2.0 * sample(g - 1) - sample(g - 2);
        let hi = 2.0 * sample(g + 1) - sample(g + 2);
        let z = &mut m.vertices[vi].z;
        *z = z.min(lo).min(hi);
    }
}

#[derive(Clone, Copy)]
enum Along {
    /// The varying grid index is x; `fixed` is the y index of the edge.
    X { fixed: usize },
    /// The varying grid index is y; `fixed` is the x index of the edge.
    Y { fixed: usize },
}

#[cfg(test)]
mod tests {
    // Index arithmetic in test fixtures, bounded by the loops that build them.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;
    use crate::consts::{SIDE_VERTEX_COUNT, VERTEX_COUNT};
    use crate::land::mesh::generate_landblock_with_table;
    use dereth_assets::world::CellLandblock;
    use dereth_primitives::DataId;

    fn table() -> [f32; LAND_HEIGHT_TABLE_LEN] {
        let mut t = [0.0f32; LAND_HEIGHT_TABLE_LEN];
        for (i, v) in t.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            {
                *v = 2.0 * i as f32;
            }
        }
        t
    }

    fn lb(height: [u8; VERTEX_COUNT]) -> CellLandblock {
        CellLandblock {
            id: DataId(0x0001_FFFF),
            lbi_exists: 0,
            terrain: [0; VERTEX_COUNT],
            height,
            dense: None,
        }
    }

    fn unstitched(
        lb: &CellLandblock,
        r: &dereth_assets::region::Region,
        t: &[f32; LAND_HEIGHT_TABLE_LEN],
        lod_div: u8,
    ) -> LandblockMesh {
        generate_landblock_with_table(lb, r, t, 0, 0, lod_div, Direction::InViewerBlock)
    }

    fn region() -> dereth_assets::region::Region {
        // A minimal region: only land_height_table and land_surf are read by generate.
        dereth_assets::region::Region {
            id: DataId(0x1300_0000),
            region_number: 1,
            version: 0,
            region_name: String::new(),
            land_defs: dereth_assets::region::LandDefs {
                num_block_length: 255,
                num_block_width: 255,
                square_length: 24.0,
                lblock_length: 8,
                vertex_per_cell: 1,
                max_obj_height: 200.0,
                sky_height: 1000.0,
                road_width: 5.0,
                land_height_table: table().to_vec(),
            },
            game_time: dereth_assets::region::GameTime {
                zero_time_of_year: 0.0,
                zero_year: 0,
                day_length: 1.0,
                days_per_year: 1,
                year_spec: String::new(),
                times_of_day: Vec::new(),
                days_of_the_week: Vec::new(),
                seasons: Vec::new(),
            },
            parts_mask: 0,
            sky_info: None,
            sound_info: None,
            scene_info: None,
            terrain_types: Vec::new(),
            land_surf: dereth_assets::region::LandSurf {
                surf_type: 0,
                tex_merge: Some(dereth_assets::region::TexMerge {
                    base_tex_size: 1024,
                    corner_terrain_maps: Vec::new(),
                    side_terrain_maps: Vec::new(),
                    road_maps: Vec::new(),
                    terrain_desc: Vec::new(),
                }),
                pal_shift: None,
            },
            region_misc: None,
        }
    }

    /// Oracle: the LOD-stitching guard — `TransAdjust` runs only when the direction is
    /// not `InViewerBlock` and `1 < side_cell_count < 8`. A full-detail block and a 1x1 block are
    /// both left alone.
    #[test]
    fn the_guard_excludes_full_detail_and_one_by_one_blocks() {
        assert!(!should_trans_adjust(8, Direction::North));
        assert!(!should_trans_adjust(1, Direction::North));
        assert!(!should_trans_adjust(4, Direction::InViewerBlock));
        assert!(should_trans_adjust(4, Direction::North));
        assert!(should_trans_adjust(2, Direction::SouthEast));
    }

    /// Oracle: LOD-stitching fix (a) replaces every *odd-indexed* vertex on that edge with
    /// the average of its two neighbours along the edge. Built on a ramp whose odd vertices are
    /// deliberately off the straight line so the averaging is visible.
    #[test]
    fn the_outward_edge_becomes_the_coarse_neighbours_polyline() {
        // A block whose north edge zig-zags: even grid columns at height index 10, odd at 30.
        let mut height = [10u8; VERTEX_COUNT];
        for i in 0..SIDE_VERTEX_COUNT {
            if i % 2 == 1 {
                height[i * SIDE_VERTEX_COUNT + 8] = 30;
            }
        }
        let r = region();
        let t = table();
        // lod_div 2 -> side_cell_count 4, so the block's own vertices sample every second grid
        // column: all of them land on the even (height 10) columns.
        let mut m = unstitched(&lb(height), &r, &t, 2);
        let before: Vec<f32> = m.vertices.iter().map(|v| v.z).collect();
        trans_adjust(&mut m, &height, None, &t, Direction::North);
        // The north edge (j == 4) odd vertices i = 1, 3 become the average of their neighbours.
        for i in [1usize, 3] {
            let expect = (before[(i - 1) * 5 + 4] + before[(i + 1) * 5 + 4]) / 2.0;
            assert!((m.vertices[i * 5 + 4].z - expect).abs() < 1e-5, "i={i}");
        }
        // Even vertices on that edge, and every vertex off it, are untouched.
        for i in [0usize, 2, 4] {
            assert_eq!(m.vertices[i * 5 + 4].z, before[i * 5 + 4]);
        }
        for j in 0..4 {
            assert_eq!(m.vertices[2 * 5 + j].z, before[2 * 5 + j]);
        }
    }

    /// Oracle: LOD-stitching fix (b) exists **only** for `side_cell_count == 4` with
    /// a cardinal direction, and only ever lowers a vertex. A 2x2 block on the same terrain gets
    /// fix (a) alone.
    #[test]
    fn the_inward_crack_guard_only_lowers_and_only_for_four_cell_cardinals() {
        // A valley along the south edge: the fine terrain dips at odd grid columns, so
        // 2*H(2i-1,0) - H(2i-2,0) is well below the coarse vertex's own height.
        let mut height = [40u8; VERTEX_COUNT];
        for i in 0..SIDE_VERTEX_COUNT {
            if i % 2 == 1 {
                height[i * SIDE_VERTEX_COUNT] = 5;
            }
        }
        let r = region();
        let t = table();
        let mut m = unstitched(&lb(height), &r, &t, 2);
        let before: Vec<f32> = m.vertices.iter().map(|v| v.z).collect();
        trans_adjust(&mut m, &height, None, &t, Direction::North);
        for i in [1usize, 3] {
            let v = m.vertices[i * 5].z;
            assert!(
                v < before[i * 5],
                "vertex ({i},0) must be clamped down: {v} vs {}",
                before[i * 5]
            );
            let g = 2 * i;
            let lo = 2.0 * t[height[(g - 1) * SIDE_VERTEX_COUNT] as usize]
                - t[height[(g - 2) * SIDE_VERTEX_COUNT] as usize];
            let hi = 2.0 * t[height[(g + 1) * SIDE_VERTEX_COUNT] as usize]
                - t[height[(g + 2) * SIDE_VERTEX_COUNT] as usize];
            assert!((v - before[i * 5].min(lo).min(hi)).abs() < 1e-5);
        }
        // A 2x2 block (lod_div 4) never gets fix (b): only its outward edge changes.
        let mut m2 = unstitched(&lb(height), &r, &t, 4);
        let b2: Vec<f32> = m2.vertices.iter().map(|v| v.z).collect();
        trans_adjust(&mut m2, &height, None, &t, Direction::North);
        // side_vertex_count is 3; the south edge (j == 0) is untouched.
        for i in 0..3 {
            assert_eq!(
                m2.vertices[i * 3].z,
                b2[i * 3],
                "2x2 blocks get no inward clamp"
            );
        }
    }

    /// Oracle: a diagonal stitch direction fixes **two** outward edges and gets no
    /// inward clamp at all because the clamp is cardinal-only.
    #[test]
    fn diagonal_directions_fix_two_edges_and_never_clamp() {
        // A curved surface, so that a coarse vertex is genuinely off the chord between its two
        // coarse neighbours. A checkerboard would not do: at side_cell_count 4 the block samples
        // only even grid indices, so an every-other-vertex pattern reads as flat.
        let mut height = [0u8; VERTEX_COUNT];
        for i in 0..SIDE_VERTEX_COUNT {
            for j in 0..SIDE_VERTEX_COUNT {
                // LINT-OK: i, j <= 8 so i*i + j*j <= 128.
                height[i * SIDE_VERTEX_COUNT + j] = (i * i + j * j) as u8;
            }
        }
        let r = region();
        let t = table();
        let mut m = unstitched(&lb(height), &r, &t, 2);
        let before: Vec<f32> = m.vertices.iter().map(|v| v.z).collect();
        trans_adjust(&mut m, &height, None, &t, Direction::NorthEast);
        let changed: Vec<usize> = (0..m.vertices.len())
            .filter(|&i| m.vertices[i].z != before[i])
            .collect();
        // Only the north edge (j == 4) and the east edge (i == 4) odd vertices may move.
        for &i in &changed {
            let (a, b) = (i / 5, i % 5);
            assert!(
                (b == 4 && a % 2 == 1) || (a == 4 && b % 2 == 1),
                "vertex ({a},{b}) moved but is on neither outward edge"
            );
        }
        assert!(
            !changed.is_empty(),
            "the pass must actually do something on this terrain"
        );
    }

    /// Fix a closes the ring four to ring five boundary exactly.
    #[test]
    fn fix_a_closes_the_ring_four_to_ring_five_boundary_exactly() {
        // A convex ridge along the east edge, so the midpoint knot is genuinely off the chord.
        let mut height = [0u8; VERTEX_COUNT];
        for j in 0..SIDE_VERTEX_COUNT {
            // 0, 7, 12, 15, 16, 15, 12, 7, 0 — a parabola in the height index.
            // LINT-OK: j <= 8, so the product is far inside u8.
            height[8 * SIDE_VERTEX_COUNT + j] = (16 - (j as i32 - 4).pow(2)) as u8;
        }
        let r = region();
        let t = table();
        // side_cell_count 2: the east edge has knots at grid 0, 4, 8 and fix (a) averages the
        // middle one, leaving the straight run grid 0 -> grid 8.
        let mut m = unstitched(&lb(height), &r, &t, 4);
        let mid_before = m.vertices[2 * 3 + 1].z;
        trans_adjust(&mut m, &height, None, &t, Direction::East);
        let (lo, hi) = (m.vertices[2 * 3].z, m.vertices[2 * 3 + 2].z);
        let mid = m.vertices[2 * 3 + 1].z;
        assert!(
            (mid_before - t[height[8 * SIDE_VERTEX_COUNT + 4] as usize]).abs() < 1e-5,
            "the baseline must be the sampled terrain, not something already stitched"
        );
        assert!(
            mid_before > (lo + hi) / 2.0,
            "this fixture's midpoint is not off the chord, so the test cannot fail"
        );
        // The ring-5 neighbour draws the single span (lo, hi); the stitched edge must be on it.
        assert!(
            (mid - (lo + hi) / 2.0).abs() < 1e-5,
            "the stitched midpoint {mid} is not on the chord from {lo} to {hi}, so the ring-4 edge \
             is not the ring-5 neighbour's span and the boundary is open"
        );
        // And the pass still touches nothing but that one edge.
        for i in 0..3usize {
            for j in 0..3usize {
                if i == 2 && j == 1 {
                    continue;
                }
                let z = m.vertices[i * 3 + j].z;
                let want = t[height[i * 4 * SIDE_VERTEX_COUNT + j * 4] as usize];
                assert!(
                    (z - want).abs() < 1e-5,
                    "vertex ({i},{j}) moved and is not on the edge"
                );
            }
        }
    }

    // A world whose land cells are cut into three vertex steps: 25 x 25 vertices 8 m apart.

    const DENSE_SIDE: usize = 25;

    fn dense_region() -> dereth_assets::region::Region {
        let mut r = region();
        r.land_defs.vertex_per_cell = 3;
        r.land_defs.road_width = 4.0;
        r
    }

    fn dense_lb(height: Vec<u8>, split: Vec<u8>) -> CellLandblock {
        let n = DENSE_SIDE * DENSE_SIDE;
        CellLandblock::from_dense(
            DataId(0x0001_FFFF),
            0,
            dereth_assets::world::DenseLand {
                steps: 3,
                terrain: vec![4u16; n].into(),
                height: height.into(),
                split: split.into(),
                fine: dereth_assets::world::DenseLand::no_fine(3),
            },
        )
    }

    fn ripple() -> Vec<u8> {
        (0..DENSE_SIDE * DENSE_SIDE)
            .map(|i| ((i * 7) % 23) as u8)
            .collect()
    }

    #[test]
    fn a_three_step_block_draws_two_triangles_on_each_8_metre_square() {
        let t = table();
        let mut h = vec![0u8; DENSE_SIDE * DENSE_SIDE];
        h[4 * DENSE_SIDE + 7] = 15;
        let m = unstitched(&dense_lb(h, vec![0; 72]), &dense_region(), &t, 1);
        assert_eq!(
            (m.side_cell_count, m.vertex_steps, m.side_vertex_count),
            (8, 3, 25)
        );
        assert_eq!(m.squares_per_side(), 24);
        assert_eq!((m.vertices.len(), m.polygons.len()), (625, 1152));
        assert_eq!(
            (m.cell_keys.len(), m.cell_water.len(), m.sw_to_ne_cut.len()),
            (576, 576, 576)
        );
        let v = m.vertices[4 * DENSE_SIDE + 7];
        assert_eq!((v.x, v.y, v.z), (32.0, 56.0, 30.0));
    }

    #[test]
    fn a_three_step_block_cuts_each_square_by_its_stored_bit() {
        let t = table();
        let mut bits = vec![0u8; 72];
        let k = 5 * 24 + 11;
        bits[k / 8] |= 1 << (k % 8);
        let m = unstitched(
            &dense_lb(vec![0; DENSE_SIDE * DENSE_SIDE], bits),
            &dense_region(),
            &t,
            1,
        );
        assert!(m.sw_to_ne_cut[k]);
        assert_eq!(m.sw_to_ne_cut.iter().filter(|&&c| c).count(), 1);
    }

    #[test]
    fn a_reduced_three_step_block_samples_the_full_grid() {
        let t = table();
        let h = ripple();
        let m = unstitched(&dense_lb(h.clone(), vec![0; 72]), &dense_region(), &t, 2);
        // Four cells a side, three squares each: 13 x 13 vertices 16 m apart.
        assert_eq!(
            (m.side_cell_count, m.squares_per_side(), m.side_vertex_count),
            (4, 12, 13)
        );
        for i in 0..13 {
            for j in 0..13 {
                let v = m.vertices[i * 13 + j];
                assert_eq!(v.x, i as f32 * 16.0);
                assert_eq!(v.z, t[h[(2 * i) * DENSE_SIDE + 2 * j] as usize]);
            }
        }
        assert!(m.cell_water.is_empty(), "a reduced block carries no water");
    }

    #[test]
    fn a_reduced_three_step_block_stitches_its_outward_edge_to_the_coarser_ring() {
        let t = table();
        let lb = dense_lb(ripple(), vec![0; 72]);
        let m =
            generate_landblock_with_table(&lb, &dense_region(), &t, 0, 0, 2, Direction::NorthWest);
        let idx = |a: usize, b: usize| a * 13 + b;
        for i in (1..12).step_by(2) {
            let want = (m.vertices[idx(i - 1, 12)].z + m.vertices[idx(i + 1, 12)].z) / 2.0;
            assert_eq!(m.vertices[idx(i, 12)].z, want, "north edge vertex {i}");
        }
        for j in (1..12).step_by(2) {
            let want = (m.vertices[idx(0, j - 1)].z + m.vertices[idx(0, j + 1)].z) / 2.0;
            assert_eq!(m.vertices[idx(0, j)].z, want, "west edge vertex {j}");
        }
    }

    #[test]
    fn a_three_step_cell_is_partly_water_square_by_square() {
        let t = table();
        let mut lb = dense_lb(vec![0; DENSE_SIDE * DENSE_SIDE], vec![0; 72]);
        let d = lb.dense.as_mut().unwrap();
        d.terrain[DENSE_SIDE + 1] = 17 << 2; // vertex (1, 1), inside cell (0, 0)
        let m = unstitched(&lb, &dense_region(), &t, 1);
        use crate::land::water::WaterType;
        let wet: Vec<usize> = (0..576)
            .filter(|&n| m.cell_water[n] != WaterType::NotWater)
            .collect();
        // The four squares around vertex (1, 1).
        assert_eq!(wet, vec![0, 1, 24, 25]);
        assert_eq!(m.water_type, WaterType::PartiallyWater);
    }
}
