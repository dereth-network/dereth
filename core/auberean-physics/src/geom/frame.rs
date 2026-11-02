//! More of what the client does with a frame and a rotation: a vector turned by a rotation, the
//! rotation about an axis, a frame's matrix, the direction it faces and its pitch, a frame set up
//! from the directions it faces and has above it, a frame tilted to a pitch, and the frame part of
//! the way between two others.
//!
//! As elsewhere, each sum is worked in double precision and rounded once where the client stores
//! a float.

use dereth_primitives::num::math;
use dereth_primitives::{Frame, Quat, Vec3};

use super::{rotation_axes, rotation_from_axes, set_vector_heading, slerp_rotation, Matrix4};

fn w(x: f32) -> f64 {
    f64::from(x)
}

#[allow(clippy::cast_possible_truncation)]
fn n(x: f64) -> f32 {
    x as f32
}

/// Degrees in a radian, as the client keeps it for a pitch.
const DEGREES_PER_RADIAN: f64 = 57.295_779_513_082_32;
/// Radians in a degree, as a float.
const RADIANS_PER_DEGREE: f32 = 0.017_453_292;

/// `v` turned by the rotation `q`.
// D2-13250
#[must_use]
pub fn quat_transform(q: Quat, v: Vec3) -> Vec3 {
    let (qw, qx, qy, qz) = (w(q.w), w(q.x), w(q.y), w(q.z));
    let (x2, y2, z2) = (qx + qx, qy + qy, qz + qz);
    let (vx, vy, vz) = (w(v.x), w(v.y), w(v.z));
    Vec3::new(
        n(
            ((1.0 - (z2 * qz + y2 * qy)) * vx + (y2 * qx - z2 * qw) * vy)
                + (y2 * qw + z2 * qx) * vz,
        ),
        n(
            ((1.0 - (x2 * qx + z2 * qz)) * vy + (y2 * qx + z2 * qw) * vx)
                + (z2 * qy - x2 * qw) * vz,
        ),
        n(
            ((1.0 - (x2 * qx + y2 * qy)) * vz + (x2 * qw + z2 * qy) * vy)
                + (z2 * qx - y2 * qw) * vx,
        ),
    )
}

/// The rotation of `angle` radians about the axis `axis` (taken as given, not made unit length).
// D2-13251
#[must_use]
pub fn rotation_axis(axis: Vec3, angle: f32) -> Quat {
    let half = w(angle) * 0.5;
    let s = math::sin(half);
    Quat::new(
        n(math::cos(half)),
        n(s * w(axis.x)),
        n(s * w(axis.y)),
        n(s * w(axis.z)),
    )
}

/// The frame's matrix: its rotation's axes as rows, its origin the last row.
// D2-13162
#[must_use]
pub fn frame_matrix(frame: &Frame) -> Matrix4 {
    let m = rotation_axes(frame.rotation);
    let o = frame.origin;
    Matrix4([
        m[0], m[1], m[2], 0.0, m[3], m[4], m[5], 0.0, m[6], m[7], m[8], 0.0, o.x, o.y, o.z, 1.0,
    ])
}

/// The direction the frame faces: its local `+y` axis in the landblock's frame.
// D2-13163
#[must_use]
pub fn frame_facing(frame: &Frame) -> Vec3 {
    let m = rotation_axes(frame.rotation);
    Vec3::new(m[3], m[4], m[5])
}

/// How far the frame faces above the level, in degrees (below it negative).
// D2-13165
#[must_use]
pub fn frame_pitch(frame: &Frame) -> f32 {
    let m = rotation_axes(frame.rotation);
    n(math::asin(w(m[5])) * DEGREES_PER_RADIAN)
}

/// Sets the frame's rotation from the direction it faces (`facing`, its local `+y`) and the
/// direction above it (`up`, its local `+z`), its right the cross of the two; both are taken as
/// given (unit length and square to each other).
// D2-13170
pub fn orient_frame(frame: &mut Frame, up: Vec3, facing: Vec3) {
    let right = [
        n(w(facing.y) * w(up.z) - w(up.y) * w(facing.z)),
        n(w(up.x) * w(facing.z) - w(facing.x) * w(up.z)),
        n(w(up.y) * w(facing.x) - w(up.x) * w(facing.y)),
    ];
    let m = [
        right[0], right[1], right[2], facing.x, facing.y, facing.z, up.x, up.y, up.z,
    ];
    frame.rotation = rotation_from_axes(&m);
}

/// Tilts the frame to face `degrees` above the level, keeping the heading it faces across the
/// ground.
// D2-13174
pub fn set_pitch(frame: &mut Frame, degrees: f32) {
    let m = rotation_axes(frame.rotation);
    let z = n(math::sin(w(degrees) * w(RADIANS_PER_DEGREE)));
    set_vector_heading(frame, Vec3::new(m[3], m[4], z));
}

/// The frame `t` of the way from `a` to `b`: the origin along the line, the rotation along the
/// shorter arc.
// D2-13175
#[must_use]
pub fn frame_slerp(a: &Frame, b: &Frame, t: f32) -> Frame {
    let (p, q) = (a.origin, b.origin);
    let mut out = *a;
    let along = |a: f32, b: f32| n((w(b) - w(a)) * w(t) + w(a));
    out.origin = Vec3::new(along(p.x, q.x), along(p.y, q.y), along(p.z, q.z));
    slerp_rotation(&mut out, a, b, t);
    out
}
