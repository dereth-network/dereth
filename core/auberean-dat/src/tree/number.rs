//! Numbers in a text tree, read and written as the client's C library does: `strtoul`, `strtol`
//! and their 64-bit forms, `strtod`, the checks that a whole name reads as a number, and the
//! `printf` forms each kind of name is written in.
//!
//! A name is narrow text (bytes in the Windows Latin-1 code page). The C functions see it up to its
//! first zero byte. White space is the English locale's: tab to carriage return, space and the
//! no-break space.

/// The bytes a C function sees: up to the first zero.
#[must_use]
pub fn cstr(s: &[u8]) -> &[u8] {
    match s.iter().position(|&b| b == 0) {
        Some(n) => &s[..n],
        None => s,
    }
}

fn is_space(b: u8) -> bool {
    matches!(b, 0x09..=0x0D | 0x20 | 0xA0)
}

fn digit_value(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some(u32::from(b - b'0')),
        b'a'..=b'z' => Some(u32::from(b - b'a') + 10),
        b'A'..=b'Z' => Some(u32::from(b - b'A') + 10),
        _ => None,
    }
}

/// What a scan of a number found: the value, where the scan stopped, and whether the number was
/// out of range (`errno` set to `ERANGE`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scan<T> {
    pub value: T,
    pub end: usize,
    pub out_of_range: bool,
}

impl<T> Scan<T> {
    /// Whether the scan read the whole text and stayed in range: the client's `is_*` checks.
    #[must_use]
    pub fn whole(&self, s: &[u8]) -> bool {
        self.end == cstr(s).len() && !self.out_of_range
    }
}

/// The integer scan of the C library (`strtol` and its kind) in base 0: white space, a sign, then
/// `0x` and hexadecimal digits, a leading `0` and octal digits, or decimal digits. `bits` is the
/// width (32 or 64); `unsigned` the `strtoul` rules (a minus sign counts back from 2^bits, only
/// the magnitude can overflow). With no digit the scan stops where it began and the value is 0.
fn scan_integer(s: &[u8], bits: u32, unsigned: bool) -> Scan<u64> {
    let s = cstr(s);
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let mut p = 0;
    while is_space(at(p)) {
        p += 1;
    }
    let mut negative = false;
    if at(p) == b'-' {
        negative = true;
        p += 1;
    } else if at(p) == b'+' {
        p += 1;
    }
    let base: u64 = if at(p) != b'0' {
        10
    } else if matches!(at(p + 1), b'x' | b'X') {
        16
    } else {
        8
    };
    if base == 16 {
        p += 2;
    }
    let max: u64 = if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let limit = max / base;
    let mut number: u64 = 0;
    let mut read = false;
    let mut overflow = false;
    while let Some(d) = digit_value(at(p)) {
        let d = u64::from(d);
        if d >= base {
            break;
        }
        read = true;
        if number < limit || (number == limit && d <= max % base) {
            number = number * base + d;
        } else {
            overflow = true;
        }
        p += 1;
    }
    if !read {
        return Scan {
            value: 0,
            end: 0,
            out_of_range: false,
        };
    }
    let half = max / 2;
    let mut out_of_range = false;
    if overflow || (!unsigned && ((negative && number > half + 1) || (!negative && number > half)))
    {
        out_of_range = true;
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

/// `strtoul(s, &end, 0)`.
#[must_use]
pub fn strtoul(s: &[u8]) -> Scan<u32> {
    let r = scan_integer(s, 32, true);
    Scan {
        value: u32::try_from(r.value).unwrap_or(u32::MAX),
        end: r.end,
        out_of_range: r.out_of_range,
    }
}

/// `strtol(s, &end, 0)`.
#[must_use]
pub fn strtol(s: &[u8]) -> Scan<i32> {
    let r = scan_integer(s, 32, false);
    Scan {
        value: u32::try_from(r.value).unwrap_or(u32::MAX).cast_signed(),
        end: r.end,
        out_of_range: r.out_of_range,
    }
}

/// `_strtoui64(s, &end, 0)`.
#[must_use]
pub fn strtoui64(s: &[u8]) -> Scan<u64> {
    scan_integer(s, 64, true)
}

/// `_strtoi64(s, &end, 0)`.
#[must_use]
pub fn strtoi64(s: &[u8]) -> Scan<i64> {
    let r = scan_integer(s, 64, false);
    Scan {
        value: r.value.cast_signed(),
        end: r.end,
        out_of_range: r.out_of_range,
    }
}

/// `strtod(s, &end)`: white space, a sign, digits with an optional point, and an exponent
/// (`e`, `E`, `d` or `D`, a sign, digits) kept only when digits follow it. No digit before or after
/// the point: nothing read, 0. Out of range when the value overflows, or underflows to zero from
/// a nonzero number.
#[must_use]
pub fn strtod(s: &[u8]) -> Scan<f64> {
    let s = cstr(s);
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let mut p = 0;
    while is_space(at(p)) {
        p += 1;
    }
    let mut text = String::new();
    if matches!(at(p), b'-' | b'+') {
        text.push(char::from(at(p)));
        p += 1;
    }
    let mut digits = 0;
    let mut nonzero = false;
    while at(p).is_ascii_digit() {
        nonzero |= at(p) != b'0';
        text.push(char::from(at(p)));
        digits += 1;
        p += 1;
    }
    if at(p) == b'.' {
        text.push('.');
        p += 1;
        while at(p).is_ascii_digit() {
            nonzero |= at(p) != b'0';
            text.push(char::from(at(p)));
            digits += 1;
            p += 1;
        }
    }
    if digits == 0 {
        return Scan {
            value: 0.0,
            end: 0,
            out_of_range: false,
        };
    }
    if matches!(at(p), b'e' | b'E' | b'd' | b'D') {
        let mut q = p + 1;
        let mut exp = String::from("e");
        if matches!(at(q), b'-' | b'+') {
            exp.push(char::from(at(q)));
            q += 1;
        }
        if at(q).is_ascii_digit() {
            while at(q).is_ascii_digit() {
                exp.push(char::from(at(q)));
                q += 1;
            }
            text.push_str(&exp);
            p = q;
        }
    }
    let value: f64 = text.parse().unwrap_or(0.0);
    let out_of_range = value.is_infinite() || (nonzero && value == 0.0);
    Scan {
        value,
        end: p,
        out_of_range,
    }
}

/// Whether a name reads whole as an unsigned 32-bit number.
// D2-04875
#[must_use]
pub fn is_uint32(s: &[u8]) -> bool {
    strtoul(s).whole(s)
}

/// Whether a name reads whole as a signed 32-bit number.
// D2-14506, D2-08762
#[must_use]
pub fn is_int32(s: &[u8]) -> bool {
    strtol(s).whole(s)
}

/// Whether a name reads whole as an unsigned 64-bit number.
// D2-14507, D2-08764
#[must_use]
pub fn is_uint64(s: &[u8]) -> bool {
    strtoui64(s).whole(s)
}

/// Whether a name reads whole as a signed 64-bit number.
// D2-14508, D2-08763
#[must_use]
pub fn is_int64(s: &[u8]) -> bool {
    strtoi64(s).whole(s)
}

/// Whether a name reads whole as a floating-point number.
// D2-14509, D2-08761
#[must_use]
pub fn is_double(s: &[u8]) -> bool {
    strtod(s).whole(s)
}

/// `%.Ng`: `precision` significant digits, in exponent form when the exponent is below -4 or not
/// below the precision; trailing zeros and a bare point removed; the exponent signed and at least
/// three digits, as the client's C library writes it. Infinities and not-a-numbers as that library
/// writes them.
#[must_use]
pub fn general(x: f64, precision: usize) -> String {
    if x.is_nan() {
        return if x.is_sign_negative() {
            "-1.#IND"
        } else {
            "1.#QNAN"
        }
        .into();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-1.#INF" } else { "1.#INF" }.into();
    }
    let p = precision.max(1);
    let e_form = format!("{:.*e}", p - 1, x);
    let (mantissa, exp) = e_form.split_once('e').unwrap_or((&e_form, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let trim = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s.to_owned()
        }
    };
    let p_i = i32::try_from(p).unwrap_or(i32::MAX);
    if exp < -4 || exp >= p_i {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:03}", trim(mantissa), exp.unsigned_abs())
    } else {
        let decimals = usize::try_from(p_i - 1 - exp).unwrap_or(0);
        trim(&format!("{x:.decimals$}"))
    }
}
