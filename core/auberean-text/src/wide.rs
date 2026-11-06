//! The client's string library for wide text where it does more than Rust's `Vec<u16>`: equality
//! ignoring case, finding a part ignoring case, reading whole numbers in any base, the hexadecimal
//! and floating-point forms it appends, and conversion to and from narrow text.
//!
//! Text is UTF-16 units; a C function sees it up to its first zero unit ([`crt::cstr`]). Narrow
//! text is the English Windows code page (1252), the code page of the client's locale ("English"),
//! which the conversions ask for. Building, copying, joining, slicing, plain (case-kept) equality
//! and decimal numbers are Rust's own.

use crate::crt::{self, cstr};

/// Whether two wide strings are equal as C strings. With case kept, unit for unit; ignoring case,
/// each unit lowered as `towlower` lowers it ([`crt::to_lower`]).
// D2-00650, D2-09239
#[must_use]
pub fn equal(a: &[u16], b: &[u16], keep_case: bool) -> bool {
    let (a, b) = (cstr(a), cstr(b));
    if keep_case {
        return a == b;
    }
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(x, y)| crt::to_lower(*x) == crt::to_lower(*y))
}

/// A unit as the search ignoring case folds it: the narrow C library's `tolower` given the unit,
/// which sees only its low byte (above 255 too) and lowers that as the English code page does.
fn search_fold(u: u16) -> u8 {
    #[allow(clippy::cast_possible_truncation)]
    auberean_primitives::strings::lower(u as u8)
}

/// Whether `pattern` starts `text` at `at`, both folded ([`search_fold`]); the text ends at its
/// first zero, and an empty pattern always matches.
// D2-07668
fn starts_ignoring_case(text: &[u16], at: usize, pattern: &[u16]) -> bool {
    let mut k = 0;
    loop {
        let p = pattern.get(k).copied().unwrap_or(0);
        if p == 0 {
            return true;
        }
        let t = text.get(at + k).copied().unwrap_or(0);
        if t == 0 || search_fold(t) != search_fold(p) {
            return false;
        }
        k += 1;
    }
}

/// Find `pattern` in `text` ignoring case, from position `from` (clamped to the text's length):
/// forward, each position from `from` to the end; backward, each from `from` less the pattern's
/// length down to the start. A unit is folded by its low byte alone ([`search_fold`]), so `Ł`
/// (`U+0141`) matches `a`. Answers where the pattern starts, or `None`: then a forward search leaves
/// its position at the end of the text and a backward one at the start. An empty pattern is found
/// where the search stands.
// D2-07670
#[must_use]
pub fn find_ignoring_case(
    text: &[u16],
    pattern: &[u16],
    from: usize,
    backward: bool,
) -> Option<usize> {
    let pattern = cstr(pattern);
    let from = from.min(text.len());
    if pattern.is_empty() {
        return Some(from);
    }
    if backward {
        let mut at = from.checked_sub(pattern.len())?;
        loop {
            if starts_ignoring_case(text, at, pattern) {
                return Some(at);
            }
            at = at.checked_sub(1)?;
        }
    }
    (from..=text.len()).find(|&at| starts_ignoring_case(text, at, pattern))
}

/// What reading a whole number found: its value (two's complement for a signed read), where the
/// read stopped, and whether it was out of range (`errno` set to `ERANGE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scan {
    pub value: u64,
    pub end: usize,
    pub out_of_range: bool,
}

/// A digit's value: `0`-`9`, then a letter of either case from 10; any other unit is no digit.
fn digit(u: u16) -> Option<u32> {
    match u {
        0x30..=0x39 => Some(u32::from(u - 0x30)),
        0x41..=0x5A => Some(u32::from(u - 0x41) + 10),
        0x61..=0x7A => Some(u32::from(u - 0x61) + 10),
        _ => None,
    }
}

/// Read a whole number of `bits` (32 or 64) the C library's way: white space (`iswspace`), a sign,
/// then digits in `base`. Base 0 picks 16 after `0x` or `0X`, 8 after a leading `0`, else 10; base
/// 16 skips a `0x`. A base of 1 or above 36 reads nothing. `unsigned` takes `strtoul`'s rules (only
/// the magnitude overflows, to all ones; a minus sign then counts back, so `-1` is all ones), else
/// `strtol`'s (out of range clamps to the type's least or greatest). With no digit the value is 0
/// and the read stops where it began.
// D2-14312
#[must_use]
pub fn scan(text: &[u16], base: u32, bits: u32, unsigned: bool) -> Scan {
    let s = cstr(text);
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let nothing = Scan {
        value: 0,
        end: 0,
        out_of_range: false,
    };
    let mut p = 0;
    while crt::is_space(at(p)) {
        p += 1;
    }
    let negative = at(p) == u16::from(b'-');
    if negative || at(p) == u16::from(b'+') {
        p += 1;
    }
    if base == 1 || base > 36 {
        return nothing;
    }
    let hex_mark = |i: usize| matches!(at(i), 0x78 | 0x58);
    let base = match base {
        0 if at(p) != 0x30 => 10,
        0 if !hex_mark(p + 1) => 8,
        0 => 16,
        b => b,
    };
    if base == 16 && at(p) == 0x30 && hex_mark(p + 1) {
        p += 2;
    }
    let max: u64 = if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let base = u64::from(base);
    let (limit, rest) = (max / base, max % base);
    let mut number: u64 = 0;
    let mut read = false;
    let mut overflow = false;
    while let Some(d) = digit(at(p)).map(u64::from).filter(|d| *d < base) {
        read = true;
        if number < limit || (number == limit && d <= rest) {
            number = number * base + d;
        } else {
            overflow = true;
        }
        p += 1;
    }
    if !read {
        return nothing;
    }
    let half = max / 2;
    let out_of_range = overflow
        || (!unsigned
            && if negative {
                number > half + 1
            } else {
                number > half
            });
    if out_of_range {
        number = if unsigned {
            max
        } else if negative {
            half + 1
        } else {
            half
        };
    }
    if negative {
        number = number.wrapping_neg() & max;
    }
    Scan {
        value: number,
        end: p,
        out_of_range,
    }
}

/// `wcstoint64(s, &end, base)`: a signed 64-bit read ([`scan`]).
// D2-14313
#[must_use]
pub fn to_int64(text: &[u16], base: u32) -> (i64, usize) {
    let r = scan(text, base, 64, false);
    (r.value.cast_signed(), r.end)
}

/// `wcstouint64(s, &end, base)`: an unsigned 64-bit read ([`scan`]).
// D2-14314
#[must_use]
pub fn to_uint64(text: &[u16], base: u32) -> (u64, usize) {
    let r = scan(text, base, 64, true);
    (r.value, r.end)
}

/// The string's leading signed 32-bit number, in base 0 (`wcstol(s, NULL, 0)`).
// D2-02191
#[must_use]
pub fn to_int32(text: &[u16]) -> i32 {
    #[allow(clippy::cast_possible_truncation)]
    (scan(text, 0, 32, false).value as u32).cast_signed()
}

fn units(s: &str) -> impl Iterator<Item = u16> + '_ {
    s.encode_utf16()
}

/// Append a 32-bit word as `0x%08X`: `0x` and eight upper-case hexadecimal digits.
pub fn append_hex32(text: &mut Vec<u16>, v: i32) {
    text.extend(units(&format!("0x{v:08X}")));
}

/// Append a 64-bit word as `0x%016I64X`: `0x` and sixteen upper-case hexadecimal digits.
// D2-16503
pub fn append_hex64(text: &mut Vec<u16>, v: u64) {
    text.extend(units(&format!("0x{v:016X}")));
}

/// Append a float as `%f` writes it: six decimals ([`crt::fixed`]); an infinity as `1.#INF00` and
/// not-a-number as `1.#QNAN0`, each signed when negative, except the indefinite one (what `0/0`
/// makes) as `-1.#IND00`.
// D2-09255
pub fn append_float(text: &mut Vec<u16>, v: f32) {
    const INDEFINITE: u64 = 0xFFF8_0000_0000_0000;
    let x = f64::from(v);
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let s = if x.is_infinite() {
        format!("{sign}1.#INF00")
    } else if x.to_bits() == INDEFINITE {
        "-1.#IND00".into()
    } else if x.is_nan() {
        format!("{sign}1.#QNAN0")
    } else {
        crt::fixed(x, 6)
    };
    text.extend(units(&s));
}

/// The English code page's bytes 0x80 to 0x9F as units; every other byte is its own unit.
const CP1252_HIGH: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

/// A byte of the English code page as a unit.
fn widen_byte(b: u8) -> u16 {
    match b {
        0x80..=0x9F => CP1252_HIGH[usize::from(b - 0x80)],
        _ => u16::from(b),
    }
}

/// A unit as a byte of the English code page, when the code page holds it.
fn narrow_unit(u: u16) -> Option<u8> {
    match u {
        0x00..=0x7F | 0xA0..=0xFF => u8::try_from(u).ok(),
        _ => CP1252_HIGH
            .iter()
            .position(|&h| h == u)
            .and_then(|i| u8::try_from(0x80 + i).ok()),
    }
}

/// Narrow text as wide text, read in the English code page; each byte one unit.
// D2-00082
#[must_use]
pub fn widen(text: &[u8]) -> Vec<u16> {
    text.iter().map(|&b| widen_byte(b)).collect()
}

/// Wide text as narrow text in the English code page. When any unit has no byte there, the whole
/// text is written instead as its units below 128 kept and every other unit as `<%04x>` (four
/// lower-case hexadecimal digits in angle brackets), the code page's own letters included. The
/// operating system's best-fit replacements (a letter with an accent the code page lacks written
/// as the bare letter) are not made.
// D2-01922
#[must_use]
pub fn narrow(text: &[u16]) -> Vec<u8> {
    if let Some(bytes) = text
        .iter()
        .map(|&u| narrow_unit(u))
        .collect::<Option<Vec<u8>>>()
    {
        return bytes;
    }
    let mut out = Vec::with_capacity(text.len());
    for &u in text {
        match u8::try_from(u) {
            Ok(b) if b < 0x80 => out.push(b),
            _ => out.extend_from_slice(format!("<{u:04x}>").as_bytes()),
        }
    }
    out
}
