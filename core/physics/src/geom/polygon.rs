//! Convex polygons with cached planes.
//!
//! Transcribed function by function from the client's own polygon code.
//!
//! **The in-plane edge normal is `cross(N, e)`, not `cross(e, N)`.** The client computes
//! `(e.z*N.y - e.y*N.z, e.x*N.z - e.z*N.x, e.y*N.x - e.x*N.y)`, which is `cross(N, e)`. With the
//! opposite sign every "inside this edge" test inverts and a sphere is
//! reported as hitting a polygon exactly when it does not. The plane build, the sphere-hit
//! test, the walkability check and the 2-D point test all use `cross(N, e)`, and so does ACE.

use dereth_primitives::Vec3;

use crate::geom::plane::Plane;
use crate::geom::sphere::Sphere;
use crate::geom::PlaneExt;
use crate::globals::EPSILON;
use crate::math::V3;

/// `Sidedness` as `point_in_poly2D` takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolySide {
    Negative,
    Positive,
}

/// A convex polygon. The client shares `CVertex` objects between the renderer and physics; here
/// the positions are copied, because nothing in physics mutates them after construction.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Polygon {
    pub vertices: Vec<Vec3>,
    pub plane: Plane,
    /// `pos_surface`: sets it to 0 when all three terrain vertices have
    /// `z == 0` exactly, otherwise 1. Physics never reads it; it is kept for the renderer.
    pub pos_surface: u16,
}

impl Polygon {
    /// Build a polygon and run [`Self::make_plane`].
    #[must_use]
    pub fn new(vertices: Vec<Vec3>) -> Self {
        let mut p = Self {
            vertices,
            plane: Plane::default(),
            pos_surface: 0,
        };
        p.make_plane();
        p
    }

    #[inline]
    #[must_use]
    pub fn num_pts(&self) -> usize {
        self.vertices.len()
    }

    /// Build the polygon's plane.
    ///
    /// The normal is the sum of `cross(v[k+1] - v[0], v[k+2] - v[0])` over `k = 0 ..= n - 3`,
    /// normalised; `d` is minus the **mean** of `dot(N, v[i])`, not `-dot(N, v[0])`.
    pub fn make_plane(&mut self) {
        let n = self.vertices.len();
        let mut acc = Vec3::ZERO;
        if n >= 3 {
            let v0 = self.vertices[0];
            for k in 0..n - 2 {
                let a = self.vertices[k + 1].sub(v0);
                let b = self.vertices[k + 2].sub(v0);
                acc = acc.add(a.cross(b));
            }
        }
        let inv = 1.0 / acc.mag2().sqrt();
        let normal = acc.mul(inv);
        let mut sum = 0.0_f32;
        for v in &self.vertices {
            sum += normal.x * v.x + normal.y * v.y + normal.z * v.z;
        }
        #[allow(clippy::cast_precision_loss)]
        let d = -(sum / n as f32);
        self.plane = Plane { normal, d };
    }

    /// The `(prev, cur)` edge walk every routine below shares: `prev` starts at the **last**
    /// vertex and `cur` at index 0.
    fn edges(&self) -> impl Iterator<Item = (Vec3, Vec3)> + '_ {
        let n = self.vertices.len();
        (0..n).map(move |i| (self.vertices[(i + n - 1) % n], self.vertices[i]))
    }

    /// The 2-D test land cells use to pick which of a
    /// cell's two triangles contains a point.
    ///
    /// The loop runs **backwards** from `num_pts - 1` with `prev` seeded to `vertices[0]`, so the
    /// first edge tested is `(v[0], v[n-1])`. That order is preserved because a caller can only
    /// observe it through short-circuiting, and short-circuiting is what decides ties.
    #[must_use]
    pub fn point_in_poly2d(&self, point: Vec3, side: PolySide) -> bool {
        let n = self.vertices.len();
        if n == 0 {
            return true;
        }
        let mut prev = self.vertices[0];
        for i in (0..n).rev() {
            let cur = self.vertices[i];
            let a = prev.y - cur.y;
            let b = cur.x - prev.x;
            let v = a * point.x + b * point.y + (-(a * cur.x) - b * cur.y);
            match side {
                PolySide::Positive => {
                    if v > 0.0 {
                        return false;
                    }
                }
                PolySide::Negative => {
                    if v < 0.0 {
                        return false;
                    }
                }
            }
            prev = cur;
        }
        true
    }

    /// The 3-D edge loop, no radius.
    #[must_use]
    pub fn point_in_polygon(&self, point: Vec3) -> bool {
        for (prev, cur) in self.edges() {
            let e = cur.sub(prev);
            let n = self.plane.normal.cross(e);
            if point.sub(prev).dot(n) < 0.0 {
                return false;
            }
        }
        true
    }

    /// The fast test. Writes the sphere centre
    /// projected onto the plane into `contact_point`.
    ///
    /// Note `rad = radius - 0.0002`: this is an **overlap** test, so the epsilon is subtracted.
    pub fn polygon_hits_sphere(&self, sphere: &Sphere, contact_point: &mut Vec3) -> bool {
        let dp = self.plane.dot_point(sphere.center);
        let rad = sphere.radius - EPSILON;
        if rad < dp.abs() {
            return false;
        }
        let diff = rad * rad - dp * dp;
        *contact_point = sphere.center.sub(self.plane.normal.mul(dp));
        let mut result = true;
        for (prev, cur) in self.edges() {
            let e = cur.sub(prev);
            let n = self.plane.normal.cross(e);
            let disp = contact_point.sub(prev);
            let dot = disp.dot(n);
            if dot < 0.0 {
                if n.mag2() * diff < dot * dot {
                    return false;
                }
                let along = disp.dot(e);
                if along >= 0.0 && along < e.mag2() {
                    return true;
                }
                result = false;
            }
            if disp.mag2() < diff {
                return true;
            }
        }
        result
    }

    /// The slow-but-sure sphere-hit test.
    ///
    /// If the projected centre is inside every edge the answer is 1 immediately; otherwise it
    /// runs the full loop from the top. The difference from the fast version is that a `result`
    /// that was set to 0 by an earlier edge cannot be rescued by a later one.
    pub fn polygon_hits_sphere_slow_but_sure(
        &self,
        sphere: &Sphere,
        contact_point: &mut Vec3,
    ) -> bool {
        let dp = self.plane.dot_point(sphere.center);
        let rad = sphere.radius - EPSILON;
        if rad < dp.abs() {
            return false;
        }
        let diff = rad * rad - dp * dp;
        *contact_point = sphere.center.sub(self.plane.normal.mul(dp));
        let mut any_outside = false;
        for (prev, cur) in self.edges() {
            let e = cur.sub(prev);
            let n = self.plane.normal.cross(e);
            if contact_point.sub(prev).dot(n) < 0.0 {
                any_outside = true;
                break;
            }
        }
        if !any_outside {
            return true;
        }
        for (prev, cur) in self.edges() {
            let e = cur.sub(prev);
            let n = self.plane.normal.cross(e);
            let disp = contact_point.sub(prev);
            let dot = disp.dot(n);
            if dot < 0.0 {
                if n.mag2() * diff < dot * dot {
                    return false;
                }
                let along = disp.dot(e);
                if along >= 0.0 && along < e.mag2() {
                    return true;
                }
            }
            if disp.mag2() < diff {
                return true;
            }
        }
        false
    }

    /// The slow-but-sure version with a scratch contact
    /// point.
    #[must_use]
    pub fn hits_sphere(&self, sphere: &Sphere) -> bool {
        let mut scratch = Vec3::ZERO;
        self.polygon_hits_sphere_slow_but_sure(sphere, &mut scratch)
    }

    /// Transcribe the polygon-sphere query in full, including **both** of its answers.
    ///
    /// The client's five statements, in order:
    ///
    /// ```text
    // r = the slow-but-sure overlap test of this polygon, the sphere and the contact point
    // if r: record this polygon as the hit polygon  <-- BEFORE the direction test
    // d = dot(this polygon's plane normal, movement)
    // return r if d < 0, else 0
    /// ```
    ///
    /// So the polygon is **recorded whenever the sphere geometrically overlaps it**, and the
    /// return value only says whether the sphere was moving *into* it. A back-facing overlap
    /// therefore returns `0` while leaving `hit_poly` set, and the BSP collision search reads that:
    /// its default arm and its `PATH_CLIPPED` arm both test
    /// `returned || hit_poly.is_some()`, and its `CONTACT` arm routes the case to the sphere
    /// path's negative-polygon hit.
    ///
    /// Returns `(overlaps, hits)`. Both halves are needed: with only the second, a back-facing
    /// overlap would be invisible everywhere and `set_neg_poly_hit` would have no writer.
    #[must_use]
    pub fn pos_hits_sphere_parts(
        &self,
        sphere: &Sphere,
        movement: Vec3,
        contact_point: &mut Vec3,
    ) -> (bool, bool) {
        let overlap = self.polygon_hits_sphere_slow_but_sure(sphere, contact_point);
        (overlap, overlap && self.plane.normal.dot(movement) < 0.0)
    }

    /// This method exposes the polygon test's **return value** alone — the movement must be
    /// *into* the polygon. See [`Self::pos_hits_sphere_parts`] for the out-parameter it also sets.
    pub fn pos_hits_sphere(
        &self,
        sphere: &Sphere,
        movement: Vec3,
        contact_point: &mut Vec3,
    ) -> bool {
        self.pos_hits_sphere_parts(sphere, movement, contact_point)
            .1
    }

    /// Gate on the up vector against the path's
    /// `walkable_allowance`, then the slow-but-sure test.
    ///
    /// The original then runs the fast test as well and compares the two, re-running both if they
    /// disagree — a leftover consistency assertion with no effect on the result. Not reproduced;
    /// it is observationally inert.
    #[must_use]
    pub fn walkable_hits_sphere(&self, sphere: &Sphere, up: Vec3, walkable_allowance: f32) -> bool {
        if up.dot(self.plane.normal) <= walkable_allowance {
            return false;
        }
        let mut scratch = Vec3::ZERO;
        self.polygon_hits_sphere_slow_but_sure(sphere, &mut scratch)
    }

    /// The walkability check.
    ///
    /// Two differences from ACE, both taken from the client: the up/normal guard is on the
    /// **absolute** value (`ABS(dot) < 2e-4`, not `dot < 2e-4`), and the radius term is the plain
    /// `radius^2` rather than `radius^2 - d^2`.
    #[must_use]
    pub fn check_walkable(&self, sphere: &Sphere, up: Vec3) -> bool {
        self.check_walkable_inner(sphere, up, false)
    }

    /// The same with the radius term quartered.
    #[must_use]
    pub fn check_small_walkable(&self, sphere: &Sphere, up: Vec3) -> bool {
        self.check_walkable_inner(sphere, up, true)
    }

    fn check_walkable_inner(&self, sphere: &Sphere, up: Vec3, small: bool) -> bool {
        let angle_up = up.dot(self.plane.normal);
        if angle_up.abs() < EPSILON {
            return false;
        }
        let t = self.plane.dot_point(sphere.center) / angle_up;
        let center = sphere.center.sub(up.mul(t));
        let mut radsum = sphere.radius * sphere.radius;
        if small {
            radsum *= 0.25;
        }
        let mut result = true;
        for (prev, cur) in self.edges() {
            let e = cur.sub(prev);
            let n = self.plane.normal.cross(e);
            let disp = center.sub(prev);
            let dot = disp.dot(n);
            if dot < 0.0 {
                if n.mag2() * radsum < dot * dot {
                    return false;
                }
                let along = disp.dot(e);
                if along >= 0.0 && along < e.mag2() {
                    return true;
                }
                result = false;
            }
            if disp.mag2() < radsum {
                return true;
            }
        }
        result
    }

    /// The first edge whose inward test fails,
    /// returned as the normalised `cross(N, e)`. `None` when the up vector grazes the plane.
    #[must_use]
    pub fn find_crossed_edge(&self, sphere: &Sphere, up: Vec3) -> Option<Vec3> {
        let angle_up = self.plane.normal.dot(up);
        if angle_up.abs() < EPSILON {
            return None;
        }
        let t = self.plane.dot_point(sphere.center) / angle_up;
        let center = sphere.center.sub(up.mul(t));
        for (prev, cur) in self.edges() {
            let e = cur.sub(prev);
            let n = self.plane.normal.cross(e);
            if center.sub(prev).dot(n) < 0.0 {
                return Some(n.normalize());
            }
        }
        None
    }

    /// The time at which a moving sphere touches
    /// this polygon's plane.
    ///
    /// ACE decides the sign of the radius term from `movement.LengthSquared()`; the client
    /// decides it from the sign of the current signed distance. This is the client's version.
    #[must_use]
    pub fn adjust_sphere_to_poly(&self, radius: f32, cur_pos: Vec3, movement: Vec3) -> f32 {
        let d0 = self.plane.dot_point(cur_pos);
        if d0.abs() < radius {
            return 1.0;
        }
        let den = self.plane.normal.dot(movement);
        if den.abs() < EPSILON {
            return 0.0;
        }
        let r = if d0 <= 0.0 { -radius } else { radius };
        (r - d0) / den
    }

    /// Push the sphere back out of the plane and
    /// consume `walk_interp`.
    ///
    /// The rejection floor here is **`-0.5`**, not the `-0.1` the step-down paths use. Returns
    /// `Some` of the new `walk_interp` and moves `center` on success.
    pub fn adjust_sphere_to_plane(
        &self,
        center: &mut Vec3,
        radius: f32,
        movement: Vec3,
        walk_interp: f32,
    ) -> Option<f32> {
        let dp_pos = self.plane.dot_point(*center);
        let dp_move = self.plane.normal.dot(movement);
        let dist = if dp_move <= EPSILON {
            if dp_move >= -EPSILON {
                return None;
            }
            dp_pos - radius
        } else {
            -radius - dp_pos
        };
        let i_dist = dist / dp_move;
        let interp = (1.0 - i_dist) * walk_interp;
        if interp < walk_interp && interp >= crate::globals::WALK_INTERP_FLOOR_PLANE {
            *center = center.sub(movement.mul(i_dist));
            Some(interp)
        } else {
            None
        }
    }

    /// Push a pair of spheres a fixed distance
    /// off this plane. Both spheres move by the same vector.
    pub fn adjust_to_placement_poly(
        &self,
        hit: &mut Sphere,
        other: Option<&mut Sphere>,
        radius: f32,
        center_solid: bool,
        solid_check: bool,
    ) {
        let dp = self.plane.dot_point(hit.center);
        let mut r = radius;
        if solid_check {
            if center_solid {
                r *= -1.0;
            }
            if dp <= 0.0 {
                r = -r;
            }
        }
        let adjust = self.plane.normal.mul(r - dp);
        hit.center = hit.center.add(adjust);
        if let Some(o) = other {
            o.center = o.center.add(adjust);
        }
    }

    /// `landblock_cull` is `SidesType == Landblock`,
    /// which rejects a ray arriving from behind the plane.
    #[must_use]
    pub fn polygon_hits_ray(
        &self,
        ray: &crate::geom::sphere::Ray,
        landblock_cull: bool,
    ) -> Option<f32> {
        if landblock_cull && self.plane.normal.dot(ray.dir) > 0.0 {
            return None;
        }
        let t = self.plane.compute_time_of_intersection(ray)?;
        if self.point_in_polygon(ray.point.add(ray.dir.mul(t))) {
            Some(t)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: Where a value is
    // asserted it was computed by hand from the polygon's own geometry, not copied out of the
    // implementation.

    fn unit_square() -> Polygon {
        // Counter-clockwise seen from +Z, so make_plane gives N = +Z.
        Polygon::new(vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ])
    }

    fn triangle() -> Polygon {
        Polygon::new(vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(0.0, 4.0, 0.0),
        ])
    }

    #[test]
    fn make_plane_normal_points_up_for_a_ccw_polygon_and_d_is_the_mean() {
        let p = unit_square();
        assert!(
            p.plane.normal.eq_eps(Vec3::new(0.0, 0.0, 1.0)),
            "{:?}",
            p.plane.normal
        );
        assert_eq!(p.plane.d, 0.0);
        // Raise the whole polygon: d must follow, and it is the mean of dot(N, v).
        let raised = Polygon::new(vec![
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(1.0, 1.0, 3.0),
            Vec3::new(0.0, 1.0, 3.0),
        ]);
        let mean: f32 = raised
            .vertices
            .iter()
            .map(|v| raised.plane.normal.dot(*v))
            .sum::<f32>()
            / 4.0;
        assert!(
            (raised.plane.d + mean).abs() < 1e-5,
            "{} vs {mean}",
            raised.plane.d
        );
    }

    #[test]
    fn make_plane_of_a_terrain_triangle_is_the_single_cross_product() {
        // SW, SE, NE of a 24 m land cell with the NE corner 6 m up.
        let t = Polygon::new(vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(24.0, 0.0, 0.0),
            Vec3::new(24.0, 24.0, 6.0),
        ]);
        assert!(
            t.plane.normal.z > 0.0,
            "terrain normals point up: {:?}",
            t.plane.normal
        );
        // and the plane really contains all three vertices
        for v in &t.vertices {
            assert!(t.plane.dot_point(*v).abs() < 1e-4, "{v:?} off plane");
        }
    }

    /// This is the assertion that catches the `cross(e, N)` sign error the knowledge base had.
    #[test]
    fn point_in_polygon_accepts_the_interior_and_rejects_the_exterior() {
        let p = unit_square();
        assert!(p.point_in_polygon(Vec3::new(0.5, 0.5, 0.0)));
        assert!(!p.point_in_polygon(Vec3::new(-0.5, 0.5, 0.0)));
        assert!(!p.point_in_polygon(Vec3::new(1.5, 0.5, 0.0)));
        assert!(!p.point_in_polygon(Vec3::new(0.5, -0.5, 0.0)));
        assert!(!p.point_in_polygon(Vec3::new(0.5, 1.5, 0.0)));
    }

    /// Point in poly2d agrees with barycentric over 100k random points.
    #[test]
    fn point_in_poly2d_agrees_with_barycentric_over_100k_random_points() {
        let mut seed = 0x2545_F491_u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            #[allow(clippy::cast_precision_loss)]
            let f = (seed >> 8) as f32 / 16_777_216.0;
            f
        };
        let mut checked = 0_u32;
        let mut inside = 0_u32;
        for _ in 0..100_000 {
            // A random CCW triangle in the XY plane and a random query point around it.
            let a = Vec3::new(rnd() * 10.0 - 5.0, rnd() * 10.0 - 5.0, 0.0);
            let b = Vec3::new(rnd() * 10.0 - 5.0, rnd() * 10.0 - 5.0, 0.0);
            let c = Vec3::new(rnd() * 10.0 - 5.0, rnd() * 10.0 - 5.0, 0.0);
            // signed area; skip near-degenerate triangles where the two tests can legitimately
            // disagree on a point exactly on an edge
            let area2 = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            if area2.abs() < 0.5 {
                continue;
            }
            let (a, b, c) = if area2 > 0.0 { (a, b, c) } else { (a, c, b) };
            let p = Vec3::new(rnd() * 12.0 - 6.0, rnd() * 12.0 - 6.0, 0.0);
            // barycentric sign test for a CCW triangle
            let s = |u: Vec3, v: Vec3| (v.x - u.x) * (p.y - u.y) - (v.y - u.y) * (p.x - u.x);
            let (d1, d2, d3) = (s(a, b), s(b, c), s(c, a));
            if d1.abs() < 1e-3 || d2.abs() < 1e-3 || d3.abs() < 1e-3 {
                continue; // on an edge: the strict/non-strict comparisons differ, by design
            }
            let expect = d1 > 0.0 && d2 > 0.0 && d3 > 0.0;
            // The client's POSITIVE convention requires every 2-D edge value <= 0, and its edge
            // value has the opposite sign to `s` above for a CCW winding.
            let poly = Polygon::new(vec![a, b, c]);
            let got = poly.point_in_poly2d(p, PolySide::Positive);
            assert_eq!(got, expect, "tri {a:?} {b:?} {c:?} pt {p:?}");
            checked += 1;
            if expect {
                inside += 1;
            }
        }
        assert!(checked > 80_000, "only {checked} usable samples");
        assert!(
            inside > 1_000,
            "only {inside} interior samples; the test is not exercising both arms"
        );
    }

    #[test]
    fn polygon_hits_sphere_uses_the_overlap_epsilon() {
        let p = unit_square();
        let mut cp = Vec3::ZERO;
        // A sphere centred over the middle, just touching the plane: radius must exceed the
        // distance by more than the epsilon.
        let s = Sphere::new(Vec3::new(0.5, 0.5, 0.5), 0.5);
        assert!(
            !p.polygon_hits_sphere(&s, &mut cp),
            "radius - 2e-4 < 0.5, so no hit"
        );
        let s = Sphere::new(Vec3::new(0.5, 0.5, 0.5), 0.500_3);
        assert!(p.polygon_hits_sphere(&s, &mut cp));
        assert!(cp.eq_eps(Vec3::new(0.5, 0.5, 0.0)), "{cp:?}");
    }

    #[test]
    fn polygon_hits_sphere_catches_an_edge_and_a_vertex() {
        let p = unit_square();
        let mut cp = Vec3::ZERO;
        // just off the +X edge, close enough to reach it
        assert!(p.polygon_hits_sphere(&Sphere::new(Vec3::new(1.1, 0.5, 0.0), 0.5), &mut cp));
        // and far enough away not to
        assert!(!p.polygon_hits_sphere(&Sphere::new(Vec3::new(1.6, 0.5, 0.0), 0.5), &mut cp));
        // diagonally off a corner, within the radius
        assert!(p.polygon_hits_sphere(&Sphere::new(Vec3::new(1.2, 1.2, 0.0), 0.5), &mut cp));
        // diagonally off a corner, outside it
        assert!(!p.polygon_hits_sphere(&Sphere::new(Vec3::new(1.5, 1.5, 0.0), 0.5), &mut cp));
    }

    #[test]
    fn slow_but_sure_agrees_with_the_fast_version_on_a_dense_grid() {
        // The two differ only in how a `result = false` from one edge interacts with a later
        // edge; over a convex polygon they must agree everywhere.
        let p = triangle();
        let mut disagreements = 0;
        let mut samples = 0;
        for i in -20..40 {
            for j in -20..40 {
                #[allow(clippy::cast_precision_loss)]
                let c = Vec3::new(i as f32 * 0.25, j as f32 * 0.25, 0.1);
                let s = Sphere::new(c, 0.6);
                let (mut a, mut b) = (Vec3::ZERO, Vec3::ZERO);
                if p.polygon_hits_sphere(&s, &mut a)
                    != p.polygon_hits_sphere_slow_but_sure(&s, &mut b)
                {
                    disagreements += 1;
                }
                samples += 1;
            }
        }
        assert_eq!(
            disagreements, 0,
            "{disagreements} of {samples} samples disagreed"
        );
    }

    #[test]
    fn pos_hits_sphere_requires_movement_into_the_polygon() {
        let p = unit_square();
        let s = Sphere::new(Vec3::new(0.5, 0.5, 0.1), 0.5);
        let mut cp = Vec3::ZERO;
        assert!(
            p.pos_hits_sphere(&s, Vec3::new(0.0, 0.0, -1.0), &mut cp),
            "moving down into it"
        );
        assert!(
            !p.pos_hits_sphere(&s, Vec3::new(0.0, 0.0, 1.0), &mut cp),
            "moving away from it"
        );
    }

    #[test]
    fn check_walkable_guard_is_on_the_absolute_dot_product() {
        // ACE tests `angleUp < EPSILON`, so an up vector pointing *into* the polygon would be
        // rejected there; the client's ABS accepts it. This is the difference, exercised.
        let p = unit_square();
        let s = Sphere::new(Vec3::new(0.5, 0.5, 1.0), 0.5);
        assert!(
            p.check_walkable(&s, Vec3::new(0.0, 0.0, -1.0)),
            "up pointing down still probes"
        );
        assert!(p.check_walkable(&s, Vec3::new(0.0, 0.0, 1.0)));
        // truly grazing: rejected
        assert!(!p.check_walkable(&s, Vec3::new(1.0, 0.0, 0.000_1)));
    }

    #[test]
    fn check_small_walkable_quarters_the_radius_term() {
        let p = unit_square();
        // A sphere whose projection sits just outside the +X edge by 0.4: the full-radius probe
        // (r = 0.5) reaches, the small one (effective r = 0.25) does not.
        let s = Sphere::new(Vec3::new(1.4, 0.5, 1.0), 0.5);
        let up = Vec3::new(0.0, 0.0, 1.0);
        assert!(p.check_walkable(&s, up));
        assert!(!p.check_small_walkable(&s, up));
    }

    #[test]
    fn find_crossed_edge_names_the_edge_that_was_crossed() {
        let p = unit_square();
        // A centre past the +X edge: the crossed edge is (1,0)->(1,1), whose cross(N, e) is -X.
        let s = Sphere::new(Vec3::new(1.5, 0.5, 1.0), 0.5);
        let n = p
            .find_crossed_edge(&s, Vec3::new(0.0, 0.0, 1.0))
            .expect("an edge was crossed");
        assert!(n.eq_eps(Vec3::new(-1.0, 0.0, 0.0)), "{n:?}");
        // A centre inside crosses nothing.
        let inside = Sphere::new(Vec3::new(0.5, 0.5, 1.0), 0.5);
        assert_eq!(p.find_crossed_edge(&inside, Vec3::new(0.0, 0.0, 1.0)), None);
        // A grazing up vector is rejected outright.
        assert_eq!(p.find_crossed_edge(&s, Vec3::new(1.0, 0.0, 0.0)), None);
    }

    #[test]
    fn adjust_sphere_to_poly_returns_one_when_already_interpenetrating() {
        let p = unit_square();
        assert_eq!(
            p.adjust_sphere_to_poly(1.0, Vec3::new(0.5, 0.5, 0.5), Vec3::new(0.0, 0.0, -1.0)),
            1.0
        );
        // approaching from above: touch happens when the centre reaches +radius
        let t = p.adjust_sphere_to_poly(1.0, Vec3::new(0.5, 0.5, 5.0), Vec3::new(0.0, 0.0, -1.0));
        assert!((t - 4.0).abs() < 1e-5, "{t}");
        // approaching from below: the radius term flips sign
        let t = p.adjust_sphere_to_poly(1.0, Vec3::new(0.5, 0.5, -5.0), Vec3::new(0.0, 0.0, 1.0));
        assert!((t - 4.0).abs() < 1e-5, "{t}");
        // parallel movement: no time
        assert_eq!(
            p.adjust_sphere_to_poly(1.0, Vec3::new(0.5, 0.5, 5.0), Vec3::new(1.0, 0.0, 0.0)),
            0.0
        );
    }

    #[test]
    fn adjust_sphere_to_plane_rejects_below_minus_one_half() {
        // This floor is -0.5, where the step-down paths use -0.1.
        let p = unit_square();
        let mut c = Vec3::new(0.5, 0.5, 0.5);
        // A movement so slow that reaching the plane consumes five times the available interp:
        // i_dist = (0.5 - 1) / -0.1 = 5, so interp = -4, well below the -0.5 floor.
        assert_eq!(
            p.adjust_sphere_to_plane(&mut c, 1.0, Vec3::new(0.0, 0.0, -0.1), 1.0),
            None
        );
        // A modest one succeeds and reduces walk_interp.
        let mut c = Vec3::new(0.5, 0.5, 0.9);
        let got = p.adjust_sphere_to_plane(&mut c, 1.0, Vec3::new(0.0, 0.0, -1.0), 1.0);
        assert!(got.is_some(), "expected an adjustment");
        assert!(got.unwrap() < 1.0);
    }

    #[test]
    fn polygon_hits_ray_culls_a_back_facing_landblock_polygon() {
        let p = unit_square();
        let down = crate::geom::sphere::Ray {
            point: Vec3::new(0.5, 0.5, 5.0),
            dir: Vec3::new(0.0, 0.0, -1.0),
        };
        assert_eq!(p.polygon_hits_ray(&down, true), Some(5.0));
        let up = crate::geom::sphere::Ray {
            point: Vec3::new(0.5, 0.5, -5.0),
            dir: Vec3::new(0.0, 0.0, 1.0),
        };
        assert_eq!(
            p.polygon_hits_ray(&up, true),
            None,
            "landblock culling rejects it"
        );
        assert_eq!(p.polygon_hits_ray(&up, false), Some(5.0));
        // a ray that misses the polygon in-plane
        let miss = crate::geom::sphere::Ray {
            point: Vec3::new(5.0, 5.0, 5.0),
            dir: Vec3::new(0.0, 0.0, -1.0),
        };
        assert_eq!(p.polygon_hits_ray(&miss, true), None);
    }
}
