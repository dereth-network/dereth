//! Digit grouping — the client's own, not Rust's.
//!
//! The formatter and its one-record memo live here rather than in
//! `dereth_ui_screens::panels::numfmt` so the HUD's view (which formats the slumlord's payment
//! lines) and the panels format with the
//! same rule. The screen crate reads the language-info record out of its asset environment and
//! primes the memo (`set_shipped`); see that module for the sources.

use dereth_primitives::num::math;
use std::cell::RefCell;

/// One language's grouping rule, as the numeric formatter reads it from language-info.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grouping {
    /// The grouping size.
    pub size: i16,
    /// The grouping separator.
    pub separator: String,
    /// The negative-number format — `"-%s"` in English. The `%s` is the digits.
    pub negative_format: String,
}

/// `NUMBERFMTA::Grouping`.
pub const XPTOSTRING_GROUPING: i16 = 3;
/// `NUMBERFMTA::lpThousandSep`, a fixed literal in retail.
pub const XPTOSTRING_THOUSAND_SEP: &str = ",";
/// `NUMBERFMTA::lpDecimalSep`, a fixed literal in retail.
pub const XPTOSTRING_DECIMAL_SEP: &str = ".";
/// `NUMBERFMTA::NegativeOrder` — 1 is "minus sign, then the number".
pub const XPTOSTRING_NEGATIVE_ORDER: u32 = 1;

/// The group and enum value the language-info start-up looks the record up by (type `0x30`).
/// Never a `DataID` at a call site.
pub const LANGUAGE_INFO_GROUP: u32 = 4;
/// See [`LANGUAGE_INFO_GROUP`].
pub const LANGUAGE_INFO_ENUM: u32 = 1;

thread_local! {
    // ORDER-OK: a one-slot memo of an immutable shipped file.
    static SHIPPED: RefCell<Option<Grouping>> = const { RefCell::new(None) };
}

/// Forget the memoised [`Grouping`]. A test does this so a second dat build is read again.
pub fn clear_cache() {
    SHIPPED.with(|c| *c.borrow_mut() = None);
}

/// The experience formatter's `NUMBERFMTA` pair, which is what [`shipped`] answers with when no
/// asset source has been read.
pub fn fallback() -> Grouping {
    Grouping {
        size: XPTOSTRING_GROUPING,
        separator: XPTOSTRING_THOUSAND_SEP.to_owned(),
        negative_format: "-%s".to_owned(),
    }
}

/// The shipped language-info grouping rule `prime` read, or the experience formatter's
/// `NUMBERFMTA` pair when no asset source has been read.
///
/// Memoized: the client formatter reaches language-info through its database cache; this does not.
#[must_use]
pub fn shipped() -> Grouping {
    SHIPPED
        .with(|c| c.borrow().clone())
        .unwrap_or_else(fallback)
}

/// The numeric formatter's integer path: the digits of `v` in base 10, with a
/// [`Grouping::separator`] inserted every [`Grouping::size`] digits from the right, and
/// [`Grouping::negative_format`] applied to a negative value.
///
/// The long-integer variant first converts the signed integer to a double.
/// The formatter then walks that rounded value from the least significant digit: it
/// multiplies the remaining value by `pow(base, -digit_index)`, adds 0.5, truncates to an
/// integer, and takes the unsigned remainder by the base before subtracting that digit's value.
/// That conversion is observable above the double's exact-integer range, so formatting the
/// original i64's decimal string is not equivalent.
///
/// When grouping is on and the digit index is a non-zero multiple of the grouping size, the
/// separator is inserted (backwards, like the digits) before it.
///
/// A grouping size of 0 or less means "no grouping", which is the same guard the client's
/// `i % size` would divide by.
#[must_use]
pub fn group(v: i64, g: &Grouping) -> String {
    // The cast is retail's integer-to-double conversion. Preserve the following floating
    // operations as separate operations: retail uses `pow(double, double)`, multiply, add 0.5,
    // then truncation to an integer.
    #[allow(clippy::cast_precision_loss)]
    let mut remaining = (v as f64).abs();
    let limit = remaining.max(1.0);
    let mut power = 1.0_f64;
    let mut digit_index = 0_i32;
    let mut reversed = String::new();
    loop {
        let reciprocal_power = math::pow(10.0_f64, -f64::from(digit_index));
        let mut scaled = remaining * reciprocal_power;
        scaled += 0.5;
        let digit = u8::try_from((dereth_primitives::num::to_i64_f64(scaled) as u64) % 10)
            .expect("a base-ten remainder");
        reversed.push(char::from(b'0' + digit));
        remaining -= f64::from(digit) * power;
        power *= 10.0;
        digit_index += 1;
        if power > limit {
            break;
        }
    }
    let digits: String = reversed.chars().rev().collect();
    let mut out = String::with_capacity(digits.len() * 2);
    let size = usize::try_from(g.size).unwrap_or(0);
    for (i, c) in digits.chars().enumerate() {
        let left = digits.len() - i;
        if i > 0 && size > 0 && left.is_multiple_of(size) {
            out.push_str(&g.separator);
        }
        out.push(c);
    }
    if v < 0 {
        // The negative-number format is a `%s` template, not a bare sign.
        if g.negative_format.contains("%s") {
            return g.negative_format.replace("%s", &out);
        }
        return format!("-{out}");
    }
    out
}

/// [`group`] against the [`shipped`] rule — what every integer variable in the two stat panels
/// renders to.
#[must_use]
pub fn number(v: i64) -> String {
    group(v, &shipped())
}

/// Memoise `g` as the shipped grouping rule — what the screen crate's `prime` does once it has
/// read the language-info record out of its asset environment.
pub fn set_shipped(g: Grouping) {
    SHIPPED.with(|c| *c.borrow_mut() = Some(g));
}
