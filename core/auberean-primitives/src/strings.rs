//! The client's string library where it does more than Rust's `String` and `Vec`: a narrow
//! string's hash and its hash ignoring case, comparing and lowering narrow text in the client's
//! locale, trimming, finding a part ignoring case, replacing every occurrence, and the size a
//! string takes when packed.
//!
//! Narrow text is bytes in the English Windows code page (1252), the locale the client sets at
//! startup ("English"). A C function given narrow text sees it up to its first zero byte. Building,
//! copying, joining, slicing and plain (case-kept) equality are Rust's own.

use alloc::vec::Vec;

/// The bytes a C function sees: up to the first zero.
#[must_use]
pub fn cstr(s: &[u8]) -> &[u8] {
    match s.iter().position(|&b| b == 0) {
        Some(n) => &s[..n],
        None => s,
    }
}

/// A byte lowered as the English locale lowers it: `A`-`Z`, `À`-`Þ` but `×`, and `Š` `Œ` `Ž` `Ÿ`
/// (the code page's other capitals); every other byte unchanged.
#[must_use]
pub fn lower(b: u8) -> u8 {
    match b {
        b'A'..=b'Z' | 0xC0..=0xD6 | 0xD8..=0xDE => b + 0x20,
        0x8A | 0x8C | 0x8E => b + 0x10,
        0x9F => 0xFF,
        _ => b,
    }
}

/// Add one character to a string hash: the hash shifted left four bits plus the character, its
/// top four bits folded into the low byte's high nibble and cleared.
fn mix(h: u32, c: i32) -> u32 {
    #[allow(clippy::cast_sign_loss)]
    let mut h = (h << 4).wrapping_add(c as u32);
    let top = h & 0xF000_0000;
    if top != 0 {
        h = ((top >> 24) ^ h) & 0x0FFF_FFFF;
    }
    h
}

/// A narrow string's hash: each byte, taken as a signed character, mixed in; all ones becomes all
/// ones but the lowest bit.
// D2-00718
#[must_use]
pub fn hash(s: &[u8]) -> u32 {
    #[allow(clippy::cast_possible_wrap)]
    let h = cstr(s).iter().fold(0, |h, &b| mix(h, i32::from(b as i8)));
    if h == u32::MAX {
        0xFFFF_FFFE
    } else {
        h
    }
}

/// The bucket of a narrow key among `buckets`: its hash modulo the count (`buckets` is never 0).
// D2-04878
#[must_use]
pub fn bucket(s: &[u8], buckets: u32) -> u32 {
    hash(s) % buckets.max(1)
}

/// A narrow string's hash ignoring case: each byte lowered ([`lower`]), taken as a signed
/// character, mixed in. Unlike [`hash`], all ones stays all ones.
// D2-12668
#[must_use]
pub fn hash_ignoring_case(s: &[u8]) -> u32 {
    #[allow(clippy::cast_possible_wrap)]
    cstr(s)
        .iter()
        .fold(0, |h, &b| mix(h, i32::from(lower(b) as i8)))
}

/// Compare two narrow strings as C strings. With case kept: byte by byte as unsigned, the answer
/// -1, 0 or 1. Ignoring case: each byte lowered ([`lower`]), the answer the difference of the
/// first two lowered bytes that differ (0 when none does).
// D2-01661
#[must_use]
pub fn compare(a: &[u8], b: &[u8], keep_case: bool) -> i32 {
    let (a, b) = (cstr(a), cstr(b));
    let mut k = 0;
    loop {
        let x = a.get(k).copied().unwrap_or(0);
        let y = b.get(k).copied().unwrap_or(0);
        let (x, y) = if keep_case {
            (x, y)
        } else {
            (lower(x), lower(y))
        };
        if x != y || x == 0 {
            let d = i32::from(x) - i32::from(y);
            return if keep_case { d.signum() } else { d };
        }
        k += 1;
    }
}

/// Whether two narrow strings are equal as C strings, ignoring case unless asked to keep it.
// D2-01667
#[must_use]
pub fn equal(a: &[u8], b: &[u8], keep_case: bool) -> bool {
    compare(a, b, keep_case) == 0
}

/// Lower a narrow string in place ([`lower`]), up to its first zero.
// D2-00075
pub fn to_lower(s: &mut [u8]) {
    for b in s.iter_mut().take_while(|b| **b != 0) {
        *b = lower(*b);
    }
}

/// The characters a narrow string is trimmed of unless the caller names others.
pub const WHITESPACE: &[u8] = b" \n\r\t\x0C";

/// Trim a narrow string of the bytes of `set`, from the left, the right, or both. From the left it
/// stops at a zero byte; from the right a zero byte counts as one of the set (`strchr` finds the
/// set's own end).
// D2-00105
pub fn trim(s: &mut Vec<u8>, left: bool, right: bool, set: &[u8]) {
    let set = cstr(set);
    let mut start = 0;
    if left {
        while start < s.len() && s[start] != 0 && set.contains(&s[start]) {
            start += 1;
        }
    }
    let mut end = s.len();
    if right {
        while end > start && (s[end - 1] == 0 || set.contains(&s[end - 1])) {
            end -= 1;
        }
    }
    s.truncate(end);
    s.drain(..start);
}

/// Whether `pattern` starts `text` at `at`, both lowered ([`lower`]); the text ends at its first
/// zero, and an empty pattern always matches.
// D2-11839
fn starts_ignoring_case(text: &[u8], at: usize, pattern: &[u8]) -> bool {
    let mut k = 0;
    loop {
        let p = pattern.get(k).copied().unwrap_or(0);
        if p == 0 {
            return true;
        }
        let t = text.get(at + k).copied().unwrap_or(0);
        if t == 0 || lower(t) != lower(p) {
            return false;
        }
        k += 1;
    }
}

/// Find `pattern` in `text` ignoring case ([`lower`]), from position `from` (clamped to the text's
/// length): forward, each position from `from` to the end; backward, each from `from` less the
/// pattern's length down to the start. Answers where it starts, or `None`: then a forward search
/// leaves its position at the end of the text and a backward one at the start. An empty pattern is
/// found where the search stands.
// D2-11843
#[must_use]
pub fn find_ignoring_case(
    text: &[u8],
    pattern: &[u8],
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

/// Replace every occurrence of `find` in `text` with `with`, searching on after each replaced
/// occurrence (case kept, as C strings). Answers how many; `None` for an empty `find`, which never
/// ends in the client.
// D2-01702
pub fn replace(text: &mut Vec<u8>, find: &[u8], with: &[u8]) -> Option<usize> {
    let (find, with) = (cstr(find), cstr(with));
    if find.is_empty() {
        return None;
    }
    let source = cstr(text).to_vec();
    let mut out = Vec::with_capacity(source.len());
    let mut count = 0;
    let mut i = 0;
    while i < source.len() {
        if source[i..].starts_with(find) {
            out.extend_from_slice(with);
            i += find.len();
            count += 1;
        } else {
            out.push(source[i]);
            i += 1;
        }
    }
    if count > 0 {
        *text = out;
    }
    Some(count)
}

/// The bytes a string of `units` characters of `unit_bytes` each takes packed: a 16-bit length
/// (followed by a 32-bit one when the length is 0xFFFF or more), the characters, then padding to
/// four bytes.
// D2-00241, D2-00374
#[must_use]
pub fn packed_size(units: usize, unit_bytes: usize) -> usize {
    let head = if units > 0xFFFE { 6 } else { 2 };
    (head + units * unit_bytes).next_multiple_of(4)
}
