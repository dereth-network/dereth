//! Just enough of .NET `decimal` for ACE's chance arithmetic: an exact `mantissa / 10^scale`.
//!
//! Two ported callers sum chances in `decimal`: `ChanceTable.VerifyTable` adds `(decimal)float`
//! values and compares the total with 1, and the mutation scripts' loader parses percentages as
//! decimal text, divides them by 100 and hands the running total back as a `float`. This is the
//! value both use; it is not a complete `decimal` (no 96-bit overflow, no rounding on
//! division, no culture), only the operations those two need, each exact over their inputs.
//!
//! The scale is kept as the value was built, so [`Decimal::dotnet_string`] prints trailing zeros
//! as .NET does (`1.50 + 0.25` is `1.75`, `0.10 + 0.20` is `0.30`).

use std::cmp::Ordering;
use std::ops::{Add, Sub};

/// An exact decimal `mantissa / 10^scale`, as .NET's `decimal` holds it.
#[derive(Clone, Copy, Debug)]
pub struct Decimal {
    mantissa: i128,
    scale: u32,
}

impl Decimal {
    /// `0m`.
    pub const ZERO: Self = Self {
        mantissa: 0,
        scale: 0,
    };
    /// `1m`.
    pub const ONE: Self = Self {
        mantissa: 1,
        scale: 0,
    };

    /// `mantissa / 10^scale`, as written.
    #[must_use]
    pub const fn new(mantissa: i128, scale: u32) -> Self {
        Self { mantissa, scale }
    }

    /// `(decimal)f`: .NET keeps 7 significant digits of a `float` and drops trailing zeros.
    #[must_use]
    pub fn from_f32(f: f32) -> Self {
        if f == 0.0 {
            return Self::ZERO;
        }
        let s = format!("{f:.6e}");
        let (m, e) = s.split_once('e').expect("exponent");
        let negative = m.starts_with('-');
        let digits: String = m.chars().filter(char::is_ascii_digit).collect();
        let mut mantissa: i128 = digits.parse().expect("digits");
        let exp: i32 = e.parse().expect("exponent");
        let mut scale = 6 - exp;
        while scale > 0 && mantissa % 10 == 0 {
            mantissa /= 10;
            scale -= 1;
        }
        if scale < 0 {
            mantissa *= 10i128.pow(scale.unsigned_abs());
            scale = 0;
        }
        let mantissa = if negative { -mantissa } else { mantissa };
        Self {
            mantissa,
            scale: scale.unsigned_abs(),
        }
    }

    /// `decimal.TryParse` of a run of digits and dots (en-US): at most one `.`, at least one digit.
    /// The scale is the number of digits after the point, trailing zeros included.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let (int, frac) = match s.split_once('.') {
            Some((i, f)) => (i, f),
            None => (s, ""),
        };
        if frac.contains('.') || (int.is_empty() && frac.is_empty()) {
            return None;
        }
        let digits = format!("{int}{frac}");
        let mantissa = if digits.is_empty() {
            0
        } else {
            digits.parse::<i128>().ok()?
        };
        Some(Self {
            mantissa,
            scale: u32::try_from(frac.len()).ok()?,
        })
    }

    fn rescaled(self, scale: u32) -> i128 {
        self.mantissa * 10i128.pow(scale - self.scale)
    }

    /// `Math.Abs(d)`, keeping the scale.
    #[must_use]
    pub fn abs(self) -> Self {
        Self {
            mantissa: self.mantissa.abs(),
            scale: self.scale,
        }
    }

    /// `d / 100`, exact: the scale grows by two.
    #[must_use]
    pub fn div_100(self) -> Self {
        Self {
            mantissa: self.mantissa,
            scale: self.scale + 2,
        }
    }

    /// The numeric comparison, whatever the two scales (`1.0` equals `1`).
    #[must_use]
    pub fn cmp_value(self, other: Self) -> Ordering {
        let scale = self.scale.max(other.scale);
        self.rescaled(scale).cmp(&other.rescaled(scale))
    }

    /// `a > b`, numerically.
    #[must_use]
    pub fn gt(self, other: Self) -> bool {
        self.cmp_value(other) == Ordering::Greater
    }

    /// `(float)d`: .NET converts through `double` as `(double)mantissa / 10^scale`.
    #[must_use]
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn to_f32(self) -> f32 {
        let dbl = self.mantissa as f64 / 10f64.powi(i32::try_from(self.scale).unwrap_or(i32::MAX));
        dbl as f32
    }

    /// `decimal.ToString()`: every digit of the scale, trailing zeros included.
    #[must_use]
    pub fn dotnet_string(self) -> String {
        let digits = self.mantissa.unsigned_abs().to_string();
        let scale = self.scale as usize;
        let sign = if self.mantissa < 0 { "-" } else { "" };
        if scale == 0 {
            return format!("{sign}{digits}");
        }
        let padded = format!("{digits:0>width$}", width = scale + 1);
        let (int, frac) = padded.split_at(padded.len() - scale);
        format!("{sign}{int}.{frac}")
    }
}

impl Add for Decimal {
    type Output = Self;

    /// `a + b`, at the larger of the two scales.
    fn add(self, other: Self) -> Self {
        let scale = self.scale.max(other.scale);
        Self {
            mantissa: self.rescaled(scale) + other.rescaled(scale),
            scale,
        }
    }
}

impl Sub for Decimal {
    type Output = Self;

    /// `a - b`, at the larger of the two scales.
    fn sub(self, other: Self) -> Self {
        let scale = self.scale.max(other.scale);
        Self {
            mantissa: self.rescaled(scale) - other.rescaled(scale),
            scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Decimal;
    use std::cmp::Ordering;

    #[test]
    fn a_float_keeps_seven_significant_digits_and_no_trailing_zeros() {
        assert_eq!(Decimal::from_f32(0.1).dotnet_string(), "0.1");
        assert_eq!(Decimal::from_f32(0.333_333_34).dotnet_string(), "0.3333333");
        assert_eq!(Decimal::from_f32(-2.5).dotnet_string(), "-2.5");
        assert_eq!(
            Decimal::from_f32(1_234_567_800.0).dotnet_string(),
            "1234568000"
        );
        assert_eq!(Decimal::from_f32(0.0).dotnet_string(), "0");
    }

    #[test]
    fn parsed_text_keeps_its_scale_and_divides_by_a_hundred_exactly() {
        let d = Decimal::parse("12.50").expect("digits");
        assert_eq!(d.dotnet_string(), "12.50");
        assert_eq!(d.div_100().dotnet_string(), "0.1250");
        assert_eq!(
            Decimal::parse(".5").map(Decimal::dotnet_string).as_deref(),
            Some("0.5")
        );
        assert_eq!(
            Decimal::parse("7").map(Decimal::dotnet_string).as_deref(),
            Some("7")
        );
        assert!(Decimal::parse("1.2.3").is_none());
        assert!(Decimal::parse(".").is_none());
        assert!(Decimal::parse("").is_none());
        assert_eq!(
            Decimal::parse("33.3").expect("digits").div_100().to_f32(),
            0.333_f32
        );
    }

    #[test]
    fn sums_compare_by_value_across_scales() {
        let third = Decimal::from_f32(0.333_333_34);
        let total = third + third + third;
        assert_eq!(total.dotnet_string(), "0.9999999");
        assert!(!(Decimal::ONE - total).abs().gt(Decimal::new(1, 7)));
        assert_eq!(
            Decimal::parse("1.00")
                .expect("digits")
                .cmp_value(Decimal::ONE),
            Ordering::Equal
        );
        let sum = Decimal::parse("0.10").expect("digits") + Decimal::parse("0.20").expect("digits");
        assert_eq!(sum.dotnet_string(), "0.30");
    }
}
