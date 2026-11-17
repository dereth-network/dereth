//! Vector, frame and position arithmetic, preserving the observed operand order.
//!
//! The vector and frame arithmetic is the shared implementation ([`dereth_primitives::frame`]),
//! re-exported here at the paths physics has always used; the animation player and the landscape
//! use the same code. What only physics needs is defined here: the block-margin test, the
//! heading-only validity check and frame interpolation. Position arithmetic across landblocks
//! delegates to `dereth_primitives::position`.

use dereth_primitives::num::consts::EPSILON;
use dereth_primitives::num::math;
use dereth_primitives::{Frame, Position, Quat, Vec3};

pub use dereth_primitives::frame::{
    combine, euler_set_rotate, frame_is_valid, get_heading, get_vector_heading, globaltolocal,
    globaltolocalvec, grotate, l2g, localtoglobal, localtoglobalvec, set_heading, set_rotate,
    set_vector_heading, vector_get_heading, Mat3, V3,
};

use crate::landdefs;

/// Whether a landblock-relative origin is at least `margin` from every edge of its 192 m
/// block.
#[must_use]
pub fn within_block(p: Vec3, margin: f32) -> bool {
    margin <= p.x
        && margin <= p.y
        && p.x < crate::globals::BLOCK_LENGTH - margin
        && p.y < crate::globals::BLOCK_LENGTH - margin
}

// ---------------------------------------------------------------------------------------------
// Frame
// ---------------------------------------------------------------------------------------------

/// The compass heading of a vector's horizontal part, in **degrees** (0 = +Y, north, increasing
/// clockwise). The vertical part is dropped first, and a horizontal part too small to normalise
/// answers 0. The direction is normalised as [`normalize_check_small_wide`] does it.
#[must_use]
pub fn flat_heading(v: Vec3) -> f32 {
    let mut flat = Vec3::new(v.x, v.y, 0.0);
    if normalize_check_small_wide(&mut flat) {
        return 0.0;
    }
    vector_get_heading(flat)
}

/// The rotation matrix of `q` as the client keeps it beside every frame: each entry worked in
/// double precision from the quaternion and rounded to single once. [`l2g`] rounds every step and
/// can land an entry an ulp away.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn l2g_wide(q: Quat) -> Mat3 {
    let (w, x, y, z) = (
        f64::from(q.w),
        f64::from(q.x),
        f64::from(q.y),
        f64::from(q.z),
    );
    let (x2, y2, z2) = (x + x, y + y, z + z);
    let (wx, wy, wz) = (w * x2, w * y2, w * z2);
    let (xx, xy, xz) = (x * x2, x * y2, x * z2);
    let (yy, yz, zz) = (y * y2, y * z2, z * z2);
    let one_minus_xx = 1.0 - xx;
    Mat3([
        ((1.0 - yy) - zz) as f32,
        (xy + wz) as f32,
        (xz - wy) as f32,
        (xy - wz) as f32,
        (one_minus_xx - zz) as f32,
        (yz + wx) as f32,
        (xz + wy) as f32,
        (yz - wx) as f32,
        (one_minus_xx - yy) as f32,
    ])
}

/// [`set_rotate`] as the client works it: the length in double precision, and each component
/// multiplied by its reciprocal there and rounded to single once. An invalid result leaves the
/// rotation as it was.
#[allow(clippy::cast_possible_truncation)]
pub fn set_rotate_wide(f: &mut Frame, w: f32, x: f32, y: f32, z: f32) {
    let old = f.rotation;
    let (w, x, y, z) = (f64::from(w), f64::from(x), f64::from(y), f64::from(z));
    let inv = 1.0 / (((w * w + x * x) + y * y) + z * z).sqrt();
    f.rotation = Quat::new(
        (w * inv) as f32,
        (x * inv) as f32,
        (y * inv) as f32,
        (z * inv) as f32,
    );
    if !frame_is_valid(f) {
        f.rotation = old;
    }
}

/// [`combine`] as the client works it when a body's position takes the sub-step's offset, in
/// double precision: `b`'s origin turned by `a`'s matrix as the client keeps it ([`l2g_wide`])
/// and added to `a`'s origin, rounded to single once; the rotations' product rounded to single
/// once and normalised by [`set_rotate_wide`].
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn combine_wide(a: &Frame, b: &Frame) -> Frame {
    let m = l2g_wide(a.rotation).0.map(f64::from);
    let (x, y, z) = (
        f64::from(b.origin.x),
        f64::from(b.origin.y),
        f64::from(b.origin.z),
    );
    let o = a.origin;
    let origin = Vec3::new(
        (((m[6] * z + m[0] * x) + m[3] * y) + f64::from(o.x)) as f32,
        (((m[7] * z + m[1] * x) + m[4] * y) + f64::from(o.y)) as f32,
        (((m[8] * z + m[2] * x) + m[5] * y) + f64::from(o.z)) as f32,
    );
    let mut out = Frame::new(origin, Quat::IDENTITY);
    let wide = |q: Quat| {
        (
            f64::from(q.w),
            f64::from(q.x),
            f64::from(q.y),
            f64::from(q.z),
        )
    };
    let ((pw, px, py, pz), (qw, qx, qy, qz)) = (wide(a.rotation), wide(b.rotation));
    set_rotate_wide(
        &mut out,
        (((pw * qw - qx * px) - qy * py) - qz * pz) as f32,
        (((pw * qx + qz * py) + qw * px) - qy * pz) as f32,
        (((qy * pw - qz * px) + pz * qx) + py * qw) as f32,
        (((qy * px + qz * pw) - py * qx) + pz * qw) as f32,
    );
    out
}

/// Normalise `v` in place as the client's physics does: the length in double precision, and each
/// component multiplied by the length's reciprocal there and rounded to single once. Answers
/// `true`, leaving `v` as it was, when the length is under [`EPSILON`].
///
/// Worked wholly in single precision instead, a component can land an ulp away.
pub fn normalize_check_small_wide(v: &mut Vec3) -> bool {
    let (x, y, z) = (f64::from(v.x), f64::from(v.y), f64::from(v.z));
    let len = ((x * x + y * y) + z * z).sqrt();
    if len < f64::from(EPSILON) {
        return true;
    }
    let inv = 1.0 / len;
    #[allow(clippy::cast_possible_truncation)]
    {
        *v = Vec3::new((inv * x) as f32, (inv * y) as f32, (inv * z) as f32);
    }
    false
}

/// a finite origin and **at least one** NaN
/// quaternion component.
#[must_use]
pub fn frame_is_valid_except_for_heading(f: &Frame) -> bool {
    if f.origin.x.is_nan() || f.origin.y.is_nan() || f.origin.z.is_nan() {
        return false;
    }
    let q = f.rotation;
    q.w.is_nan() || q.x.is_nan() || q.y.is_nan() || q.z.is_nan()
}

/// Interpolate the origin - plain lerp.
#[must_use]
pub fn interpolate_origin(a: &Frame, b: &Frame, t: f32) -> Vec3 {
    let inv = 1.0 - t;
    Vec3::new(
        t * b.origin.x + inv * a.origin.x,
        inv * a.origin.y + t * b.origin.y,
        inv * a.origin.z + t * b.origin.z,
    )
}

/// Slerp with the shortest-arc negation, a lerp
/// fallback below `1 - |dot| <= 2e-4`, and a second lerp fallback when either computed sine
/// weight escapes `[0, 1]`.
///
/// Worked as the client works it, in double precision: the dot product, the weights (never
/// rounded to single), each blended component rounded to single once, and the result normalised
/// by [`set_rotate_wide`].
#[allow(clippy::cast_possible_truncation)]
pub fn interpolate_rotation(out: &mut Frame, a: &Frame, b: &Frame, t: f32) {
    let (qa, mut qb) = (a.rotation, b.rotation);
    let wide = |q: Quat| {
        (
            f64::from(q.w),
            f64::from(q.x),
            f64::from(q.y),
            f64::from(q.z),
        )
    };
    let (aw, ax, ay, az) = wide(qa);
    let (bw, bx, by, bz) = wide(qb);
    let mut dot = ((ay * by + aw * bw) + ax * bx) + az * bz;
    if dot < 0.0 {
        dot = -dot;
        qb = Quat::new(-qb.w, -qb.x, -qb.y, -qb.z);
    }
    let t = f64::from(t);
    let (mut wa, mut wb) = (1.0 - t, t);
    if 1.0 - dot > f64::from(EPSILON) {
        let theta = math::acos(dot);
        let inv_sin = 1.0 / math::sin(theta);
        let sa = math::sin(theta * (1.0 - t)) * inv_sin;
        let sb = math::sin(t * theta) * inv_sin;
        if (0.0..=1.0).contains(&sa) && (0.0..=1.0).contains(&sb) {
            (wa, wb) = (sa, sb);
        }
    }
    let (bw, bx, by, bz) = wide(qb);
    set_rotate_wide(
        out,
        (aw * wa + bw * wb) as f32,
        (ax * wa + bx * wb) as f32,
        (ay * wa + by * wb) as f32,
        (bz * wb + az * wa) as f32,
    );
}

/// Set a frame's rotation, made unit length only when its squared length exceeds `0.0002²`; a
/// rotation of no length is kept as it is. Worked out in double precision, so that a rotation
/// already unit length comes out unchanged.
pub fn set_rotate_plain(f: &mut Frame, q: Quat) {
    let [w, x, y, z] = [q.w, q.x, q.y, q.z].map(f64::from);
    let n2 = z * z + y * y + x * x + w * w;
    f.rotation = if n2 > f64::from(EPSILON * EPSILON) {
        let k = 1.0 / n2.sqrt();
        #[allow(clippy::cast_possible_truncation)]
        Quat::new(
            (k * w) as f32,
            (k * x) as f32,
            (k * y) as f32,
            (k * z) as f32,
        )
    } else {
        q
    };
}

/// Slerp along the shorter arc in single precision, with a straight blend when `1 - |dot|` is at
/// most `0.001`; the result set by [`set_rotate_plain`].
pub fn slerp_rotation_plain(out: &mut Frame, a: &Frame, b: &Frame, t: f32) {
    let (p, mut q) = (a.rotation, b.rotation);
    let mut cos = p.x * q.x + p.y * q.y + p.z * q.z + p.w * q.w;
    if cos < 0.0 {
        cos = -cos;
        q = Quat::new(-q.w, -q.x, -q.y, -q.z);
    }
    let (wa, wb) = if 1.0 - cos > 0.001 {
        let theta = math::acosf(cos);
        let s = math::sinf(theta);
        (math::sinf((1.0 - t) * theta) / s, math::sinf(t * theta) / s)
    } else {
        (1.0 - t, t)
    };
    set_rotate_plain(
        out,
        Quat::new(
            wa * p.w + wb * q.w,
            wa * p.x + wb * q.x,
            wa * p.y + wb * q.y,
            wa * p.z + wb * q.z,
        ),
    );
}

// ---------------------------------------------------------------------------------------------
// Position
// ---------------------------------------------------------------------------------------------

/// **The** primitive: never compare two positions without it,
/// because two objects one metre apart across a landblock boundary have origins ~192 m apart.
#[must_use]
pub fn get_offset(from: &Position, to: &Position) -> Vec3 {
    dereth_primitives::position::get_offset(from, to)
}

/// The velocity a body achieved moving from `from` to `to` in a sub-step of `quantum` seconds,
/// worked as the client works it, to the last bit.
///
/// The offset's x is the block offset plus the end less the start in double precision, rounded
/// once; its y and z keep the block offset plus the end as a float first. The step is taken as a
/// float and the offset divided by it through its reciprocal: in double precision for x, kept as
/// a float for y and z.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn achieved_velocity(from: &Position, to: &Position, quantum: f64) -> Vec3 {
    let off = dereth_primitives::position::block_offset(from.cell, to.cell);
    let (a, b) = (from.frame.origin, to.frame.origin);
    let x = ((f64::from(off.x) + f64::from(b.x)) - f64::from(a.x)) as f32;
    let y = (off.y + b.y) - a.y;
    let z = (off.z + b.z) - a.z;
    let inv = 1.0 / f64::from(quantum as f32);
    let inv_kept = f64::from(inv as f32);
    Vec3::new(
        (inv * f64::from(x)) as f32,
        (inv_kept * f64::from(y)) as f32,
        (inv_kept * f64::from(z)) as f32,
    )
}

/// The distance between two positions - full 3-D.
#[must_use]
pub fn distance(a: &Position, b: &Position) -> f32 {
    dereth_primitives::position::distance(a, b)
}

/// The horizontal distance between two positions.
#[must_use]
pub fn xy_distance(a: &Position, b: &Position) -> f32 {
    let d = get_offset(a, b);
    (d.x * d.x + d.y * d.y).sqrt()
}

/// take `v` in `from`'s frame and express it in `at`'s
/// cell's block space.
#[must_use]
pub fn pos_localtoglobal(at: &Position, from: &Position, v: Vec3) -> Vec3 {
    landdefs::get_block_offset(at.cell, from.cell).add(localtoglobal(&from.frame, v))
}

/// `v` in `from`'s local frame, expressed in `at`'s local
/// frame, crossing landblocks correctly.
#[must_use]
pub fn localtolocal(at: &Position, from: &Position, v: Vec3) -> Vec3 {
    globaltolocal(&at.frame, pos_localtoglobal(at, from, v))
}

/// The cylinder distance between two positions.
///
/// The horizontal term is the **3-D** centre distance, not the 2-D one. That is what the client
/// does and what melee range checks are calibrated against.
#[must_use]
pub fn cylinder_distance(r1: f32, h1: f32, p1: &Position, r2: f32, h2: f32, p2: &Position) -> f32 {
    dereth_primitives::position::cylinder_distance(r1, h1, p1, r2, h2, p2)
}

/// The 3-D centre distance minus the radii, never clamped.
#[must_use]
pub fn cylinder_distance_no_z(r1: f32, p1: &Position, r2: f32, p2: &Position) -> f32 {
    get_offset(p1, p2).mag2().sqrt() - (r1 + r2)
}

/// The hit-location word used by melee and by
/// `report_object_collision` for missiles.
#[must_use]
pub fn determine_quadrant(other: &Position, height: f32, this: &Position) -> u32 {
    let local = localtolocal(other, this, Vec3::ZERO);
    let mut q = if local.x >= 0.0 { 0x10 } else { 0x08 };
    q |= if local.y >= 0.0 { 0x20 } else { 0x40 };
    if local.z < height * 0.333_333_34 {
        q |= 0x04;
    } else if local.z < height * 0.666_666_7 {
        q |= 0x02;
    } else {
        q |= 0x01;
    }
    q
}

/// Whether a position is valid.
#[must_use]
pub fn position_is_valid(p: &Position) -> bool {
    landdefs::inbound_valid_cellid(p.cell) && frame_is_valid(&p.frame)
}

/// Frame equality - component-wise, exact.
#[must_use]
pub fn frame_is_equal(a: &Frame, b: &Frame) -> bool {
    a.origin == b.origin && a.rotation == b.rotation
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::CellId;

    // Oracle: the client's own frame code -- the cached rotation, the local-to-global vector
    // transform, the global rotate, the rotation setter and the heading accessor.

    fn quat_axis(axis: Vec3, radians: f32) -> Quat {
        let h = radians * 0.5;
        let s = math::sinf(h);
        let a = axis.normalize();
        Quat::new(math::cosf(h), a.x * s, a.y * s, a.z * s)
    }

    #[test]
    fn identity_quaternion_caches_to_the_identity_matrix() {
        assert_eq!(l2g(Quat::IDENTITY), Mat3::IDENTITY);
        // set_frame writes an all-zero quaternion when the heading is NaN; cache() on that
        // degenerates to the identity, which is the behaviour the rebuild note calls out.
        assert_eq!(l2g(Quat::new(0.0, 0.0, 0.0, 0.0)), Mat3::IDENTITY);
    }

    #[test]
    fn local_to_global_columns_are_the_rotated_basis_vectors() {
        // 90 degrees about +Z takes local +X to global +Y.
        let q = quat_axis(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_2);
        let m = l2g(q);
        let x = localtoglobalvec(m, Vec3::new(1.0, 0.0, 0.0));
        assert!(x.eq_eps(Vec3::new(0.0, 1.0, 0.0)), "{x:?}");
        // and globaltolocalvec is its exact transpose
        let back = globaltolocalvec(m, x);
        assert!(back.eq_eps(Vec3::new(1.0, 0.0, 0.0)), "{back:?}");
    }

    #[test]
    fn grotate_is_a_no_op_below_the_epsilon_squared_guard() {
        let mut f = Frame::default();
        grotate(&mut f, Vec3::new(0.0, 0.0, 0.000_1));
        assert_eq!(f.rotation, Quat::IDENTITY);
        // and does rotate above it
        grotate(&mut f, Vec3::new(0.0, 0.0, std::f32::consts::FRAC_PI_2));
        let x = localtoglobalvec(l2g(f.rotation), Vec3::new(1.0, 0.0, 0.0));
        assert!(x.eq_eps(Vec3::new(0.0, 1.0, 0.0)), "{x:?}");
    }

    #[test]
    fn set_rotate_restores_the_old_quaternion_on_an_invalid_result() {
        let mut f = Frame::default();
        set_rotate(&mut f, f32::NAN, 0.0, 0.0, 0.0);
        assert_eq!(
            f.rotation,
            Quat::IDENTITY,
            "an invalid rotation must be rejected, not stored"
        );
    }

    #[test]
    fn heading_is_degrees_clockwise_from_north() {
        let mut f = Frame::default();
        assert!((get_heading(&f) - 0.0).abs() < 1e-3, "{}", get_heading(&f));
        set_heading(&mut f, 90.0);
        let h = get_vector_heading(&f);
        // 90 degrees clockwise from +Y is +X
        assert!(h.eq_eps(Vec3::new(1.0, 0.0, 0.0)), "{h:?}");
        assert!((get_heading(&f) - 90.0).abs() < 1e-2, "{}", get_heading(&f));
    }

    #[test]
    fn a_blend_with_no_turn_keeps_a_unit_rotation_to_the_last_bit() {
        // East: made unit length in single precision, its parts would each grow by one unit in
        // the last place (0x3f35_04f4).
        let east = Frame::new(
            Vec3::ZERO,
            Quat::new(
                f32::from_bits(0x3f35_04f3),
                0.0,
                0.0,
                f32::from_bits(0xbf35_04f3),
            ),
        );
        for t in [1.0, 0.5] {
            let mut out = Frame::default();
            interpolate_rotation(&mut out, &east, &east, t);
            let q = out.rotation;
            assert_eq!(
                [q.w, q.x, q.y, q.z].map(f32::to_bits),
                [0x3f35_04f3, 0, 0, 0xbf35_04f3],
                "{t}: {q:?}"
            );
        }
        let mut f = Frame::default();
        set_rotate_wide(&mut f, east.rotation.w, 0.0, 0.0, east.rotation.z);
        assert_eq!(f.rotation, east.rotation);
    }

    #[test]
    fn a_flat_heading_normalises_in_double_precision() {
        // Normalised in single precision, this direction's heading is one unit in the last
        // place smaller (0x4219_e355).
        let v = Vec3::new(
            f32::from_bits(0x3e3a_9140),
            f32::from_bits(0x3e6a_c860),
            f32::from_bits(0x3e24_6744),
        );
        assert_eq!(
            flat_heading(v).to_bits(),
            0x4219_e356,
            "{}",
            flat_heading(v)
        );
        // Too short to have a direction once the vertical part is gone: north.
        assert_eq!(flat_heading(Vec3::new(1e-4, -1e-4, -9.8)).to_bits(), 0);
    }

    #[test]
    fn set_heading_round_trips_over_a_sweep() {
        for deg in 0..360 {
            #[allow(clippy::cast_precision_loss)]
            let d = deg as f32;
            let mut f = Frame::default();
            set_heading(&mut f, d);
            let back = get_heading(&f);
            let diff = ((back - d + 540.0) % 360.0) - 180.0;
            assert!(diff.abs() < 0.05, "{d} -> {back}");
        }
    }

    #[test]
    fn combine_composes_parent_then_child() {
        // Parent rotated 90 degrees about Z at (10, 0, 0); child offset (1, 0, 0) locally.
        let parent = Frame::new(
            Vec3::new(10.0, 0.0, 0.0),
            quat_axis(Vec3::new(0.0, 0.0, 1.0), std::f32::consts::FRAC_PI_2),
        );
        let child = Frame::new(Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY);
        let out = combine(&parent, &child);
        assert!(
            out.origin.eq_eps(Vec3::new(10.0, 1.0, 0.0)),
            "{:?}",
            out.origin
        );
    }

    #[test]
    fn is_zero_uses_per_component_magnitude() {
        assert!(Vec3::new(0.000_1, -0.000_1, 0.0).is_zero());
        assert!(!Vec3::new(0.000_3, 0.0, 0.0).is_zero());
        // a vector that is short overall but has one large component is NOT zero
        assert!(!Vec3::new(0.0, 0.0, 0.001).is_zero());
    }

    #[test]
    fn normalize_check_small_reports_true_only_when_it_did_nothing() {
        let mut v = Vec3::new(0.0, 0.0, 0.000_1);
        assert!(v.normalize_check_small());
        assert_eq!(v, Vec3::new(0.0, 0.0, 0.000_1));
        let mut w = Vec3::new(0.0, 0.0, 3.0);
        assert!(!w.normalize_check_small());
        assert_eq!(w, Vec3::new(0.0, 0.0, 1.0));
    }

    // Oracle: the recovered position and frame behavior, the worked
    // definition of cylinder_distance, evaluated (not restated).
    #[test]
    fn cylinder_distance_branches() {
        let c = CellId(0x0001_0001);
        let at =
            |x: f32, z: f32| Position::new(c, Frame::new(Vec3::new(x, 0.0, z), Quat::IDENTITY));
        // side by side, no vertical gap: the horizontal clearance
        let d = cylinder_distance(0.5, 2.0, &at(0.0, 0.0), 0.5, 2.0, &at(3.0, 0.0));
        assert!((d - 2.0).abs() < 1e-5, "{d}");
        // Vertically separated but with radii wide enough that the (3-D!) horizontal term is
        // negative: the result is the vertical gap alone.
        let d = cylinder_distance(3.0, 2.0, &at(0.0, 0.0), 3.0, 2.0, &at(0.0, 5.0));
        assert!((d - 3.0).abs() < 1e-5, "{d}");
        // With narrow radii the same pair reports the diagonal, because the horizontal term is
        // the 3-D centre distance and so already contains the vertical separation. That is
        // the full three-dimensional distance in one line.
        let d = cylinder_distance(0.5, 2.0, &at(0.0, 0.0), 0.5, 2.0, &at(0.0, 5.0));
        assert!((d - 5.0).abs() < 1e-5, "{d}");
        // fully overlapping: negative penetration
        let d = cylinder_distance(0.5, 2.0, &at(0.0, 0.0), 0.5, 2.0, &at(0.2, 0.5));
        assert!(d < 0.0, "{d}");
    }

    #[test]
    fn cylinder_distance_horizontal_term_is_three_dimensional() {
        // Two positions separated only in Z: the "horizontal" term still sees that separation,
        // which a two-dimensional implementation would get wrong.
        let c = CellId(0x0001_0001);
        let a = Position::new(c, Frame::new(Vec3::ZERO, Quat::IDENTITY));
        let b = Position::new(c, Frame::new(Vec3::new(0.0, 0.0, 10.0), Quat::IDENTITY));
        assert!((cylinder_distance_no_z(0.5, &a, 0.5, &b) - 9.0).abs() < 1e-5);
    }

    #[test]
    fn determine_quadrant_encodes_the_seven_documented_bits() {
        let c = CellId(0x0001_0001);
        let other = Position::new(c, Frame::new(Vec3::ZERO, Quat::IDENTITY));
        // "this" in front, to the right, high up on a 3 m creature
        let this = Position::new(c, Frame::new(Vec3::new(1.0, 1.0, 2.5), Quat::IDENTITY));
        let q = determine_quadrant(&other, 3.0, &this);
        assert_eq!(q & 0x10, 0x10, "Right");
        assert_eq!(q & 0x20, 0x20, "Front");
        assert_eq!(q & 0x01, 0x01, "High");
        // behind, left, low
        let this = Position::new(c, Frame::new(Vec3::new(-1.0, -1.0, 0.1), Quat::IDENTITY));
        let q = determine_quadrant(&other, 3.0, &this);
        assert_eq!(q & 0x08, 0x08, "Left");
        assert_eq!(q & 0x40, 0x40, "Back");
        assert_eq!(q & 0x04, 0x04, "Low");
    }
}
