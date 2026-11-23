//! An art pack's ground cover as the remaster draws it: each cover's models scattered over a
//! block's terrain types, as placements of the pack's models beside the block's own.
//!
//! Where a cover grows comes from the block alone, so the block grows the same cover every time it
//! is built, on every machine: a hash of the block, the cell, the cover and the candidate's number
//! places each one. A candidate grows where the terrain type under it is one its cover names,
//! off the road, off steep ground, out of the water and clear of the block's buildings. The type
//! under a candidate is one of its cell's four corners', chosen with the corners' weights, so two
//! types' covers mingle where the types meet as the ground's pictures do.

use dereth_assets::art_pack::ManifestCover;
use dereth_primitives::num::math;
use dereth_remaster::scene as rm;
use dereth_terrain::consts::CELL_SIZE;
use dereth_terrain::land::mesh::LandblockMesh;
use dereth_terrain::land::water::WaterType;

/// The ground's height at a block-local point, and how upright it is there (its normal's height);
/// `None` off the block.
pub(super) type Surface<'a> = Box<dyn Fn(f32, f32) -> Option<(f32, f32)> + 'a>;

/// Whether the square `(sx, sy)` of the block's full-detail grid holds water.
pub(super) type Wet<'a> = Box<dyn Fn(usize, usize) -> bool + 'a>;

/// A block's full-detail ground, as cover grows on it: its surface, which of its squares hold
/// water, the vertex steps along a land cell side on its grid, and a road's half width there.
pub(super) struct CoverGround<'a> {
    pub surface: Surface<'a>,
    pub wet: Wet<'a>,
    pub steps: usize,
    pub road_width: f32,
}

/// `mesh` as cover grows on it, roads `road_width` wide on its grid; `None` where it is not at
/// full detail.
pub(super) fn ground_of(mesh: &LandblockMesh, road_width: f32) -> Option<CoverGround<'_>> {
    (mesh.side_cell_count == 8).then(|| CoverGround {
        surface: Box::new(move |x: f32, y: f32| {
            let index = dereth_terrain::scenery::outside_cell_index(x, y);
            let plane = dereth_terrain::scenery::find_terrain_poly(mesh, index, x, y)?;
            let n = plane.normal;
            (n.z > 0.0).then(|| (-(n.x * x + n.y * y + plane.d) / n.z, n.z))
        }),
        wet: Box::new(move |sx: usize, sy: usize| {
            mesh.cell_water.get(sx * mesh.squares_per_side() + sy) != Some(&WaterType::NotWater)
        }),
        steps: usize::from(mesh.vertex_steps.max(1)),
        road_width,
    })
}

/// How far from a building's origin no cover grows, meters: its walls and its yard.
const BUILDING_CLEAR: f32 = 14.0;

/// A hash of four words, its bits well mixed.
fn hash(a: u32, b: u32, c: u32, d: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.rotate_left(13).wrapping_mul(0x85EB_CA6B);
    h ^= c.rotate_left(7).wrapping_mul(0xC2B2_AE35) ^ d.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^ (h >> 16)
}

/// A hash as a number from 0 up to 1.
fn unit(h: u32) -> f32 {
    // LINT-OK: 24 bits are exact in an f32.
    #[allow(clippy::cast_precision_loss)]
    let u = (h >> 8) as f32 / 16_777_216.0;
    u
}

/// The cover the block at `at` grows, block-local: `ground` its full-detail ground, `words` its
/// terrain words (its whole grid's), `buildings` where its buildings stand, and `lifts` how far a
/// placement at a global point is drawn above it.
#[cfg(test)]
pub(super) fn cover_placements(
    at: (i32, i32),
    ground: &CoverGround<'_>,
    words: &[u16],
    covers: &[ManifestCover],
    buildings: &[glam::Vec3],
    lifts: &dyn Fn(f32, f32, f32) -> [f32; 2],
) -> Vec<rm::Placement> {
    let mut out = Vec::new();
    for row in 0..cover_rows(covers) {
        grow_row(at, ground, words, covers, buildings, lifts, row, &mut out);
    }
    out
}

/// How many rows of cells `covers` grow in, a cover's eight after another's: a block's cover is
/// grown a row at a time, in order, and the rows together are the whole of it.
pub(super) fn cover_rows(covers: &[ManifestCover]) -> usize {
    covers.len() * 8
}

/// Grow row `row` of [`cover_rows`] onto `out`, as the whole block's cover grows it. Each land cell
/// grows its candidates; what lies under one is its square's on the block's grid (the cell itself
/// on the default grid).
#[allow(clippy::too_many_arguments)]
pub(super) fn grow_row(
    at: (i32, i32),
    ground: &CoverGround<'_>,
    words: &[u16],
    covers: &[ManifestCover],
    buildings: &[glam::Vec3],
    lifts: &dyn Fn(f32, f32, f32) -> [f32; 2],
    row: usize,
    out: &mut Vec<rm::Placement>,
) {
    let Some(cover) = covers.get(row / 8) else {
        return;
    };
    // LINT-OK: a row of a block's eight.
    #[allow(clippy::cast_possible_truncation)]
    let cx = (row % 8) as u32;
    // LINT-OK: block coordinates, far inside f32's exact range.
    #[allow(clippy::cast_sign_loss)]
    let block = (at.0 as u32) << 16 | (at.1 as u32 & 0xFFFF);
    let area = CELL_SIZE * CELL_SIZE / 100.0;
    // The squares a land cell side is cut into, their side, and the grid's vertices a side.
    let steps = ground.steps.max(1);
    // LINT-OK: a few steps a cell.
    #[allow(clippy::cast_precision_loss)]
    let square = CELL_SIZE / steps as f32;
    let side = 8 * steps + 1;
    {
        let expected = cover.density * area;
        // LINT-OK: a count a cell, from a density a hundred square meters: a few dozen.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = expected.floor() as u32;
        let fraction = expected - expected.floor();
        let lean = math::hypotf(cover.max_slope, 1.0).recip();
        // Covers of one model each grow their own: their types are in their seed.
        let own = cover.id.0
            ^ cover
                .grounds
                .iter()
                .fold(0u32, |m, t| m | 1 << (t & 31))
                .rotate_left(7);
        {
            for cy in 0..8u32 {
                let cell = cx * 8 + cy;
                let seed = hash(block, cell, own, 0);
                let count = whole + u32::from(unit(seed) < fraction);
                for k in 1..=count {
                    let h = hash(block, cell, own, k);
                    let (h1, h2, h3, h4) = (
                        hash(h, 1, 0, 0),
                        hash(h, 2, 0, 0),
                        hash(h, 3, 0, 0),
                        hash(h, 4, 0, 0),
                    );
                    let (fx, fy) = (unit(h1), unit(h2));
                    // LINT-OK: 0..8 cells of 24 meters.
                    #[allow(clippy::cast_precision_loss)]
                    let (x, y) = ((cx as f32 + fx) * CELL_SIZE, (cy as f32 + fy) * CELL_SIZE);
                    // Its square, and where in it.
                    // LINT-OK: a few steps a cell.
                    #[allow(clippy::cast_precision_loss)]
                    let (ux, uy) = (fx * steps as f32, fy * steps as f32);
                    // LINT-OK: 0..steps, from a fraction under one.
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let (ix, iy) = (ux.floor() as usize, uy.floor() as usize);
                    let (sx, sy) = (cx as usize * steps + ix, cy as usize * steps + iy);
                    // LINT-OK: 0..steps.
                    #[allow(clippy::cast_precision_loss)]
                    let (gx, gy) = (ux - ix as f32, uy - iy as f32);
                    if (ground.wet)(sx, sy) {
                        continue;
                    }
                    let word_at = |dx: usize, dy: usize| {
                        words.get((sx + dx) * side + sy + dy).copied().unwrap_or(0)
                    };
                    // The type under it: a corner's, by the corners' weights.
                    let corners = [
                        ((1.0 - gx) * (1.0 - gy), 0, 0),
                        ((1.0 - gx) * gy, 0, 1),
                        (gx * (1.0 - gy), 1, 0),
                        (gx * gy, 1, 1),
                    ];
                    let mut pick = unit(h3);
                    let mut word = 0;
                    for (w, dx, dy) in corners {
                        word = word_at(dx, dy);
                        if pick < w {
                            break;
                        }
                        pick -= w;
                    }
                    let ty = ((word >> 2) & 31) as u8;
                    if !cover.grounds.contains(&ty) {
                        continue;
                    }
                    if dereth_primitives::land_grid::on_road(
                        words,
                        8 * steps + 1,
                        square,
                        ground.road_width,
                        x,
                        y,
                    ) {
                        continue;
                    }
                    let Some((z, upright)) = (ground.surface)(x, y) else {
                        continue;
                    };
                    if upright < lean {
                        continue;
                    }
                    if buildings
                        .iter()
                        .any(|b| math::hypotf(b.x - x, b.y - y) < BUILDING_CLEAR)
                    {
                        continue;
                    }
                    let scale = cover.scale.0 + (cover.scale.1 - cover.scale.0) * unit(h4);
                    let turn = unit(hash(h, 5, 0, 0)) * std::f32::consts::TAU;
                    let frame = glam::Mat4::from_translation(glam::Vec3::new(x, y, z))
                        * glam::Mat4::from_rotation_z(turn);
                    // LINT-OK: block coordinates, far inside f32's exact range.
                    #[allow(clippy::cast_precision_loss)]
                    let lifted = lifts(
                        at.0 as f32 * dereth_terrain::land::drawn::BLOCK_METERS + x,
                        at.1 as f32 * dereth_terrain::land::drawn::BLOCK_METERS + y,
                        z,
                    );
                    out.push(rm::Placement {
                        id: cover.id.0,
                        kind: rm::PlacementKind::Object,
                        frame,
                        scale,
                        lifts: lifted,
                        parts: Vec::new(),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;
    use dereth_primitives::DataId;

    /// Every vertex of type `ty`, and a road along the block's west edge where `road` says.
    fn words(ty: u16, road: bool) -> [u16; 81] {
        let mut words = [ty << 2; 81];
        if road {
            for w in words.iter_mut().take(9) {
                *w |= 1;
            }
        }
        words
    }

    /// Flat ground at height 10, rising `rise` a meter east of x = 96, and dry.
    fn grow(
        at: (i32, i32),
        rise: f32,
        words: &[u16],
        covers: &[ManifestCover],
        buildings: &[glam::Vec3],
    ) -> Vec<rm::Placement> {
        let surface = move |x: f32, _y: f32| {
            let r = if x < 96.0 { 0.0 } else { rise };
            Some((10.0 + r * (x - 96.0).max(0.0), math::hypotf(r, 1.0).recip()))
        };
        let wet = |_: usize, _: usize| false;
        let ground = CoverGround {
            surface: Box::new(surface),
            wet: Box::new(wet),
            steps: 1,
            road_width: 5.0,
        };
        cover_placements(at, &ground, words, covers, buildings, &|_, _, _| [0.0; 2])
    }

    fn cover(grounds: &[u8], density: f32) -> ManifestCover {
        ManifestCover {
            id: DataId(0xEC12_3456),
            grounds: grounds.to_vec(),
            density,
            scale: (0.5, 2.0),
            max_slope: 0.6,
            reach: 40.0,
        }
    }

    #[test]
    fn a_cover_grows_as_thickly_as_it_says_on_its_types_and_the_same_every_time() {
        let w = words(1, false);
        let c = [cover(&[1], 4.0)];
        let a = grow((0x12, 0x34), 0.0, &w, &c, &[]);
        // 4 a hundred square meters over 192 × 192: about 1,475.
        assert!((1350..1600).contains(&a.len()), "{}", a.len());
        assert_eq!(
            a,
            grow((0x12, 0x34), 0.0, &w, &c, &[]),
            "the same block, the same cover"
        );
        assert_ne!(
            a,
            grow((0x13, 0x34), 0.0, &w, &c, &[]),
            "another block grows its own"
        );
        for p in &a {
            let o = p.frame.w_axis;
            assert!((0.0..192.0).contains(&o.x) && (0.0..192.0).contains(&o.y));
            assert!((o.z - 10.0).abs() < 1e-3, "on the ground: {}", o.z);
            assert!((0.5..=2.0).contains(&p.scale));
            assert_eq!(p.id, 0xEC12_3456);
        }
        assert!(
            grow((0x12, 0x34), 0.0, &w, &[cover(&[2], 4.0)], &[]).is_empty(),
            "none on a type the cover does not name"
        );
    }

    #[test]
    fn where_two_types_meet_each_grows_its_own_cover_mingled_at_the_edge() {
        // Type 1 on the west half's vertices, type 2 on the east half's.
        let mut w = words(1, false);
        for cx in 5..9 {
            for cy in 0..9 {
                w[cx * 9 + cy] = 2 << 2;
            }
        }
        let ones = grow((1, 1), 0.0, &w, &[cover(&[1], 4.0)], &[]);
        assert!(ones.iter().all(|p| p.frame.w_axis.x < 5.0 * CELL_SIZE));
        assert!(
            ones.iter().any(|p| p.frame.w_axis.x > 4.0 * CELL_SIZE),
            "into the edge cell"
        );
        assert!(ones.iter().any(|p| p.frame.w_axis.x < 4.0 * CELL_SIZE));
    }

    /// On a grid of three steps a land cell, a cover grows by its squares' types, keeps out of
    /// its wet squares, and keeps off a road along its squares' edges.
    #[test]
    fn on_a_finer_grid_cover_grows_by_its_squares_types_water_and_roads() {
        // Type 2 everywhere but land cell (1, 1)'s vertices, which are type 1.
        let mut w = vec![2u16 << 2; 25 * 25];
        for x in 3..=6 {
            for y in 3..=6 {
                w[x * 25 + y] = 1 << 2;
            }
        }
        let surface = |_: f32, _: f32| Some((10.0, 1.0));
        let wet = |sx: usize, sy: usize| (sx, sy) == (4, 4);
        let ground = CoverGround {
            surface: Box::new(surface),
            wet: Box::new(wet),
            steps: 3,
            road_width: 2.0,
        };
        let c = [cover(&[1], 40.0)];
        let ones = cover_placements((5, 5), &ground, &w, &c, &[], &|_, _, _| [0.0; 2]);
        assert!(ones.len() > 100, "{}", ones.len());
        let o = |p: &rm::Placement| p.frame.w_axis;
        // Only on the squares that touch type 1's vertices: squares 2 to 6 each way.
        assert!(ones
            .iter()
            .all(|p| (16.0..56.0).contains(&o(p).x) && (16.0..56.0).contains(&o(p).y)));
        assert!(ones.iter().any(|p| o(p).x < 24.0) && ones.iter().any(|p| o(p).x >= 48.0));
        // None in the wet square.
        assert!(!ones
            .iter()
            .any(|p| (32.0..40.0).contains(&o(p).x) && (32.0..40.0).contains(&o(p).y)));
        // A road on the vertices of column 4: none within two meters of it in the squares beside.
        for y in 0..25 {
            w[4 * 25 + y] |= 1;
        }
        let roads = cover_placements((5, 5), &ground, &w, &c, &[], &|_, _, _| [0.0; 2]);
        assert!(roads.len() < ones.len());
        assert!(roads.iter().all(|p| (o(p).x - 32.0).abs() >= 2.0 - 1e-4));
    }

    #[test]
    fn no_cover_grows_on_the_road_on_steep_ground_or_by_a_building() {
        let w = words(1, true);
        let c = [cover(&[1], 4.0)];
        let a = grow((0x12, 0x34), 0.0, &w, &c, &[]);
        assert!(!a.is_empty());
        assert!(a.iter().all(|p| {
            let o = p.frame.w_axis;
            !dereth_terrain::road::on_road(&w, o.x, o.y)
        }));
        let steep = grow((0x12, 0x34), 1.0, &w, &c, &[]);
        assert!(
            steep.iter().all(|p| p.frame.w_axis.x < 96.0),
            "none past 0.6 a meter"
        );
        assert!(grow((0x12, 0x34), 0.5, &w, &c, &[])
            .iter()
            .any(|p| p.frame.w_axis.x > 96.0));
        let house = glam::Vec3::new(96.0, 96.0, 10.0);
        let b = grow((0x12, 0x34), 0.0, &w, &c, &[house]);
        assert!(b.len() < a.len());
        assert!(b.iter().all(|p| {
            let o = p.frame.w_axis;
            math::hypotf(o.x - house.x, o.y - house.y) >= BUILDING_CLEAR
        }));
    }
}
