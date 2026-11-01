//! Planes as this world tests against them. The plane itself is the shared one (a unit normal and
//! an offset, a point `p` lying `n·p + d` from it); what this world does with one is here.

pub use dereth_primitives::shape::Plane;
use dereth_primitives::Vec3;

use super::{EPS, EPS_SQ};

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
}

impl PlaneOps for Plane {
    // D2-03797
    fn through(normal: Vec3, point: Vec3) -> Plane {
        Plane::from_normal_and_point(normal, point)
    }

    // D2-03798
    fn vertical(a: Vec3, b: Vec3) -> Plane {
        let dx = a.x - b.x;
        let dy = a.y - b.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < EPS_SQ {
            return Plane::new(Vec3::new(0.0, 1.0, 0.0), (a.z + a.x) * 0.0 - a.y);
        }
        let k = 1.0 / len;
        let nx = dy * k;
        let ny = -(k * dx);
        Plane::new(
            Vec3::new(nx, ny, 0.0),
            (-(b.x * nx + b.y * ny) + -(a.y * ny + a.x * nx)) * 0.5,
        )
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
