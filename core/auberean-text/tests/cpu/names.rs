//! Character names: which characters a name may hold, the word filter's patterns, and a name's
//! written form.

use auberean_assets::script::WordFilter;
use auberean_text::names::{
    check_censors, check_characters, check_characters_with, format_for_audience, is_name_character,
    is_name_character_composing, is_vowel, matches_filter, CHINESE, ENGLISH, FRENCH, GERMAN,
    KOREAN, NAME_BAD, NAME_NO_FILTER, NAME_OK,
};

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Behaviour: text.name.characters-by-language
#[test]
fn a_name_holds_ascii_letters_and_marks_and_its_languages_extra_letters() {
    for c in "Arwic O'Neil-Ra".encode_utf16() {
        assert!(is_name_character(c, ENGLISH));
    }
    for c in ['1', '_', '%', '.'] {
        assert!(!is_name_character(c as u16, ENGLISH), "{c}");
    }
    assert!(!is_name_character(0xE9, ENGLISH), "é is not English");
    assert!(is_name_character(0xE9, FRENCH));
    assert!(is_name_character(0xDA, FRENCH), "French lists Ú");
    assert!(!is_name_character(0xD9, FRENCH), "but not Ù");
    assert!(!is_name_character(0xD4, GERMAN), "Ô is not German");
    assert!(is_name_character(0xFC, GERMAN));
    assert!(!is_name_character(0xDF, GERMAN), "ß is not listed");
    assert!(is_name_character(0xAC00, KOREAN) && is_name_character(0xD7A3, KOREAN));
    assert!(!is_name_character(0xD7A4, KOREAN));
    assert!(is_name_character(0x4E00, CHINESE));
    assert!(
        !is_name_character(0x4E00, 0x1234_5678),
        "an unknown language adds nothing"
    );
    assert!(is_name_character_composing(0x3131, KOREAN));
    assert!(!is_name_character_composing(0x3131, ENGLISH));
    assert!(is_vowel(u16::from(b'e')) && is_vowel(0xDA) && !is_vowel(0xD9));
}

/// Behaviour: text.name.check-characters
#[test]
fn a_name_with_a_character_it_may_not_hold_is_refused_unless_only_markup_is_checked() {
    assert_eq!(check_characters(&w("Arwic"), false, ENGLISH), NAME_OK);
    assert_eq!(check_characters(&w(""), false, ENGLISH), NAME_BAD);
    assert_eq!(check_characters(&w("Ar1wic"), false, ENGLISH), NAME_BAD);
    assert_eq!(check_characters(&w("Ar1wic"), true, ENGLISH), NAME_OK);
    assert_eq!(check_characters(&w("Ar[wic"), true, ENGLISH), NAME_BAD);
    assert_eq!(check_characters(&w("Ar@wic"), true, ENGLISH), NAME_BAD);
    assert_eq!(
        check_characters_with(None, &w("Arwic"), false, ENGLISH),
        NAME_NO_FILTER
    );
}

/// The client's own answers (the 0930 code run in an emulator) for texts and patterns over `ab*`.
const ORACLE: &[(&str, &str, bool)] = &[
    ("", "", true),
    ("abbbaa", "*a", true),
    ("aaaab", "a*", true),
    ("bababa", "*abab*", true),
    ("bab", "*b*", true),
    ("abaaab", "*", true),
    ("aa", "bbb*a", false),
    ("aabab", "aabaa", false),
    ("", "a*a", false),
    // Two stars together never match.
    ("b", "**", false),
    ("ab", "**ab", false),
    ("aaaabb", "a***", false),
    // A failed try goes on from where it failed.
    ("bbb", "*bb", false),
    ("abaaa", "*aa", false),
    ("baaa", "b*aa", false),
    ("babaaa", "bab*aa", false),
];

/// Behaviour: text.name.filter-pattern
#[test]
fn a_pattern_matches_as_the_client_matches_it() {
    for (t, p, want) in ORACLE {
        assert_eq!(matches_filter(&w(t), &w(p)), *want, "{t:?} {p:?}");
    }
}

/// Behaviour: text.name.check-censors
#[test]
fn a_name_matching_an_audiences_pattern_is_refused_with_the_pattern() {
    let filter = WordFilter {
        buckets: 0,
        audiences: vec![(1, vec!["*bad*".into(), "worse".into()])],
    };
    let f = Some(&filter);
    assert_eq!(check_censors(f, &w("Good Name"), 1, 0), (NAME_OK, None));
    assert_eq!(
        check_censors(f, &w("NotBAD"), 1, 0),
        (NAME_BAD, Some("*bad*".into()))
    );
    // Mode 2 matches the letters only, mode 3 all but the spaces.
    assert_eq!(check_censors(f, &w("w o r s e"), 1, 0).0, NAME_OK);
    assert_eq!(check_censors(f, &w("w o r s e"), 1, 3).0, NAME_BAD);
    assert_eq!(check_censors(f, &w("w-o'r-s-e"), 1, 2).0, NAME_BAD);
    assert_eq!(check_censors(f, &w("bad"), 2, 0), (NAME_OK, None));
    assert_eq!(check_censors(None, &w("bad"), 1, 0).0, NAME_NO_FILTER);
}

/// Behaviour: text.name.written-form
#[test]
fn a_name_is_written_trimmed_with_a_capital_after_each_non_letter() {
    assert_eq!(
        format_for_audience(&w("jOHN  o'neil-ra")),
        (w("John O'Neil-Ra"), true)
    );
    // Runs of spaces are made one only when there was nothing to trim.
    assert_eq!(
        format_for_audience(&w("  jOHN  o'neil-ra ")),
        (w("John  O'Neil-Ra"), true)
    );
    assert_eq!(format_for_audience(&w("Arwic")), (w("Arwic"), false));
    assert_eq!(format_for_audience(&w("élise")), (w("Élise"), true));
}
