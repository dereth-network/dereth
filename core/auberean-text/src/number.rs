//! Numbers as a string table writes them: in its base (or the language's), with a number of
//! decimals, the language's numerals and separators, grouped or not, and leading zeros to a least
//! number of digits when the table (or the language) keeps them.
//!
//! Every number, whole or not, is written by one routine over a double: digit by digit from the
//! last decimal up, each digit `(remainder × base^-position + 0.5)` cut to an integer, modulo the
//! base, the remainder then reduced by the digit's worth; it stops past the number's size (at least
//! the units) once enough digits are written. The decimal separator goes before the units digit,
//! a grouping separator before every group of digits after the first. A negative number's digits
//! go into the language's negative pattern, a hexadecimal one's after `0x`. The arithmetic is the
//! client's: doubles (its floating point unit runs at double precision), its power function, and
//! its float-to-integer cut, so a number comes out as the client writes it, last digit included.

use crate::crt::{format_s, ftol, power};
use crate::language::Language;

/// A table's own number settings (negative for "the language's").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableNumbers {
    pub base: i16,
    pub decimal_digits: i16,
    pub leading_zero: i32,
}

impl Default for TableNumbers {
    fn default() -> Self {
        Self {
            base: -1,
            decimal_digits: -1,
            leading_zero: -1,
        }
    }
}

/// The digits buffer's size: a number longer than this is not written.
const BUFFER: usize = 64;

/// Write `x` with `decimals` decimals (at most 20), grouped when `grouping` is not 0, in `base`
/// (negative for the table's, then the language's). `None` when the client writes nothing: no
/// language, a base below 2 or beyond the numerals, too many decimals, or too long a number.
// D2-14074
#[must_use]
pub fn float_to_string(
    table: TableNumbers,
    lang: Option<&Language>,
    x: f64,
    decimals: u16,
    grouping: i32,
    base: i16,
) -> Option<Vec<u16>> {
    let lang = lang?;
    if decimals > 20 {
        return None;
    }
    let base = if base >= 0 {
        base
    } else if table.base >= 0 {
        table.base
    } else {
        lang.base()
    };
    let leading_zero = if table.leading_zero >= 0 {
        table.leading_zero
    } else {
        lang.leading_zero()
    };
    let least = if table.decimal_digits >= 0 {
        table.decimal_digits
    } else {
        lang.decimal_digits()
    };
    if base <= 1 {
        return None;
    }
    let base_u = u32::from(base.unsigned_abs());
    let numerals = lang.numerals();
    if numerals.len() < base_u as usize {
        return None;
    }
    let magnitude = x.abs();
    // Past the number's size, but at least past the units (a NaN never stops).
    let limit = if magnitude < 1.0 { 1.0 } else { magnitude };
    let base_f = f64::from(base);
    // Positions are 16-bit in the client.
    #[allow(clippy::cast_possible_wrap)]
    let mut position = (decimals as i16).wrapping_neg();
    let mut place = power(base_u, i32::from(position));
    let mut remainder = magnitude;
    let mut buf = [0u16; BUFFER];
    let mut at: isize = 62;
    let mut written: i16 = 0;
    loop {
        let scaled = power(base_u, -i32::from(position)) * remainder + 0.5;
        let digit = ftol(scaled) % u64::from(base_u);
        let idx = usize::try_from(at).ok()?;
        buf[idx] = numerals[usize::try_from(digit).ok()?];
        at -= 1;
        written = written.wrapping_add(1);
        position = position.wrapping_add(1);
        #[allow(clippy::cast_precision_loss)]
        let worth = digit as f64 * place;
        remainder -= worth;
        place *= base_f;
        if place > limit && (leading_zero == 0 || written >= least) {
            break;
        }
        if at == 0 {
            return None;
        }
        if grouping != 0
            && position > 0
            && lang.grouping_size() != 0
            && i32::from(position) % i32::from(lang.grouping_size()) == 0
        {
            for &u in lang.grouping_separator().iter().rev() {
                buf[usize::try_from(at).ok()?] = u;
                at -= 1;
            }
        }
        if position == 0 {
            for &u in lang.decimal_separator().iter().rev() {
                buf[usize::try_from(at).ok()?] = u;
                at -= 1;
            }
        }
    }
    let digits = &buf[usize::try_from(at + 1).ok()?..63];
    let fmt: Vec<u16> = if x < 0.0 {
        lang.negative_format().to_vec()
    } else if base == 16 {
        "0x%s".encode_utf16().collect()
    } else {
        "%s".encode_utf16().collect()
    };
    Some(format_s(&fmt, &[digits]))
}

/// A signed 32-bit number.
// D2-14085
#[must_use]
pub fn int_to_string(
    table: TableNumbers,
    lang: Option<&Language>,
    v: i32,
    grouping: i32,
    base: i16,
) -> Option<Vec<u16>> {
    float_to_string(table, lang, f64::from(v), 0, grouping, base)
}

/// An unsigned 32-bit number.
// D2-14086
#[must_use]
pub fn uint_to_string(
    table: TableNumbers,
    lang: Option<&Language>,
    v: u32,
    grouping: i32,
    base: i16,
) -> Option<Vec<u16>> {
    float_to_string(table, lang, f64::from(v), 0, grouping, base)
}

/// A signed 64-bit number (rounded once to a double).
// D2-14087
#[must_use]
pub fn long_to_string(
    table: TableNumbers,
    lang: Option<&Language>,
    v: i64,
    grouping: i32,
    base: i16,
) -> Option<Vec<u16>> {
    #[allow(clippy::cast_precision_loss)]
    let x = v as f64;
    float_to_string(table, lang, x, 0, grouping, base)
}

/// An unsigned 64-bit number (rounded once to a double).
// D2-14088
#[must_use]
pub fn ulong_to_string(
    table: TableNumbers,
    lang: Option<&Language>,
    v: u64,
    grouping: i32,
    base: i16,
) -> Option<Vec<u16>> {
    #[allow(clippy::cast_precision_loss)]
    let x = v as f64;
    float_to_string(table, lang, x, 0, grouping, base)
}
