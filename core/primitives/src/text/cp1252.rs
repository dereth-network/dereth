//! Windows-1252, the code page the client's narrow (`char`) strings are in: the one table, decode
//! and encode.
//!
//! The wire and the dat files carry bytes, not Unicode: a packed string copies raw bytes and the
//! client renders them through the ANSI code page, which on a western install is 1252. The table is
//! Windows' own `MultiByteToWideChar(1252, 0, ...)` over all 256 bytes (it is also the WHATWG
//! windows-1252 index): a **bijection** between the 256 byte values and 256 code points, the five
//! bytes Microsoft leaves undefined (0x81, 0x8D, 0x8F, 0x90, 0x9D) mapping to the matching C1
//! control. That bijectivity is what lets [`decode`] then [`encode`] reproduce the input byte for
//! byte.
//!
//! Encoding a character with no 1252 byte has three answers here, because the callers disagree and
//! each is observable where it is used:
//!
//! - [`encode`] refuses the whole string (`None`) -- the checked form;
//! - [`encode_lossy`] writes one `?` per UTF-16 unit, so a character outside the Basic Multilingual
//!   Plane becomes `??` -- `WideCharToMultiByte` with a `?` default, less its best fit;
//! - [`encode_lossy_chars`] writes one `?` per character, so the same character becomes a single
//!   `?` -- the server's spell-formula hash, a port of .NET's 1252 encoder.
//!
//! The narrowing conversions ([`narrow`], [`ws_field`] and [`Cp1252`]) are Windows' own
//! `WideCharToMultiByte(1252, 0, ...)`, **best fit included**: a unit with no exact byte but a
//! best-fit one (U+0101 -> `a`, fullwidth `＜` -> `<`, U+221E -> `8`) narrows to that byte with
//! `usedDefault` clear. The best-fit data is Microsoft's published table for the code page
//! (`cp1252_best_fit.rs`), and on a Windows host a test holds every BMP unit, surrogate pairs and
//! mixed strings to the operating system's answer. The checked and lossy encoders above do not
//! best-fit: they are other callers' rules, not the host conversion.

use super::cp1252_best_fit::{BEST_FIT, BEST_FIT_SINCE_PUBLISHED};
use crate::host::HostEncoding;

/// The one ANSI code page this table is. `CP_ACP` (`0`) resolves to it on a host with no ACP.
pub const ACP: u32 = 1252;

/// `0x80..=0x9F`, the 32 code points where 1252 differs from Latin-1; every other byte is itself.
const HIGH: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, // 0x80..=0x87
    0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F, // 0x88..=0x8F
    0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, // 0x90..=0x97
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178, // 0x98..=0x9F
];

/// One byte -> one UTF-16 unit. Every byte maps, so flags-0 widening never fails.
#[must_use]
pub fn byte_to_unit(byte: u8) -> u16 {
    match byte {
        0x80..=0x9F => HIGH[usize::from(byte - 0x80)],
        _ => u16::from(byte),
    }
}

/// The exact 1252 byte for one UTF-16 unit, or `None` where Windows would substitute or best-fit.
/// Surrogates never map: Windows narrows each unit of a pair to its own default character.
#[must_use]
pub fn unit_to_byte(unit: u16) -> Option<u8> {
    match unit {
        0x0000..=0x007F | 0x00A0..=0x00FF => {
            #[allow(clippy::cast_possible_truncation)] // guarded by the range arms
            Some(unit as u8)
        }
        _ => HIGH.iter().position(|&mapped| mapped == unit).map(|index| {
            #[allow(clippy::cast_possible_truncation)] // index < 32
            let index = index as u8;
            0x80 + index
        }),
    }
}

/// The exact 1252 byte for one character, or `None`. A character outside the Basic Multilingual
/// Plane has none.
#[must_use]
pub fn char_to_byte(c: char) -> Option<u8> {
    u16::try_from(u32::from(c)).ok().and_then(unit_to_byte)
}

/// One byte -> its character. Every byte decodes.
#[must_use]
pub fn byte_to_char(byte: u8) -> char {
    // Every table entry and every Latin-1 byte is a BMP scalar value, never a surrogate.
    char::from_u32(u32::from(byte_to_unit(byte))).unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// Bytes -> `String`. Total: every byte sequence decodes, one character per byte.
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| byte_to_char(b)).collect()
}

/// `str` -> bytes, or `None` at the first character with no windows-1252 byte.
#[must_use]
pub fn encode(s: &str) -> Option<Vec<u8>> {
    s.chars().map(char_to_byte).collect()
}

/// `str` -> bytes, never failing: each UTF-16 unit with no windows-1252 byte becomes `?` (so a
/// character outside the Basic Multilingual Plane, two units, becomes `??`). This is the
/// substitution a writer that must not fail applies; [`encode`] is the checked form. It does not
/// best-fit, unlike [`narrow`].
#[must_use]
pub fn encode_lossy(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .map(|u| unit_to_byte(u).unwrap_or(b'?'))
        .collect()
}

/// `str` -> bytes, never failing, one `?` per **character** with no windows-1252 byte (a character
/// outside the Basic Multilingual Plane is one `?`, not two). The server's spell-formula hash
/// encodes with it, as its port of .NET's `Encoding.GetEncoding(1252).GetBytes` always has.
#[must_use]
pub fn encode_lossy_chars(s: &str) -> Vec<u8> {
    s.chars().map(|c| char_to_byte(c).unwrap_or(b'?')).collect()
}

/// The byte Windows narrows one UTF-16 unit to in code page 1252: the exact byte, else the
/// best-fit byte, else `None` (a surrogate, half of a pair or not, never has one).
#[must_use]
pub fn best_fit_byte(unit: u16) -> Option<u8> {
    unit_to_byte(unit).or_else(|| {
        [&BEST_FIT[..], &BEST_FIT_SINCE_PUBLISHED[..]]
            .into_iter()
            .find_map(|table| {
                table
                    .binary_search_by_key(&unit, |&(from, _)| from)
                    .ok()
                    .map(|index| table[index].1)
            })
    })
}

/// One unit as Windows narrows it: its byte, and whether `usedDefault` is raised. The flag is
/// raised for a unit with no byte (which becomes the `?` default) **and** for one whose best fit is
/// `?` itself (U+FF1F): Windows reports a `?` that was not U+003F as a substitution.
fn narrow_unit(unit: u16) -> (u8, bool) {
    match best_fit_byte(unit) {
        Some(byte) => (byte, byte == b'?' && unit != u16::from(b'?')),
        None => (b'?', true),
    }
}

/// `WideCharToMultiByte(1252, 0, units, ..., "?", &usedDefault)`: one byte per unit, best fit
/// applied, `?` for every unit without a byte (each unit of a surrogate pair, paired or not, is its
/// own `?`), and whether `usedDefault` was raised.
#[must_use]
pub fn narrow(units: &[u16]) -> (Vec<u8>, bool) {
    let mut used = false;
    let bytes = units
        .iter()
        .map(|&unit| {
            let (byte, substituted) = narrow_unit(unit);
            used |= substituted;
            byte
        })
        .collect();
    (bytes, used)
}

/// `MultiByteToWideChar(1252, 0, bytes, ...)`: explicit count, interior NULs preserved.
#[must_use]
pub fn widen(bytes: &[u8]) -> Vec<u16> {
    bytes.iter().map(|&byte| byte_to_unit(byte)).collect()
}

/// The narrowing conversions' whole-string fallback, taken once any unit had to be substituted:
/// every unit below 0x80 is its byte and every other unit becomes the literal `<%04x>`.
#[must_use]
pub fn escape_non_ascii(units: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for &unit in units {
        if unit < 128 {
            #[allow(clippy::cast_possible_truncation)] // guarded by unit < 128
            bytes.push(unit as u8);
        } else {
            bytes.extend(format!("<{unit:04x}>").bytes());
        }
    }
    bytes
}

/// `wctomb` in code page 1252, one `%ws` field: each unit narrows as [`narrow`] narrows it (best
/// fit included), and the first unit that raises `usedDefault` ends the field, as does the
/// terminator.
#[must_use]
pub fn ws_field(units: &[u16]) -> Vec<u8> {
    units
        .iter()
        .take_while(|&&unit| unit != 0)
        .map_while(|&unit| match narrow_unit(unit) {
            (byte, false) => Some(byte),
            (_, true) => None,
        })
        .collect()
}

/// Windows-1252 as a [`HostEncoding`]: the implementation a host with no NLS conversion uses, and
/// the default a game model carries when the application has installed nothing. Its answers are
/// the table arm of [`crate::text`]'s chat conversions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cp1252;

impl HostEncoding for Cp1252 {
    /// The narrowing conversion over the table: one byte per unit, best fit applied, and the
    /// **whole-string**
    /// `<%04x>` fallback as soon as any unit raised `usedDefault` -- not a per-character
    /// replacement.
    fn narrow(&self, utf16: &[u16]) -> Option<Vec<u8>> {
        if utf16.len() >= i32::MAX as usize {
            return None;
        }
        match narrow(utf16) {
            (bytes, false) => Some(bytes),
            (_, true) => Some(escape_non_ascii(utf16)),
        }
    }

    /// The widening conversion over the table: explicit count, interior NULs preserved, and every
    /// byte maps, so flags-0 widening never fails.
    fn widen(&self, bytes: &[u8]) -> Option<Vec<u16>> {
        if bytes.len() >= i32::MAX as usize {
            return None;
        }
        Some(widen(bytes))
    }

    fn acp(&self) -> u32 {
        ACP
    }

    /// `wctomb` over the table: best fit applied, and the first unit that raises `usedDefault`
    /// ends the field.
    fn english_ws_field(&self, utf16: &[u16]) -> Option<Vec<u8>> {
        Some(ws_field(utf16))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 32 code points where 1252 is not Latin-1, byte by byte, as Windows' table gives them.
    #[test]
    fn the_thirty_two_windows_specific_code_points() {
        let want: [(u8, char); 32] = [
            (0x80, '\u{20AC}'),
            (0x81, '\u{0081}'),
            (0x82, '\u{201A}'),
            (0x83, '\u{0192}'),
            (0x84, '\u{201E}'),
            (0x85, '\u{2026}'),
            (0x86, '\u{2020}'),
            (0x87, '\u{2021}'),
            (0x88, '\u{02C6}'),
            (0x89, '\u{2030}'),
            (0x8A, '\u{0160}'),
            (0x8B, '\u{2039}'),
            (0x8C, '\u{0152}'),
            (0x8D, '\u{008D}'),
            (0x8E, '\u{017D}'),
            (0x8F, '\u{008F}'),
            (0x90, '\u{0090}'),
            (0x91, '\u{2018}'),
            (0x92, '\u{2019}'),
            (0x93, '\u{201C}'),
            (0x94, '\u{201D}'),
            (0x95, '\u{2022}'),
            (0x96, '\u{2013}'),
            (0x97, '\u{2014}'),
            (0x98, '\u{02DC}'),
            (0x99, '\u{2122}'),
            (0x9A, '\u{0161}'),
            (0x9B, '\u{203A}'),
            (0x9C, '\u{0153}'),
            (0x9D, '\u{009D}'),
            (0x9E, '\u{017E}'),
            (0x9F, '\u{0178}'),
        ];
        for (byte, c) in want {
            assert_eq!(byte_to_char(byte), c, "byte {byte:02x}");
            assert_eq!(char_to_byte(c), Some(byte), "U+{:04X}", u32::from(c));
            assert_eq!(decode(&[byte]), c.to_string());
        }
        assert_eq!(decode(&[0x41, 0xE9]), "A\u{e9}");
        assert_eq!(
            decode(&(0u8..=255).collect::<Vec<_>>()).chars().count(),
            256
        );
    }

    #[test]
    fn every_byte_round_trips_as_a_string_and_as_a_unit() {
        // Windows' 1252 table is a bijection over all 256 bytes, undefined ones included.
        for byte in 0..=u8::MAX {
            let s = decode(&[byte]);
            assert_eq!(
                encode(&s).as_deref(),
                Some(&[byte][..]),
                "byte 0x{byte:02X}"
            );
            let unit = byte_to_unit(byte);
            assert_eq!(
                unit_to_byte(unit),
                Some(byte),
                "byte {byte:02x} -> U+{unit:04X}"
            );
        }
        let units = [0x20AC, 0x201C, 0x41, 0x43, 0x201D, 0xE9, 0, 0xFF];
        assert_eq!(widen(b"\x80\x93AC\x94\xe9\x00\xff"), units);
        assert_eq!(
            narrow(&units),
            (b"\x80\x93AC\x94\xe9\x00\xff".to_vec(), false)
        );
    }

    #[test]
    fn the_five_undefined_bytes_are_c1_controls_both_ways() {
        for byte in [0x81u8, 0x8D, 0x8F, 0x90, 0x9D] {
            assert_eq!(byte_to_unit(byte), 0x0080 + u16::from(byte - 0x80));
            assert_eq!(unit_to_byte(0x0080 + u16::from(byte - 0x80)), Some(byte));
            assert_eq!(decode(&[byte]).chars().next(), Some(char::from(byte)));
        }
        // U+0080 itself is not a 1252 character (0x80 is the euro sign).
        assert_eq!(unit_to_byte(0x0080), None);
        assert_eq!(
            narrow(&[0x81, 0x8D, 0x8F, 0x90, 0x9D]),
            (b"\x81\x8d\x8f\x90\x9d".to_vec(), false)
        );
    }

    #[test]
    fn unmapped_units_substitute_question_mark_and_raise_used_default() {
        // CJK, a valid pair (one `?` per unit as Windows does), lone surrogates, non-characters.
        assert_eq!(narrow(&[0x41, 0x4E00, 0x42]), (b"A?B".to_vec(), true));
        assert_eq!(narrow(&[0xD83D, 0xDE00]), (b"??".to_vec(), true));
        assert_eq!(narrow(&[0xD800]), (b"?".to_vec(), true));
        assert_eq!(narrow(&[0xDC00]), (b"?".to_vec(), true));
        assert_eq!(narrow(&[0xFFFE, 0xFFFF]), (b"??".to_vec(), true));
        assert_eq!(narrow(&[0x3F]), (b"?".to_vec(), false));
        assert_eq!(narrow(&[]), (Vec::new(), false));
    }

    #[test]
    fn best_fit_units_narrow_to_their_best_fit_byte_without_used_default() {
        // Latin letters lose their marks, fullwidth forms become ASCII, infinity becomes `8`.
        assert_eq!(narrow(&[0x0101, 0x0100]), (b"aA".to_vec(), false));
        let fullwidth: Vec<u16> = "＜ＴＥＬＬ：".encode_utf16().collect();
        assert_eq!(narrow(&fullwidth), (b"<TELL:".to_vec(), false));
        assert_eq!(narrow(&[0x221E]), (b"8".to_vec(), false));
        // A mapping current Windows applies beyond the published file: figure space.
        assert_eq!(narrow(&[0x2007]), (b"\xa0".to_vec(), false));
        // A best fit that lands on `?` itself is reported as a substitution, as Windows does.
        assert_eq!(narrow(&[0xFF1F]), (b"?".to_vec(), true));
        assert_eq!(best_fit_byte(0xFF1F), Some(b'?'));
        // Exact bytes are unaffected, and nothing maps a surrogate.
        assert_eq!(best_fit_byte(0x20AC), Some(0x80));
        assert_eq!(best_fit_byte(0xD800), None);
        assert_eq!(best_fit_byte(0x4E00), None);
    }

    #[test]
    fn the_best_fit_tables_are_sorted_and_hold_no_round_trip() {
        for table in [&BEST_FIT[..], &BEST_FIT_SINCE_PUBLISHED[..]] {
            assert!(table.windows(2).all(|pair| pair[0].0 < pair[1].0));
            assert!(table.iter().all(|&(unit, _)| unit_to_byte(unit).is_none()));
        }
        assert!(BEST_FIT_SINCE_PUBLISHED.iter().all(|&(unit, _)| BEST_FIT
            .binary_search_by_key(&unit, |&(from, _)| from)
            .is_err()));
        assert_eq!(
            BEST_FIT.len() + 256,
            698,
            "the published WCTABLE less its round trips"
        );
    }

    #[test]
    fn the_checked_encoder_rejects_what_the_lossy_ones_substitute() {
        assert_eq!(encode("\u{4E00}"), None);
        assert_eq!(encode("a\u{1F600}"), None);
        assert_eq!(encode_lossy("a\u{20AC}b"), vec![b'a', 0x80, b'b']);
        // The lossy encoders do not best-fit: U+0101 is `?`, where `narrow` gives `a`.
        assert_eq!(encode_lossy("x\u{0101}y"), b"x?y".to_vec());
        assert_eq!(encode_lossy("plain"), encode("plain").unwrap());
        // The two lossy encoders differ only outside the Basic Multilingual Plane: per UTF-16 unit
        // against per character.
        assert_eq!(encode_lossy("\u{1F600}"), b"??".to_vec());
        assert_eq!(encode_lossy_chars("\u{1F600}"), b"?".to_vec());
        assert_eq!(
            encode_lossy_chars("x\u{0101}y\u{20AC}"),
            b"x?y\x80".to_vec()
        );
        assert_eq!(
            encode_lossy_chars("\u{0080}\u{0085}\u{0081}"),
            b"??\x81".to_vec()
        );
    }

    /// The seam implementation answers what the table arm of the chat conversions answers: a host
    /// without NLS conversion must not quietly change.
    #[test]
    fn the_host_encoding_is_the_table_arm_of_the_chat_conversions() {
        let cp = Cp1252;
        assert_eq!(cp.acp(), 1252);
        // No substitution: the native bytes.
        assert_eq!(
            cp.narrow(&[0x41, 0xe9, 0x20ac]),
            Some(b"A\xe9\x80".to_vec())
        );
        // One substituted unit escapes the WHOLE string, a pair one escape per unit.
        assert_eq!(
            cp.narrow(&[0x41, 0xe9, 0xd83d, 0xde00]),
            Some(b"A<00e9><d83d><de00>".to_vec())
        );
        // A best fit is not a substitution: U+0101 is `a`, and nothing is escaped.
        assert_eq!(cp.narrow(&[0x41, 0x0101]), Some(b"Aa".to_vec()));
        // Widening is total, interior NULs included.
        assert_eq!(
            cp.widen(b"a\x80\x81\x00\xff"),
            Some(vec![0x61, 0x20ac, 0x81, 0, 0xff])
        );
        // `wctomb` best-fits, and stops the field at the first unit that raises `usedDefault` and
        // at the terminator.
        assert_eq!(
            cp.english_ws_field(&[0x41, 0xe9, 0x0101, 0x4e00, 0x42, 0]),
            Some(b"A\xe9a".to_vec())
        );
        assert_eq!(
            cp.english_ws_field(&[0x41, 0xff1f, 0x42]),
            Some(b"A".to_vec())
        );
        assert_eq!(cp.english_ws_field(&[0x41, 0, 0x42]), Some(b"A".to_vec()));
    }
}
