//! The drift guard: the [`cp1252`] table arm against Windows' own conversions in code page 1252,
//! byte for byte and `usedDefault` for `usedDefault`. It runs where both arms exist, a Windows host
//! with `host-nls`; everywhere else the table arm stands alone, and this is what says it is right.

use super::{cp1252, windows, ChatConversion};

const CP: u32 = 1252;

/// `WideCharToMultiByte(1252, 0, units, ..., "?", &usedDefault)` on the host: bytes and the flag.
fn host_narrow(units: &[u16]) -> (Vec<u8>, bool) {
    let (size, _) = windows::query(CP, units, true).expect("the size query succeeds");
    windows::convert(CP, units, size, true).expect("the conversion succeeds")
}

/// Every chat conversion, both arms, over one string.
fn assert_arms_agree(units: &[u16]) {
    let host = ChatConversion::for_ansi_code_page(CP);
    let table = ChatConversion::table(CP);
    if !units.is_empty() {
        assert_eq!(
            host_narrow(units),
            cp1252::narrow(units),
            "narrow {units:04x?}"
        );
    }
    assert_eq!(
        host.to_spstring(units),
        table.to_spstring(units),
        "to_spstring {units:04x?}"
    );
    assert_eq!(
        host.english_ws_field(units),
        table.english_ws_field(units),
        "english_ws_field {units:04x?}"
    );
}

/// A small deterministic generator, so a failure names a reproducible string.
struct XorShift(u64);
impl XorShift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % n as u64).expect("below n")
    }
}

#[test]
fn every_bmp_unit_narrows_as_windows_narrows_it() {
    for unit in 0..=u16::MAX {
        assert_arms_agree(&[unit]);
        // The unit between others, where a substitution must escape the whole string.
        assert_arms_agree(&[0x41, unit, 0xE9]);
    }
}

#[test]
fn surrogates_paired_lone_and_reversed_narrow_as_windows_narrows_them() {
    let lows = [0xDC00, 0xDC01, 0xDE00, 0xDE42, 0xDFFE, 0xDFFF];
    let highs = [0xD800, 0xD801, 0xD83D, 0xDB7F, 0xDB80, 0xDBFF];
    for high in 0xD800..=0xDBFF {
        for low in lows {
            assert_arms_agree(&[high, low]);
            assert_arms_agree(&[0x61, high, low, 0x62]);
            assert_arms_agree(&[low, high]);
        }
        assert_arms_agree(&[high, 0x41]);
    }
    for low in 0xDC00..=0xDFFF {
        for high in highs {
            assert_arms_agree(&[high, low]);
        }
        assert_arms_agree(&[0x41, low]);
    }
}

#[test]
fn mixed_strings_narrow_as_windows_narrows_them() {
    // Pools that cover each kind of unit: ASCII, the 1252 upper half, best-fit sources (Latin
    // Extended, fullwidth forms, punctuation and symbols), units with no byte (CJK, unassigned,
    // private use, non-characters), surrogates and NUL.
    let pools: [&[u16]; 8] = [
        &[0x00, 0x20, 0x3C, 0x3E, 0x3F, 0x41, 0x5C, 0x7A, 0x7F],
        &[0x80, 0x81, 0x9F, 0xA0, 0xE9, 0xFF, 0x20AC, 0x2019, 0x0178],
        &[
            0x0100, 0x0101, 0x0141, 0x01CD, 0x02C7, 0x2007, 0x2033, 0x221E, 0x2500,
        ],
        &[
            0xFF01, 0xFF1A, 0xFF1C, 0xFF1E, 0xFF1F, 0xFF34, 0xFF41, 0xFF5E, 0xFF5F,
        ],
        &[
            0x0378, 0x4E00, 0x3042, 0x0416, 0x05D0, 0xE000, 0xF8FF, 0xFFFE, 0xFFFF,
        ],
        &[0xD800, 0xD83D, 0xDBFF],
        &[0xDC00, 0xDE42, 0xDFFF],
        &[0x0300, 0x0301, 0x0327, 0x2028, 0xFEFF, 0xFFFD],
    ];
    let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
    let mut strings = 0;
    for _ in 0..40_000 {
        let len = rng.below(40);
        let mut units = Vec::with_capacity(len + 2);
        for _ in 0..len {
            match rng.below(10) {
                // A well-formed pair now and then.
                0 => {
                    let high = 0xD800 + u16::try_from(rng.below(0x400)).expect("< 0x400");
                    let low = 0xDC00 + u16::try_from(rng.below(0x400)).expect("< 0x400");
                    units.extend([high, low]);
                }
                // Any BMP unit at all.
                1 => units.push(u16::try_from(rng.below(0x1_0000)).expect("< 0x10000")),
                _ => {
                    let pool = pools[rng.below(pools.len())];
                    units.push(pool[rng.below(pool.len())]);
                }
            }
        }
        assert_arms_agree(&units);
        strings += 1;
    }
    assert_eq!(strings, 40_000);
}

#[test]
fn widening_agrees_for_every_byte_and_mixed_byte_strings() {
    let host = ChatConversion::for_ansi_code_page(CP);
    let table = ChatConversion::table(CP);
    for byte in 0..=u8::MAX {
        assert_eq!(
            host.to_wpstring(&[byte]),
            table.to_wpstring(&[byte]),
            "byte {byte:02x}"
        );
    }
    let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
    for _ in 0..10_000 {
        let bytes: Vec<u8> = (0..rng.below(40))
            .map(|_| u8::try_from(rng.below(256)).expect("< 256"))
            .collect();
        assert_eq!(
            host.to_wpstring(&bytes),
            table.to_wpstring(&bytes),
            "{bytes:02x?}"
        );
    }
}
