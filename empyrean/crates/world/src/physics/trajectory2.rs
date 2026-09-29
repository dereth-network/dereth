// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Physics/Trajectory2.cs, Source/ACE.Server/Physics/Common/Vector.cs
//! Port of `Source/ACE.Server/Physics/Trajectory2.cs`.
//!
//! Pure arithmetic in ACE's types; expected values come from ACE's compiled code
//! (`physics/trajectory2_calculate_trajectory` vectors).

use empyrean_common::dotnet::math::min;
use empyrean_common::dotnet::Vector3;

use crate::physics::trajectory::GRAVITY;

/// `PhysicsGlobals.EPSILON`.
const EPSILON: f32 = 0.0002;

/// Normalizes `v` in place unless it is shorter than `EPSILON`; answers whether it was too small.
/// A helper of ACE's physics folder (shared), ported here for its one server caller.
// ACE: Vec.NormalizeCheckSmall
fn normalize_check_small(v: &mut Vector3) -> bool {
    let dist = v.length();
    if dist < EPSILON {
        return true;
    }

    *v = *v * (1.0 / dist);
    false
}

/// The velocity that carries a projectile from `start_pos` to `end_pos` at `speed`, leading a
/// target moving at `target_velocity` (XY only), with a gravity term when `gravity` is set.
// ACE: Trajectory2.CalculateTrajectory
#[must_use]
#[allow(clippy::many_single_char_names, clippy::cast_possible_truncation)] // ACE's names; (float) casts
pub fn calculate_trajectory(
    start_pos: Vector3,
    end_pos: Vector3,
    target_velocity: Vector3,
    speed: f32,
    gravity: bool,
) -> Vector3 {
    let target_offset = end_pos - start_pos;

    let mut result;
    let t: f32;

    #[allow(clippy::float_cmp)] // C#'s Vector3 ==
    if target_velocity == Vector3::ZERO {
        let target_dist = target_offset.length();

        t = target_dist / speed;
        result = target_offset / t;
    } else {
        let p0 = target_offset;
        let p1 = Vector3::ZERO;

        let mut v0 = target_velocity;
        if normalize_check_small(&mut v0) {
            v0 = Vector3::ZERO;
        }

        let s1 = speed;

        let a = (v0.x * v0.x) + (v0.y * v0.y) - (s1 * s1);
        let b = 2.0 * ((p0.x * v0.x) + (p0.y * v0.y) - (p1.x * v0.x) - (p1.y * v0.y));
        let c = (p0.x * p0.x) + (p0.y * p0.y) + (p1.x * p1.x) + (p1.y * p1.y)
            - (2.0 * p1.x * p0.x)
            - (2.0 * p1.y * p0.y);

        let t0 = f64::from((b * b) - (4.0 * a * c)).sqrt();
        let mut t1 = (f64::from(-b) + t0) / f64::from(2.0 * a);
        let mut t2 = (f64::from(-b) - t0) / f64::from(2.0 * a);

        if t1 < 0.0 {
            t1 = f64::from(f32::MAX);
        }
        if t2 < 0.0 {
            t2 = f64::from(f32::MAX);
        }

        t = min(t1, t2) as f32;

        if t >= 100.0 {
            return calculate_trajectory(start_pos, end_pos, Vector3::ZERO, speed, gravity);
        }

        let s0 = target_velocity.length();

        result = (p0 + v0 * (t * s0)) / t;
    }

    if gravity {
        result.z -= GRAVITY * t * 0.5;
    }

    result
}
