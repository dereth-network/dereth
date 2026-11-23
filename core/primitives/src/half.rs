//! Half floats: a value held in 16 bits (a sign, five bits of exponent and ten of fraction) where
//! that much precision is all a value keeps, so a field of them takes half the room of single
//! floats and reads back exactly the values it was given.
//!
//! [`bits`] holds a value a half float can hold exactly; [`value`] reads it back. The two are
//! exact inverses over every half float, subnormal ones and both zeros among them, so a value
//! rounded to a half float first loses nothing more by being held this way. The rounding itself
//! is the caller's.

/// A subnormal half float's unit, 2^-24.
const UNIT: f32 = 1.0 / 16_777_216.0;

/// The bits of the half float holding `v`, which must be one a half float holds exactly (a value
/// already rounded to one); infinities and NaN keep their kind.
#[must_use]
pub fn bits(v: f32) -> u16 {
    let b = v.to_bits();
    // LINT-OK: the sign bit, moved down to a half float's.
    #[allow(clippy::cast_possible_truncation)]
    let sign = ((b >> 16) & 0x8000) as u16;
    let a = v.abs();
    if a.is_nan() {
        return sign | 0x7E00;
    }
    if a.is_infinite() {
        return sign | 0x7C00;
    }
    // LINT-OK: an 8-bit exponent field.
    #[allow(clippy::cast_possible_wrap)]
    let exp = ((b >> 23) & 0xFF) as i32 - 127;
    if exp >= -14 {
        // A normal half float: its exponent rebiased and its ten fraction bits, the top ten of
        // the single float's, whose other thirteen are clear.
        debug_assert!(exp <= 15 && b & 0x1FFF == 0, "{v} is not a half float");
        // LINT-OK: an exponent of 1..=30 and ten bits of fraction.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let h = sign | ((exp + 15) as u16) << 10 | ((b >> 13) & 0x3FF) as u16;
        h
    } else {
        // A subnormal half float (or zero): a whole number of 2^-24 units, fewer than 1024.
        let units = a / UNIT;
        debug_assert!(
            units.fract() == 0.0 && units < 1024.0,
            "{v} is not a half float"
        );
        // LINT-OK: a whole number under 1024, checked above.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let h = sign | units as u16;
        h
    }
}

/// The value of the half float `h`, exactly.
#[must_use]
pub fn value(h: u16) -> f32 {
    let sign = u32::from(h & 0x8000) << 16;
    let exp = u32::from(h >> 10) & 0x1F;
    let fraction = u32::from(h & 0x3FF);
    match exp {
        // Zero or subnormal: a whole number of 2^-24 units, which a single float holds exactly
        // as a normal number.
        0 => f32::from_bits(sign | (f32::from(h & 0x3FF) * UNIT).to_bits()),
        0x1F => f32::from_bits(sign | 0x7F80_0000 | fraction << 13),
        _ => f32::from_bits(sign | (exp + 112) << 23 | fraction << 13),
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a value held in 16 bits)
    use super::*;

    /// The value a half float's bits stand for, worked out in double precision from its fields.
    fn reference(h: u16) -> f64 {
        let sign = if h & 0x8000 == 0 { 1.0 } else { -1.0 };
        let exp = i32::from((h >> 10) & 0x1F);
        let fraction = f64::from(h & 0x3FF);
        if exp == 0 {
            sign * fraction * 2f64.powi(-24)
        } else {
            sign * (1.0 + fraction / 1024.0) * 2f64.powi(exp - 15)
        }
    }

    /// Every half float, subnormal ones and both zeros among them, reads back as exactly the
    /// value its bits stand for, and holding that value gives back the same bits.
    #[test]
    fn every_half_float_reads_back_as_itself() {
        for h in 0..=u16::MAX {
            let (exp, fraction) = ((h >> 10) & 0x1F, h & 0x3FF);
            let v = value(h);
            if exp == 0x1F {
                assert_eq!(v.is_nan(), fraction != 0, "{h:#06x}");
                assert_eq!(bits(v) & 0x7C00, 0x7C00, "{h:#06x}");
                continue;
            }
            assert_eq!(f64::from(v).to_bits(), reference(h).to_bits(), "{h:#06x}");
            assert_eq!(bits(v), h, "{h:#06x}");
        }
    }
}
