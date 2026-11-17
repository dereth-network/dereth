//! `Plane` — `{ Vector3 N; float d; }`, with `dot(N, p) + d` the signed distance.
//!
//! Transcribed from the client's own plane code, plus the plane constructor, the
//! snap-to-plane step, the height setter and the time-of-intersection solve.

use dereth_primitives::shape::PlaneArithmetic;
use dereth_primitives::{Position, Vec3};

use crate::geom::sphere::Ray;
use crate::globals::EPSILON;
use crate::math::{self, V3};

/// `Sidedness`. `POSITIVE = 1`, `NEGATIVE = 0`; `CROSSING` is the third value the box test adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sidedness {
    Negative = 0,
    Positive = 1,
    Crossing = 2,
}

/// A plane -- the normal and `d` -- in the frame its `d` was computed in. The data and the plane
/// equation are `dereth_primitives`', shared with the dat decoders and world rendering; the physics
/// over it is [`PlaneExt`].
pub use dereth_primitives::shape::Plane;

/// The plane's physics: classification, the snap and height solves, the ray intersection, the
/// frame change and the box test. Bring the trait into scope to call them.
pub trait PlaneExt {
    /// Classify point `p` with tolerance `eps`.
    #[must_use]
    fn which_side(&self, p: Vec3, eps: f32) -> Sidedness;

    /// Force a movement vector's Z so that the vector runs
    /// **parallel** to the plane. A no-op when the plane is near vertical.
    ///
    /// The original computes it as `-(v.x*N.x + v.y*N.y + d)/N.z - (-d)/N.z`, which is the
    /// direction-only form: the two `d/N.z` terms cancel, leaving `-(v.x*N.x + v.y*N.y)/N.z`.
    /// It is written out here in the original's shape because the two divisions round separately.
    /// This is the first game's, worked as [`PlaneArithmetic::Double`].
    fn snap_to_plane(&self, v: &mut Vec3);

    /// [`Self::snap_to_plane`] worked in `arithmetic`: in double precision, the reciprocal too,
    /// and rounded to single once; or in single precision at every step.
    fn snap_to_plane_in(&self, v: &mut Vec3, arithmetic: PlaneArithmetic);

    /// Solve the plane for Z at `(p.x, p.y)`. Returns `false`
    /// (leaving `p` untouched) for a near-vertical plane.
    fn set_height(&self, p: &mut Vec3) -> bool;

    /// Requires `|dot(N, dir)| >= 2e-4` and
    /// a non-negative time.
    #[must_use]
    fn compute_time_of_intersection(&self, ray: &Ray) -> Option<f32>;

    /// Express a plane given in `from`'s frame in `at`'s.
    ///
    /// Note the original rebuilds `d` from the transformed point `-d * N`, not by transforming
    /// `d` directly; the two differ once the frames have different origins. This is the first
    /// game's, worked as [`PlaneArithmetic::Double`].
    #[must_use]
    fn localtoglobal(&self, at: &Position, from: &Position) -> Self;

    /// [`Self::localtoglobal`] worked in `arithmetic`: in double precision with `from`'s
    /// matrix as the client keeps it ([`math::l2g_wide`]), each part of the normal, the point
    /// and `d` rounded to single once; or in single precision at every step.
    #[must_use]
    fn localtoglobal_in(&self, at: &Position, from: &Position, arithmetic: PlaneArithmetic)
        -> Self;

    /// The eight corners against the plane with a `2e-4`
    /// band; any disagreement is `CROSSING`.
    #[must_use]
    fn intersect_box(&self, min: Vec3, max: Vec3) -> Sidedness;
}

impl PlaneExt for Plane {
    fn which_side(&self, p: Vec3, eps: f32) -> Sidedness {
        let d = self.dot_point(p);
        if d <= eps {
            if d >= -eps {
                Sidedness::Crossing
            } else {
                Sidedness::Negative
            }
        } else {
            Sidedness::Positive
        }
    }

    fn snap_to_plane(&self, v: &mut Vec3) {
        self.snap_to_plane_in(v, PlaneArithmetic::Double);
    }

    fn snap_to_plane_in(&self, v: &mut Vec3, arithmetic: PlaneArithmetic) {
        if self.normal.z.abs() <= EPSILON {
            return;
        }
        if arithmetic == PlaneArithmetic::Single {
            let inv = 1.0 / self.normal.z;
            v.z = 0.0;
            v.z = -(self.normal.z * 0.0 + v.x * self.normal.x + v.y * self.normal.y + self.d) * inv
                - -self.d * inv;
            return;
        }
        let (nx, ny, nz) = (
            f64::from(self.normal.x),
            f64::from(self.normal.y),
            f64::from(self.normal.z),
        );
        let d = f64::from(self.d);
        let inv = 1.0 / nz;
        let sum = ((f64::from(v.y) * ny + f64::from(v.x) * nx) + nz * 0.0) + d;
        #[allow(clippy::cast_possible_truncation)]
        {
            v.z = (-sum * inv - -d * inv) as f32;
        }
    }

    fn set_height(&self, p: &mut Vec3) -> bool {
        if self.normal.z.abs() > EPSILON {
            // Worked in double precision and rounded once.
            let n = self.normal;
            let sum = (f64::from(p.y) * f64::from(n.y) + f64::from(p.x) * f64::from(n.x))
                + f64::from(self.d);
            #[allow(clippy::cast_possible_truncation)]
            {
                p.z = (-(sum / f64::from(n.z))) as f32;
            }
            true
        } else {
            false
        }
    }

    fn compute_time_of_intersection(&self, ray: &Ray) -> Option<f32> {
        let den = ray.dir.x * self.normal.x + ray.dir.y * self.normal.y + ray.dir.z * self.normal.z;
        if den.abs() < EPSILON {
            return None;
        }
        let t = (-1.0 / den) * self.dot_point(ray.point);
        if t >= 0.0 {
            Some(t)
        } else {
            None
        }
    }

    fn localtoglobal(&self, at: &Position, from: &Position) -> Self {
        self.localtoglobal_in(at, from, PlaneArithmetic::Double)
    }

    #[allow(clippy::cast_possible_truncation)]
    fn localtoglobal_in(
        &self,
        at: &Position,
        from: &Position,
        arithmetic: PlaneArithmetic,
    ) -> Self {
        let point = self.normal.mul(-self.d);
        if arithmetic == PlaneArithmetic::Single {
            let m = math::l2g(from.frame.rotation);
            let normal = math::localtoglobalvec(m, self.normal);
            let p = math::pos_localtoglobal(at, from, point);
            return Self {
                normal,
                d: -(normal.x * p.x + normal.y * p.y + normal.z * p.z),
            };
        }
        let m = math::l2g_wide(from.frame.rotation).0.map(f64::from);
        let n = self.normal;
        let (nx, ny, nz) = (f64::from(n.x), f64::from(n.y), f64::from(n.z));
        let normal = Vec3::new(
            ((m[6] * nz + m[3] * ny) + m[0] * nx) as f32,
            ((m[7] * nz + m[4] * ny) + m[1] * nx) as f32,
            ((m[8] * nz + m[5] * ny) + m[2] * nx) as f32,
        );
        let (px, py, pz) = (f64::from(point.x), f64::from(point.y), f64::from(point.z));
        let o = from.frame.origin;
        let turned = Vec3::new(
            (((m[3] * py + m[0] * px) + m[6] * pz) + f64::from(o.x)) as f32,
            (((m[4] * py + m[1] * px) + m[7] * pz) + f64::from(o.y)) as f32,
            (((m[5] * py + m[2] * px) + m[8] * pz) + f64::from(o.z)) as f32,
        );
        let p = crate::landdefs::get_block_offset(at.cell, from.cell).add(turned);
        let (gx, gy, gz) = (
            f64::from(normal.x),
            f64::from(normal.y),
            f64::from(normal.z),
        );
        Self {
            normal,
            d: (-((gz * f64::from(p.z) + gy * f64::from(p.y)) + gx * f64::from(p.x))) as f32,
        }
    }

    fn intersect_box(&self, min: Vec3, max: Vec3) -> Sidedness {
        let side = self.which_side(min, EPSILON);
        if side == Sidedness::Crossing {
            return Sidedness::Crossing;
        }
        // The original's corner order, which only matters for which corner short-circuits first.
        let corners = [
            Vec3::new(max.x, max.y, max.z),
            Vec3::new(max.x, min.y, min.z),
            Vec3::new(min.x, max.y, min.z),
            Vec3::new(min.x, min.y, max.z),
            Vec3::new(max.x, max.y, min.z),
            Vec3::new(max.x, min.y, max.z),
            Vec3::new(min.x, max.y, max.z),
        ];
        for c in corners {
            if self.which_side(c, EPSILON) != side {
                return Sidedness::Crossing;
            }
        }
        side
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: and the three
    // out-of-file copies named in the module docs. Each assertion below is evaluated, not
    // restated: the expected numbers are computed from the plane equation by hand.

    #[test]
    fn from_normal_and_point_puts_the_point_on_the_plane() {
        let n = Vec3::new(0.0, 0.0, 1.0);
        let p = Plane::from_normal_and_point(n, Vec3::new(3.0, 4.0, 5.0));
        assert_eq!(p.d, -5.0);
        assert_eq!(p.dot_point(Vec3::new(-9.0, 2.0, 5.0)), 0.0);
        assert_eq!(p.dot_point(Vec3::new(0.0, 0.0, 7.0)), 2.0);
    }

    #[test]
    fn set_height_solves_the_plane_and_refuses_a_vertical_one() {
        // A 45-degree ramp rising in +X: N = (-1, 0, 1)/sqrt(2) through the origin.
        let inv = 1.0 / 2.0_f32.sqrt();
        let plane = Plane {
            normal: Vec3::new(-inv, 0.0, inv),
            d: 0.0,
        };
        let mut p = Vec3::new(4.0, 0.0, 999.0);
        assert!(plane.set_height(&mut p));
        assert!((p.z - 4.0).abs() < 1e-4, "{p:?}");

        let vertical = Plane {
            normal: Vec3::new(1.0, 0.0, 0.0),
            d: 0.0,
        };
        let mut q = Vec3::new(1.0, 1.0, 42.0);
        assert!(!vertical.set_height(&mut q));
        assert_eq!(q.z, 42.0, "a rejected solve must leave the point alone");
    }

    #[test]
    fn set_height_boundary_is_strictly_greater_than_epsilon() {
        let just_over = Plane {
            normal: Vec3::new(0.0, 0.0, 0.000_200_1),
            d: 0.0,
        };
        let just_under = Plane {
            normal: Vec3::new(0.0, 0.0, 0.000_199_9),
            d: 0.0,
        };
        let mut p = Vec3::ZERO;
        assert!(just_over.set_height(&mut p));
        assert!(!just_under.set_height(&mut p));
    }

    #[test]
    fn set_height_is_worked_in_double_precision_and_rounded_once() {
        // The 48 degree slope's plane: worked in single precision, the height lands one unit in
        // the last place lower (0x42ed_4e1d).
        let plane = Plane {
            normal: Vec3::new(
                f32::from_bits(0xbf39_7652),
                f32::from_bits(0xbe2b_3224),
                f32::from_bits(0x3f2b_3224),
            ),
            d: f32::from_bits(0xc245_f1fa),
        };
        let mut p = Vec3::new(
            f32::from_bits(0x3f93_3f32),
            f32::from_bits(0x432d_a017),
            0.0,
        );
        assert!(plane.set_height(&mut p));
        assert_eq!(p.z.to_bits(), 0x42ed_4e1e, "{p:?}");
    }

    #[test]
    fn snap_to_plane_makes_the_vector_parallel_to_the_plane() {
        // 45-degree ramp again; a purely horizontal +X movement must gain +X worth of Z.
        let inv = 1.0 / 2.0_f32.sqrt();
        let plane = Plane {
            normal: Vec3::new(-inv, 0.0, inv),
            d: -3.0,
        };
        let mut v = Vec3::new(2.0, 0.0, 0.0);
        plane.snap_to_plane(&mut v);
        assert!((v.z - 2.0).abs() < 1e-4, "{v:?}");
        // and the result really is parallel: adding it to a point on the plane stays on it
        let mut on = Vec3::new(0.0, 0.0, 3.0 / inv);
        assert!((plane.dot_point(on)).abs() < 1e-5);
        on = on.add(v);
        assert!(
            (plane.dot_point(on)).abs() < 1e-4,
            "{}",
            plane.dot_point(on)
        );
    }

    #[test]
    fn snap_to_plane_is_worked_in_double_precision_and_rounded_once() {
        // A sled's step on a 9 degree slope far from the origin: the two terms in `d` cancel,
        // and worked in single precision what is left of them is off in its fifth digit
        // (0x3c2b_c000).
        let plane = Plane {
            normal: Vec3::new(
                0.0,
                f32::from_bits(0x3e28_5835),
                f32::from_bits(0x3f7c_8450),
            ),
            d: f32::from_bits(0xc2bd_633c),
        };
        let mut v = Vec3::new(
            f32::from_bits(0x3e2a_26b0),
            f32::from_bits(0xbd80_cfd3),
            0.0,
        );
        plane.snap_to_plane(&mut v);
        assert_eq!(v.z.to_bits(), 0x3c2b_bfc4, "{v:?}");
    }

    #[test]
    fn snap_to_plane_is_a_no_op_on_a_vertical_plane() {
        let vertical = Plane {
            normal: Vec3::new(1.0, 0.0, 0.0),
            d: 0.0,
        };
        let mut v = Vec3::new(1.0, 2.0, 3.0);
        vertical.snap_to_plane(&mut v);
        assert_eq!(v, Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn compute_time_of_intersection_rejects_a_grazing_ray_and_a_backwards_hit() {
        let plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -10.0,
        };
        let hit = Ray {
            point: Vec3::ZERO,
            dir: Vec3::new(0.0, 0.0, 2.0),
        };
        assert_eq!(plane.compute_time_of_intersection(&hit), Some(5.0));
        // parallel
        let graze = Ray {
            point: Vec3::ZERO,
            dir: Vec3::new(1.0, 0.0, 0.000_1),
        };
        assert_eq!(plane.compute_time_of_intersection(&graze), None);
        // behind
        let back = Ray {
            point: Vec3::ZERO,
            dir: Vec3::new(0.0, 0.0, -1.0),
        };
        assert_eq!(plane.compute_time_of_intersection(&back), None);
    }

    #[test]
    fn intersect_box_agrees_on_all_eight_corners_or_says_crossing() {
        let plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: 0.0,
        };
        assert_eq!(
            plane.intersect_box(Vec3::new(-1.0, -1.0, 1.0), Vec3::new(1.0, 1.0, 2.0)),
            Sidedness::Positive
        );
        assert_eq!(
            plane.intersect_box(Vec3::new(-1.0, -1.0, -2.0), Vec3::new(1.0, 1.0, -1.0)),
            Sidedness::Negative
        );
        assert_eq!(
            plane.intersect_box(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0)),
            Sidedness::Crossing
        );
        // a box sitting inside the 2e-4 band is CROSSING, not POSITIVE
        assert_eq!(
            plane.intersect_box(Vec3::new(-1.0, -1.0, 0.0), Vec3::new(1.0, 1.0, 0.000_1)),
            Sidedness::Crossing
        );
    }
}
