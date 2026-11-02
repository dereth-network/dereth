use auberean_text::wide::{
    append_float, append_hex32, append_hex64, equal, find_ignoring_case, narrow, scan, to_int32,
    to_int64, to_uint64, widen,
};

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Wide equality keeps case unit for unit, or lowers each unit as `towlower` does, letters above
/// 255 included; both stop at the first zero unit.
///
/// Behaviour: text.strings.wide-equality-ignoring-case-lowers-every-letter
#[test]
fn wide_equality_ignoring_case_lowers_every_letter() {
    assert!(equal(&w("Fist"), &w("fist"), false));
    assert!(!equal(&w("Fist"), &w("fist"), true));
    assert!(equal(&w("ÉŁÆ"), &w("éłæ"), false));
    assert!(equal(&w("ab\0c"), &w("ab"), true));
    assert!(!equal(&w("ab"), &w("abc"), false));
}

/// The wide search ignoring case folds each unit by its low byte alone, lowered in the English
/// code page, so `Ł` (`U+0141`) matches `a`; it looks forward to the end or backward to the start.
///
/// Behaviour: text.strings.wide-search-ignoring-case-folds-the-low-byte
#[test]
fn wide_search_ignoring_case_folds_the_low_byte() {
    let text = w("Read the Rules, rules");
    assert_eq!(find_ignoring_case(&text, &w("RULES"), 0, false), Some(9));
    assert_eq!(find_ignoring_case(&text, &w("rules"), 10, false), Some(16));
    assert_eq!(
        find_ignoring_case(&text, &w("rules"), text.len(), true),
        Some(16)
    );
    assert_eq!(find_ignoring_case(&text, &w("rules"), 15, true), Some(9));
    assert_eq!(find_ignoring_case(&text, &w("none"), 0, false), None);
    assert_eq!(find_ignoring_case(&w("xŁy"), &w("A"), 0, false), Some(1));
    assert_eq!(find_ignoring_case(&w("xay"), &w("Ł"), 0, false), Some(1));
    assert_eq!(find_ignoring_case(&w("ÉT"), &w("ét"), 0, false), Some(0));
}

/// Reading a wide whole number: white space, a sign, then base 0's `0x` (16), leading `0` (8) or
/// decimal digits; with no digit nothing is read. Signed reads clamp out of range; unsigned ones
/// saturate the magnitude, and a minus sign then counts back from all ones.
///
/// Behaviour: text.strings.wide-whole-numbers-read-as-the-c-library-reads-them
#[test]
fn wide_whole_numbers_read_as_the_c_library_reads_them() {
    assert_eq!(to_int64(&w("  -42z"), 0), (-42, 5));
    assert_eq!(to_int64(&w("\u{a0}0x1F"), 0), (31, 5));
    assert_eq!(to_int64(&w("017"), 0), (15, 3));
    assert_eq!(to_int64(&w("09"), 0), (0, 1));
    assert_eq!(to_int64(&w("0x"), 0), (0, 0));
    assert_eq!(to_int64(&w("+"), 0), (0, 0));
    assert_eq!(to_int64(&w("zz"), 36), (1295, 2));
    assert_eq!(to_int64(&w("0x10"), 16), (16, 4));
    assert_eq!(to_int64(&w("01"), 1), (0, 0));
    assert_eq!(to_int64(&w("99999999999999999999"), 0), (i64::MAX, 20));
    assert_eq!(to_int64(&w("-9223372036854775808"), 0).0, i64::MIN);
    assert_eq!(to_int64(&w("-9223372036854775809"), 0).0, i64::MIN);
    assert_eq!(to_uint64(&w("-1"), 0), (u64::MAX, 2));
    assert_eq!(to_uint64(&w("99999999999999999999"), 0).0, u64::MAX);
    assert_eq!(to_uint64(&w("-99999999999999999999"), 0).0, 1);
    assert_eq!(to_int32(&w("0x7FFFFFFF")), i32::MAX);
    assert_eq!(to_int32(&w("0x80000000")), i32::MAX);
    assert_eq!(to_int32(&w("-3000000000")), i32::MIN);
    assert_eq!(to_int32(&w("12abc")), 12);
    let r = scan(&w("4294967296"), 10, 32, true);
    assert!(r.out_of_range);
    assert_eq!(r.value, u64::from(u32::MAX));
}

/// The appended forms: a word as `0x` and eight upper-case hexadecimal digits, a 64-bit word as
/// `0x` and sixteen, a float with six decimals as the C library rounds it, and its infinities and
/// not-a-numbers as that library writes them.
///
/// Behaviour: text.strings.wide-hexadecimal-and-float-appends
#[test]
fn wide_hexadecimal_and_float_appends() {
    let mut t = w("id ");
    append_hex32(&mut t, -1);
    assert_eq!(t, w("id 0xFFFFFFFF"));
    let mut t = Vec::new();
    append_hex32(&mut t, 0x1A);
    append_hex64(&mut t, 0x1_0000_00AB);
    assert_eq!(t, w("0x0000001A0x00000001000000AB"));
    let cases: [(f32, &str); 6] = [
        (1.5, "1.500000"),
        (-0.25, "-0.250000"),
        (0.1, "0.100000"),
        (f32::INFINITY, "1.#INF00"),
        (f32::NEG_INFINITY, "-1.#INF00"),
        (f32::from_bits(0x7FC0_0000), "1.#QNAN0"),
    ];
    for (v, want) in cases {
        let mut t = Vec::new();
        append_float(&mut t, v);
        assert_eq!(t, w(want), "{v}");
    }
    let mut t = Vec::new();
    append_float(&mut t, f32::from_bits(0xFFC0_0000));
    assert_eq!(t, w("-1.#IND00"));
}

/// Narrow text is read in the English code page, a byte a unit; wide text is written in it, and
/// when any unit has no byte there, every unit from 128 up is written as `<%04x>` instead.
///
/// Behaviour: text.strings.narrow-and-wide-text-convert-in-the-english-code-page
#[test]
fn narrow_and_wide_text_convert_in_the_english_code_page() {
    assert_eq!(widen(b"a\x80\x9F\xE9\x81"), w("a€Ÿé\u{81}"));
    assert_eq!(narrow(&w("a€Ÿé")), b"a\x80\x9F\xE9");
    assert_eq!(
        narrow(&widen(b"\x00\x7F\xA0\xFF\x8D")),
        b"\x00\x7F\xA0\xFF\x8D"
    );
    assert_eq!(narrow(&w("Łódź é")), b"<0141><00f3>d<017a> <00e9>");
}
