//! The C library the client's text code leans on, as it behaves in the client: wide strings that
//! end at their first zero for the C functions, character classes, reading a number, a power, the
//! conversion of a float to a 64-bit integer, and the few formats the text code writes.
//!
//! Strings are UTF-16 units. A C function given a string sees it up to its first zero unit.
//!
//! The character classes are the C library's for the English locale the client sets at startup:
//! for units below 256 its own table (a space is tab to carriage return, space and the no-break
//! space; Latin-1 punctuation is the whole of `U+00A1`-`U+00BF` and `×` `÷`; the superscript digits
//! count as digits), above it the operating system's classification, taken here as Unicode's.

/// The units of a wide string up to its first zero, as a C function sees it.
#[must_use]
pub fn cstr(s: &[u16]) -> &[u16] {
    match s.iter().position(|&u| u == 0) {
        Some(n) => &s[..n],
        None => s,
    }
}

/// Whether `wcschr(s, c)` finds `c`: a unit before the first zero, or the zero itself.
#[must_use]
pub fn wcschr(s: &[u16], c: u16) -> bool {
    c == 0 || cstr(s).contains(&c)
}

/// The first `target` that the unit before it does not escape: a unit is escaped when the one
/// before it is `escape` (and an escape escapes even another escape's escape).
// D2-17777, D2-17775
#[must_use]
pub fn find_unescaped(s: &[u16], target: u16, escape: u16) -> Option<usize> {
    let mut escaped = false;
    for (i, &c) in cstr(s).iter().enumerate() {
        if c == target && !escaped {
            return Some(i);
        }
        escaped = c == escape;
    }
    None
}

/// The white space the string library trims: space, line feed, carriage return, tab, form feed.
pub const WHITESPACE: [u16; 5] = [0x20, 0x0A, 0x0D, 0x09, 0x0C];

/// Trim the units of [`WHITESPACE`] from the left, the right, or both. From the left it stops at
/// a zero unit; from the right a zero unit counts as white space too (`wcschr` finds the end).
// D2-02273
pub fn trim(s: &mut Vec<u16>, left: bool, right: bool) {
    let mut start = 0;
    if left {
        while start < s.len() && s[start] != 0 && WHITESPACE.contains(&s[start]) {
            start += 1;
        }
    }
    let mut end = s.len();
    if right {
        while end > start && (s[end - 1] == 0 || WHITESPACE.contains(&s[end - 1])) {
            end -= 1;
        }
    }
    if start != 0 || end != s.len() {
        *s = s[start..end].to_vec();
    }
}

fn latin1(c: u16) -> Option<u8> {
    u8::try_from(c).ok()
}

fn unicode(c: u16) -> Option<char> {
    char::from_u32(u32::from(c))
}

/// `iswspace`.
#[must_use]
pub fn is_space(c: u16) -> bool {
    match latin1(c) {
        Some(b) => matches!(b, 0x09..=0x0D | 0x20 | 0xA0),
        None => unicode(c).is_some_and(char::is_whitespace),
    }
}

/// `iswpunct`.
#[must_use]
pub fn is_punct(c: u16) -> bool {
    match latin1(c) {
        Some(b) => b.is_ascii_punctuation() || matches!(b, 0xA1..=0xBF | 0xD7 | 0xF7),
        None => unicode(c)
            .is_some_and(|ch| !ch.is_alphanumeric() && !ch.is_whitespace() && !ch.is_control()),
    }
}

/// `iswdigit`.
#[must_use]
pub fn is_digit(c: u16) -> bool {
    match latin1(c) {
        Some(b) => b.is_ascii_digit() || matches!(b, 0xB2 | 0xB3 | 0xB9),
        None => unicode(c).is_some_and(char::is_numeric),
    }
}

/// `iswlower` below 256.
fn is_lower_latin1(b: u8) -> bool {
    b.is_ascii_lowercase() || matches!(b, 0xDF..=0xF6 | 0xF8..=0xFF)
}

/// `iswupper` below 256.
fn is_upper_latin1(b: u8) -> bool {
    b.is_ascii_uppercase() || matches!(b, 0xC0..=0xD6 | 0xD8..=0xDE)
}

/// One unit's case mapped by the operating system: a single unit, else the unit unchanged.
fn mapped(c: u16, f: impl Fn(char) -> String) -> u16 {
    let Some(ch) = unicode(c) else {
        return c;
    };
    let m = f(ch);
    let mut it = m.chars();
    match (it.next(), it.next()) {
        (Some(one), None) => u16::try_from(u32::from(one)).unwrap_or(c),
        _ => c,
    }
}

/// `towupper` in the English locale: below 256 only a lower-case letter changes (`ß` stays,
/// `ÿ` becomes `Ÿ`); above, the operating system's mapping when it is one unit.
#[must_use]
pub fn to_upper(c: u16) -> u16 {
    if let Some(b) = latin1(c) {
        if !is_lower_latin1(b) {
            return c;
        }
    }
    mapped(c, |ch| ch.to_uppercase().collect())
}

/// `towlower` in the English locale, as [`to_upper`] the other way.
#[must_use]
pub fn to_lower(c: u16) -> u16 {
    if let Some(b) = latin1(c) {
        if !is_upper_latin1(b) {
            return c;
        }
    }
    mapped(c, |ch| ch.to_lowercase().collect())
}

/// `wcstol(s, &end, 10)`: white space, a sign, then decimal digits, the value clamped to the
/// 32-bit range when it overflows. Answers the value and the index `end` points at: just past the
/// last digit, or 0 (the start) when there was no digit.
#[must_use]
pub fn wcstol(s: &[u16]) -> (i32, usize) {
    let s = cstr(s);
    let mut i = 0;
    while i < s.len() && is_space(s[i]) {
        i += 1;
    }
    let mut negative = false;
    match s.get(i) {
        Some(&0x2D) => {
            negative = true;
            i += 1;
        }
        Some(&0x2B) => i += 1,
        _ => {}
    }
    let mut n: u32 = 0;
    let mut overflow = false;
    let mut digits = 0;
    while let Some(&c) = s.get(i) {
        if !(0x30..=0x39).contains(&c) {
            break;
        }
        let d = u32::from(c - 0x30);
        match n.checked_mul(10).and_then(|v| v.checked_add(d)) {
            Some(v) => n = v,
            None => overflow = true,
        }
        digits += 1;
        i += 1;
    }
    if digits == 0 {
        return (0, 0);
    }
    let limit = if negative { 0x8000_0000 } else { 0x7FFF_FFFF };
    let v = if overflow || n > limit {
        if negative {
            i32::MIN
        } else {
            i32::MAX
        }
    } else if negative {
        0i32.wrapping_sub_unsigned(n)
    } else {
        i32::try_from(n).unwrap_or(i32::MAX)
    };
    (v, i)
}

/// A signed number in decimal, as the string library appends one.
#[must_use]
pub fn decimal(v: i64) -> Vec<u16> {
    v.to_string().encode_utf16().collect()
}

/// Upper-case hexadecimal, at least eight digits (`%08X`).
#[must_use]
pub fn hex8(v: u32) -> Vec<u16> {
    format!("{v:08X}").encode_utf16().collect()
}

/// One float as the C library's `%f` reads it from `s` at `*at`: after any white space, a sign,
/// digits with a point among them, and an exponent if a whole one follows; `None` (`*at` where the
/// white space ended) when there are no digits.
pub fn scan_float(s: &[u16], at: &mut usize) -> Option<f32> {
    let digit = |i: usize| s.get(i).is_some_and(|c| is_digit(*c));
    let is = |i: usize, c: u8| s.get(i) == Some(&u16::from(c));
    while s.get(*at).is_some_and(|c| is_space(*c)) {
        *at += 1;
    }
    let start = *at;
    let mut i = start;
    if is(i, b'+') || is(i, b'-') {
        i += 1;
    }
    let mut digits = 0;
    while digit(i) {
        i += 1;
        digits += 1;
    }
    if is(i, b'.') {
        i += 1;
        while digit(i) {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if is(i, b'e') || is(i, b'E') {
        let mut e = i + 1;
        if is(e, b'+') || is(e, b'-') {
            e += 1;
        }
        if digit(e) {
            while digit(e) {
                e += 1;
            }
            i = e;
        }
    }
    let text = String::from_utf16_lossy(&s[start..i]);
    *at = i;
    text.parse().ok()
}

/// A finite number as `%.*f` writes it with `decimals` decimals: the number's first seventeen
/// significant digits, then rounded at the last decimal kept, a following digit of 5 or more
/// rounding up (so `6.25` writes `6.3`, and `0.05`, whose seventeen digits are `5000...3`, `0.1`).
/// A number below zero leads with `-`, even when it rounds to zero (negative zero does not); no
/// decimals, no point.
#[must_use]
pub fn fixed(x: f64, decimals: usize) -> String {
    let sign = if x < 0.0 { "-" } else { "" };
    let a = x.abs();
    let decimals_i = i32::try_from(decimals).unwrap_or(i32::MAX);
    // The kept digits, as a whole number of units of the last decimal.
    let mut kept: Vec<u8> = Vec::new();
    if a != 0.0 {
        let e17 = format!("{a:.16e}");
        let (mantissa, exponent) = e17.split_once('e').unwrap_or((&e17, "0"));
        let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
        let point = exponent.parse::<i32>().unwrap_or(0) + 1;
        let keep = point.saturating_add(decimals_i);
        if keep >= 0 {
            let keep = usize::try_from(keep).unwrap_or(0);
            kept = (0..keep)
                .map(|i| digits.get(i).copied().unwrap_or(b'0'))
                .collect();
            if digits.get(keep).is_some_and(|d| *d >= b'5') {
                let mut i = kept.len();
                loop {
                    if i == 0 {
                        kept.insert(0, b'1');
                        break;
                    }
                    i -= 1;
                    if kept[i] == b'9' {
                        kept[i] = b'0';
                    } else {
                        kept[i] += 1;
                        break;
                    }
                }
            }
        }
    }
    if kept.len() <= decimals {
        let mut padded = vec![b'0'; decimals + 1 - kept.len()];
        padded.extend_from_slice(&kept);
        kept = padded;
    }
    let (whole, part) = kept.split_at(kept.len() - decimals);
    let whole = String::from_utf8_lossy(whole);
    if decimals == 0 {
        format!("{sign}{whole}")
    } else {
        format!("{sign}{whole}.{}", String::from_utf8_lossy(part))
    }
}

/// A format with `%s` places filled in order with `args` (`%%` writes one `%`); the client formats
/// a number's digits into the language's negative pattern this way.
#[must_use]
pub fn format_s(fmt: &[u16], args: &[&[u16]]) -> Vec<u16> {
    let fmt = cstr(fmt);
    let mut out = Vec::with_capacity(fmt.len() + 16);
    let mut next = 0;
    let mut i = 0;
    while i < fmt.len() {
        if fmt[i] == 0x25 {
            match fmt.get(i + 1) {
                Some(&0x73) => {
                    if let Some(a) = args.get(next) {
                        out.extend_from_slice(cstr(a));
                    }
                    next += 1;
                    i += 2;
                    continue;
                }
                Some(&0x25) => {
                    out.push(0x25);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(fmt[i]);
        i += 1;
    }
    out
}

/// `_ftol2`: a float cut toward zero to a 64-bit integer; a value out of its range, or not a
/// number, gives `0x8000_0000_0000_0000`.
// D2-17863
#[must_use]
pub fn ftol(x: f64) -> u64 {
    // The 64-bit range is [-2^63, 2^63).
    if x.is_nan() || x >= 9_223_372_036_854_775_808.0 || x < -9_223_372_036_854_775_808.0 {
        return 0x8000_0000_0000_0000;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let v = x.trunc() as i64 as u64;
    v
}

/// `base` raised to the whole power `e` as the C library's power gives it for a whole exponent:
/// rounded once to the nearest double, an exact tie toward zero (`7^19` is one).
#[must_use]
pub fn power(base: u32, e: i32) -> f64 {
    if base == 0 {
        return if e == 0 {
            1.0
        } else if e > 0 {
            0.0
        } else {
            f64::INFINITY
        };
    }
    let mut n = Big::one();
    for _ in 0..e.unsigned_abs() {
        n.mul_small(base);
    }
    if e >= 0 {
        n.to_f64()
    } else {
        n.reciprocal()
    }
}

/// An unsigned integer of any size, little-endian 32-bit words: enough to round a power exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Big(Vec<u32>);

impl Big {
    fn one() -> Self {
        Self(vec![1])
    }

    fn mul_small(&mut self, m: u32) {
        let mut carry = 0u64;
        for w in &mut self.0 {
            let v = u64::from(*w) * u64::from(m) + carry;
            #[allow(clippy::cast_possible_truncation)]
            {
                *w = v as u32;
            }
            carry = v >> 32;
        }
        if carry != 0 {
            #[allow(clippy::cast_possible_truncation)]
            self.0.push(carry as u32);
        }
        while self.0.len() > 1 && self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    fn bits(&self) -> u32 {
        let top = self.0.last().copied().unwrap_or(0);
        #[allow(clippy::cast_possible_truncation)]
        let words = (self.0.len() - 1) as u32;
        words * 32 + (32 - top.leading_zeros())
    }

    fn bit(&self, i: u32) -> bool {
        self.0
            .get((i / 32) as usize)
            .is_some_and(|w| (w >> (i % 32)) & 1 == 1)
    }

    fn is_zero(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }

    fn shl1(&mut self) {
        let mut carry = 0;
        for w in &mut self.0 {
            let next = *w >> 31;
            *w = (*w << 1) | carry;
            carry = next;
        }
        if carry != 0 {
            self.0.push(carry);
        }
    }

    fn ge(&self, o: &Self) -> bool {
        let n = self.0.len().max(o.0.len());
        for i in (0..n).rev() {
            let a = self.0.get(i).copied().unwrap_or(0);
            let b = o.0.get(i).copied().unwrap_or(0);
            if a != b {
                return a > b;
            }
        }
        true
    }

    fn sub(&mut self, o: &Self) {
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let v = i64::from(self.0[i]) - i64::from(o.0.get(i).copied().unwrap_or(0)) - borrow;
            borrow = i64::from(v < 0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                self.0[i] = (v + (borrow << 32)) as u32;
            }
        }
        while self.0.len() > 1 && self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    /// The nearest double, ties to even.
    fn to_f64(&self) -> f64 {
        let bits = self.bits();
        if bits <= 53 {
            let mut v = 0u64;
            for i in (0..bits).rev() {
                v = (v << 1) | u64::from(self.bit(i));
            }
            #[allow(clippy::cast_precision_loss)]
            return v as f64;
        }
        // The top 53 bits, the next (round) bit, and whether any below it is set.
        let mut m = 0u64;
        for i in (bits - 53..bits).rev() {
            m = (m << 1) | u64::from(self.bit(i));
        }
        let round = self.bit(bits - 54);
        let sticky = (0..bits - 54).any(|i| self.bit(i));
        // An exact tie stays down.
        round_up(
            m,
            round && sticky,
            sticky,
            i32::try_from(bits - 53).unwrap_or(i32::MAX),
        )
    }

    /// The nearest double to one over this, ties to even (by long division, one bit at a time).
    fn reciprocal(&self) -> f64 {
        let mut r = Self::one();
        // Shift until the first quotient bit: 2^k / n with 2^k >= n.
        let mut k = 0i32;
        while !r.ge(self) {
            r.shl1();
            k += 1;
        }
        let mut m = 0u64;
        let mut taken = 0;
        while taken < 55 {
            let bit = r.ge(self);
            if bit {
                r.sub(self);
            }
            m = (m << 1) | u64::from(bit);
            taken += 1;
            r.shl1();
        }
        // m holds 55 bits of the quotient, its first bit set: 53 kept, the round bit, and one more
        // that joins the remainder as the sticky part.
        let round = (m >> 1) & 1 == 1;
        let sticky = m & 1 == 1 || !r.is_zero();
        round_up(m >> 2, round, sticky, -k - 52)
    }
}

/// `m * 2^e` with `m` a 53-bit mantissa, rounded up once when the dropped part is above half, or
/// exactly half and `m` is odd.
fn round_up(m: u64, round: bool, sticky: bool, e: i32) -> f64 {
    let up = round && (sticky || m & 1 == 1);
    let m = m + u64::from(up);
    #[allow(clippy::cast_precision_loss)]
    let v = m as f64;
    v * 2f64.powi(e)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the C library's pieces, held to known values)
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn an_exact_tie_rounds_toward_zero() {
        // 7^19 = 11398895185373143 lies halfway between two doubles.
        assert_eq!(power(7, 19), 11_398_895_185_373_142.0);
        assert_eq!(power(10, 23).to_bits(), 1e23f64.to_bits());
    }

    #[test]
    fn powers_are_the_nearest_double() {
        for e in -30..=30 {
            let want: f64 = format!("1e{e}").parse().expect("a number");
            assert_eq!(power(10, e).to_bits(), want.to_bits(), "10^{e}");
        }
        assert_eq!(power(2, -60), 2f64.powi(-60));
        assert_eq!(power(16, 3), 4096.0);
        assert_eq!(power(3, -1).to_bits(), (1.0f64 / 3.0).to_bits());
    }

    #[test]
    fn a_number_is_read_as_the_c_library_reads_it() {
        assert_eq!(wcstol(&w("  -42x")), (-42, 5));
        assert_eq!(wcstol(&w("")), (0, 0));
        assert_eq!(wcstol(&w("  ")), (0, 0));
        assert_eq!(wcstol(&w("99999999999")), (i32::MAX, 11));
        assert_eq!(wcstol(&w("-2147483648")), (i32::MIN, 11));
        assert_eq!(wcstol(&w("+7")), (7, 2));
    }

    #[test]
    fn a_float_out_of_range_cuts_to_the_indefinite_integer() {
        assert_eq!(ftol(2.5), 2);
        assert_eq!(ftol(-2.5), (-2i64) as u64);
        assert_eq!(ftol(1.8e19), 0x8000_0000_0000_0000);
        assert_eq!(ftol(f64::NAN), 0x8000_0000_0000_0000);
    }
}
