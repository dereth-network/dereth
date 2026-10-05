//! [`viewcone_check`] and the outdoor vertical-column tests.
//!
//! The view-cone test, the per-column plane limits, the clip height, the corner and block plane
//! checks, the block check, and the landscape draw's own block and cell tests.
//!
//! Object and terrain visibility use the vertical-column tests described below.
//!
//! **All terrain culling is done on vertical columns**, not on triangles: a block or cell is
//! reduced to its `(x, y)` footprint plus the `[min_zval, max_zval]` band, and each of its four
//! corner columns is intersected with every clip plane. All the epsilons are `0.0002`, and the
//! `1000.0` in `get_pt_limit` plus the `+200 / -1` in the landblock's height limits bound the column.

use dereth_primitives::Vec3;

use crate::Plane;
use {
    dereth_terrain::consts::EPSILON, dereth_terrain::consts::OUTSIDE_VAL,
    dereth_terrain::consts::SKY_HEIGHT,
};

/// The view-cone and column tests' outside / straddling / inside answer, shared with physics.
pub use dereth_primitives::shape::Bounding;

/// The object scale -- the **largest** of the three axis
/// scales, which is the single factor `viewcone_check` then applies to both the sphere's centre and
/// its radius.
///
/// The client keeps the scale vector too, for the draw; the cone uses only this
/// scalar, so a non-uniformly scaled part is tested against a sphere sized by its longest axis.
/// That is deliberately conservative — it can only over-estimate the extent — and it is why the
/// cone cannot be given the vector "to be more accurate".
#[must_use]
pub fn object_scale(v: Vec3) -> f32 {
    // The client's own two compares, in its order: start at x, take y if x < y, then take z if the
    // running value is < z.
    let mut s = v.x;
    if v.x < v.y {
        s = v.y;
    }
    if s < v.z {
        s = v.z;
    }
    s
}

/// The viewer's near plane -- the plane
/// [`viewcone_check`] tests **first**, before any of the view polygon's own.
///
/// ```text
/// CY.N = Yaxis                                  // the camera frame's forward axis
/// CY.d = -(origin · Yaxis) - znear
/// ```
///
/// so `CY.dot_point(P)` is `(P - origin) · forward - znear`: the signed distance in front of the
/// near clip plane. It is the **only** plane of the cone that is not through the eye.
///
/// **Its OUTSIDE arm is unfalsifiable in this build, and that is a reading rather than a gap.**
/// Mutating `viewcone_check`'s `if d < -radius` on this plane so it can never fire
/// **survives** an isolated re-run against a driven scene. The reason is structural: for a view
/// polygon spanning a field of view under 180° every corner ray has a positive forward component,
/// so the edge planes already imply `forward > 0` and reject the whole half-space behind the eye on
/// their own. What `CY` adds that they cannot is the `znear` **offset** — and a sphere whose centre
/// is more than its own radius behind a plane 0.1 m in front of the camera is a sphere the camera
/// is inside, which no station here can construct. The arithmetic is still pinned (mutating
/// `- znear` to `+ znear` reddens `tests::the_near_plane_sits_znear_in_front_of_the_eye`); it is
/// the *client consuming its OUTSIDE answer* that nothing can currently observe. Kept, because it
/// is the client's, and said here rather than dressed up as a kill.
#[must_use]
pub fn viewer_near_plane(viewpoint: Vec3, forward: Vec3, znear: f32) -> Plane {
    Plane {
        normal: forward,
        d: -viewpoint.dot(forward) - znear,
    }
}

/// The per-object frustum test.
///
/// A plain sphere-versus-half-space test against `1 + n` planes (`n` the view polygon's vertex count): the near plane first
/// ([`viewer_near_plane`]), then every edge plane of the current view polygon
/// ([`crate::cells::clip::ViewPoly::planes`]). Any plane with `d < -r` is an immediate OUTSIDE;
/// any with `d <= r` makes the result PARTIALLY_INSIDE.
///
/// **The two boundaries are not the same, and the second is inclusive**: it is `<=`, not `<`.
/// In the retail client the OUTSIDE test fires only on a strict *less*; the partial test fires on
/// *less* **and** *equal*. A sphere exactly tangent
/// to an edge plane is PARTIALLY_INSIDE, not ENTIRELY_INSIDE.
///
/// The client also stores the object's local centre and radius as a side effect, which is
/// what picking then reads; that bookkeeping belongs to the caller here.
#[must_use]
pub fn viewcone_check(center: Vec3, radius: f32, near: &Plane, edges: &[Plane]) -> Bounding {
    let mut partial = false;
    let d = near.dot_point(center);
    if d < -radius {
        return Bounding::Outside;
    }
    if d <= radius {
        partial = true;
    }
    for p in edges {
        let d = p.dot_point(center);
        if d < -radius {
            return Bounding::Outside;
        }
        if d <= radius {
            partial = true;
        }
    }
    if partial {
        Bounding::PartiallyInside
    } else {
        Bounding::EntirelyInside
    }
}

/// Where the plane crosses the vertical line
/// through `(x, y)`, encoded as:
///
/// * [`OUTSIDE_VAL`] (1001.0) — the column is entirely on the wrong side;
/// * a **positive** `b` — inside only where `z < b`;
/// * a **negative** `-L` — inside only where `z > L`;
/// * `0` — the column is unbounded on that plane.
///
/// ```text
/// if N.z > 0.0002:   z = -(x*N.x + y*N.y + d)/N.z ; if z >= 1000 -> OUTSIDE_VAL ; if z > 0 -> -z ; else 0
/// if N.z < -0.0002:  z = -(x*N.x + y*N.y + d)/N.z ; if z <= 0 -> OUTSIDE_VAL ; if z < 1000 -> z ; else 0
/// otherwise (vertical plane): which_side((x,y,0), 0.0002) == NEGATIVE -> OUTSIDE_VAL, else 0
/// ```
#[must_use]
pub fn get_pt_limit(x: f32, y: f32, p: &Plane) -> f32 {
    let n = p.normal;
    if n.z > EPSILON {
        let z = -(x * n.x + y * n.y + p.d) / n.z;
        if z >= SKY_HEIGHT {
            return OUTSIDE_VAL;
        }
        return if z > 0.0 { -z } else { 0.0 };
    }
    if n.z < -EPSILON {
        let z = -(x * n.x + y * n.y + p.d) / n.z;
        if z <= 0.0 {
            return OUTSIDE_VAL;
        }
        return if z < SKY_HEIGHT { z } else { 0.0 };
    }
    // A vertical plane reduces to a 2-D side test with the same epsilon.
    let d = x * n.x + y * n.y + p.d;
    if d < -EPSILON {
        OUTSIDE_VAL
    } else {
        0.0
    }
}

/// The corner column check against one plane.
///
/// | bound | result |
/// |---|---|
/// | `== OUTSIDE_VAL` | OUTSIDE |
/// | `> 0` (upper limit `b`) | `b >= maxZ` → ENTIRELY_INSIDE; `b <= minZ` → OUTSIDE; else PARTIAL |
/// | `< 0` (lower limit `L = -bound`) | `L < minZ` → ENTIRELY_INSIDE; `maxZ <= L` → OUTSIDE; else PARTIAL |
/// | `== 0` | ENTIRELY_INSIDE |
#[must_use]
pub fn corner_plane_check(bound: f32, min_z: f32, max_z: f32) -> Bounding {
    if bound == OUTSIDE_VAL {
        return Bounding::Outside;
    }
    if bound > 0.0 {
        return if bound >= max_z {
            Bounding::EntirelyInside
        } else if bound <= min_z {
            Bounding::Outside
        } else {
            Bounding::PartiallyInside
        };
    }
    if bound < 0.0 {
        let l = -bound;
        return if l < min_z {
            Bounding::EntirelyInside
        } else if max_z <= l {
            Bounding::Outside
        } else {
            Bounding::PartiallyInside
        };
    }
    Bounding::EntirelyInside
}

/// Combine the four corner columns of
/// one square against **one** plane: all four OUTSIDE → OUTSIDE, all four ENTIRELY_INSIDE →
/// ENTIRELY_INSIDE, otherwise PARTIALLY_INSIDE.
#[must_use]
pub fn block_plane_check(bounds: [f32; 4], min_z: f32, max_z: f32) -> Bounding {
    let r = bounds.map(|b| corner_plane_check(b, min_z, max_z));
    if r.iter().all(|&x| x == Bounding::Outside) {
        Bounding::Outside
    } else if r.iter().all(|&x| x == Bounding::EntirelyInside) {
        Bounding::EntirelyInside
    } else {
        Bounding::PartiallyInside
    }
}

/// Run [`block_plane_check`] for **every**
/// plane. Any plane returning OUTSIDE makes the whole square OUTSIDE; the result is
/// ENTIRELY_INSIDE only if every plane said so.
///
/// `corners[p]` is the four corner bounds against plane `p`, i.e. what
/// the clip height produces for the square's four corner columns.
#[must_use]
pub fn block_check(corners: &[[f32; 4]], min_z: f32, max_z: f32) -> Bounding {
    let mut result = Bounding::EntirelyInside;
    for c in corners {
        match block_plane_check(*c, min_z, max_z) {
            Bounding::Outside => return Bounding::Outside,
            Bounding::PartiallyInside => result = Bounding::PartiallyInside,
            Bounding::EntirelyInside => {}
        }
    }
    result
}

/// The clip height -- `bound[0]` from the near plane, then
/// `bound[1 ..= n]` from the current view polygon's `n` edge planes.
#[must_use]
pub fn get_clip_height(x: f32, y: f32, near: &Plane, edges: &[Plane]) -> Vec<f32> {
    let mut out = Vec::with_capacity(1 + edges.len());
    out.push(get_pt_limit(x, y, near));
    for p in edges {
        out.push(get_pt_limit(x, y, p));
    }
    out
}

/// The landscape cell check's two short-circuits.
///
/// * `side_cell_count != 8` (an LOD block) → **every** cell gets `PARTIALLY_INSIDE`
///   unconditionally: distant blocks are never cell-culled.
/// * block `in_view == ENTIRELY_INSIDE` → every cell gets `ENTIRELY_INSIDE`.
///
/// Otherwise the caller computes clip heights at 24-unit spacing over the block's `(N+1)²` cell
/// corners and runs [`block_check`] per cell, **using the block's** `min_zval` / `max_zval` rather
/// than a per-cell z band. Cells already marked non-OUTSIDE by an earlier view are skipped, which
/// is what makes the loop OR results across views.
#[must_use]
pub fn landcell_check_shortcut(side_cell_count: u8, block: Bounding) -> Option<Bounding> {
    if side_cell_count != 8 {
        return Some(Bounding::PartiallyInside);
    }
    if block == Bounding::EntirelyInside {
        return Some(Bounding::EntirelyInside);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(nx: f32, ny: f32, nz: f32, d: f32) -> Plane {
        Plane {
            normal: Vec3::new(nx, ny, nz),
            d,
        }
    }

    /// Oracle: the view-cone test -- a sphere-versus-half-space test against the near
    /// plane and every edge plane, with the `d < -r` / `d < r` thresholds.
    #[test]
    fn viewcone_check_classifies_against_every_plane() {
        // A near plane at y = 0 looking north, plus two side planes making a wedge.
        let near = plane(0.0, 1.0, 0.0, 0.0);
        let edges = [plane(1.0, 0.0, 0.0, 0.0), plane(-1.0, 0.0, 0.0, 100.0)];
        // Well inside all three.
        assert_eq!(
            viewcone_check(Vec3::new(50.0, 50.0, 0.0), 1.0, &near, &edges),
            Bounding::EntirelyInside
        );
        // Straddling the near plane.
        assert_eq!(
            viewcone_check(Vec3::new(50.0, 0.5, 0.0), 1.0, &near, &edges),
            Bounding::PartiallyInside
        );
        // Behind the near plane by more than its radius.
        assert_eq!(
            viewcone_check(Vec3::new(50.0, -5.0, 0.0), 1.0, &near, &edges),
            Bounding::Outside
        );
        // Past a side plane.
        assert_eq!(
            viewcone_check(Vec3::new(-10.0, 50.0, 0.0), 1.0, &near, &edges),
            Bounding::Outside
        );
        // An empty edge list leaves only the near plane, which is the full-screen view.
        assert_eq!(
            viewcone_check(Vec3::new(0.0, 500.0, 0.0), 1.0, &near, &[]),
            Bounding::EntirelyInside
        );
    }

    /// The object scale is the largest axis.
    #[test]
    fn the_object_scale_is_the_largest_axis() {
        assert_eq!(object_scale(Vec3::new(1.0, 1.0, 1.0)), 1.0);
        assert_eq!(object_scale(Vec3::new(3.0, 1.0, 2.0)), 3.0);
        assert_eq!(object_scale(Vec3::new(1.0, 3.0, 2.0)), 3.0);
        assert_eq!(object_scale(Vec3::new(1.0, 2.0, 3.0)), 3.0);
        // Not the mean, not the product, and not the x component.
        assert_eq!(object_scale(Vec3::new(0.5, 0.25, 0.25)), 0.5);
    }

    /// Oracle: the retail boundaries -- the OUTSIDE test is strict and the partial test is
    /// inclusive. The two boundaries differ and only one is inclusive.
    ///
    /// This is an easy comparison to get wrong, and the pins are literal
    /// distances rather than symbols, so a transcription that flips either comparison fails here
    /// even though every other assertion in this file stays green.
    #[test]
    fn the_partial_boundary_is_inclusive_and_the_outside_boundary_is_not() {
        // One plane, `x >= 0`. A unit sphere at x = +1 touches it from the inside.
        let near = plane(1.0, 0.0, 0.0, 0.0);
        // d == +r exactly: the inclusive partial test counts equal, so the flag is set.
        assert_eq!(
            viewcone_check(Vec3::new(1.0, 0.0, 0.0), 1.0, &near, &[]),
            Bounding::PartiallyInside,
            "a sphere tangent to a plane from inside is PARTIALLY_INSIDE, not ENTIRELY_INSIDE"
        );
        // A hair further in and nothing touches: ENTIRELY_INSIDE.
        assert_eq!(
            viewcone_check(Vec3::new(1.001, 0.0, 0.0), 1.0, &near, &[]),
            Bounding::EntirelyInside
        );
        // d == -r exactly: the strict OUTSIDE test does not fire, so the OUTSIDE return is
        // **skipped**. A sphere tangent from the far side is still PARTIAL.
        assert_eq!(
            viewcone_check(Vec3::new(-1.0, 0.0, 0.0), 1.0, &near, &[]),
            Bounding::PartiallyInside,
            "the OUTSIDE test is strict, so d == -r survives"
        );
        assert_eq!(
            viewcone_check(Vec3::new(-1.001, 0.0, 0.0), 1.0, &near, &[]),
            Bounding::Outside
        );
        // And the same pair on an *edge* plane rather than the near plane, because the loop is a
        // second copy of the two comparisons and could differ.
        let edge = plane(0.0, 1.0, 0.0, 0.0);
        assert_eq!(
            viewcone_check(Vec3::new(50.0, 1.0, 0.0), 1.0, &near, &[edge]),
            Bounding::PartiallyInside
        );
        assert_eq!(
            viewcone_check(Vec3::new(50.0, -1.0, 0.0), 1.0, &near, &[edge]),
            Bounding::PartiallyInside
        );
        assert_eq!(
            viewcone_check(Vec3::new(50.0, -1.001, 0.0), 1.0, &near, &[edge]),
            Bounding::Outside
        );
    }

    /// Oracle: the viewpoint update -- `CY.N = Yaxis` and
    /// `CY.d = -(origin · Yaxis) - znear`, so the plane sits `znear` **in front of** the eye and
    /// faces the way the camera looks.
    #[test]
    fn the_near_plane_sits_znear_in_front_of_the_eye() {
        let eye = Vec3::new(10.0, 20.0, 30.0);
        let fwd = Vec3::new(0.0, 1.0, 0.0);
        let p = viewer_near_plane(eye, fwd, 0.1);
        assert_eq!(p.normal, fwd);
        // The eye itself is behind the plane by exactly znear.
        assert!(
            (p.dot_point(eye) + 0.1).abs() < 1.0e-5,
            "{}",
            p.dot_point(eye)
        );
        // A point one metre ahead is 0.9 in front of it.
        assert!((p.dot_point(Vec3::new(10.0, 21.0, 30.0)) - 0.9).abs() < 1.0e-5);
        // A point behind the camera is behind the plane.
        assert!(p.dot_point(Vec3::new(10.0, 15.0, 30.0)) < 0.0);
    }

    /// Oracle: the four encodings of the plane limit, evaluated by hand from the recovered rules.
    #[test]
    fn get_pt_limit_encodes_the_four_documented_cases() {
        // An upward-facing plane at z = 20: crossing z is 20 > 0, so the answer is -20, a lower
        // limit -- the column is inside only above 20.
        let up = plane(0.0, 0.0, 1.0, -20.0);
        assert_eq!(get_pt_limit(0.0, 0.0, &up), -20.0);
        // The same plane at z = -5: the crossing is at -5, which is not > 0, so 0 (unbounded).
        let low = plane(0.0, 0.0, 1.0, 5.0);
        assert_eq!(get_pt_limit(0.0, 0.0, &low), 0.0);
        // Beyond the 1000 cut-off: outside_val.
        let sky = plane(0.0, 0.0, 1.0, -2000.0);
        assert_eq!(get_pt_limit(0.0, 0.0, &sky), OUTSIDE_VAL);
        // A downward-facing plane at z = 20: an upper limit of 20.
        let down = plane(0.0, 0.0, -1.0, 20.0);
        assert_eq!(get_pt_limit(0.0, 0.0, &down), 20.0);
        // A downward plane crossing at or below 0 is outside_val.
        let under = plane(0.0, 0.0, -1.0, -5.0);
        assert_eq!(get_pt_limit(0.0, 0.0, &under), OUTSIDE_VAL);
        // A vertical plane reduces to the 2-D side test.
        let vert = plane(1.0, 0.0, 0.0, 0.0);
        assert_eq!(
            get_pt_limit(5.0, 0.0, &vert),
            0.0,
            "on the positive side: unbounded"
        );
        assert_eq!(
            get_pt_limit(-5.0, 0.0, &vert),
            OUTSIDE_VAL,
            "on the negative side: outside"
        );
    }

    /// Oracle: the corner check's table, row by row.
    #[test]
    fn corner_plane_check_matches_the_table() {
        assert_eq!(
            corner_plane_check(OUTSIDE_VAL, 0.0, 100.0),
            Bounding::Outside
        );
        assert_eq!(
            corner_plane_check(0.0, 0.0, 100.0),
            Bounding::EntirelyInside
        );
        // Upper limit b.
        assert_eq!(
            corner_plane_check(150.0, 0.0, 100.0),
            Bounding::EntirelyInside
        );
        assert_eq!(
            corner_plane_check(50.0, 0.0, 100.0),
            Bounding::PartiallyInside
        );
        assert_eq!(corner_plane_check(0.0001, 10.0, 100.0), Bounding::Outside);
        // Lower limit L = -bound.
        assert_eq!(
            corner_plane_check(-5.0, 10.0, 100.0),
            Bounding::EntirelyInside
        );
        assert_eq!(
            corner_plane_check(-50.0, 10.0, 100.0),
            Bounding::PartiallyInside
        );
        assert_eq!(corner_plane_check(-150.0, 10.0, 100.0), Bounding::Outside);
    }

    /// Oracle: the block plane check and the block check -- a square is OUTSIDE
    /// when **any** plane rejects it, and ENTIRELY_INSIDE only when **every** plane accepts it
    /// wholly.
    #[test]
    fn block_check_needs_every_plane_to_agree() {
        let all_in = [0.0f32; 4];
        let all_out = [OUTSIDE_VAL; 4];
        let mixed = [0.0, OUTSIDE_VAL, 0.0, 0.0];
        assert_eq!(
            block_plane_check(all_in, 0.0, 10.0),
            Bounding::EntirelyInside
        );
        assert_eq!(block_plane_check(all_out, 0.0, 10.0), Bounding::Outside);
        assert_eq!(
            block_plane_check(mixed, 0.0, 10.0),
            Bounding::PartiallyInside
        );
        assert_eq!(
            block_check(&[all_in, all_in], 0.0, 10.0),
            Bounding::EntirelyInside
        );
        assert_eq!(
            block_check(&[all_in, mixed], 0.0, 10.0),
            Bounding::PartiallyInside
        );
        assert_eq!(
            block_check(&[all_in, all_out], 0.0, 10.0),
            Bounding::Outside
        );
        assert_eq!(
            block_check(&[mixed, all_out], 0.0, 10.0),
            Bounding::Outside,
            "one rejecting plane is enough"
        );
        // No planes at all is the degenerate full-screen case.
        assert_eq!(block_check(&[], 0.0, 10.0), Bounding::EntirelyInside);
    }

    /// A reduced-detail block is never cell-culled, and a wholly visible block
    /// bypasses the individual cell tests.
    #[test]
    fn landcell_check_short_circuits_for_lod_blocks_and_whole_blocks() {
        for scc in [1u8, 2, 4] {
            assert_eq!(
                landcell_check_shortcut(scc, Bounding::Outside),
                Some(Bounding::PartiallyInside),
                "an LOD block's cells are always partially inside"
            );
        }
        assert_eq!(
            landcell_check_shortcut(8, Bounding::EntirelyInside),
            Some(Bounding::EntirelyInside)
        );
        assert_eq!(
            landcell_check_shortcut(8, Bounding::PartiallyInside),
            None,
            "a partially visible full-detail block needs the per-cell work"
        );
    }

    /// Oracle: outdoor culling computes clip heights once per row of block/cell corners and reuses
    /// them for the row above and below, so a
    /// `mid_width = 11` window costs `12 x 12` calls per view rather than `4 x 121`. The corner
    /// grid is `(N+1)²` and shared between adjacent squares; this checks that sharing is sound by
    /// asserting a corner column's clip heights depend only on `(x, y)`.
    #[test]
    fn clip_heights_are_a_function_of_the_corner_column_alone() {
        let near = plane(0.0, 1.0, 0.0, 0.0);
        let edges = [plane(0.0, 0.0, -1.0, 60.0)];
        let a = get_clip_height(24.0, 48.0, &near, &edges);
        let b = get_clip_height(24.0, 48.0, &near, &edges);
        assert_eq!(a, b);
        assert_eq!(a.len(), 2, "one entry for the near plane plus one per edge");
        assert_eq!(a[1], 60.0, "the downward plane gives an upper limit of 60");
    }
}
