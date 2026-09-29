//! The `Frame` arithmetic the animation player needs.
//!
//! The shared frame arithmetic ([`dereth_primitives::frame`]) is re-exported here at the paths the
//! animation crate has always used; physics and the landscape use the same implementation. Only
//! [`subtract1`], which only the animation sequence needs (it unapplies a placement frame when an
//! animation plays backwards), is defined here.

use dereth_primitives::{Frame, Quat, Vec3};

pub use dereth_primitives::frame::{
    combine, euler_set_rotate, frame_is_valid, get_heading, globaltolocalvec, grotate, l2g,
    localtoglobalvec, rotate, set_heading, set_rotate, set_vector_heading, Mat3, V3,
};

/// `out = a ∘ b⁻¹` for a placement frame `b`.
///
/// This is the backwards counterpart of [`combine`]: sequence updating unapplies one
/// `pos_frames` entry for every animation frame crossed in reverse. The rotation is
/// `a.q ⊗ conj(b.q)` and the origin uses the **new** matrix.
#[must_use]
pub fn subtract1(a: &Frame, b: &Frame) -> Frame {
    let (p, q) = (a.rotation, b.rotation);
    let mut out = Frame::new(Vec3::ZERO, Quat::IDENTITY);
    set_rotate(
        &mut out,
        q.z * p.z + p.w * q.w + q.x * p.x + p.y * q.y,
        p.z * q.y + ((p.x * q.w - q.x * p.w) - p.y * q.z),
        (q.z * p.x + (p.y * q.w - p.w * q.y)) - q.x * p.z,
        p.y * q.x + (p.z * q.w - (q.y * p.x + p.w * q.z)),
    );
    let m = l2g(out.rotation);
    out.origin = a.origin.add(localtoglobalvec(m, b.origin.negate()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn about(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-4, "{a} != {b}");
    }

    /// ORACLE: the recovered position and frame behavior's matrix-cache description. The identity
    /// quaternion must give the identity matrix, or every composition below
    /// is wrong in a way that cancels.
    #[test]
    fn the_identity_quaternion_gives_the_identity_matrix() {
        assert_eq!(
            l2g(Quat::IDENTITY).0,
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
        );
    }

    /// `combine` then `subtract1` with the same child frame is the identity, which is the property
    /// `update_internal` relies on when it plays an animation forwards and then backwards.
    #[test]
    fn subtract1_undoes_combine() {
        let a = {
            let mut f = Frame::new(Vec3::new(3.0, -2.0, 0.5), Quat::IDENTITY);
            set_heading(&mut f, 37.0);
            f
        };
        let b = {
            let mut f = Frame::new(Vec3::new(0.25, 1.5, -0.75), Quat::IDENTITY);
            set_heading(&mut f, 110.0);
            f
        };
        let c = combine(&a, &b);
        let back = subtract1(&c, &b);
        about(back.origin.x, a.origin.x);
        about(back.origin.y, a.origin.y);
        about(back.origin.z, a.origin.z);
        about(get_heading(&back), get_heading(&a));
    }

    /// `get_heading`/`set_heading` round-trip, and 0 degrees points at +Y.
    #[test]
    fn heading_is_degrees_clockwise_from_north() {
        let mut f = Frame::default();
        set_heading(&mut f, 0.0);
        let m = l2g(f.rotation).0;
        about(m[3], 0.0);
        about(m[4], 1.0);
        for h in [0.0_f32, 45.0, 90.0, 180.0, 270.0, 359.0] {
            let mut f = Frame::default();
            set_heading(&mut f, h);
            about(get_heading(&f), h);
        }
    }

    /// `grotate` is a no-op below the 0.0002 guard, which is what keeps a zero omega from
    /// producing a NaN quaternion once per frame crossed.
    #[test]
    fn grotate_ignores_a_tiny_axis() {
        let mut f = Frame::default();
        grotate(&mut f, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(f.rotation, Quat::IDENTITY);
        grotate(&mut f, Vec3::new(0.0, 0.0, 0.000_1));
        assert_eq!(f.rotation, Quat::IDENTITY);
    }

    /// `normalize_check_small` returns *true* on failure. Getting this backwards silently disables
    /// the particle birth offset, so it is worth one test.
    #[test]
    fn normalize_check_small_reports_true_when_it_could_not_normalise() {
        let mut v = Vec3::new(0.0, 0.0, 0.0);
        assert!(v.normalize_check_small());
        let mut v = Vec3::new(0.0, 3.0, 4.0);
        assert!(!v.normalize_check_small());
        about(v.y, 0.6);
        about(v.z, 0.8);
    }
}
