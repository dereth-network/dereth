//! `System.Math` members whose .NET semantics differ from Rust's `f64` methods.
//!
//! Source: dotnet/runtime `src/libraries/System.Private.CoreLib/src/System/Math.cs` (the
//! implementation ACE's net10.0 runtime executes). Expected values in the tests were produced by
//! running these members on the .NET 8.0.22 runtime (`tests/fixtures/dotnet_oracle.cs`); the
//! members involved are unchanged between .NET 8 and .NET 10.

/// `System.MidpointRounding`, with its numeric values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidpointRounding {
    /// Round half to even (banker's rounding); the default of every `Math.Round` overload.
    ToEven = 0,
    /// Round half away from zero.
    AwayFromZero = 1,
    /// Directed rounding toward zero.
    ToZero = 2,
    /// Directed rounding toward negative infinity.
    ToNegativeInfinity = 3,
    /// Directed rounding toward positive infinity.
    ToPositiveInfinity = 4,
}

/// `Math.Round(double, int, MidpointRounding)` accepts at most this many digits.
pub const MAX_ROUNDING_DIGITS: i32 = 15;

/// Values at or above this magnitude are already integral at every digit count, so `Math.Round`
/// returns them unchanged (`doubleRoundLimit` in Math.cs).
const DOUBLE_ROUND_LIMIT: f64 = 1e16;

/// `Math.Round(double)`: round half to even.
#[must_use]
pub fn round(value: f64) -> f64 {
    value.round_ties_even()
}

/// `Math.Round(double, MidpointRounding)`.
#[must_use]
pub fn round_mode(value: f64, mode: MidpointRounding) -> f64 {
    round_digits_mode(value, 0, mode)
}

/// `Math.Round(double, int)`: `digits` fractional digits, half to even.
///
/// # Panics
/// When `digits` is outside `0..=15`, as .NET throws `ArgumentOutOfRangeException`.
#[must_use]
pub fn round_digits(value: f64, digits: i32) -> f64 {
    round_digits_mode(value, digits, MidpointRounding::ToEven)
}

/// `Math.Round(double, int, MidpointRounding)`.
///
/// Math.cs: scale by `10^digits`, apply the mode, divide back; values of magnitude `>= 1e16` are
/// returned unchanged. `AwayFromZero` is `Truncate(value + CopySign(BitDecrement(0.5), value))`.
///
/// # Panics
/// When `digits` is outside `0..=15`, as .NET throws `ArgumentOutOfRangeException`.
#[must_use]
pub fn round_digits_mode(value: f64, digits: i32, mode: MidpointRounding) -> f64 {
    assert!(
        (0..=MAX_ROUNDING_DIGITS).contains(&digits),
        "ArgumentOutOfRangeException: Math.Round digits {digits} is outside 0..=15"
    );
    let mut value = value;
    if value.abs() < DOUBLE_ROUND_LIMIT {
        let power10 = POWERS_OF_TEN[usize::try_from(digits).unwrap_or(0)];
        value *= power10;
        value = match mode {
            MidpointRounding::ToEven => value.round_ties_even(),
            MidpointRounding::AwayFromZero => (value + BIT_DECREMENT_HALF.copysign(value)).trunc(),
            MidpointRounding::ToZero => value.trunc(),
            MidpointRounding::ToNegativeInfinity => value.floor(),
            MidpointRounding::ToPositiveInfinity => value.ceil(),
        };
        value /= power10;
    }
    value
}

/// `Math.BitDecrement(0.5)`, the largest double below one half.
const BIT_DECREMENT_HALF: f64 = 0.499_999_999_999_999_94;

/// `RoundPower10Double` in Math.cs.
const POWERS_OF_TEN: [f64; 16] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15,
];

/// `Math.Max(double, double)`: IEEE 754:2019 `maximum` (NaN propagates, `+0 > -0`), not Rust's
/// NaN-ignoring `f64::max`.
#[must_use]
pub fn max(val1: f64, val2: f64) -> f64 {
    if val1 != val2 {
        if !val1.is_nan() {
            return if val2 < val1 { val1 } else { val2 };
        }
        return val1;
    }
    if val2.is_sign_negative() {
        val1
    } else {
        val2
    }
}

/// `Math.Min(double, double)`: IEEE 754:2019 `minimum` (NaN propagates, `-0 < +0`).
#[must_use]
pub fn min(val1: f64, val2: f64) -> f64 {
    if val1 != val2 {
        if !val1.is_nan() {
            return if val1 < val2 { val1 } else { val2 };
        }
        return val1;
    }
    if val1.is_sign_negative() {
        val1
    } else {
        val2
    }
}

/// `Math.Max(float, float)`, same rules as [`max`].
#[must_use]
pub fn max_f32(val1: f32, val2: f32) -> f32 {
    if val1 != val2 {
        if !val1.is_nan() {
            return if val2 < val1 { val1 } else { val2 };
        }
        return val1;
    }
    if val2.is_sign_negative() {
        val1
    } else {
        val2
    }
}

/// `Math.Min(float, float)`, same rules as [`min`].
#[must_use]
pub fn min_f32(val1: f32, val2: f32) -> f32 {
    if val1 != val2 {
        if !val1.is_nan() {
            return if val1 < val2 { val1 } else { val2 };
        }
        return val1;
    }
    if val1.is_sign_negative() {
        val1
    } else {
        val2
    }
}
