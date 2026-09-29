// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Extensions/FloatExtensions.cs
//! `FloatExtensions`.

use crate::dotnet::math::{self, MidpointRounding};
use crate::dotnet::CsCast;

// ACE: FloatExtensions.Round(float, int)
/// `(int)Math.Round(num, decimalPlaces, MidpointRounding.AwayFromZero)`; the `float` widens to
/// `double` first. Out-of-range results saturate and NaN gives 0 (net10.0 casts).
///
/// # Panics
/// When `decimal_places` is outside `0..=15` (`ArgumentOutOfRangeException`).
#[must_use]
pub fn round(num: f32, decimal_places: i32) -> i32 {
    math::round_digits_mode(
        f64::from(num),
        decimal_places,
        MidpointRounding::AwayFromZero,
    )
    .cs_cast()
}

// ACE: FloatExtensions.Round(double, int)
/// The `double` overload of [`round`].
///
/// # Panics
/// When `decimal_places` is outside `0..=15` (`ArgumentOutOfRangeException`).
#[must_use]
pub fn round_f64(num: f64, decimal_places: i32) -> i32 {
    math::round_digits_mode(num, decimal_places, MidpointRounding::AwayFromZero).cs_cast()
}

// ACE: FloatExtensions.Truncate
/// `(float)(Math.Truncate(num * 10^decimalPlaces) / 10^decimalPlaces)`, in `double`.
#[must_use]
pub fn truncate(num: f32, decimal_places: i32) -> f32 {
    let multiplier = crate::math::pow(10.0, f64::from(decimal_places));
    ((f64::from(num) * multiplier).trunc() / multiplier).cs_cast()
}

// ACE: FloatExtensions.EpsilonEquals
/// `Math.Abs(num - val) < 0.0001f`, in `float`.
#[must_use]
pub fn epsilon_equals(num: f32, val: f32) -> bool {
    let epsilon = 0.0001f32;
    (num - val).abs() < epsilon
}
