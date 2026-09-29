//! [`Sphere`] and [`CylSphere`] — the pure geometric half.
//!
//! Transcribed from the client's own sphere and cylinder-sphere code.
//!
//! The two shapes are plain data in `dereth_primitives::shape`, shared with the dat decoders and the
//! animation layer; the collision tests on them are this crate's, as the [`SphereExt`] and
//! [`CylSphereExt`] extension traits. Bring the trait into scope to call them.
//!
//! The epsilon convention is applied by the **caller**, not here: the
//! dispatchers in [`crate::transition::collide`] build `sum = r1 + r2 - 0.0002` for an overlap
//! test and `sum + 0.0002` for a swept one, and hand the finished sum to these functions. Keeping
//! the sign out of the primitive is what the client does and is why both signs are visible at the
//! one place that matters.

use dereth_primitives::Vec3;

use crate::globals::EPSILON;
use crate::math::V3;

/// A sphere — a centre and a radius, in whatever space the caller is working in.
pub use dereth_primitives::shape::Sphere;

/// A cylinder-sphere — `{ low point, height, radius }`, the vertical primitive used
/// for creature collision volumes.
pub use dereth_primitives::shape::CylSphere;

/// `Ray` — a point and a direction. The direction is **not** normalised; every consumer divides
/// by `|dir|^2`, so a longer ray is a shorter time.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Ray {
    pub point: Vec3,
    pub dir: Vec3,
}

/// The sphere's collision tests.
pub trait SphereExt: Sized {
    /// The dummy sphere the client uses for any object with no part array or no
    /// spheres: centre `(0, 0, 0.1)`, radius `0.1`.
    #[must_use]
    fn dummy() -> Self;

    /// Sphere-versus-sphere -- `|delta|^2 <= radius_sum^2`.
    ///
    /// `delta` is `other.center - this.center` and `radius_sum` already carries the caller's
    /// epsilon.
    #[must_use]
    fn collides_with_sphere(delta: Vec3, radius_sum: f32) -> bool;

    /// The time of collision -- the quadratic sweep.
    ///
    /// `movement` is the sub-step displacement and `delta` the centre separation; the original's
    /// first parameter is the one squared into `a`, i.e. the movement. Returns `-1.0` when there
    /// is no usable root, which is the sentinel every caller tests for.
    #[must_use]
    fn find_time_of_collision(movement: Vec3, delta: Vec3, radius_sum: f32) -> f32;

    /// Sphere-versus-ray.
    ///
    /// The first guard is `c <= 0.0` — a ray starting **inside** the sphere is accepted; it is the
    /// NaN-aware `!(c > 0)`.
    #[must_use]
    fn sphere_intersects_ray(&self, ray: &Ray) -> Option<f32>;
}

impl SphereExt for Sphere {
    fn dummy() -> Self {
        Self {
            center: Vec3::new(0.0, 0.0, crate::globals::DUMMY_SPHERE_CENTER_Z),
            radius: crate::globals::DUMMY_SPHERE_RADIUS,
        }
    }

    #[inline]
    fn collides_with_sphere(delta: Vec3, radius_sum: f32) -> bool {
        delta.mag2() <= radius_sum * radius_sum
    }

    fn find_time_of_collision(movement: Vec3, delta: Vec3, radius_sum: f32) -> f32 {
        let a = movement.mag2();
        let b = -(movement.x * delta.x + delta.y * movement.y + delta.z * movement.z);
        let c = delta.mag2() - radius_sum * radius_sum;
        // Note the guard order: c first, then the discriminant is computed, then a, then the
        // discriminant sign. Reordering changes nothing numerically but this is the shape.
        if c >= EPSILON {
            let disc = b * b - c * a;
            if a >= EPSILON && disc >= 0.0 {
                let q = disc.sqrt();
                let t = b - q;
                return if t >= 0.0 { t / a } else { (q + b) / a };
            }
        }
        -1.0
    }

    fn sphere_intersects_ray(&self, ray: &Ray) -> Option<f32> {
        let d = ray.point.sub(self.center);
        let a = ray.dir.mag2();
        let b = -(d.x * ray.dir.x + d.y * ray.dir.y + d.z * ray.dir.z);
        let c = d.mag2() - self.radius * self.radius;
        if c <= 0.0 {
            let disc = b * b - c * a;
            if a >= EPSILON && disc >= 0.0 {
                let q = disc.sqrt();
                return Some(if q < b { (b - q) / a } else { (q + b) / a });
            }
        }
        None
    }
}

/// The cylinder-sphere's collision tests.
pub trait CylSphereExt {
    /// A 2-D radial test plus a vertical slab
    /// test centred on the cylinder's mid-height.
    ///
    /// Note the vertical half of the test applies its **own** `- 0.0002` to the sphere radius,
    /// independently of whatever `radius_sum` the caller built for the radial half.
    #[must_use]
    fn collides_with_sphere(&self, sphere: &Sphere, delta: Vec3, radius_sum: f32) -> bool;

    /// The cylinder-sphere's normal of collision.
    ///
    /// `curr_center` is the sphere path's current global centre for `index`, in the cylinder's
    /// space; `delta` is the
    /// separation used by the caller's overlap test. Returns `None` where the original returns 0.
    ///
    /// **This throws away half the original's answer** and is kept only because existing callers
    /// use this shape. The client writes the normal *and* returns a flag, and the two are
    /// independent: on the "radially outside but vertically clear of the cap" path it returns 0
    /// **having already written the radial normal**, and
    /// `crate::transition::cylinder::collide_with_point` uses both halves. New code wants
    /// [`Self::normal_of_collision_full`].
    #[must_use]
    fn normal_of_collision(
        &self,
        curr_center: Vec3,
        delta: Vec3,
        sphere_radius: f32,
        radius: f32,
    ) -> Option<Vec3>;

    /// Both halves of the original's answer: the
    /// integer flag it returns and the vector it writes through its out-parameter.
    ///
    /// The out-parameter is written on **every** path, including the one that returns 0, and the
    /// cylinder collision path branches on the flag while still using the
    /// vector.
    ///
    /// The cap-normal sign is `!(delta.z - d.z <= 0)` -> `-1`: the test is strictly greater than
    /// zero, so an exact tie takes `+1`.
    #[must_use]
    fn normal_of_collision_full(
        &self,
        curr_center: Vec3,
        delta: Vec3,
        sphere_radius: f32,
        radius: f32,
    ) -> (bool, Vec3);
}

impl CylSphereExt for CylSphere {
    fn collides_with_sphere(&self, sphere: &Sphere, delta: Vec3, radius_sum: f32) -> bool {
        if delta.y * delta.y + delta.x * delta.x > radius_sum * radius_sum {
            return false;
        }
        let half = self.height * 0.5;
        (half - delta.z).abs() <= (sphere.radius - EPSILON) + half
    }

    fn normal_of_collision(
        &self,
        curr_center: Vec3,
        delta: Vec3,
        sphere_radius: f32,
        radius: f32,
    ) -> Option<Vec3> {
        let (ok, n) = self.normal_of_collision_full(curr_center, delta, sphere_radius, radius);
        if ok {
            Some(n)
        } else {
            None
        }
    }

    fn normal_of_collision_full(
        &self,
        curr_center: Vec3,
        delta: Vec3,
        sphere_radius: f32,
        radius: f32,
    ) -> (bool, Vec3) {
        let d = curr_center.sub(self.low_pt);
        if radius * radius < d.x * d.x + d.y * d.y {
            let n = Vec3::new(d.x, d.y, 0.0);
            let half = self.height * 0.5;
            if (sphere_radius - EPSILON) + half < (half - d.z).abs()
                && (d.z - delta.z).abs() > EPSILON
            {
                // The normal is still written; only the flag says "no".
                return (false, n);
            }
            (true, n)
        } else {
            let up = if delta.z - d.z <= 0.0 { 1.0 } else { -1.0 };
            (true, Vec3::new(0.0, 0.0, up))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the client's own sphere code (collides_with_sphere, find_time_of_collision,
    // sphere_intersects_ray) and its cylinder-sphere code (collides_with_sphere,
    // normal_of_collision).

    /// Contract item 5.5: overlap subtracts the epsilon, swept tests add it. This table walks
    /// both sides of the boundary in both directions.
    #[test]
    fn overlap_epsilon_boundary_is_r1_plus_r2_minus_epsilon() {
        let (r1, r2) = (1.0_f32, 2.0_f32);
        let sum = r1 + r2 - EPSILON; // what the dispatcher builds
                                     // Exactly at the sum: `<=` accepts.
        assert!(Sphere::collides_with_sphere(Vec3::new(sum, 0.0, 0.0), sum));
        // A hair beyond: rejected.
        assert!(!Sphere::collides_with_sphere(
            Vec3::new(sum + 1e-4, 0.0, 0.0),
            sum
        ));
        // At the *unadjusted* r1 + r2 the epsilon is what decides: with the epsilon subtracted
        // the spheres do NOT touch, and with it added they would. That is the whole point.
        assert!(!Sphere::collides_with_sphere(
            Vec3::new(r1 + r2, 0.0, 0.0),
            sum
        ));
        let swept_sum = r1 + r2 + EPSILON;
        assert!(Sphere::collides_with_sphere(
            Vec3::new(r1 + r2, 0.0, 0.0),
            swept_sum
        ));
    }

    #[test]
    fn find_time_of_collision_returns_the_entry_time_of_a_head_on_sweep() {
        // A unit sphere 10 m away, closing at 1 m per sub-step, radius sum 1: contact at t = 9.
        // `delta` points FROM the obstacle TO the mover's current centre and `movement` is the
        // mover's displacement, which is the operand order collide_with_point passes.
        let delta = Vec3::new(10.0, 0.0, 0.0);
        let movement = Vec3::new(-1.0, 0.0, 0.0);
        let t = Sphere::find_time_of_collision(movement, delta, 1.0);
        assert!((t - 9.0).abs() < 1e-4, "{t}");
    }

    #[test]
    fn find_time_of_collision_rejects_an_already_overlapping_pair() {
        // c < 0.0002 means the spheres already overlap; the original returns the -1 sentinel.
        let delta = Vec3::new(0.5, 0.0, 0.0);
        let movement = Vec3::new(-1.0, 0.0, 0.0);
        assert_eq!(Sphere::find_time_of_collision(movement, delta, 1.0), -1.0);
    }

    #[test]
    fn find_time_of_collision_rejects_a_stationary_sweep() {
        // a < 0.0002: no movement, so no time of collision however close the spheres are.
        let delta = Vec3::new(10.0, 0.0, 0.0);
        assert_eq!(Sphere::find_time_of_collision(Vec3::ZERO, delta, 1.0), -1.0);
    }

    #[test]
    fn find_time_of_collision_rejects_a_miss() {
        // Passing 5 m to the side of a sphere pair whose radii sum to 1.
        let delta = Vec3::new(10.0, 5.0, 0.0);
        let movement = Vec3::new(-1.0, 0.0, 0.0);
        assert_eq!(Sphere::find_time_of_collision(movement, delta, 1.0), -1.0);
    }

    #[test]
    fn find_time_of_collision_takes_the_far_root_when_the_near_one_is_behind() {
        // Moving away from a sphere we have already passed: b - q is negative, so the second
        // root is returned. The client does this rather than reporting "no collision".
        let delta = Vec3::new(10.0, 0.0, 0.0);
        let movement = Vec3::new(1.0, 0.0, 0.0);
        let t = Sphere::find_time_of_collision(movement, delta, 1.0);
        assert!(t < 0.0, "the far root of a receding sweep is negative: {t}");
    }

    #[test]
    fn sphere_intersects_ray_accepts_a_ray_starting_inside() {
        let s = Sphere::new(Vec3::ZERO, 2.0);
        // Starting at the centre: c < 0, so the guard passes and the exit time comes back.
        let t = s.sphere_intersects_ray(&Ray {
            point: Vec3::ZERO,
            dir: Vec3::new(1.0, 0.0, 0.0),
        });
        assert_eq!(t, Some(2.0));
        // Starting outside and pointing away: c > 0, rejected.
        let away = Ray {
            point: Vec3::new(5.0, 0.0, 0.0),
            dir: Vec3::new(1.0, 0.0, 0.0),
        };
        assert_eq!(s.sphere_intersects_ray(&away), None);
    }

    #[test]
    fn cylsphere_collision_is_a_radial_test_and_a_slab_test() {
        let cyl = CylSphere {
            low_pt: Vec3::ZERO,
            height: 2.0,
            radius: 0.5,
        };
        let sphere = Sphere::new(Vec3::ZERO, 0.25);
        let sum = cyl.radius - EPSILON + sphere.radius;
        // inside the radius, mid-height: hit
        assert!(cyl.collides_with_sphere(&sphere, Vec3::new(0.0, 0.0, 1.0), sum));
        // outside the radius: miss, whatever the height
        assert!(!cyl.collides_with_sphere(&sphere, Vec3::new(2.0, 0.0, 1.0), sum));
        // inside the radius but far above the cap: miss
        assert!(!cyl.collides_with_sphere(&sphere, Vec3::new(0.0, 0.0, 5.0), sum));
        // just above the cap, within the sphere's own radius: hit
        assert!(cyl.collides_with_sphere(&sphere, Vec3::new(0.0, 0.0, 2.2), sum));
    }

    #[test]
    fn cylsphere_normal_is_radial_outside_and_axial_over_the_caps() {
        let cyl = CylSphere {
            low_pt: Vec3::ZERO,
            height: 2.0,
            radius: 0.5,
        };
        // curr_center outside the radius: a horizontal normal
        let n = cyl.normal_of_collision(
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(1.0, 0.0, 1.0),
            0.25,
            0.5,
        );
        assert_eq!(n, Some(Vec3::new(1.0, 0.0, 0.0)));
        // curr_center inside the radius, delta above: the -Z cap normal
        let n = cyl.normal_of_collision(
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, 3.0),
            0.25,
            0.5,
        );
        assert_eq!(n, Some(Vec3::new(0.0, 0.0, -1.0)));
        // curr_center inside the radius, delta below: the +Z cap normal
        let n = cyl.normal_of_collision(
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, -3.0),
            0.25,
            0.5,
        );
        assert_eq!(n, Some(Vec3::new(0.0, 0.0, 1.0)));
    }

    /// Normal of collision writes the radial normal even when it answers no.
    #[test]
    fn normal_of_collision_writes_the_radial_normal_even_when_it_answers_no() {
        let cyl = CylSphere {
            low_pt: Vec3::ZERO,
            height: 2.0,
            radius: 0.5,
        };
        // Radially outside (2 m from the axis, radius argument 0.5) and vertically clear of the
        // cap by more than the sphere's radius, with a delta whose Z differs: the client returns
        // 0 having already written (dx, dy, 0).
        let curr = Vec3::new(2.0, 0.0, 9.0);
        let delta = Vec3::new(2.0, 0.0, 0.0);
        let (ok, n) = cyl.normal_of_collision_full(curr, delta, 0.25, 0.5);
        assert!(!ok, "the flag says no");
        assert_eq!(n, Vec3::new(2.0, 0.0, 0.0), "the normal is written anyway");
        assert_eq!(
            cyl.normal_of_collision(curr, delta, 0.25, 0.5),
            None,
            "the Option form agrees"
        );
    }

    /// The cap-normal sign is `> 0`, not `>= 0`: the retail test is true only strictly above zero,
    /// so an exact tie takes `+1`. A `>=` transcription flips the normal on the tie.
    #[test]
    fn the_cap_normal_takes_plus_z_on_an_exact_tie() {
        let cyl = CylSphere {
            low_pt: Vec3::ZERO,
            height: 2.0,
            radius: 0.5,
        };
        // curr_center inside the radius, delta.z exactly equal to d.z.
        let curr = Vec3::new(0.0, 0.0, 1.0);
        let (ok, n) = cyl.normal_of_collision_full(curr, Vec3::new(0.0, 0.0, 1.0), 0.25, 0.5);
        assert!(ok);
        assert_eq!(n, Vec3::new(0.0, 0.0, 1.0));
        // A hair above the tie flips it.
        let (_, n) = cyl.normal_of_collision_full(curr, Vec3::new(0.0, 0.0, 1.001), 0.25, 0.5);
        assert_eq!(n, Vec3::new(0.0, 0.0, -1.0));
    }

    #[test]
    fn dummy_sphere_matches_the_static_initialiser() {
        let d = Sphere::dummy();
        assert_eq!(d.center, Vec3::new(0.0, 0.0, 0.1));
        assert_eq!(d.radius, 0.1);
    }
}
