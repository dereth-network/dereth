//! A data version stamp: a 128-bit unique id naming a major version, and a minor version counted
//! up from it, written as text (`<id> : <minor>`, the id in the usual 8-4-4-4-12 hexadecimal
//! form) and packed as that text in a narrow string.

use crate::reader::Result;
use crate::{Reader, Writer};

/// A major version (a unique id) and a minor version.
// D2-17611, D2-17612, D2-13298
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DatIdStamp {
    /// The unique id: a 32-bit word, two 16-bit words and eight bytes, as the system keeps one.
    pub major: [u8; 16],
    pub minor: u32,
}

impl DatIdStamp {
    /// Back to no version: a zero id, minor 0.
    // D2-17608
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// A new major version from 16 random bytes (made a version-4 unique id, as the system makes
    /// one), the minor version back to 0.
    // D2-17609
    pub fn generate_new_major_version(&mut self, random: [u8; 16]) {
        let mut b = random;
        // The version (4) in the third group's top four bits, the variant (binary 10) in the
        // fourth group's top two.
        b[7] = (b[7] & 0x0F) | 0x40;
        b[8] = (b[8] & 0x3F) | 0x80;
        self.major = b;
        self.minor = 0;
    }

    /// The next minor version.
    // D2-17610
    pub fn generate_new_minor_version(&mut self) {
        self.minor = self.minor.wrapping_add(1);
    }

    fn data1(&self) -> u32 {
        u32::from_le_bytes([self.major[0], self.major[1], self.major[2], self.major[3]])
    }

    fn data2(&self) -> u16 {
        u16::from_le_bytes([self.major[4], self.major[5]])
    }

    fn data3(&self) -> u16 {
        u16::from_le_bytes([self.major[6], self.major[7]])
    }

    /// The stamp as text: `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx : <minor>`, the id in small
    /// hexadecimal letters.
    // D2-17614
    #[must_use]
    pub fn to_text(&self) -> String {
        let d4 = &self.major[8..];
        format!(
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x} : {}",
            self.data1(),
            self.data2(),
            self.data3(),
            d4[0],
            d4[1],
            d4[2],
            d4[3],
            d4[4],
            d4[5],
            d4[6],
            d4[7],
            self.minor
        )
    }

    /// Read a stamp from its text: a word (no white space), white space, `:`, white space and an
    /// unsigned number (kept modulo 2^32; a `-` before it counts back). The word must be a well-formed
    /// unique id, 36 characters of hexadecimal digits (either case) with dashes in their places.
    /// `false` (the stamp's minor version possibly changed) when the text is not a stamp.
    // D2-17613
    pub fn from_text(&mut self, text: &str) -> bool {
        let t = text.trim_start();
        let end = t.find(char::is_whitespace).unwrap_or(t.len());
        let (word, rest) = t.split_at(end);
        if word.is_empty() {
            return false;
        }
        let Some(rest) = rest.trim_start().strip_prefix(':') else {
            return false;
        };
        let Some(minor) = scan_unsigned(rest) else {
            return false;
        };
        self.minor = minor;
        match parse_id(word) {
            Some(id) => {
                self.major = id;
                true
            }
            None => false,
        }
    }

    /// How many bytes the stamp packs to: its text as a narrow string, aligned.
    // D2-17615, D2-17616
    #[must_use]
    pub fn pack_size(&self) -> usize {
        let n = self.to_text().len();
        (2 + if n >= 0xFFFF { 4 } else { 0 } + n).next_multiple_of(4)
    }

    /// Pack the stamp as its text.
    // D2-17617
    pub fn write(&self, w: &mut Writer) {
        w.narrow_string(&self.to_text());
        w.align();
    }

    /// Unpack a stamp; `Ok(false)` when the text is not a stamp.
    // D2-17618
    pub fn read(&mut self, r: &mut Reader<'_>) -> Result<bool> {
        let text = r.narrow_string()?;
        let ok = self.from_text(&text);
        r.align()?;
        Ok(ok)
    }
}

/// `%lu` as the C library reads it: white space, an optional sign, decimal digits (at least
/// one), the value kept modulo 2^32 as it goes; a `-` negates it.
fn scan_unsigned(s: &str) -> Option<u32> {
    let s = s.trim_start();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let n = digits.bytes().take_while(u8::is_ascii_digit).count();
    if n == 0 {
        return None;
    }
    let v = digits[..n].bytes().fold(0u32, |a, d| {
        a.wrapping_mul(10).wrapping_add(u32::from(d - b'0'))
    });
    Some(if neg { v.wrapping_neg() } else { v })
}

/// A unique id in its 36-character text form, in the byte order the system keeps it.
fn parse_id(s: &str) -> Option<[u8; 16]> {
    let b = s.as_bytes();
    if b.len() != 36 || b[8] != b'-' || b[13] != b'-' || b[18] != b'-' || b[23] != b'-' {
        return None;
    }
    let hex = |range: std::ops::Range<usize>| -> Option<u64> {
        let part = s.get(range)?;
        if !part.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        u64::from_str_radix(part, 16).ok()
    };
    let d1 = u32::try_from(hex(0..8)?).ok()?;
    let d2 = u16::try_from(hex(9..13)?).ok()?;
    let d3 = u16::try_from(hex(14..18)?).ok()?;
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&d1.to_le_bytes());
    out[4..6].copy_from_slice(&d2.to_le_bytes());
    out[6..8].copy_from_slice(&d3.to_le_bytes());
    for (i, at) in [19, 21, 24, 26, 28, 30, 32, 34].into_iter().enumerate() {
        out[8 + i] = u8::try_from(hex(at..at + 2)?).ok()?;
    }
    Some(out)
}
