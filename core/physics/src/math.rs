//! Vector, frame and position arithmetic, preserving the observed operand order.
//!
//! The vector and frame arithmetic is the shared implementation ([`dereth_primitives::frame`]),
//! re-exported here at the paths physics has always used; the animation player and the landscape
//! use the same code. What only physics needs is defined here: the block-margin test, the
//! heading-only validity check, frame interpolation and the position arithmetic that crosses
//! landblocks.

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
pub fn interpolate_rotation(out: &mut Frame, a: &Frame, b: &Frame, t: f32) {
    let (qa, mut qb) = (a.rotation, b.rotation);
    let mut dot = qa.w * qb.w + qa.x * qb.x + qa.y * qb.y + qa.z * qb.z;
    if dot < 0.0 {
        dot = -dot;
        qb = Quat::new(-qb.w, -qb.x, -qb.y, -qb.z);
    }
    let (wa, wb);
    if 1.0 - dot <= EPSILON {
        wa = 1.0 - t;
        wb = t;
    } else {
        let theta = math::acos(f64::from(dot));
        let sin_theta = math::sin(theta);
        let sa = math::sin((1.0 - f64::from(t)) * theta) / sin_theta;
        let sb = math::sin(f64::from(t) * theta) / sin_theta;
        if (0.0..=1.0).contains(&sa) && (0.0..=1.0).contains(&sb) {
            #[allow(clippy::cast_possible_truncation)]
            {
                wa = sa as f32;
                wb = sb as f32;
            }
        } else {
            wa = 1.0 - t;
            wb = t;
        }
    }
    set_rotate(
        out,
        qb.w * wb + qa.w * wa,
        qb.x * wb + qa.x * wa,
        qb.y * wb + qa.y * wa,
        qa.z * wa + qb.z * wb,
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
