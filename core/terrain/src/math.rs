//! The frame and vector operations this crate needs.
//!
//! The frame arithmetic is the shared implementation ([`dereth_primitives::frame`]), re-exported
//! here at the paths the landscape has always used; physics and the animation player use the same
//! code. The billboarding rotation, which only the degradation path needs, is defined here.

use dereth_primitives::{Frame, Vec3};

#[cfg(test)]
use dereth_primitives::Quat;

pub use dereth_primitives::frame::{
    combine, euler_set_rotate, frame_is_valid, get_heading, get_vector_heading, grotate, l2g,
    localtoglobal, localtoglobalvec, rotate, set_heading, set_rotate, set_vector_heading,
    vector_get_heading, Mat3, V3,
};

/// Re-aim one local axis toward `target` so
/// another points as close to `target` as possible. `axis` is 0, 1 or 2.
///
/// The degradation path uses this operation for `degrade_mode` 3, 4
/// and 5 — billboarding about X, Y and Z.
pub fn rotate_around_axis_to_vector(f: &mut Frame, axis: usize, target: Vec3) {
    const NXT: [usize; 3] = [2, 0, 1];
    let mut m = l2g(f.rotation).0;
    let axis = axis % 3;
    let a = Vec3::new(m[axis * 3], m[axis * 3 + 1], m[axis * 3 + 2]);
    let mut p = target.sub(a.mul(a.dot(target)));
    if p.normalize_check_small() {
        let n = NXT[axis];
        p = Vec3::new(m[n * 3], m[n * 3 + 1], m[n * 3 + 2]);
    }
    if p.dot(a).abs() > 0.000_999_999_9 {
        return;
    }
    let n1 = NXT[axis];
    let n2 = NXT[n1];
    let c = p.cross(a);
    m[n1 * 3] = p.x;
    m[n1 * 3 + 1] = p.y;
    m[n1 * 3 + 2] = p.z;
    m[n2 * 3] = c.x;
    m[n2 * 3 + 1] = c.y;
    m[n2 * 3 + 2] = c.z;
    set_rotate_from_matrix(f, &m);
}

/// Inverse of [`l2g`], used by
/// [`rotate_around_axis_to_vector`] after it writes axes into the matrix directly.
fn set_rotate_from_matrix(f: &mut Frame, m: &[f32; 9]) {
    let trace = m[0] + m[4] + m[8];
    let (w, x, y, z) = if trace > 0.0 {
        let s = (trace + 1.0).sqrt();
        let t = 0.5 / s;
        (
            s * 0.5,
            (m[5] - m[7]) * t,
            (m[6] - m[2]) * t,
            (m[1] - m[3]) * t,
        )
    } else {
        let i = if m[4] > m[0] {
            if m[8] > m[4] {
                2
            } else {
                1
            }
        } else if m[8] > m[0] {
            2
        } else {
            0
        };
        let j = (i + 1) % 3;
        let k = (j + 1) % 3;
        let s = (m[i * 3 + i] - m[j * 3 + j] - m[k * 3 + k] + 1.0).sqrt();
        let t = 0.5 / s;
        let mut q = [0.0f32; 3];
        q[i] = s * 0.5;
        q[j] = (m[i * 3 + j] + m[j * 3 + i]) * t;
        q[k] = (m[i * 3 + k] + m[k * 3 + i]) * t;
        ((m[j * 3 + k] - m[k * 3 + j]) * t, q[0], q[1], q[2])
    };
    set_rotate(f, w, x, y, z);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: heading 0 is +Y (north) and increases clockwise; the
    /// `get_heading`/`set_heading` round trip jointly checks both formulas.
    #[test]
    fn heading_is_degrees_clockwise_from_north() {
        let mut f = Frame::default();
        set_heading(&mut f, 0.0);
        let v = get_vector_heading(&f);
        assert!(v.y > 0.99, "heading 0 must point north: {v:?}");
        assert!(v.x.abs() < 1e-5);
        set_heading(&mut f, 90.0);
        let v = get_vector_heading(&f);
        assert!(v.x > 0.99, "heading 90 must point east: {v:?}");
        for h in [0.0f32, 17.0, 90.0, 179.0, 271.0, 359.0] {
            let mut f = Frame::default();
            set_heading(&mut f, h);
            let back = get_heading(&f);
            assert!(
                (back - h).abs() < 0.01 || (back - h).abs() > 359.99,
                "{h} -> {back}"
            );
        }
    }

    /// Oracle: `l2g` of the identity quaternion is the identity matrix, and
    /// `set_rotate` restores the old quaternion when handed something invalid.
    #[test]
    fn matrix_and_set_rotate_guard_behave_as_documented() {
        assert_eq!(l2g(Quat::IDENTITY), Mat3::IDENTITY);
        let mut f = Frame::default();
        set_rotate(&mut f, f32::NAN, 0.0, 0.0, 0.0);
        assert_eq!(
            f.rotation,
            Quat::IDENTITY,
            "an invalid rotation must be rejected, not stored"
        );
    }

    /// Oracle: re-aiming about Z
    /// (the classic upright foliage card) must leave the local Z axis alone and point the local Y
    /// axis at the target's horizontal component.
    #[test]
    fn billboarding_about_z_keeps_the_z_axis_and_aims_y() {
        let mut f = Frame::default();
        rotate_around_axis_to_vector(&mut f, 2, Vec3::new(1.0, 0.0, 0.7));
        let m = l2g(f.rotation).0;
        let z_axis = Vec3::new(m[6], m[7], m[8]);
        assert!((z_axis.z - 1.0).abs() < 1e-4, "z axis moved: {z_axis:?}");
        let y_axis = Vec3::new(m[3], m[4], m[5]);
        assert!(
            y_axis.x > 0.99,
            "y axis should face the target's xy direction: {y_axis:?}"
        );
    }

    /// Oracle: `combine` composes rotations and transforms the child origin
    /// through the parent's matrix. Composing with the identity must be the identity.
    #[test]
    fn combine_with_identity_is_the_identity() {
        let mut a = Frame::new(Vec3::new(1.0, 2.0, 3.0), Quat::IDENTITY);
        set_heading(&mut a, 40.0);
        let out = combine(&a, &Frame::default());
        assert!((out.origin.x - a.origin.x).abs() < 1e-5);
        assert!((get_heading(&out) - 40.0).abs() < 0.01);
    }
}
