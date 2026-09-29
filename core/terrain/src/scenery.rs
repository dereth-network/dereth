//! Deterministic scenery generation.
//!
//! The landblock's scene fetch plus an object description's placement, its frame, its
//! alignment, the slope check, the scale step and the height step, and the region's scene-type
//! count, scene count and scene accessors.
//!
//! The scene and terrain records are described in `docs/formats/15-region.md` and
//! `docs/formats/17-scene-and-particles.md`.
//!
//! Scenery is not stored per landblock: it is **generated** from a hash of global cell coordinates
//! so that every client and the server put the same tree in the same spot.
//!
//! Two facts govern this file.
//!
//! * **ACE is not a valid oracle here.** Its six hashes match, but it stubs `OnRoad`, does
//!   not implement the slope test at all, uses inclusive bounds where the client is strict, clamps
//!   the scene index where the client skips, and has no within-block rejection. Every one of
//!   those changes which trees exist.
//! * Scene generation returns immediately unless `side_cell_count == 8`, so
//!   **LOD blocks carry no scenery at all**.
//!
//! Every hash is 32-bit wrapping signed multiply/add, reinterpreted **unsigned** and scaled by
//! 2⁻³² — never a 64-bit intermediate. A different constant or a signed conversion moves the entire
//! world's scenery, and it looks plausible.

use dereth_assets::region::Region;
use dereth_assets::world::{CellLandblock, ObjectDesc, Scene};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, DataId, Frame, LandblockId, Quat, Vec3};

use crate::consts::{BLOCK_LENGTH, CELL_SIZE, SIDE_VERTEX_COUNT};
use crate::land::mesh::{plane_set_height, LandblockMesh};
use crate::math::{set_heading, set_vector_heading};
use crate::narrow::{i32_of, u16_of_i32, u32_of};
use crate::road::on_road;
use crate::Plane;

/// 2⁻³². The client's literal is `2.3283064e-10`.
const INV_2_32: f32 = 2.328_306_4e-10;

/// `(uint32)v * 2^-32` — the *unsigned* reinterpretation of a 32-bit product, as a float in
/// `[0, 1)`. Doing this signed, or through a 64-bit intermediate, moves every tree.
#[inline]
#[must_use]
fn unit(v: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // the loss is the client's; it converts u32 to float too
    {
        (v as f32) * INV_2_32
    }
}

/// The per-object placement hash shared by the placement, frame and scale steps:
///
/// ```text
/// hash(gx, gy, s) = (uint32)(gy*0x6C1AC587 - (gx*gy*0x5111BFEF + 0x70892FB7)*s
///                            + gx*(-0x421BE3BD)) * 2^-32
/// ```
///
/// The retail scene fetch hoists the three terms, kept here as `cell_x_mat`, `cell_y_mat` and
/// `cell_mat`.
#[inline]
#[must_use]
pub fn scenery_hash(gx: i32, gy: i32, salt: u32) -> f32 {
    let (gx, gy) = (gx as u32, gy as u32);
    let cell_x_mat = gx.wrapping_mul(0xBDE4_1C43); // gx * -0x421BE3BD
    let cell_y_mat = gy.wrapping_mul(0x6C1A_C587);
    let cell_mat = gx
        .wrapping_mul(gy)
        .wrapping_mul(0x5111_BFEF)
        .wrapping_add(0x7089_2FB7);
    unit(
        cell_y_mat
            .wrapping_sub(cell_mat.wrapping_mul(salt))
            .wrapping_add(cell_x_mat),
    )
}

/// The salts, each named for the operation that uses it.
pub mod salt {
    /// Scenery generation's frequency draw: `noise = hash(gx, gy, 0x5B67 + k)`.
    pub const FREQ: u32 = 0x5B67;
    /// Placement displacement along x.
    pub const DISPLACE_X: u32 = 0xB2CD;
    /// Placement displacement along y.
    pub const DISPLACE_Y: u32 = 0x1_1C0F;
    /// Placement heading.
    pub const ROTATION: u32 = 0xF697;
    /// Scale exponent: the scale step hashes the object index plus this, and scales by
    /// `pow(max / min, r) * min`.
    pub const SCALE: u32 = 0x7F51;
}

/// The scene fetch's scene-index hash, which uses a **different** first constant
/// (`0x2A7F2B89`, not `0x5111BFEF`) and a different tail:
///
/// ```text
/// v = (gx*0x2A7F2B89 + 0x6C1AC587)*gy + gx*(-0x421BE3BD) + 0x7F8CDA01
/// sceneIndex = (int)floor(((uint32)v * 2^-32) * count)
/// ```
///
/// Only evaluated when `count != 1`; a single-scene list takes index 0 without hashing.
#[must_use]
pub fn scene_index(gx: i32, gy: i32, count: usize) -> usize {
    if count == 1 {
        return 0;
    }
    let (gxu, gyu) = (gx as u32, gy as u32);
    let v = gxu
        .wrapping_mul(0x2A7F_2B89)
        .wrapping_add(0x6C1A_C587)
        .wrapping_mul(gyu)
        .wrapping_add(gxu.wrapping_mul(0xBDE4_1C43))
        .wrapping_add(0x7F8C_DA01);
    #[allow(clippy::cast_precision_loss)]
    let f = unit(v) * (count as f32);
    let i = dereth_primitives::num::floor_to_i32(f);
    i.max(0) as usize
}

/// One placed object, as the client's static-object add would have received it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedScenery {
    /// Graphics-object id passed to object creation.
    pub gfxobj: DataId,
    /// Block-local frame: origin in `[0, 192)²` with z dropped onto the terrain plane.
    pub frame: Frame,
    pub scale: f32,
    /// The outdoor cell the object was added to.
    pub cell: CellId,
}

/// The slope check for a scenery object.
///
/// The retail client tests `min_slope <= n && ((n < max_slope) != (n == max_slope))`, and that
/// second clause is `n <= max_slope` written the long way — so **both bounds are inclusive**.
/// An earlier description printed it as `min_slope <= N.z < max_slope`; that description was
/// corrected.
#[must_use]
pub fn check_slope(obj: &ObjectDesc, normal_z: f32) -> bool {
    obj.min_slope <= normal_z && normal_z <= obj.max_slope
}

/// The scale step:
/// `min_scale == max_scale ? max_scale : pow(max_scale/min_scale, hash) * min_scale`.
#[must_use]
pub fn scale_obj(obj: &ObjectDesc, gx: i32, gy: i32, k: usize) -> f32 {
    if obj.min_scale == obj.max_scale {
        return obj.max_scale;
    }
    // LINT-OK: k indexes a scene's object list, far inside u32.
    let r = scenery_hash(gx, gy, salt::SCALE.wrapping_add(u32_of(k)));
    math::powf(obj.max_scale / obj.min_scale, r) * obj.min_scale
}

/// Compute displacement inside the cell and apply the quadrant flip.
///
/// Returns the cell-local `(x, y, z)` before the `+ i*24, + j*24` cell offset the caller adds.
#[must_use]
pub fn place(obj: &ObjectDesc, gx: i32, gy: i32, k: usize) -> Vec3 {
    // LINT-OK: k indexes a scene's object list, far inside u32.
    let k = u32_of(k);
    let mut x = obj.base_loc.origin.x;
    let mut y = obj.base_loc.origin.y;
    let z = obj.base_loc.origin.z;
    if obj.displace_x > 0.0 {
        x += scenery_hash(gx, gy, salt::DISPLACE_X.wrapping_add(k)) * obj.displace_x;
    }
    if obj.displace_y > 0.0 {
        y += scenery_hash(gx, gy, salt::DISPLACE_Y.wrapping_add(k)) * obj.displace_y;
    }
    // The quadrant flip has its own hash, with no per-object salt.
    let (gxu, gyu) = (gx as u32, gy as u32);
    let q = unit(
        gyu.wrapping_mul(0x6C1A_C587)
            .wrapping_sub(gxu.wrapping_mul(gyu.wrapping_mul(0x6F7B_D965).wrapping_add(0x421B_E3BD)))
            .wrapping_sub(0x17FC_EDFD),
    );
    let (x, y) = if q < 0.25 {
        (x, y)
    } else if q < 0.5 {
        (-y, x)
    } else if q < 0.75 {
        (-x, -y)
    } else {
        (y, -x)
    };
    Vec3::new(x, y, z)
}

/// Build the placed frame: copy `base_loc`, move the origin to the placed point, and
/// (when `max_rot > 0`) set the heading from the rotation hash. `max_rot` is in **degrees**; ACE
/// multiplies the same expression by `0.0174533`.
#[must_use]
pub fn get_obj_frame(obj: &ObjectDesc, gx: i32, gy: i32, k: usize, p: Vec3) -> Frame {
    let mut f = Frame::new(p, obj.base_loc.rotation);
    if obj.max_rot > 0.0 {
        // LINT-OK: k indexes a scene's object list, far inside u32.
        let r = scenery_hash(gx, gy, salt::ROTATION.wrapping_add(u32_of(k)));
        set_heading(&mut f, r * obj.max_rot);
    }
    f
}

/// When `align != 0`, copy `base_loc`, move the origin, and
/// set the heading from the negated polygon normal — the object faces down the slope.
///
/// The client sets vector heading from the negated normal, which also picks up the slope's pitch;
/// `orient` is serialized but read by nothing,
/// so `align` is the only field that matters.
#[must_use]
pub fn obj_align(obj: &ObjectDesc, plane: &Plane, p: Vec3) -> Frame {
    let mut f = Frame::new(p, obj.base_loc.rotation);
    set_vector_heading(
        &mut f,
        Vec3::new(-plane.normal.x, -plane.normal.y, -plane.normal.z),
    );
    f
}

/// The within-block test, which is what the object's own within-block check
/// reduces to for a scenery object: the sorting sphere, transformed into block space, must clear
/// every edge of the 192 m block by its own radius.
#[must_use]
pub fn within_block(p: Vec3, radius: f32) -> bool {
    radius <= p.x && radius <= p.y && p.x < BLOCK_LENGTH - radius && p.y < BLOCK_LENGTH - radius
}

/// What scenery generation needs from outside this crate.
pub struct SceneryEnv<'a> {
    /// Fetch `scene_id` as record type `0x1B`.
    pub scenes: &'a dyn Fn(DataId) -> Option<Scene>,
    /// A cell that owns a building grows no scenery. The argument
    /// is the outdoor cell index `1..=64`.
    pub has_building: &'a dyn Fn(u16) -> bool,
    /// Sorting sphere for a `gfxobj` id: `(centre, radius)` in object space. The
    /// client falls back to `(origin, 0)` for an object with no spheres, which is what `None` means
    /// here.
    ///
    /// Part arrays belong to animation and collision spheres to physics, so this crate asks rather
    /// than computes. With `None` for every object the within-block filter degenerates to the
    /// point test, which is the client's own behaviour for a sphere-less object.
    pub sorting_sphere: &'a dyn Fn(DataId) -> Option<(Vec3, f32)>,
}

impl std::fmt::Debug for SceneryEnv<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneryEnv").finish_non_exhaustive()
    }
}

/// The region's scene-type count.
#[must_use]
pub fn num_scene_type(region: &Region, terrain_type: usize) -> usize {
    region
        .terrain_types
        .get(terrain_type)
        .map_or(0, |t| t.scene_types.len())
}

/// The region's scene count for a scene type.
///
/// `scene_types[s]` is an index into the region's scene-description array, resolved to a pointer at
/// unpack time; `-1` becomes a null pointer, and the client's null check makes that count 0.
#[must_use]
pub fn scene_count(region: &Region, terrain_type: usize, scene_type: usize) -> usize {
    let Some(t) = region.terrain_types.get(terrain_type) else {
        return 0;
    };
    let Some(&idx) = t.scene_types.get(scene_type) else {
        return 0;
    };
    if idx < 0 {
        return 0;
    }
    region
        .scene_info
        .as_ref()
        .and_then(|s| s.get(idx as usize))
        .map_or(0, |d| d.scenes.len())
}

/// The region's scene accessor. Returns `None` for
/// `INVALID_DID`, which is what an out-of-range scene index produces — and which makes the client
/// **skip** the vertex where ACE clamps the index to 0.
#[must_use]
pub fn get_scene(
    region: &Region,
    terrain_type: usize,
    scene_type: usize,
    index: usize,
) -> Option<DataId> {
    let t = region.terrain_types.get(terrain_type)?;
    let &idx = t.scene_types.get(scene_type)?;
    if idx < 0 {
        return None;
    }
    let d = region.scene_info.as_ref()?.get(idx as usize)?;
    d.scenes.get(index).copied()
}

/// Outdoor cell index `1 + cellY + cellX*8` for a block-local point.
#[must_use]
pub fn outside_cell_index(x: f32, y: f32) -> u16 {
    let cx = dereth_primitives::num::floor_to_i32(x / CELL_SIZE).clamp(0, 7);
    let cy = dereth_primitives::num::floor_to_i32(y / CELL_SIZE).clamp(0, 7);
    // LINT-OK: both clamped to 0..=7.
    u16_of_i32(1 + cy + cx * 8)
}

/// Select the **first** of the cell's two triangles that
/// contains the point in 2-D. A land cell has exactly two polygons and several client routines
/// hard-code that loop bound.
#[must_use]
pub fn find_terrain_poly(m: &LandblockMesh, cell_index: u16, x: f32, y: f32) -> Option<&Plane> {
    if cell_index == 0 || cell_index > 0x40 {
        return None;
    }
    let idx = usize::from(cell_index - 1);
    let (cx, cy) = (idx / 8, idx % 8);
    let (a, b) = m.cell_polygons(cx, cy);
    for p in [a, b] {
        let v: Vec<Vec3> = p.v.iter().map(|&i| m.vertices[i as usize]).collect();
        if point_in_poly2d(&v, Vec3::new(x, y, 0.0)) {
            return Some(&p.plane);
        }
    }
    None
}

/// Test whether a point lies inside a polygon by requiring every directed edge to place the point
/// on the POSITIVE side.
///
/// The loop runs **backwards** from `num_pts - 1` with `prev` seeded to `vertices[0]`, so the first
/// edge tested is `(v[0], v[n-1])`. The order is only observable through short-circuiting, and
/// short-circuiting is what decides a tie on an edge — which is where a tree either exists or does
/// not.
#[must_use]
pub fn point_in_poly2d(v: &[Vec3], point: Vec3) -> bool {
    let n = v.len();
    if n == 0 {
        return true;
    }
    let mut prev = v[0];
    for i in (0..n).rev() {
        let cur = v[i];
        let a = prev.y - cur.y;
        let b = cur.x - prev.x;
        if a * point.x + b * point.y + (-(a * cur.x) - b * cur.y) > 0.0 {
            return false;
        }
        prev = cur;
    }
    true
}

/// The landblock's scene fetch, with the static-object initialisation's full-detail guard
/// in front of it. Contracts 3.4-3.6.
///
/// Iterates all **81 vertices** `(i, j)` — not the 64 cells — of the full-detail grid, and applies
/// all five filters in the client's order: strict bounds, `on_road`, no building in the cell,
/// [`check_slope`], within-block. Entries with `weenie_obj != 0` are skipped before any of
/// that, because the server owns those.
#[must_use]
pub fn generate_scenery(
    lb: &CellLandblock,
    mesh: &LandblockMesh,
    region: &Region,
    block_x: i32,
    block_y: i32,
    env: &SceneryEnv<'_>,
) -> Vec<PlacedScenery> {
    let mut out = Vec::new();
    // init_static_objs returns immediately unless side_cell_count == 8.
    if mesh.side_cell_count != 8 {
        return out;
    }
    // LINT-OK: a landblock id is two bytes, one per axis, each 0..=0xFE.
    let block_id = LandblockId((u16_of_i32(block_x & 0xFF) << 8) | u16_of_i32(block_y & 0xFF));

    for i in 0..SIDE_VERTEX_COUNT {
        for j in 0..SIDE_VERTEX_COUNT {
            let w = lb.terrain[i * SIDE_VERTEX_COUNT + j];
            let terrain_type = usize::from((w >> 2) & 0x1F);
            let scene_type = usize::from(w >> 11);
            if scene_type >= num_scene_type(region, terrain_type) {
                continue;
            }
            let count = scene_count(region, terrain_type, scene_type);
            if count == 0 {
                continue;
            }
            // LINT-OK: index arithmetic over the 9x9 grid.
            let (gx, gy) = (block_x * 8 + i32_of(i), block_y * 8 + i32_of(j));
            let si = scene_index(gx, gy, count);
            let Some(scene_id) = get_scene(region, terrain_type, scene_type, si) else {
                // `get_scene` returns no id for an out-of-range index and the vertex is skipped.
                // ACE clamps to 0 here, which invents trees the client never places.
                continue;
            };
            let Some(scene) = (env.scenes)(scene_id) else {
                continue;
            };

            for (k, obj) in scene.objects.iter().enumerate() {
                // LINT-OK: k indexes a scene's object list, far inside u32.
                let noise = scenery_hash(gx, gy, salt::FREQ.wrapping_add(u32_of(k)));
                // The client's test is the negated `!(noise < obj.freq)`, not `noise >=
                // obj.freq`. The two differ on NaN, and the negated form is what shipped.
                #[allow(clippy::neg_cmp_op_on_partial_ord)]
                let rejected = !(noise < obj.freq);
                if rejected || obj.weenie_obj != 0 {
                    continue;
                }
                let p = place(obj, gx, gy, k);
                #[allow(clippy::cast_precision_loss)] // i, j <= 8
                let mut p = Vec3::new(p.x + i as f32 * CELL_SIZE, p.y + j as f32 * CELL_SIZE, p.z);
                // Filter 1: strict bounds. ACE uses inclusive bounds and keeps objects the client
                // drops.
                if !(0.0..BLOCK_LENGTH).contains(&p.x) || !(0.0..BLOCK_LENGTH).contains(&p.y) {
                    continue;
                }
                // Filter 2: roads.
                if on_road(&lb.terrain, p.x, p.y) {
                    continue;
                }
                // Filter 3: no scenery in a cell that owns a building.
                let cell_index = outside_cell_index(p.x, p.y);
                if (env.has_building)(cell_index) {
                    continue;
                }
                let Some(plane) = find_terrain_poly(mesh, cell_index, p.x, p.y) else {
                    continue;
                };
                // Filter 4: slope, with inclusive bounds. ACE does not implement this at all.
                if !check_slope(obj, plane.normal.z) {
                    continue;
                }
                let Some(z) = plane_set_height(plane, p.x, p.y) else {
                    continue;
                };
                p.z = z;

                let frame = if obj.align == 0 {
                    get_obj_frame(obj, gx, gy, k, p)
                } else {
                    obj_align(obj, plane, p)
                };
                // Filter 5: within-block. ACE has no equivalent rejection.
                let (centre, radius) =
                    (env.sorting_sphere)(obj.obj_id).unwrap_or((Vec3::ZERO, 0.0));
                let world_centre = crate::math::localtoglobal(&frame, centre);
                if !within_block(world_centre, radius) {
                    continue;
                }
                out.push(PlacedScenery {
                    gfxobj: obj.obj_id,
                    frame,
                    scale: scale_obj(obj, gx, gy, k),
                    cell: block_id.cell(cell_index),
                });
            }
        }
    }
    out
}

/// `ObjectDesc`'s constructor defaults, for a caller building a synthetic scene.
#[must_use]
pub fn default_object_desc(obj_id: DataId) -> ObjectDesc {
    ObjectDesc {
        obj_id,
        base_loc: Frame::new(Vec3::ZERO, Quat::IDENTITY),
        freq: 1.0,
        displace_x: 0.0,
        displace_y: 0.0,
        min_scale: 1.0,
        max_scale: 1.0,
        max_rot: 0.0,
        min_slope: 0.0,
        max_slope: 90.0,
        align: 0,
        orient: 0,
        weenie_obj: 0,
    }
}

#[cfg(test)]
mod tests {
    // Index arithmetic in test fixtures, bounded by the loops that build them.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;

    /// Oracle: an independent Python transcription of the scenery-placement hash, evaluated with
    /// arbitrary-precision integers and masked to 32 bits. The expectations below are that
    /// transcription's output, not this function's.
    #[test]
    fn the_placement_hash_matches_an_independent_transcription() {
        // (gx, gy, salt, the raw 32-bit value)
        const CASES: &[(i32, i32, u32, u32)] = &[
            (0, 0, 0x5B67, 0xF4D7_C05F),
            (1, 0, 0x5B67, 0xB2BB_DCA2),
            (0, 1, 0x5B67, 0x60F2_85E6),
            (1352, 1440, 0x5B67, 0x16DC_1C97),
            (1352, 1440, 0xB2CD, 0x3666_E6AD),
            (1352, 1440, 0xF697, 0x6DF7_4D47),
        ];
        for &(gx, gy, s, raw) in CASES {
            #[allow(clippy::cast_precision_loss)]
            let expect = (raw as f32) * INV_2_32;
            let got = scenery_hash(gx, gy, s);
            assert!(
                (got - expect).abs() < 1e-9,
                "({gx},{gy},{s:#X}) {got} != {expect}"
            );
            assert!((0.0..1.0).contains(&got));
        }
    }

    /// Oracle: the same Python transcription, of the *scene index* hash — which uses a different
    /// first constant (`0x2A7F2B89`) and a different tail (`+0x7F8CDA01`) from the placement hash.
    /// Mixing the two is the single easiest way to move the whole world's trees.
    #[test]
    fn the_scene_index_hash_uses_its_own_constants() {
        const CASES: &[(i32, i32, u32)] = &[
            (0, 0, 0x7F8C_DA01),
            (1, 0, 0x3D70_F644),
            (0, 1, 0xEBA7_9F88),
            (1352, 1440, 0x4B26_1039),
        ];
        for &(gx, gy, raw) in CASES {
            let count = 6usize;
            #[allow(clippy::cast_precision_loss)]
            let expect =
                dereth_primitives::num::floor_to_i32((raw as f32) * INV_2_32 * count as f32)
                    as usize;
            assert_eq!(scene_index(gx, gy, count), expect, "({gx},{gy})");
        }
        // count == 1 never hashes at all.
        assert_eq!(scene_index(1352, 1440, 1), 0);
    }

    /// Oracle: every scenery hash's `x 2^-32` conversion is *unsigned*
    /// reinterpretation of the 32-bit signed product. A signed conversion would produce negatives
    /// for half of all inputs, and `noise < obj.freq` would then accept objects it must reject.
    #[test]
    fn the_hash_is_reinterpreted_unsigned_and_stays_in_zero_to_one() {
        for gx in [0i32, 1, 977, 2039] {
            for gy in [0i32, 3, 1234, 2039] {
                for s in [salt::FREQ, salt::DISPLACE_X, salt::ROTATION, salt::SCALE] {
                    let v = scenery_hash(gx, gy, s);
                    assert!((0.0..1.0).contains(&v), "({gx},{gy},{s:#X}) = {v}");
                }
            }
        }
    }

    /// Oracle: the retail slope check --
    /// `min_slope <= n && ((n < max) != (n == max))`, which simplifies to `n <= max`. Both the
    /// lower and upper slope bounds are therefore inclusive.
    #[test]
    fn check_slope_is_inclusive_at_both_ends() {
        let mut o = default_object_desc(DataId(1));
        o.min_slope = 0.25;
        o.max_slope = 0.75;
        assert!(check_slope(&o, 0.25), "the lower bound is inclusive");
        assert!(check_slope(&o, 0.75), "the upper bound is inclusive too");
        assert!(check_slope(&o, 0.5));
        assert!(!check_slope(&o, 0.2499));
        assert!(!check_slope(&o, 0.7501));
    }

    /// Oracle: the scale step's two branches.
    #[test]
    fn scale_obj_short_circuits_when_the_range_is_a_point() {
        let mut o = default_object_desc(DataId(1));
        o.min_scale = 2.0;
        o.max_scale = 2.0;
        assert_eq!(scale_obj(&o, 5, 7, 0), 2.0);
        o.min_scale = 1.0;
        o.max_scale = 4.0;
        let s = scale_obj(&o, 5, 7, 0);
        assert!((1.0..=4.0).contains(&s), "{s}");
        // pow(4, r) * 1 with r in [0,1) is monotone in r, so the ends are the bounds.
        let r = scenery_hash(5, 7, salt::SCALE);
        assert!((s - math::powf(4.0f32, r)).abs() < 1e-6);
    }

    /// Oracle: [`displaced_xy`]'s quadrant table. All four quadrants must be
    /// reachable and each must be the corresponding rotation of `(x, y)`.
    #[test]
    fn place_applies_the_documented_quadrant_flip() {
        let mut o = default_object_desc(DataId(1));
        o.base_loc = Frame::new(Vec3::new(3.0, 5.0, 0.0), Quat::IDENTITY);
        let mut seen = [false; 4];
        for gx in 0..40i32 {
            for gy in 0..40i32 {
                let p = place(&o, gx, gy, 0);
                let q = unit(
                    (gy as u32)
                        .wrapping_mul(0x6C1A_C587)
                        .wrapping_sub(
                            (gx as u32).wrapping_mul(
                                (gy as u32)
                                    .wrapping_mul(0x6F7B_D965)
                                    .wrapping_add(0x421B_E3BD),
                            ),
                        )
                        .wrapping_sub(0x17FC_EDFD),
                );
                let expect = if q < 0.25 {
                    (3.0, 5.0)
                } else if q < 0.5 {
                    (-5.0, 3.0)
                } else if q < 0.75 {
                    (-3.0, -5.0)
                } else {
                    (5.0, -3.0)
                };
                assert_eq!((p.x, p.y), expect, "({gx},{gy}) q={q}");
                seen[dereth_primitives::num::to_i32(q * 4.0).clamp(0, 3) as usize] = true;
            }
        }
        assert!(
            seen.iter().all(|&b| b),
            "all four quadrants must be reachable"
        );
    }

    /// Oracle: `displace_x`/`displace_y` are applied only when strictly
    /// positive, and the displacement is a fraction of the range, so it stays inside the cell.
    #[test]
    fn displacement_is_gated_on_a_positive_range() {
        let mut o = default_object_desc(DataId(1));
        assert_eq!(
            place(&o, 11, 13, 0).x.abs(),
            0.0,
            "no displacement when displace_x is 0"
        );
        o.displace_x = 24.0;
        o.displace_y = 24.0;
        let p = place(&o, 11, 13, 0);
        assert!(p.x.abs() < 24.0 && p.y.abs() < 24.0, "{p:?}");
    }

    /// Oracle: the bounds check that `obj_within_block` applies to a scenery object. The bounds are
    /// strict at the far edge and inclusive at the near one, exactly as
    /// `r <= p.x && p.x < 192 - r` reads.
    #[test]
    fn within_block_clears_the_edges_by_the_sphere_radius() {
        assert!(within_block(Vec3::new(0.0, 0.0, 0.0), 0.0));
        assert!(!within_block(Vec3::new(192.0, 10.0, 0.0), 0.0));
        assert!(within_block(Vec3::new(191.999, 10.0, 0.0), 0.0));
        assert!(
            !within_block(Vec3::new(1.0, 10.0, 0.0), 2.0),
            "a 2 m sphere at x=1 pokes out"
        );
        assert!(within_block(Vec3::new(2.0, 10.0, 0.0), 2.0));
    }

    /// Oracle: the cell's terrain-polygon lookup and the 2-D point-in-polygon test.
    /// A cell's two triangles must partition its 24 m square: every interior point lands in
    /// exactly one of them under the POSITIVE test, or on their shared edge in both.
    #[test]
    fn the_two_triangles_of_a_cell_cover_it() {
        let svc = 9usize;
        let mut verts = Vec::new();
        for i in 0..svc {
            for j in 0..svc {
                #[allow(clippy::cast_precision_loss)]
                verts.push(Vec3::new(i as f32 * 24.0, j as f32 * 24.0, 0.0));
            }
        }
        for cut in [true, false] {
            let tris = crate::land::mesh::cell_triangles(svc, 0, 0, cut);
            let mut covered = 0;
            for a in 1..24u32 {
                for b in 1..24u32 {
                    let p = Vec3::new(f32::from(a as u16) + 0.5, f32::from(b as u16) + 0.5, 0.0);
                    let hits = tris
                        .iter()
                        .filter(|t| {
                            let v: Vec<Vec3> = t.iter().map(|&i| verts[i as usize]).collect();
                            point_in_poly2d(&v, p)
                        })
                        .count();
                    assert!(hits >= 1, "cut={cut} {p:?} is in neither triangle");
                    covered += 1;
                }
            }
            assert_eq!(covered, 23 * 23);
        }
    }

    /// Oracle: land-cell ids are `1 + cellY + cellX*8` in the low 16
    /// bits. Transposing this mirrors every building test and every scenery cell assignment.
    #[test]
    fn outside_cell_index_is_one_plus_celly_plus_cellx_times_eight() {
        assert_eq!(outside_cell_index(0.0, 0.0), 1);
        assert_eq!(outside_cell_index(25.0, 0.0), 9);
        assert_eq!(outside_cell_index(0.0, 25.0), 2);
        assert_eq!(outside_cell_index(191.0, 191.0), 64);
    }
}
