//! The spatial primitives: a sphere, a cylinder-sphere, a plane and an axis-aligned box, as plain
//! data with their pure constructors and arithmetic.
//!
//! Defined once, here, because several crates use the same shapes: the dat decoders produce them
//! from a setup or a BSP node, the animation layer carries them in its setup data, physics collides
//! with them, and world rendering culls and picks against planes. Each crate keeps its own
//! behaviour over these structs (physics' collision tests are its extension traits); only the data
//! and the arithmetic every user agrees on are shared, so a plane decoded from the dat is the plane
//! physics tests against, with no field-by-field copy in between.

use crate::space::Vec3;

/// A sphere: a centre and a radius, in whatever space the caller is working in.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
}

impl Sphere {
    #[must_use]
    pub const fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }
}

/// A cylinder-sphere: a vertical cylinder described by its low point, radius and height, the
/// primitive used for creature collision volumes. The dat serialises `radius` before `height`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CylSphere {
    pub low_pt: Vec3,
    pub radius: f32,
    pub height: f32,
}

/// How a game works out its planes and the physics over them: building a polygon's plane, and
/// moving a body along, onto and out of one. The two games' clients differ in the last bit, so
/// each game chooses its own, once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaneArithmetic {
    /// The first game's: in double precision, kept as a float only where its client keeps one.
    #[default]
    Double,
    /// Single precision at every step.
    Single,
}

/// A plane in the client's form: a normal `N` and the offset `d` such that `N·p + d == 0` on the
/// plane, so `N·p + d` is the signed distance of `p` when `N` is a unit vector. The dat serialises
/// the normal, then `d`. The plane is in whatever frame its `d` was computed in.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Plane {
    pub normal: Vec3,
    pub d: f32,
}

impl Plane {
    #[must_use]
    pub const fn new(normal: Vec3, d: f32) -> Self {
        Self { normal, d }
    }

    /// The plane through `point` with normal `normal`: `d = -dot(N, point)`.
    #[inline]
    #[must_use]
    pub fn from_normal_and_point(normal: Vec3, point: Vec3) -> Self {
        Self {
            normal,
            d: -(point.x * normal.x + point.y * normal.y + point.z * normal.z),
        }
    }

    /// The plane of the triangle `a`, `b`, `c`, worked in `arithmetic`. The normal is
    /// `(b - a) x (c - a)` divided by its length, so a triangle wound counter-clockwise seen from
    /// above faces up; a triangle with no area faces straight up. `d` is
    /// `-(N·a + N·b + N·c) / 3`, not `-N·a`: for a planar triangle the two are the same number
    /// mathematically and different floats in practice.
    ///
    /// In [`PlaneArithmetic::Double`] it is worked in double precision and kept as a float only
    /// at certain steps, and the plane's last bit depends on which: the first edge's x and y, each
    /// part of the cross product, and the reciprocal length that scales y and z are kept as
    /// floats; the reciprocal that scales x and the whole sum behind `d` stay in double precision,
    /// and `d` is rounded once. In [`PlaneArithmetic::Single`] every step is a float.
    #[must_use]
    pub fn of_triangle(a: Vec3, b: Vec3, c: Vec3, arithmetic: PlaneArithmetic) -> Self {
        match arithmetic {
            PlaneArithmetic::Double => Self::triangle_double(a, b, c),
            PlaneArithmetic::Single => Self::triangle_single(a, b, c),
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn triangle_double(a: Vec3, b: Vec3, c: Vec3) -> Self {
        let kept = |x: f64| f64::from(x as f32);
        let edge = |v: Vec3| {
            (
                f64::from(v.x) - f64::from(a.x),
                f64::from(v.y) - f64::from(a.y),
                f64::from(v.z) - f64::from(a.z),
            )
        };
        let (ux, uy, uz) = edge(b);
        let (ux, uy) = (kept(ux), kept(uy));
        let (vx, vy, vz) = edge(c);
        let nx = kept(uy * vz - uz * vy);
        let ny = kept(uz * vx - vz * ux);
        let nz = kept(vy * ux - uy * vx);
        let len = ((nz * nz + ny * ny) + nx * nx).sqrt();
        let normal = if len > 0.0 {
            let inv = 1.0 / len;
            let inv_kept = kept(inv);
            Vec3::new(
                (inv * nx) as f32,
                (inv_kept * ny) as f32,
                (inv_kept * nz) as f32,
            )
        } else {
            Vec3::new(0.0, 0.0, 1.0)
        };
        let (gx, gy, gz) = (
            f64::from(normal.x),
            f64::from(normal.y),
            f64::from(normal.z),
        );
        let sum: f64 = [a, b, c]
            .iter()
            .map(|v| (gz * f64::from(v.z) + gy * f64::from(v.y)) + gx * f64::from(v.x))
            .sum();
        Self {
            normal,
            d: (-(sum / 3.0)) as f32,
        }
    }

    fn triangle_single(a: Vec3, b: Vec3, c: Vec3) -> Self {
        let u = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
        let v = Vec3::new(c.x - a.x, c.y - a.y, c.z - a.z);
        let n = Vec3::new(
            u.y * v.z - u.z * v.y,
            u.z * v.x - u.x * v.z,
            u.x * v.y - u.y * v.x,
        );
        let len = n.magnitude();
        let normal = if len > 0.0 {
            let inv = 1.0 / len;
            Vec3::new(n.x * inv, n.y * inv, n.z * inv)
        } else {
            Vec3::new(0.0, 0.0, 1.0)
        };
        let sum = normal.dot(a) + normal.dot(b) + normal.dot(c);
        Self {
            normal,
            d: -(sum / 3.0),
        }
    }

    /// The plane equation at `p`: `dot(N, p) + d`, summed in that order.
    #[inline]
    #[must_use]
    pub fn dot_point(&self, p: Vec3) -> f32 {
        self.normal.x * p.x + self.normal.y * p.y + self.normal.z * p.z + self.d
    }
}

/// An axis-aligned box given by its minimum and maximum corners, in whatever frame they were
/// computed in.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BBox {
    /// Minimum corner.
    pub min: Vec3,
    /// Maximum corner.
    pub max: Vec3,
}

impl BBox {
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Grow the box to contain `p`.
    pub fn adjust(&mut self, p: Vec3) {
        self.min = Vec3::new(
            self.min.x.min(p.x),
            self.min.y.min(p.y),
            self.min.z.min(p.z),
        );
        self.max = Vec3::new(
            self.max.x.max(p.x),
            self.max.y.max(p.y),
            self.max.z.max(p.z),
        );
    }

    /// The box over a set of points: seeded from the first and grown over the rest. No points
    /// give the zero box, which is what the client writes for an empty vertex array.
    #[must_use]
    pub fn of_points(points: impl IntoIterator<Item = Vec3>) -> Self {
        let mut it = points.into_iter();
        let Some(first) = it.next() else {
            return Self::default();
        };
        let mut b = Self {
            min: first,
            max: first,
        };
        for p in it {
            b.adjust(p);
        }
        b
    }
}

/// Where a volume lies against a bounding region: wholly outside it, straddling its boundary, or
/// wholly inside it. Physics answers it for a sphere against a cell's volume; drawing answers it
/// for a sphere or a terrain column against the view cone. The tests that produce it differ; the
/// three answers, their order and their numbers (`0`, `1`, `2`) are the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bounding {
    Outside = 0,
    PartiallyInside = 1,
    EntirelyInside = 2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bounding_answers_keep_their_numbers_and_order() {
        assert_eq!(Bounding::Outside as i32, 0);
        assert_eq!(Bounding::PartiallyInside as i32, 1);
        assert_eq!(Bounding::EntirelyInside as i32, 2);
        assert!(Bounding::Outside < Bounding::PartiallyInside);
        assert!(Bounding::PartiallyInside < Bounding::EntirelyInside);
    }

    #[test]
    fn a_plane_through_a_point_has_that_point_on_it() {
        let p = Plane::from_normal_and_point(Vec3::new(0.0, 0.0, 1.0), Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(p.d, -5.0);
        assert_eq!(p.dot_point(Vec3::new(-9.0, 2.0, 5.0)), 0.0);
        assert_eq!(p.dot_point(Vec3::new(0.0, 0.0, 7.0)), 2.0);
    }

    #[test]
    fn a_box_of_points_is_their_componentwise_hull() {
        assert_eq!(BBox::of_points([]), BBox::default());
        let b = BBox::of_points([
            Vec3::new(1.0, -2.0, 3.0),
            Vec3::new(-1.0, 5.0, 0.0),
            Vec3::new(0.0, 0.0, 9.0),
        ]);
        assert_eq!(
            b,
            BBox::new(Vec3::new(-1.0, -2.0, 0.0), Vec3::new(1.0, 5.0, 9.0))
        );
    }
}
