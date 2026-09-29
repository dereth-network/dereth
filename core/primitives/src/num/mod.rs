//! Numerics kernel: the arithmetic policy every other crate is measured against.
//!
//! Nothing here is a matter of taste. Each item reproduces a specific behaviour of the retail client
//! that is observable in play, and getting one wrong silently invalidates every later comparison
//! against the original, including floating-point edge cases and bit patterns.

pub mod hash;
pub mod math;
pub mod rng;

/// Reproduce the MSVC 7 runtime's float-to-int conversion, which the retail client uses for
/// **every** float-to-int cast.
///
/// It truncates toward zero regardless of the current rounding mode. For NaN and for anything outside
/// `i32` range it yields the x86 "integer indefinite" value `0x8000_0000`.
///
/// Rust's `as` conversion is *saturating*: NaN becomes `0`, large positives become `i32::MAX`. The
/// two therefore disagree by the full width of the type on NaN and on positive overflow, which is
/// why this helper exists and why raw `as i32` on a float is forbidden by the workspace lints.
///
/// Matches the original runtime's float-to-integer conversion.
#[inline]
#[must_use]
pub fn to_i32(x: f32) -> i32 {
    if x.is_nan() || x < -2_147_483_648.0 || x >= 2_147_483_648.0 {
        i32::MIN
    } else {
        // The range check above is exactly what makes this cast safe and exact; this is the one
        // sanctioned `as i32` on a float in the workspace.
        #[allow(clippy::cast_possible_truncation)]
        {
            x as i32
        }
    }
}

/// The floor-then-convert idiom, which the original uses in many places and which is *not* the
/// same as [`to_i32`] for negative values.
#[inline]
#[must_use]
pub fn floor_to_i32(x: f32) -> i32 {
    to_i32(x.floor())
}

/// Same as [`to_i32`] for `f64`, which the original reaches through the same helper.
#[inline]
#[must_use]
pub fn to_i32_f64(x: f64) -> i32 {
    if x.is_nan() || x < -2_147_483_648.0 || x >= 2_147_483_648.0 {
        i32::MIN
    } else {
        #[allow(clippy::cast_possible_truncation)]
        {
            x as i32
        }
    }
}

/// The full 64-bit result of the same conversion. The store is a full 64-bit integer store and
/// the correction after it truncates toward zero. An invalid conversion yields `i64::MIN`. A caller
/// that wants only the low 32 bits takes them from this value afterward.
#[inline]
#[must_use]
pub fn to_i64_f64(x: f64) -> i64 {
    if !x.is_finite() || !(-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0).contains(&x) {
        i64::MIN
    } else {
        #[allow(clippy::cast_possible_truncation)]
        {
            x as i64
        }
    }
}

#[test]
#[allow(clippy::cast_possible_truncation)] // the low 32 bits are the point
fn ftol2_full_result_preserves_low_word_and_invalid_value() {
    assert_eq!(to_i64_f64(90.9), 90);
    assert_eq!(to_i64_f64(-90.9), -90);
    assert_eq!(to_i64_f64(4_294_967_297.0) as u32, 1);
    for x in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        9_223_372_036_854_775_808.0,
    ] {
        assert_eq!(to_i64_f64(x), i64::MIN);
    }
}

/// Constants the engine treats as fixed. Values are those the retail client uses; each names where
/// it comes from so it can be re-checked.
pub mod consts {
    /// Physics gravity. It is fixed in the client, never read from a dat or a server message, so
    /// every client agrees on it.
    pub const GRAVITY: f32 = -9.8;

    /// The physics tick gate: the world does not advance at all below this. A frame shorter than
    /// 1/30 s leaves every object exactly where it was rather than integrating a short step.
    pub const MIN_QUANTUM: f32 = 1.0 / 30.0;

    /// The per-object sub-step ceiling. Note ACE uses 0.1 here and its own comments call that buggy;
    /// this is a recorded correction to the common truncating implementation.
    pub const MAX_QUANTUM: f32 = 0.2;

    /// Maximum walkable slope, as the cosine of the surface normal's Z.
    ///
    /// The original computes this at startup as `cos(3437.746770784939)`, which is 48.381 degrees.
    /// That argument is exactly one radian expressed in arcminutes, so it looks like a units mix-up
    /// in Turbine's source rather than a chosen angle — but the value shipped, so it is the value.
    /// Baked here rather than recomputed, because the result must not drift with the libm used.
    pub const FLOOR_Z: f32 = 0.664_174_15;

    /// The general comparison epsilon used throughout the physics and geometry code.
    pub const EPSILON: f32 = 0.0002;

    /// Landscape units, in metres: one landblock is 192 m of 8 cells at 24 m.
    pub const LANDBLOCK_SIZE: f32 = 192.0;
    /// One land cell, in metres.
    pub const CELL_SIZE: f32 = 24.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the recovered runtime behavior records the truncating float-to-int conversion of
    // the MSVC 7 runtime.
    #[test]
    fn to_i32_truncates_toward_zero() {
        assert_eq!(to_i32(1.9), 1);
        assert_eq!(to_i32(-1.9), -1); // toward zero, not floor
        assert_eq!(to_i32(0.0), 0);
        assert_eq!(to_i32(-0.5), 0);
    }

    #[test]
    fn to_i32_uses_integer_indefinite_where_rust_would_saturate() {
        // This is the whole reason the helper exists: `as` disagrees here.
        assert_eq!(to_i32(f32::NAN), i32::MIN);
        assert_eq!(to_i32(f32::INFINITY), i32::MIN);
        assert_eq!(to_i32(1e30), i32::MIN);
        assert_eq!(to_i32(-1e30), i32::MIN);
        // and confirm Rust's cast really does differ, so this test fails loudly if that ever changes
        #[allow(clippy::cast_possible_truncation, clippy::cast_nan_to_int)]
        {
            assert_eq!(f32::NAN as i32, 0);
            assert_eq!(f32::INFINITY as i32, i32::MAX);
        }
    }

    #[test]
    fn floor_differs_from_truncation_below_zero() {
        assert_eq!(to_i32(-2.5), -2);
        assert_eq!(floor_to_i32(-2.5), -3);
    }

    #[test]
    fn floor_z_matches_the_startup_cosine() {
        #[allow(clippy::cast_possible_truncation)]
        let computed = math::cos(3_437.746_770_784_939_f64) as f32;
        assert_eq!(consts::FLOOR_Z, computed);
        // 48.381 degrees
        let degrees = math::acos(consts::FLOOR_Z as f64).to_degrees();
        assert!((degrees - 48.381_000_7).abs() < 1e-6, "{degrees}");
    }
}
