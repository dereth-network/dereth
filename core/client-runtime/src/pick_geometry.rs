//! The pick's geometry: the ray, the sphere and polygon tests and the sweep over drawn parts.
//!
//! The smart box's object find, the selection-cursor setter, the pick ray, the selection-ray
//! test and the two selection accessors.
//!
//! Picking preserves the frame-local selection behavior described below.
//!
//! Picking happens **during the normal frame**, not in a separate pass, and the pick is always for
//! exactly one frame. Three details decide the answer and all three are easy to "improve" away:
//!
//! * a **polygon hit always beats a sphere hit**, even a farther one;
//! * only the **first** polygon that reports a hit is considered per mesh — the loop `break`s — so a
//!   self-overlapping mesh can report a farther polygon than the true nearest one;
//! * there is **no depth occlusion**: the polygon loop ignores the mesh subsets, `NoDraw` and
//!   translucency, so a fully transparent part is still pickable. Terrain and objects whose
//!   `physobj.id` is 0 (scenery, particle hosts, buildings) are excluded because
//!   the current-object check is false for them.

use dereth_primitives::num::math;
use dereth_primitives::{Frame, ObjectId, Vec3};

use dereth_physics::globals::EPSILON;
use dereth_physics::math::{globaltolocal, globaltolocalvec, l2g, localtoglobalvec, V3};
use dereth_primitives::shape::Plane;

/// Pick rays extend 10,000 world units.
pub const PICK_RAY_LENGTH: f32 = 10_000.0;

/// The pick ray. `length` is **10000.0**, set by [`gfx_obj_under_selection_ray`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub pt: Vec3,
    /// Already divided by `|gfxobj_scale|` when the ray was built for a scaled part.
    pub dir: Vec3,
    pub length: f32,
}

impl Ray {
    /// A ray with the client's fixed length.
    #[must_use]
    pub fn new(pt: Vec3, dir: Vec3) -> Self {
        Self {
            pt,
            dir,
            length: PICK_RAY_LENGTH,
        }
    }

    /// [`gfx_obj_under_selection_ray`]'s scaling: the direction is divided by **`|gfxobj_scale|`**
    /// — the length of the scale vector — and not by each component, so a non-uniform scale gives a
    /// slightly wrong ray. That is faithful, not a bug.
    #[must_use]
    pub fn for_part(pt: Vec3, dir: Vec3, gfxobj_scale: Vec3) -> Self {
        let s = gfxobj_scale.magnitude();
        let d = if s > 0.0 { dir.mul(1.0 / s) } else { dir };
        Self::new(pt, d)
    }
}

/// Build a pick ray with the **legacy scalar unprojection**, not
/// `D3DXMatrixPerspectiveFovLH`.
///
/// ```text
/// sx = px * xinvscale - tx
/// sy = py * yinvscale - ty
/// d  = x_axis * sx + y_axis * vdst - z_axis * sy
/// selection_ray = normalize(d)
/// ```
///
/// with `xinvscale = yinvscale = 1/scale` (1/4000), `tx = (viewport.0 - 1)*0.5*xinvscale`,
/// `ty = (viewport.1 - 1)*0.5*yinvscale` and `vdst = ty / tan(fov/2)`. The three axes are
/// the viewer frame rows (+y forward, +x right, +z up), so the ray already points into the world.
///
/// It has no aspect-ratio term; the pick ray matches the rendered image only because the client
/// derives `vdst` from the same field of view that the D3D projection uses. This relationship is
/// inferred from the shared input.
#[must_use]
pub fn pick_ray(
    px: f32,
    py: f32,
    viewport: (u32, u32),
    scale: f32,
    fov_radians: f32,
    axes: (Vec3, Vec3, Vec3),
) -> Vec3 {
    let inv = 1.0 / scale;
    #[allow(clippy::cast_precision_loss)] // viewport dimensions are small integers
    let tx = (viewport.0 as f32 - 1.0) * 0.5 * inv;
    #[allow(clippy::cast_precision_loss)]
    let ty = (viewport.1 as f32 - 1.0) * 0.5 * inv;
    let vdst = ty / math::tanf(fov_radians * 0.5);
    let sx = px * inv - tx;
    let sy = py * inv - ty;
    let (xa, ya, za) = axes;
    let mut d = xa.mul(sx).add(ya.mul(vdst)).sub(za.mul(sy));
    d.normalize_check_small();
    d
}

/// Pick state, cleared by the selection-cursor setter and read at the end of the frame's no-blit draw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseSelectData {
    pub found_polygon: bool,
    pub closest_polygon: f64,
    pub polygon_id: ObjectId,
    pub polygon_index: i32,
    pub found_sphere: bool,
    pub closest_sphere: f64,
    pub sphere_id: ObjectId,
    pub sphere_index: i32,
}

impl Default for MouseSelectData {
    fn default() -> Self {
        Self {
            found_polygon: false,
            closest_polygon: f64::MAX,
            polygon_id: ObjectId(0),
            polygon_index: -1,
            found_sphere: false,
            closest_sphere: f64::MAX,
            sphere_id: ObjectId(0),
            sphere_index: -1,
        }
    }
}

impl MouseSelectData {
    /// The two mouse-selection accessors:
    ///
    /// ```text
    /// if found_polygon: return polygon_id / polygon_index
    /// return found_sphere ? sphere_id / sphere_index : 0
    /// ```
    ///
    /// So a **polygon hit always beats a sphere hit**, even a farther one.
    #[must_use]
    pub fn result(&self) -> (ObjectId, i32) {
        if self.found_polygon {
            (self.polygon_id, self.polygon_index)
        } else if self.found_sphere {
            (self.sphere_id, self.sphere_index)
        } else {
            (ObjectId(0), -1)
        }
    }
}

/// A candidate mesh, as [`gfx_obj_under_selection_ray`] receives it.
#[derive(Debug, Clone)]
pub struct PickCandidate<'a> {
    /// Physics-object id for this part. **Zero means not pickable**: the current-object check is
    /// `(physobj present and physobj.id != 0) || creature_mode`, which is what excludes scenery,
    /// particle hosts and buildings.
    pub physobj_id: ObjectId,
    pub physobj_index: i32,
    /// Drawing sphere in part space.
    pub drawing_sphere: (Vec3, f32),
    /// The raw polygon array — **not** the mesh subsets, and not filtered by
    /// `NoDraw` or translucency.
    pub polygons: &'a [PickPolygon],
    /// Whether polygon testing was enabled for this pick.
    pub check_polys: bool,
}

/// One graphics-object polygon and the plane derived from its vertices.
///
/// Picking checks `sides_type` first, so a single-sided polygon cannot be picked
/// from behind. The dat does not store the plane; the loader computes it and the
/// caller supplies that result.
#[derive(Debug, Clone, PartialEq)]
pub struct PickPolygon {
    pub plane: Plane,
    pub vertices: Vec<Vec3>,
    /// Polygon sidedness. **0 is single-sided**: `dir . N > 0` back-face-culls the ray.
    pub sides_type: i32,
}

/// The sphere-versus-ray intersection.
///
/// **The direction is not a unit vector**, and a test that assumes one fails:
/// [`gfx_obj_under_selection_ray`] divides the direction by `|gfxobj_scale|`, which for an
/// ordinary unscaled part is `|(1,1,1)| = 1.732`, so a unit-direction test fails *every*
/// candidate mesh and nothing in the world is pickable. The client's own arithmetic carries
/// `dd = dir . dir` through:
///
/// ```text
/// m  = pt - center
/// dd = dir . dir
/// b  = -(m . dir)
/// c  = |m|^2 - r^2
/// if !(c > 0) return false                 // the origin must be strictly OUTSIDE the sphere
/// disc = b*b - c*dd
/// if dd < 0.0002 or disc < 0 return false
/// s = sqrt(disc)
/// t = (s < b) ? (b - s)/dd : (s + b)/dd
/// ```
///
/// Three consequences, all faithful and none of them what a textbook ray/sphere test does:
///
/// * a ray whose origin is **inside or exactly on** the sphere returns false, so an object whose
///   drawing sphere swallows the camera is not pickable;
/// * there is **no test against `Ray::length`** — neither here nor in
///   [`polygon_hits_ray`]. On the pick path `length` is written by
///   [`gfx_obj_under_selection_ray`] and
///   read by nobody; the client's only reader belongs to a different ray-casting path that is *not*
///   used by mouse picking;
/// * there is no "the ray points away" early-out either: when the near root is behind the origin
///   the **far** root is returned, positive, so a sphere behind the viewer can answer with a `t`.
///   In the client that never decides anything, because the pick is reached only for a mesh that
///   already passed the view-cone check.
#[must_use]
pub fn sphere_intersects_ray(center: Vec3, radius: f32, ray: &Ray) -> Option<f32> {
    let m = ray.pt.sub(center);
    let dd = ray.dir.dot(ray.dir);
    let b = -m.dot(ray.dir);
    let c = m.mag2() - radius * radius;
    // The gate is `c > 0.0`, and a NaN `c` fails it too. `c <= 0.0` would let a NaN through; this does not.
    if !matches!(c.partial_cmp(&0.0), Some(std::cmp::Ordering::Greater)) {
        return None;
    }
    let disc = b * b - c * dd;
    if dd < EPSILON || disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    Some(if s < b { (b - s) / dd } else { (s + b) / dd })
}

/// The polygon-versus-ray test -- the back-face gate, the plane's time of intersection, then
/// the point-in-polygon test.
///
/// ```text
/// if sides_type == 0 and dir . N > 0: return 0          // single-sided, seen from behind
/// if |dir . N| < 0.0002: return 0
/// t = -(pt . N + d) / (dir . N)
/// if t < 0: return 0                                     // and NO upper bound
/// p = pt + dir * t
/// return point_in_polygon(p)
/// ```
///
/// There is no `t > ray.length` rejection, because the client does not do one (see
/// [`sphere_intersects_ray`]), and the `sides_type` gate is what keeps a single-sided polygon
/// from being pickable through its own back face.
///
/// `point_in_polygon` is the 3-D edge loop with `prev` seeded to the **last** vertex: the point must
/// be on the inner side of `N x (cur - prev)` for every edge, and the comparison is `< 0.0`, so a
/// point exactly on an edge is inside.
#[must_use]
pub fn polygon_hits_ray(poly: &PickPolygon, ray: &Ray) -> Option<f32> {
    let n = poly.plane.normal;
    let denom = n.dot(ray.dir);
    // `if ((sides_type == 0) && (0.0 < dir . N)) return 0;`
    if poly.sides_type == 0 && denom > 0.0 {
        return None;
    }
    if denom.abs() < EPSILON {
        return None;
    }
    let t = -(poly.plane.dot_point(ray.pt)) / denom;
    if t < 0.0 {
        return None;
    }
    let p = ray.pt.add(ray.dir.mul(t));
    let verts = &poly.vertices;
    let len = verts.len();
    for i in 0..len {
        let (prev, cur) = (verts[(i + len - 1) % len], verts[i]);
        let e = cur.sub(prev);
        let inward = n.cross(e);
        if p.sub(prev).dot(inward) < 0.0 {
            return None;
        }
    }
    Some(t)
}

/// Test one graphics-object candidate against the selection ray.
///
/// Returns false when the candidate is not pickable or the ray misses its drawing sphere. The
/// polygon loop takes the **first** hit and breaks, which is faithful and observable.
pub fn gfx_obj_under_selection_ray(
    data: &mut MouseSelectData,
    ray: &Ray,
    c: &PickCandidate<'_>,
) -> bool {
    // The current-object check: parts belonging to id-less objects are not pickable.
    if c.physobj_id.0 == 0 {
        return false;
    }
    let Some(t) = sphere_intersects_ray(c.drawing_sphere.0, c.drawing_sphere.1, ray) else {
        return false;
    };
    let t = f64::from(t);
    if data.found_polygon && t > data.closest_polygon {
        return false;
    }
    if !data.found_sphere || t < data.closest_sphere {
        data.found_sphere = true;
        data.closest_sphere = t;
        data.sphere_id = c.physobj_id;
        data.sphere_index = c.physobj_index;
    }
    if c.check_polys {
        for poly in c.polygons {
            if let Some(tp) = polygon_hits_ray(poly, ray) {
                let tp = f64::from(tp);
                if !data.found_polygon || tp < data.closest_polygon {
                    data.found_polygon = true;
                    data.closest_polygon = tp;
                    data.polygon_id = c.physobj_id;
                    data.polygon_index = c.physobj_index;
                }
                break; // the FIRST hit polygon only
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------------------------
// The two halves that make the test above reachable
//
// `gfx_obj_under_selection_ray` works in **part space** — the viewer point and a direction
// transformed through the part's inverse rotation. The two halves below build those: ray
// construction, and the loop that offers every drawn mesh to the test.
// ---------------------------------------------------------------------------------------------

/// Scale constant used to divide the window point.
///
/// It cancels out of the answer (`tx`/`ty` carry the same factor), but it is the client's and is
/// named rather than folded away, because `pick_ray`'s signature takes it.
pub const RENDER_SCALE: f32 = 4000.0;

/// The viewer frame's three rows — the right, forward and up axes — as the picker reads them
/// from the frame's local-to-global matrix.
#[must_use]
pub fn viewer_axes(viewer: &Frame) -> (Vec3, Vec3, Vec3) {
    let m = l2g(viewer.rotation);
    (
        localtoglobalvec(m, Vec3::new(1.0, 0.0, 0.0)),
        localtoglobalvec(m, Vec3::new(0.0, 1.0, 0.0)),
        localtoglobalvec(m, Vec3::new(0.0, 0.0, 1.0)),
    )
}

/// The viewpoint update's picking line:
/// when a selection check is pending, `selection_ray = pick_ray(x, y)` at the selection point.
///
/// `px`/`py` are **viewport-relative**, after subtracting the viewport origin. The answer is a unit direction in the same
/// (viewer-cell-relative) space the viewer frame is expressed in.
#[must_use]
pub fn selection_ray(
    viewer: &Frame,
    px: f32,
    py: f32,
    viewport: (u32, u32),
    fov_radians: f32,
) -> Vec3 {
    pick_ray(
        px,
        py,
        viewport,
        RENDER_SCALE,
        fov_radians,
        viewer_axes(viewer),
    )
}

/// One drawn part, offered to the sweep in **world** space.
///
/// This is what the draw loop has in hand when it draws a mesh: the part's
/// own frame, its `gfxobj_scale`, and the graphics object it draws. `check_polys` controls polygon
/// testing and is always set by object finding; the sphere-only path may therefore be dead.
#[derive(Debug, Clone)]
pub struct PickPart<'a> {
    /// Physics-object id for this part. **Zero is not pickable.**
    pub physobj_id: ObjectId,
    /// `part.physobj_index`.
    pub physobj_index: i32,
    /// Part frame.
    pub frame: Frame,
    /// Part graphics scale.
    pub gfxobj_scale: Vec3,
    /// Drawing sphere in the part's own unscaled space.
    pub drawing_sphere: (Vec3, f32),
    /// Polygons in the same space. Not the mesh subsets; see the module note.
    pub polygons: &'a [PickPolygon],
    pub check_polys: bool,
}

/// The loop's picking half: every part that would be drawn
/// is offered to [`gfx_obj_under_selection_ray`] once, in its own space.
///
/// `viewer` is the eye position and `ray` is the unit direction
/// [`selection_ray`] produced, both in world space. Per part the client has already made two
/// transform-stack calls: one that pushes the part's position as the current frame, and one (with
/// no position) that recomputes the eye in that frame's space, divided by the object scale. They
/// are exactly the pair of transforms below:
///
/// ```text
/// dir = transform_world_vector_to_part_space(part.pos, selection_ray)
/// pt  = current_frame.viewer.viewpoint          // the eye in the part's own space
/// ray = Ray{ pt, dir / |gfxobj_scale|, 10000.0 }
/// ```
///
/// **There is no frustum test here.** In the client the pick is reached only for a mesh that
/// passed the view-cone check, so an object behind the camera is never a candidate; the ray's own
/// `t >= 0` gate is what stands in for that. Consequently, only objects that are actually drawn can
/// be picked.
///
/// Returns the accumulated [`MouseSelectData`]; [`MouseSelectData::result`] reads the answer out
/// the way the smart box's draw does.
#[must_use]
pub fn find_object(viewer: Vec3, ray: Vec3, parts: &[PickPart<'_>]) -> MouseSelectData {
    let mut data = MouseSelectData::default();
    for p in parts {
        let m = l2g(p.frame.rotation);
        let dir = globaltolocalvec(m, ray);
        let pt = globaltolocal(&p.frame, viewer);
        let r = Ray::for_part(pt, dir, p.gfxobj_scale);
        let c = PickCandidate {
            physobj_id: p.physobj_id,
            physobj_index: p.physobj_index,
            drawing_sphere: p.drawing_sphere,
            polygons: p.polygons,
            check_polys: p.check_polys,
        };
        gfx_obj_under_selection_ray(&mut data, &r, &c);
    }
    data
}

#[cfg(test)]
mod tests {
    // Index arithmetic in test fixtures, bounded by the loops that build them.
    #![allow(clippy::cast_possible_truncation)]

    use super::*;

    /// A unit quad at height `z`, facing +z, and **two-sided** (`sides_type = 1`) so the tests
    /// below are about the ray and not about `polygon_hits_ray`'s back-face gate, which has a test
    /// of its own.
    fn quad(z: f32) -> PickPolygon {
        PickPolygon {
            plane: Plane {
                normal: Vec3::new(0.0, 0.0, 1.0),
                d: -z,
            },
            vertices: vec![
                Vec3::new(-1.0, -1.0, z),
                Vec3::new(1.0, -1.0, z),
                Vec3::new(1.0, 1.0, z),
                Vec3::new(-1.0, 1.0, z),
            ],
            sides_type: 1,
        }
    }

    fn candidate<'a>(
        id: u32,
        index: i32,
        sphere: (Vec3, f32),
        polys: &'a [PickPolygon],
    ) -> PickCandidate<'a> {
        PickCandidate {
            physobj_id: ObjectId(id),
            physobj_index: index,
            drawing_sphere: sphere,
            polygons: polys,
            check_polys: true,
        }
    }

    /// The pick rays length is ten thousand and the hit tests ignore it.
    #[test]
    fn the_pick_rays_length_is_ten_thousand_and_the_hit_tests_ignore_it() {
        assert_eq!(PICK_RAY_LENGTH, 10_000.0);
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(ray.length, 10_000.0);
        assert!(sphere_intersects_ray(Vec3::new(0.0, 0.0, 9_000.0), 1.0, &ray).is_some());
        assert!(
            sphere_intersects_ray(Vec3::new(0.0, 0.0, 11_000.0), 1.0, &ray).is_some(),
            "the sphere test has no upper bound"
        );
        let far = quad(11_000.0);
        assert!(
            polygon_hits_ray(&far, &ray).is_some(),
            "and neither has the polygon test"
        );
    }

    /// Oracle: sphere intersection's first gate is `c > 0.0`: the ray origin must be
    /// **strictly outside** the sphere.
    #[test]
    fn a_ray_starting_inside_the_drawing_sphere_hits_nothing() {
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        assert!(
            sphere_intersects_ray(Vec3::new(0.0, 0.0, 0.5), 2.0, &ray).is_none(),
            "inside"
        );
        assert!(
            sphere_intersects_ray(Vec3::new(0.0, 0.0, 2.0), 2.0, &ray).is_none(),
            "on it"
        );
        assert!(
            sphere_intersects_ray(Vec3::new(0.0, 0.0, 2.1), 2.0, &ray).is_some(),
            "outside"
        );
    }

    /// Oracle: polygon intersection's first two lines —
    /// `if ((sides_type == 0) && (0.0 < dir . N)) return 0;`.
    #[test]
    fn a_single_sided_polygon_is_not_hit_from_behind() {
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        // `quad`'s normal is +z and the ray travels +z, so `dir . N = +1`: the ray meets the
        // polygon's **back** face. Two-sided, that is a hit; single-sided, it is culled.
        let mut back = quad(5.0);
        assert_eq!(back.sides_type, 1);
        assert!(
            polygon_hits_ray(&back, &ray).is_some(),
            "two-sided: hit from either face"
        );
        back.sides_type = 0;
        assert!(
            polygon_hits_ray(&back, &ray).is_none(),
            "single-sided: culled"
        );
        // The same single-sided polygon turned to face the viewer is hit.
        let front = PickPolygon {
            plane: Plane {
                normal: Vec3::new(0.0, 0.0, -1.0),
                d: 5.0,
            },
            vertices: back.vertices.iter().rev().copied().collect(),
            sides_type: 0,
        };
        assert!(polygon_hits_ray(&front, &ray).is_some(), "front face: hit");
    }

    /// Oracle: a **polygon hit always beats a sphere
    /// hit**, even a farther one. This is the tie-break that a depth-sorted implementation gets
    /// wrong, so the scene here is deliberately arranged so the sphere hit is nearer.
    #[test]
    fn a_polygon_hit_beats_a_nearer_sphere_hit_unconditionally() {
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        let mut d = MouseSelectData::default();
        // Candidate A: a sphere at z = 2 with no polygons under the ray -- sphere hit at t = 1.
        let none: Vec<PickPolygon> = Vec::new();
        let a = candidate(0xAAAA, 1, (Vec3::new(0.0, 0.0, 2.0), 1.0), &none);
        assert!(gfx_obj_under_selection_ray(&mut d, &ray, &a));
        assert!(d.found_sphere && !d.found_polygon);
        assert_eq!(d.result(), (ObjectId(0xAAAA), 1));
        // Candidate B: much farther, but its polygon is hit -- t = 50 against the sphere's 1.
        let polys = vec![quad(50.0)];
        let b = candidate(0xBBBB, 2, (Vec3::new(0.0, 0.0, 51.0), 2.0), &polys);
        assert!(gfx_obj_under_selection_ray(&mut d, &ray, &b));
        assert!(d.found_polygon);
        assert_eq!(
            d.result(),
            (ObjectId(0xBBBB), 2),
            "the farther polygon wins outright"
        );
        assert!(
            d.closest_sphere < d.closest_polygon,
            "and it wins despite being farther"
        );
    }

    /// Oracle: [`gfx_obj_under_selection_ray`]'s polygon loop — "only the **first** polygon that
    /// reports a hit is considered per mesh (the loop `break`s), so a self-overlapping mesh can
    /// report a farther polygon than the true nearest one". The mesh below lists the far quad
    /// first.
    #[test]
    fn the_first_hit_polygon_wins_even_when_a_later_one_is_nearer() {
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        let polys = vec![quad(40.0), quad(5.0)];
        // The drawing sphere must not swallow the ray's origin, or the corrected
        // `sphere_intersects_ray` refuses the candidate before any polygon is tested.
        let c = candidate(0xC0DE, 7, (Vec3::new(0.0, 0.0, 60.0), 56.0), &polys);
        let mut d = MouseSelectData::default();
        assert!(gfx_obj_under_selection_ray(&mut d, &ray, &c));
        assert!(d.found_polygon);
        assert!(
            (d.closest_polygon - 40.0).abs() < 1e-3,
            "the first listed polygon wins, at t = {}",
            d.closest_polygon
        );
    }

    /// Oracle: `check_curr_object` is
    /// `(physobj present and physobj.id != 0) || creature_mode`, "so parts belonging to id-less
    /// objects (scenery, particle hosts, buildings) are **not pickable**". Terrain never reaches
    /// the pick path at all, because the landscape polygon draw does not call it.
    #[test]
    fn id_less_objects_are_not_pickable() {
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        let polys = vec![quad(5.0)];
        let mut d = MouseSelectData::default();
        let scenery = candidate(0, 3, (Vec3::new(0.0, 0.0, 5.0), 2.0), &polys);
        assert!(!gfx_obj_under_selection_ray(&mut d, &ray, &scenery));
        assert_eq!(d.result(), (ObjectId(0), -1), "nothing was picked");
        // The same geometry with an id is picked immediately.
        let thing = candidate(0x1234, 3, (Vec3::new(0.0, 0.0, 5.0), 2.0), &polys);
        assert!(gfx_obj_under_selection_ray(&mut d, &ray, &thing));
        assert_eq!(d.result(), (ObjectId(0x1234), 3));
    }

    /// Oracle: there is no depth occlusion: the polygon loop walks the graphics object's polygons,
    /// ignoring mesh subsets and `NoDraw`/translucency. A fully
    /// transparent part is still pickable. A near candidate offered first does not shadow a
    /// nearer polygon on a candidate offered later; the *closest polygon* still wins.
    #[test]
    fn there_is_no_depth_occlusion_between_candidates() {
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0));
        let mut d = MouseSelectData::default();
        let near = vec![quad(3.0)];
        let far = vec![quad(30.0)];
        // Submit the far one first, then the near one: the near polygon must take over.
        let f = candidate(0xF, 0, (Vec3::new(0.0, 0.0, 30.0), 2.0), &far);
        let n = candidate(0xE, 1, (Vec3::new(0.0, 0.0, 3.0), 2.0), &near);
        assert!(gfx_obj_under_selection_ray(&mut d, &ray, &f));
        assert!(gfx_obj_under_selection_ray(&mut d, &ray, &n));
        assert_eq!(d.result(), (ObjectId(0xE), 1));
        // And the other order gives the same answer, so submission order does not decide it.
        let mut d2 = MouseSelectData::default();
        assert!(gfx_obj_under_selection_ray(&mut d2, &ray, &n));
        // The far candidate is rejected early: its sphere hit is beyond the closest polygon.
        assert!(!gfx_obj_under_selection_ray(&mut d2, &ray, &f));
        assert_eq!(d2.result(), (ObjectId(0xE), 1));
    }

    /// Oracle: [`gfx_obj_under_selection_ray`] — "the ray direction is divided by
    /// **`|gfxobj_scale|`** (the length of the scale vector), not by each component — a non-uniform
    /// scale gives a slightly wrong ray. \[verified\]". Reproducing the *wrong* division is the
    /// point.
    #[test]
    fn the_ray_is_divided_by_the_scale_vectors_length_not_per_component() {
        let dir = Vec3::new(0.0, 0.0, 1.0);
        // A uniform scale of 2 in all axes has |s| = sqrt(12) = 3.464, not 2.
        let r = Ray::for_part(Vec3::ZERO, dir, Vec3::new(2.0, 2.0, 2.0));
        assert!((r.dir.z - 1.0 / 12.0f32.sqrt()).abs() < 1e-6, "{:?}", r.dir);
        assert!(
            (r.dir.z - 0.5).abs() > 0.2,
            "a per-component division would give 0.5"
        );
        // A non-uniform scale mixes the axes into one divisor, which is the retail oddity.
        let r = Ray::for_part(Vec3::ZERO, dir, Vec3::new(3.0, 1.0, 1.0));
        assert!((r.dir.z - 1.0 / 11.0f32.sqrt()).abs() < 1e-6, "{:?}", r.dir);
    }

    /// Oracle: pick-ray unprojection makes the centre of the viewport give
    /// the viewer's forward axis exactly, and moving right must swing the ray toward +Xaxis.
    #[test]
    fn the_centre_of_the_viewport_unprojects_to_the_forward_axis() {
        let axes = (
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
        );
        let (w, h) = (800u32, 600u32);
        let (cx, cy) = (
            (f32::from(w as u16) - 1.0) * 0.5,
            (f32::from(h as u16) - 1.0) * 0.5,
        );
        let d = pick_ray(cx, cy, (w, h), 4000.0, 1.0, axes);
        assert!(
            (d.y - 1.0).abs() < 1e-5,
            "the centre looks straight ahead: {d:?}"
        );
        let right = pick_ray(cx + 100.0, cy, (w, h), 4000.0, 1.0, axes);
        assert!(right.x > 0.0, "moving right swings the ray east: {right:?}");
        let up = pick_ray(cx, cy - 100.0, (w, h), 4000.0, 1.0, axes);
        assert!(
            up.z > 0.0,
            "moving up in window coordinates swings the ray up: {up:?}"
        );
    }

    /// Oracle: the client's unprojection driven through [`selection_ray`] and the mouse-pick path,
    /// its only caller.
    ///
    /// A viewer that has been *turned* is what separates this from the existing centre test: the
    /// axes come off the viewer frame, so the centre of the viewport must be the viewer's own
    /// forward however it is oriented. The frame below yaws 90° about +z, which takes +y to −x.
    #[test]
    fn the_selection_ray_follows_the_viewer_frame() {
        use dereth_primitives::{Frame, Quat};
        let (w, h) = (800u32, 600u32);
        let (cx, cy) = ((w as f32 - 1.0) * 0.5, (h as f32 - 1.0) * 0.5);
        let level = Frame::new(Vec3::new(10.0, 20.0, 3.0), Quat::new(1.0, 0.0, 0.0, 0.0));
        let d = selection_ray(&level, cx, cy, (w, h), 1.0);
        assert!(
            (d.y - 1.0).abs() < 1e-5,
            "an unturned viewer looks along +y: {d:?}"
        );

        let c = std::f32::consts::FRAC_1_SQRT_2;
        let turned = Frame::new(level.origin, Quat::new(c, 0.0, 0.0, c)); // +90 deg about z
        let d = selection_ray(&turned, cx, cy, (w, h), 1.0);
        assert!(d.x < -0.999, "yawed 90 degrees, forward is -x: {d:?}");
        // ..and the ray is a direction, so moving the viewer does not change it.
        let moved = Frame::new(Vec3::new(-500.0, 900.0, 12.0), turned.rotation);
        let d2 = selection_ray(&moved, cx, cy, (w, h), 1.0);
        assert!((d.x - d2.x).abs() < 1e-6 && (d.y - d2.y).abs() < 1e-6);
    }

    /// Oracle: part-space picking pushes the part's position as the current frame and then
    /// recomputes the eye in that frame, so [`gfx_obj_under_selection_ray`] is handed the eye **in
    /// the part's own space** and a direction transformed from world to part space.
    ///
    /// The scene is a quad at the origin of a part that has been moved 5 units up +y and turned, so
    /// a sweep that forgot either transform misses it entirely.
    #[test]
    fn a_part_is_swept_in_its_own_space_not_in_the_worlds() {
        use dereth_primitives::{Frame, Quat};
        let polys = vec![quad(0.0)]; // the x/y plane at part-local z = 0, normal +z
                                     // Put the part 5 units along +y, rolled 90 degrees about +x so its local +z faces -y,
                                     // i.e. back at the viewer standing at the origin looking along +y.
        let c = std::f32::consts::FRAC_1_SQRT_2;
        let frame = Frame::new(Vec3::new(0.0, 5.0, 0.0), Quat::new(c, c, 0.0, 0.0));
        let part = PickPart {
            physobj_id: ObjectId(0x900D),
            physobj_index: 0,
            frame,
            gfxobj_scale: Vec3::new(1.0, 1.0, 1.0),
            drawing_sphere: (Vec3::ZERO, 2.0),
            polygons: &polys,
            check_polys: true,
        };
        let d = find_object(
            Vec3::ZERO,
            Vec3::new(0.0, 1.0, 0.0),
            std::slice::from_ref(&part),
        );
        assert!(
            d.found_polygon,
            "the quad is square in front of the viewer: {d:?}"
        );
        assert_eq!(d.result(), (ObjectId(0x900D), 0));
        // `t` is in the units `Ray::for_part` left the direction in: it divided by
        // `|gfxobj_scale|`, and for an ordinary unscaled part that is `|(1,1,1)| = sqrt(3)`, not 1.
        // So the quad five world units away reports `t = 5 * sqrt(3)`. That is the client's own
        // arithmetic and every candidate carries the same
        // factor, so the comparisons between them still rank correctly.
        let root3 = f64::from(3.0f32.sqrt());
        assert!(
            (d.closest_polygon - 5.0 * root3).abs() < 1e-3,
            "at t = 5*sqrt(3): {}",
            d.closest_polygon
        );

        // Aim past it and nothing is hit, which is what says the transform is not being ignored.
        let d = find_object(
            Vec3::ZERO,
            Vec3::new(1.0, 0.0, 0.0),
            std::slice::from_ref(&part),
        );
        assert_eq!(d.result(), (ObjectId(0), -1));
    }

    /// Oracle: the candidate flag is
    /// `(physobj present and physobj.id != 0) || creature_mode`, and the whole
    /// sweep is the mouse-selection answer ([`MouseSelectData::result`]) at the end. Two objects,
    /// the nearer one id-less: the pick must reach past it, because scenery is not a candidate at
    /// all.
    #[test]
    fn the_sweep_reaches_past_an_id_less_object_to_the_one_behind_it() {
        use dereth_primitives::{Frame, Quat};
        let near = vec![quad(0.0)];
        let far = vec![quad(0.0)];
        let up = Quat::new(1.0, 0.0, 0.0, 0.0);
        // Both quads lie in their own x/y plane facing +z; place them along the +z ray.
        fn mk(id: u32, z: f32, polys: &[PickPolygon], up: dereth_primitives::Quat) -> PickPart<'_> {
            PickPart {
                physobj_id: ObjectId(id),
                physobj_index: 0,
                frame: Frame::new(Vec3::new(0.0, 0.0, z), up),
                gfxobj_scale: Vec3::new(1.0, 1.0, 1.0),
                drawing_sphere: (Vec3::ZERO, 2.0),
                polygons: polys,
                check_polys: true,
            }
        }
        let parts = [mk(0, 3.0, &near, up), mk(0xBEEF, 9.0, &far, up)];
        let d = find_object(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), &parts);
        assert_eq!(
            d.result(),
            (ObjectId(0xBEEF), 0),
            "the id-less near quad is not a candidate"
        );
        assert!((d.closest_polygon - 9.0 * f64::from(3.0f32.sqrt())).abs() < 1e-3);
    }
}
