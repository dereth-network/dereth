use auberean_primitives::strings::{
    bucket, compare, equal, find_ignoring_case, hash, hash_ignoring_case, lower, packed_size,
    replace, to_lower, trim, WHITESPACE,
};

/// A narrow string's hash mixes each byte in as a signed character, so a byte above 127 counts
/// below zero; it ends at the first zero byte, and a key's bucket is its hash modulo the count.
///
/// Behaviour: text.strings.a-narrow-hash-takes-bytes-as-signed-characters
#[test]
fn a_narrow_hash_takes_bytes_as_signed_characters() {
    assert_eq!(hash(b""), 0);
    assert_eq!(hash(b"a"), 0x61);
    assert_eq!(hash(b"Turbine"), 0x0BC8_9015);
    assert_eq!(hash(b"keymap.txt"), 0x0386_46F4);
    assert_eq!(hash(b"\xC0"), 0x0FFF_FF30);
    assert_eq!(hash(b"\xE9t\xE9"), 0x0FFF_2FD9);
    assert_eq!(hash(b"ab\0cd"), hash(b"ab"));
    assert_eq!(bucket(b"keymap.txt", 47), 0x2B);
}

/// The hash ignoring case lowers each byte as the English locale does (`À` as `à`, `Š` as `š`,
/// `×` kept), then mixes it in as a signed character.
///
/// Behaviour: text.strings.a-narrow-hash-ignoring-case-lowers-in-the-english-code-page
#[test]
fn a_narrow_hash_ignoring_case_lowers_in_the_english_code_page() {
    assert_eq!(hash_ignoring_case(b"ABC"), 0x6783);
    assert_eq!(hash_ignoring_case(b"abc"), hash(b"abc"));
    assert_eq!(hash_ignoring_case(b"Turbine"), 0x0BC8_9035);
    assert_eq!(hash_ignoring_case(b"\xC0"), 0x0FFF_FF10);
    assert_eq!(hash_ignoring_case(b"\x8A"), hash_ignoring_case(b"\x9A"));
    assert_ne!(hash_ignoring_case(b"\xD7"), hash_ignoring_case(b"\xF7"));
}

/// Narrow comparison keeping case answers -1, 0 or 1 by unsigned bytes; ignoring case it answers
/// the difference of the first lowered bytes that differ. Both stop at a zero byte.
///
/// Behaviour: text.strings.narrow-comparison-by-bytes-or-lowered-bytes
#[test]
fn narrow_comparison_by_bytes_or_lowered_bytes() {
    assert_eq!(compare(b"abc", b"abd", true), -1);
    assert_eq!(compare(b"b", b"a", true), 1);
    assert_eq!(compare(b"\xE9", b"z", true), 1);
    assert_eq!(compare(b"ab\0x", b"ab", true), 0);
    assert_eq!(compare(b"ABC", b"abc", false), 0);
    assert_eq!(compare(b"a", b"c", false), -2);
    assert_eq!(compare(b"abc", b"ab", false), 0x63);
    assert!(equal(b"\xC9t\xC9", b"\xE9t\xE9", false));
    assert!(!equal(b"\xC9", b"\xE9", true));
    assert!(equal(b"\x9F", b"\xFF", false));
    assert!(!equal(b"\xD7", b"\xF7", false));
}

/// Lowering narrow text lowers `A`-`Z`, `À`-`Þ` but `×`, and `Š` `Œ` `Ž` `Ÿ`, up to the first zero
/// byte.
///
/// Behaviour: text.strings.narrow-lowering-in-the-english-code-page
#[test]
fn narrow_lowering_in_the_english_code_page() {
    let mut s = b"HeLLo \xC0\xDE\xD7\x8A\x8C\x8E\x9F\xDF\0X".to_vec();
    to_lower(&mut s);
    assert_eq!(s, b"hello \xE0\xFE\xD7\x9A\x9C\x9E\xFF\xDF\0X");
    assert_eq!(lower(b'['), b'[');
    assert_eq!(lower(b'@'), b'@');
}

/// Trimming narrow text drops the set's bytes from the left up to a zero byte, and from the right,
/// where a zero byte counts as one of the set.
///
/// Behaviour: text.strings.narrow-trimming-stops-at-a-zero-on-the-left-only
#[test]
fn narrow_trimming_stops_at_a_zero_on_the_left_only() {
    let mut s = b" \t name \r\n".to_vec();
    trim(&mut s, true, true, WHITESPACE);
    assert_eq!(s, b"name");
    let mut s = b"  x \0 ".to_vec();
    trim(&mut s, true, true, WHITESPACE);
    assert_eq!(s, b"x");
    let mut s = b" \0 x".to_vec();
    trim(&mut s, true, false, WHITESPACE);
    assert_eq!(s, b"\0 x");
    let mut s = b"--x--".to_vec();
    trim(&mut s, false, true, b"-");
    assert_eq!(s, b"--x");
}

/// Finding a part ignoring case looks forward from the position to the end, or backward from the
/// position less the part's length to the start; an empty part is found where the search stands.
///
/// Behaviour: text.strings.narrow-search-ignoring-case-forward-and-back
#[test]
fn narrow_search_ignoring_case_forward_and_back() {
    let text = b"Include \"Base.wsl\" include";
    assert_eq!(find_ignoring_case(text, b"INCLUDE", 0, false), Some(0));
    assert_eq!(find_ignoring_case(text, b"include", 1, false), Some(19));
    assert_eq!(
        find_ignoring_case(text, b"include", text.len(), true),
        Some(19)
    );
    assert_eq!(find_ignoring_case(text, b"include", 25, true), Some(0));
    assert_eq!(find_ignoring_case(text, b"base.WSL", 0, false), Some(9));
    assert_eq!(find_ignoring_case(text, b"missing", 0, false), None);
    assert_eq!(find_ignoring_case(text, b"include", 6, true), None);
    assert_eq!(find_ignoring_case(text, b"", 4, false), Some(4));
    assert_eq!(
        find_ignoring_case(b"\xC9t\xC9", b"\xE9T", 0, false),
        Some(0)
    );
}

/// Replacing in narrow text replaces each occurrence from the left, searching on after each, and
/// answers how many; an empty pattern is refused.
///
/// Behaviour: text.strings.narrow-replace-every-occurrence-from-the-left
#[test]
fn narrow_replace_every_occurrence_from_the_left() {
    let mut s = b"a\\b\\c".to_vec();
    assert_eq!(replace(&mut s, b"\\", b"/"), Some(2));
    assert_eq!(s, b"a/b/c");
    let mut s = b"aaa".to_vec();
    assert_eq!(replace(&mut s, b"aa", b"b"), Some(1));
    assert_eq!(s, b"ba");
    let mut s = b"Abc".to_vec();
    assert_eq!(replace(&mut s, b"a", b"x"), Some(0));
    assert_eq!(s, b"Abc");
    assert_eq!(replace(&mut s, b"", b"x"), None);
}

/// A packed string takes a 16-bit length (and a 32-bit one after it from 0xFFFF characters on),
/// its characters, and padding to four bytes.
///
/// Behaviour: text.strings.a-packed-string-is-its-lengths-and-characters-padded-to-four
#[test]
fn a_packed_string_is_its_lengths_and_characters_padded_to_four() {
    assert_eq!(packed_size(0, 1), 4);
    assert_eq!(packed_size(2, 1), 4);
    assert_eq!(packed_size(3, 1), 8);
    assert_eq!(packed_size(1, 2), 4);
    assert_eq!(packed_size(2, 2), 8);
    assert_eq!(packed_size(0xFFFE, 1), 0x1_0000);
    assert_eq!(packed_size(0xFFFF, 1), 0x1_0008);
    assert_eq!(packed_size(0xFFFF, 2), 0x2_0004);
}
