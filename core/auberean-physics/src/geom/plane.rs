//! Planes as this world tests against them. The plane itself is the shared one (a unit normal and
//! an offset, a point `p` lying `n·p + d` from it); what this world does with one is here.

pub use dereth_primitives::shape::Plane;
use dereth_primitives::{Position, Vec3};

use super::{point_to_global_from, rotation_axes, Ray, EPS};

fn w(x: f32) -> f64 {
    f64::from(x)
}

#[allow(clippy::cast_possible_truncation)]
fn n(x: f64) -> f32 {
    x as f32
}

/// Which side of a plane a point is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Positive,
    Negative,
    InPlane,
}

/// What this world does with a plane.
pub trait PlaneOps {
    /// The plane with normal `normal` through `point`.
    fn through(normal: Vec3, point: Vec3) -> Plane;

    /// The upright plane through two points, its normal across the line from `b` to `a` turned a
    /// quarter clockwise seen from above, and halfway between the two points' own offsets; two
    /// points closer than 0.0002 across the ground give the plane facing `+y` through `a`.
    fn vertical(a: Vec3, b: Vec3) -> Plane;

    /// The side `p` is on, more than `eps` from the plane counting as off it.
    fn which_side(&self, p: Vec3, eps: f32) -> Side;

    /// How far `p` is from the plane, summed the way the client sums it (z, then y, then x, then
    /// the offset) in wide precision.
    fn dist_wide(&self, p: Vec3) -> f64;

    /// Where the line from `a` to `b` crosses the plane, from either side: the point, and how much
    /// of the line is left beyond it (one less the fraction of the way to it, kept wide). None
    /// when the line runs within 0.0002 of parallel to the plane, or crosses it only before `a`
    /// or after `b`.
    fn line_crossing(&self, a: Vec3, b: Vec3) -> Option<(Vec3, f64)>;

    /// Sets `v.z` to the height of the plane over `(v.x, v.y)`; false (and `v` unchanged) for a
    /// plane within 0.0002 of upright.
    fn set_height(&self, v: &mut Vec3) -> bool;

    /// Moves `v` straight up or down onto the plane, as [`PlaneOps::set_height`] but worked
    /// through the plane's distance at zero height and the reciprocal of its up part.
    fn snap_to_plane(&self, v: &mut Vec3);

    /// Whether the plane is one: no part not a number and its normal of unit length within
    /// 0.0004 (its square length).
    fn is_valid(&self) -> bool;

    /// The plane, given in `from`'s frame, in the landblock frame of `to`'s cell.
    fn local_to_global(&self, to: &Position, from: &Position) -> Plane;

    /// How far along `ray` (in its own units of length, wide) its line meets the plane, and
    /// whether that is ahead of its start (or not a number); none when the ray runs parallel
    /// within 0.0002 squared.
    fn time_of_intersection(&self, ray: &Ray) -> Option<(f64, bool)>;

    /// Where `ray`, carried on past its length, meets the plane.
    fn find_intercept(&self, ray: &Ray) -> Option<Vec3>;

    /// Where the segment from `a` to `b` crosses the plane: none when both ends lie on the same
    /// side or both within 0.01 of it; an end within 0.01 counts as on it.
    fn intersect_line_segment(&self, a: Vec3, b: Vec3) -> Option<Vec3>;

    /// Clips the part `start` to `end` (fractions of its length) of `ray` to the plane's positive
    /// side: false when neither end lies on it; an end on the negative side moves to the crossing.
    fn clip_ray_to_positive(&self, ray: &Ray, start: &mut f32, end: &mut f32) -> bool;

    /// Cuts `ray` by the plane into its positive and negative parts: whether each side has a
    /// part, and the parts (a ray wholly on one side is that side's part, its other part left as
    /// given). None when a crossing is not found.
    fn cut_ray(&self, ray: &Ray, positive: &mut Ray, negative: &mut Ray) -> Option<(bool, bool)>;
}

/// The side a distance lies on, more than `eps` off counting as off.
fn side_by(d: f64, eps: f64) -> Side {
    if d > eps {
        Side::Positive
    } else if d >= -eps {
        Side::InPlane
    } else {
        Side::Negative
    }
}

/// The plane of a polygon's corners: its normal the unit sum of the fan's cross products from the
/// first corner, its offset the mean of the corners' along that normal, negated.
// D2-04175
#[must_use]
pub fn plane_of_corners(corners: &[Vec3]) -> Plane {
    let Some(&first) = corners.first() else {
        return Plane::new(Vec3::ZERO, 0.0);
    };
    let (mut x, mut y, mut z) = (0.0f32, 0.0f32, 0.0f32);
    for pair in corners[1..].windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let (ax, ay, az) = (a.x - first.x, a.y - first.y, a.z - first.z);
        let (bx, by, bz) = (b.x - first.x, b.y - first.y, b.z - first.z);
        x += ay * bz - az * by;
        y += az * bx - ax * bz;
        z += ax * by - ay * bx;
    }
    let len = (x * x + y * y + z * z).sqrt();
    let normal = if len > 0.0 {
        Vec3::new(x / len, y / len, z / len)
    } else {
        Vec3::ZERO
    };
    let sum: f32 = corners
        .iter()
        .map(|p| normal.x * p.x + normal.y * p.y + normal.z * p.z)
        .sum();
    #[allow(clippy::cast_precision_loss)]
    Plane::new(normal, -(sum / corners.len() as f32))
}

impl PlaneOps for Plane {
    // D2-03797
    fn through(normal: Vec3, point: Vec3) -> Plane {
        // The offset summed z, then y, then x, wide, rounded as stored.
        let (nm, p) = (normal, point);
        Plane::new(
            normal,
            n(-((w(nm.z) * w(p.z) + w(p.y) * w(nm.y)) + w(p.x) * w(nm.x))),
        )
    }

    // D2-03798
    fn vertical(a: Vec3, b: Vec3) -> Plane {
        // Worked wide as the client works it: the normal's parts rounded only as stored, the
        // offset from the unrounded ones.
        let dx = w(a.x) - w(b.x);
        let dy = w(a.y) - w(b.y);
        let len = (dy * dy + dx * dx).sqrt();
        if len < w(EPS) * w(EPS) {
            return Plane::new(
                Vec3::new(0.0, 1.0, 0.0),
                n((w(a.z) + w(a.x)) * 0.0 - w(a.y)),
            );
        }
        let k = 1.0 / len;
        let nx = dy * k;
        let ny = -(k * dx);
        let ta = -(w(a.x) * nx + w(a.y) * ny);
        let tb = -(w(b.y) * ny + w(b.x) * nx);
        Plane::new(Vec3::new(n(nx), n(ny), 0.0), n((ta + tb) * 0.5))
    }

    // D2-03484
    fn which_side(&self, p: Vec3, eps: f32) -> Side {
        let n = self.normal;
        let d = p.x * n.x + p.y * n.y + p.z * n.z + self.d;
        if eps < d {
            Side::Positive
        } else if -eps <= d {
            Side::InPlane
        } else {
            Side::Negative
        }
    }

    fn dist_wide(&self, p: Vec3) -> f64 {
        let n = self.normal;
        f64::from(n.z) * f64::from(p.z)
            + f64::from(n.y) * f64::from(p.y)
            + f64::from(n.x) * f64::from(p.x)
            + f64::from(self.d)
    }

    /// Worked in wide precision as the client works it: the line's own offsets are narrowed
    /// first, the fraction is kept wide, and each corner of the point is the narrowed or wide
    /// product the client adds the start to. A fraction that is not a number counts as crossing.
    // D2-13131
    fn line_crossing(&self, a: Vec3, b: Vec3) -> Option<(Vec3, f64)> {
        let w = f64::from;
        let n = self.normal;
        let (dx, dy, dz) = (b.x - a.x, b.y - a.y, b.z - a.z);
        let along = (w(dz) * w(n.z) + w(dy) * w(n.y)) + w(dx) * w(n.x);
        if along.abs() < w(EPS) {
            return None;
        }
        let t = (-w(self.d) - ((w(a.x) * w(n.x) + w(n.z) * w(a.z)) + w(n.y) * w(a.y))) / along;
        if t < 0.0 || t > 1.0 {
            return None;
        }
        let narrow = |v: f64| {
            #[allow(clippy::cast_possible_truncation)]
            {
                v as f32
            }
        };
        let at = Vec3::new(
            narrow(w(dx) * t + w(a.x)),
            narrow(w(narrow(t * w(dy))) + w(a.y)),
            narrow(w(narrow(w(dz) * t)) + w(a.z)),
        );
        Some((at, 1.0 - t))
    }

    // D2-03613
    fn set_height(&self, v: &mut Vec3) -> bool {
        let m = self.normal;
        if m.z.abs() > EPS {
            v.z = n(-(((w(v.y) * w(m.y) + w(v.x) * w(m.x)) + w(self.d)) / w(m.z)));
            true
        } else {
            false
        }
    }

    // D2-05772
    fn snap_to_plane(&self, v: &mut Vec3) {
        let m = self.normal;
        if m.z.abs() > EPS {
            let k = 1.0 / w(m.z);
            let at_zero = ((w(v.y) * w(m.y) + w(v.x) * w(m.x)) + w(m.z) * 0.0) + w(self.d);
            v.z = n(-at_zero * k - -w(self.d) * k);
        }
    }

    // D2-13220
    fn is_valid(&self) -> bool {
        let m = self.normal;
        if m.x.is_nan() || m.y.is_nan() || m.z.is_nan() {
            return false;
        }
        let n2 = (w(m.x) * w(m.x) + w(m.y) * w(m.y)) + w(m.z) * w(m.z);
        (n2 - 1.0).abs() <= w(0.0004_f32) && !self.d.is_nan()
    }

    // D2-13130
    fn local_to_global(&self, to: &Position, from: &Position) -> Plane {
        let m = self.normal;
        let k = -self.d;
        let p = Vec3::new(k * m.x, k * m.y, k * m.z);
        let r = rotation_axes(from.frame.rotation);
        let turn = |a: f32, b: f32, c: f32| n((w(a) * w(m.y) + w(b) * w(m.x)) + w(c) * w(m.z));
        let g = Vec3::new(
            turn(r[3], r[0], r[6]),
            turn(r[4], r[1], r[7]),
            turn(r[5], r[2], r[8]),
        );
        let at = point_to_global_from(to, from, p);
        Plane::new(
            g,
            n(-((w(g.z) * w(at.z) + w(g.y) * w(at.y)) + w(g.x) * w(at.x))),
        )
    }

    // D2-13132
    fn time_of_intersection(&self, ray: &Ray) -> Option<(f64, bool)> {
        let m = self.normal;
        let (d, p) = (ray.dir, ray.point);
        let along = (w(d.y) * w(m.y) + w(d.z) * w(m.z)) + w(d.x) * w(m.x);
        if w(EPS) * w(EPS) > along.abs() {
            return None;
        }
        let t =
            (((w(p.y) * w(m.y) + w(p.z) * w(m.z)) + w(m.x) * w(p.x)) + w(self.d)) * (-1.0 / along);
        Some((t, t >= 0.0 || t.is_nan()))
    }

    // D2-13133
    fn find_intercept(&self, ray: &Ray) -> Option<Vec3> {
        let (t, ahead) = self.time_of_intersection(ray)?;
        if !ahead || (t < 0.0 && t > w(ray.length)) {
            return None;
        }
        let (d, p) = (ray.dir, ray.point);
        Some(Vec3::new(
            n(t * w(d.x) + w(p.x)),
            n(w(n(t * w(d.y))) + w(p.y)),
            n(w(n(t * w(d.z))) + w(p.z)),
        ))
    }

    // D2-13136
    fn intersect_line_segment(&self, a: Vec3, b: Vec3) -> Option<Vec3> {
        let m = self.normal;
        let dist = |v: Vec3| ((w(v.y) * w(m.y) + w(v.x) * w(m.x)) + w(m.z) * w(v.z)) + w(self.d);
        let (da, db) = (dist(a), dist(b));
        let (sa, sb) = (side_by(da, w(0.01_f32)), side_by(db, w(0.01_f32)));
        if !matches!(
            (sa, sb),
            (Side::Positive, Side::Negative) | (Side::Negative, Side::Positive)
        ) {
            return None;
        }
        let (fa, fb) = (n(da), n(db));
        let diff = n(w(fa) - w(fb));
        let t = if diff.abs() > EPS {
            w(fa) / w(diff)
        } else {
            0.0
        };
        let px = n((w(b.x) - w(a.x)) * t);
        let py = n((w(b.y) - w(a.y)) * t);
        let pz = n(w(n(w(b.z) - w(a.z))) * t);
        Some(Vec3::new(
            n(w(px) + w(a.x)),
            n(w(py) + w(a.y)),
            n(w(pz) + w(a.z)),
        ))
    }

    // D2-13137
    fn clip_ray_to_positive(&self, ray: &Ray, start: &mut f32, end: &mut f32) -> bool {
        let m = self.normal;
        let side_at = |frac: f32| {
            let l = w(ray.length);
            let (lx, ly, lz) = (l * w(ray.dir.x), n(l * w(ray.dir.y)), n(l * w(ray.dir.z)));
            let f = w(frac);
            let (x, y, z) = (lx * f, n(w(ly) * f), n(w(lz) * f));
            let (x, y, z) = (
                x + w(ray.point.x),
                w(y) + w(ray.point.y),
                w(z) + w(ray.point.z),
            );
            side_by(((z * w(m.z) + y * w(m.y)) + x * w(m.x)) + w(self.d), w(EPS))
        };
        let (s0, s1) = (side_at(*start), side_at(*end));
        if s0 != Side::Positive && s1 != Side::Positive {
            return false;
        }
        if s0 == Side::Negative || s1 == Side::Negative {
            let Some((t, true)) = self.time_of_intersection(ray) else {
                return false;
            };
            let frac = n(t / w(ray.length));
            if s0 == Side::Positive {
                *end = frac;
            } else {
                *start = frac;
            }
        }
        true
    }

    // D2-13140
    fn cut_ray(&self, ray: &Ray, positive: &mut Ray, negative: &mut Ray) -> Option<(bool, bool)> {
        let l = w(ray.length);
        let p = ray.point;
        let end = Vec3::new(
            n(l * w(ray.dir.x) + w(p.x)),
            n(w(n(l * w(ray.dir.y))) + w(p.y)),
            n(w(n(l * w(ray.dir.z))) + w(p.z)),
        );
        let m = self.normal;
        let dist = |v: Vec3| ((w(v.x) * w(m.x) + w(v.y) * w(m.y)) + w(v.z) * w(m.z)) + w(self.d);
        let (s0, s1) = (side_by(dist(p), w(EPS)), side_by(dist(end), w(EPS)));
        let not_neg = |s: Side| s != Side::Negative;
        let not_pos = |s: Side| s != Side::Positive;
        if not_neg(s0) && not_neg(s1) {
            *positive = *ray;
            return Some((true, false));
        }
        if not_pos(s0) && not_pos(s1) {
            *negative = *ray;
            return Some((false, true));
        }
        let at = self.find_intercept(ray)?;
        if s0 == Side::Positive {
            positive.init_line_segment(p, at);
            negative.init_line_segment(at, end);
        } else {
            negative.init_line_segment(p, at);
            positive.init_line_segment(at, end);
        }
        Some((true, true))
    }
}

/// Where `ray` first meets polygon `poly` (its corners and plane in the ray's frame), from either
/// face, as a fraction of its length: where the ray crosses the polygon's plane, that point lying
/// inside every edge.
// D2-04189
#[must_use]
pub fn polygon_trace_ray(poly: &dereth_physics::geom::Polygon, ray: &super::Ray) -> Option<f32> {
    let (at, left) = poly.plane.line_crossing(ray.point, ray.end_point())?;
    if !poly.point_in_polygon(at) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some((1.0 - left) as f32)
}

/// The side a point `d` from a plane is on, within 0.0002 counting as in it.
#[must_use]
pub fn side_of(d: f64) -> Side {
    if d > f64::from(EPS) {
        Side::Positive
    } else if d >= -f64::from(EPS) {
        Side::InPlane
    } else {
        Side::Negative
    }
}

/// A polygon: its corners in order.
pub type Polygon = Vec<Vec3>;

#[cfg(test)]
mod corner_tests {
    //! Behaviour: none (a polygon's plane, held to hand-worked values)
    use super::*;

    #[test]
    fn a_polygons_plane_is_the_mean_of_its_corners_along_its_normal() {
        let square = [
            Vec3::new(0.0, 0.0, 2.0),
            Vec3::new(1.0, 0.0, 2.0),
            Vec3::new(1.0, 1.0, 2.0),
            Vec3::new(0.0, 1.0, 2.0),
        ];
        let p = plane_of_corners(&square);
        assert_eq!(p.normal, Vec3::new(0.0, 0.0, 1.0));
        assert_eq!(p.d, -2.0);
    }
}
