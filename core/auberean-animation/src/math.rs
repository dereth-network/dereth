//! The little arithmetic animation needs beyond the world's transforms: a bone transform as a
//! matrix, the inverse of an affine transform, and quaternions.

use auberean_assets::setup::BoneTransform;
use auberean_primitives::{Quat, Vec3};
use auberean_world::math::Mat4;

/// A bone transform as a matrix: scale, then rotate, then move.
#[must_use]
pub fn transform_matrix(t: &BoneTransform) -> Mat4 {
    Mat4::from_parts(t.offset, t.rotation, t.scale)
}

/// The inverse of an affine transform (its last column `0, 0, 0, 1`). A transform that cannot be
/// inverted gives the identity.
#[must_use]
pub fn affine_inverse(m: &Mat4) -> Mat4 {
    let a = &m.0;
    let c00 = a[1][1] * a[2][2] - a[1][2] * a[2][1];
    let c01 = a[1][2] * a[2][0] - a[1][0] * a[2][2];
    let c02 = a[1][0] * a[2][1] - a[1][1] * a[2][0];
    let det = a[0][0] * c00 + a[0][1] * c01 + a[0][2] * c02;
    if det.abs() < f32::MIN_POSITIVE {
        return Mat4::IDENTITY;
    }
    let inv = 1.0 / det;
    let r = [
        [
            c00 * inv,
            (a[0][2] * a[2][1] - a[0][1] * a[2][2]) * inv,
            (a[0][1] * a[1][2] - a[0][2] * a[1][1]) * inv,
        ],
        [
            c01 * inv,
            (a[0][0] * a[2][2] - a[0][2] * a[2][0]) * inv,
            (a[0][2] * a[1][0] - a[0][0] * a[1][2]) * inv,
        ],
        [
            c02 * inv,
            (a[0][1] * a[2][0] - a[0][0] * a[2][1]) * inv,
            (a[0][0] * a[1][1] - a[0][1] * a[1][0]) * inv,
        ],
    ];
    let t = [a[3][0], a[3][1], a[3][2]];
    let mut out = [[0.0f32; 4]; 4];
    for i in 0..3 {
        out[i][..3].copy_from_slice(&r[i]);
    }
    for j in 0..3 {
        out[3][j] = -(t[0] * r[0][j] + t[1] * r[1][j] + t[2] * r[2][j]);
    }
    out[3][3] = 1.0;
    Mat4(out)
}

/// The dot product of two quaternions.
#[must_use]
pub fn quat_dot(a: Quat, b: Quat) -> f32 {
    a.w * b.w + a.x * b.x + a.y * b.y + a.z * b.z
}

/// `q` scaled to unit length; the identity for a zero quaternion.
#[must_use]
pub fn quat_normalize(q: Quat) -> Quat {
    let n = quat_dot(q, q).sqrt();
    if n < 1e-12 {
        return Quat::IDENTITY;
    }
    Quat {
        w: q.w / n,
        x: q.x / n,
        y: q.y / n,
        z: q.z / n,
    }
}

/// Linear interpolation of vectors.
#[must_use]
pub fn lerp(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    Vec3::new(
        a.x + (b.x - a.x) * t,
        a.y + (b.y - a.y) * t,
        a.z + (b.z - a.z) * t,
    )
}

/// Spherical interpolation from `a` to `b`, the short way round; nearly equal rotations
/// interpolate linearly.
#[must_use]
pub fn slerp(a: Quat, b: Quat, t: f32) -> Quat {
    let mut cos = quat_dot(a, b);
    let mut b = b;
    if cos < 0.0 {
        cos = -cos;
        b = Quat {
            w: -b.w,
            x: -b.x,
            y: -b.y,
            z: -b.z,
        };
    }
    let (wa, wb) = if 1.0 - cos > 1e-6 {
        let angle = dereth_primitives::num::math::acosf(cos.min(1.0));
        let s = dereth_primitives::num::math::sinf(angle);
        (
            dereth_primitives::num::math::sinf((1.0 - t) * angle) / s,
            dereth_primitives::num::math::sinf(t * angle) / s,
        )
    } else {
        (1.0 - t, t)
    };
    quat_normalize(Quat {
        w: a.w * wa + b.w * wb,
        x: a.x * wa + b.x * wb,
        y: a.y * wa + b.y * wb,
        z: a.z * wa + b.z * wb,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (arithmetic helpers, checked against their definitions)
    use super::*;

    fn close(a: &Mat4, b: &Mat4) -> bool {
        a.0.iter()
            .flatten()
            .zip(b.0.iter().flatten())
            .all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn an_affine_inverse_undoes_its_transform() {
        let q = quat_normalize(Quat {
            w: 0.8,
            x: 0.1,
            y: -0.4,
            z: 0.3,
        });
        let m = Mat4::from_parts(Vec3::new(1.5, -2.0, 0.25), q, Vec3::new(1.0, 2.0, 0.5));
        assert!(close(&m.then(&affine_inverse(&m)), &Mat4::IDENTITY));
        assert!(close(&affine_inverse(&m).then(&m), &Mat4::IDENTITY));
    }

    #[test]
    fn slerp_takes_the_short_way_and_keeps_unit_length() {
        let a = Quat::IDENTITY;
        let b = Quat {
            w: -std::f32::consts::FRAC_1_SQRT_2,
            x: 0.0,
            y: 0.0,
            z: -std::f32::consts::FRAC_1_SQRT_2,
        };
        let h = slerp(a, b, 0.5);
        assert!((quat_dot(h, h) - 1.0).abs() < 1e-5);
        // Halfway along a quarter turn about z, taken the short way: an eighth of a turn.
        assert!(
            (h.w.abs() - dereth_primitives::num::math::cosf(std::f32::consts::PI / 8.0)).abs()
                < 1e-5,
            "{h:?}"
        );
    }
}
