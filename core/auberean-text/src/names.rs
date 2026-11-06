//! Names a player types: which characters a character name may hold in each language, the word
//! filter's check against an audience's patterns, and how a name is put in its written form.
//!
//! The answers are the client's codes: [`NAME_OK`], [`NAME_BAD`] (a character a name may not hold,
//! or a pattern the filter bans) and [`NAME_NO_FILTER`] (no word filter to check against).

use auberean_assets::script::WordFilter;

use crate::crt::{to_lower, to_upper, wcschr};

/// The name passed the check.
pub const NAME_OK: u32 = 0;
/// The name holds a character a name may not hold, or matches a banned pattern.
pub const NAME_BAD: u32 = 200;
/// There is no word filter to check the name against, or no room to check it.
pub const NAME_NO_FILTER: u32 = 201;

/// The languages a name's extra letters are chosen by.
pub const ENGLISH: u32 = 0x8000_0001;
pub const FRENCH: u32 = 0x8100_0001;
pub const GERMAN: u32 = 0x8100_0003;
pub const JAPANESE: u32 = 0x8100_0004;
pub const KOREAN: u32 = 0x8100_0005;
pub const CHINESE: u32 = 0x4100_0001;

/// One language's extra name letters: listed letters, and a range of units.
struct Letters {
    language: u32,
    extra: &'static [u16],
    range: (u16, u16),
}

/// Every language's extra letters, in the order they are looked up. French lists `Ú` where its
/// other grave letters would have `Ù`.
const LETTERS: [Letters; 6] = [
    Letters {
        language: ENGLISH,
        extra: &[],
        range: (0, 0),
    },
    Letters {
        language: FRENCH,
        extra: &[0xE8, 0xC8, 0xE9, 0xC9, 0xE0, 0xC0, 0xE7, 0xC7, 0xF9, 0xDA],
        range: (0, 0),
    },
    Letters {
        language: GERMAN,
        extra: &[0xFC, 0xDC, 0xF6, 0xD6, 0xE4, 0xC4],
        range: (0, 0),
    },
    Letters {
        language: KOREAN,
        extra: &[],
        range: (0xAC00, 0xD7A3),
    },
    Letters {
        language: JAPANESE,
        extra: &[],
        range: (0, 0),
    },
    Letters {
        language: CHINESE,
        extra: &[],
        range: (0x3400, 0x9FB0),
    },
];

/// The marks besides letters any name may hold.
const NAME_MARKS: [u16; 3] = [0x27, 0x2D, 0x20];

/// Whether a character name may hold a unit, in a language: an ASCII letter, an apostrophe, a
/// hyphen or a space; else one of the language's extra letters or a unit in its range. A language
/// not listed allows nothing more. (The zero unit counts as found, as the C library's search finds
/// a string's end.)
// D2-12379, D2-16958, D2-16959
#[must_use]
pub fn is_name_character(c: u16, language: u32) -> bool {
    if c < 0x80 && u8::try_from(c).is_ok_and(|b| b.is_ascii_alphabetic()) {
        return true;
    }
    if wcschr(&NAME_MARKS, c) {
        return true;
    }
    let Some(l) = LETTERS.iter().find(|l| l.language == language) else {
        return false;
    };
    if wcschr(l.extra, c) {
        return true;
    }
    l.range.0 != 0 && (l.range.0..=l.range.1).contains(&c)
}

/// Whether a unit may stand in a name while it is being composed: in Korean, the compatibility
/// letters an input method builds a syllable from (`U+3131` to `U+3163`).
// D2-12380
#[must_use]
pub fn is_name_character_composing(c: u16, language: u32) -> bool {
    language == KOREAN && c > 0x3130 && c < 0x3164
}

/// Whether a unit is a vowel: `AEIOU` either case, and the accented `ÀÁÄÈÉÖÚÜàáäèéöùü`.
// D2-16961
#[must_use]
pub fn is_vowel(c: u16) -> bool {
    matches!(
        c,
        0x41 | 0x45
            | 0x49
            | 0x4F
            | 0x55
            | 0x61
            | 0x65
            | 0x69
            | 0x6F
            | 0x75
            | 0xC0
            | 0xC1
            | 0xC4
            | 0xC8
            | 0xC9
            | 0xD6
            | 0xDA
            | 0xDC
            | 0xE0
            | 0xE1
            | 0xE4
            | 0xE8
            | 0xE9
            | 0xF6
            | 0xF9
            | 0xFC
    )
}

/// `iswalpha` in the client: below 256 the C library's own table (the Latin-1 letters `À`-`Ö`,
/// `Ø`-`ö`, `ø`-`ÿ`, not `ª` `µ` `º`), above it the operating system's classification, taken
/// here as Unicode's.
#[must_use]
pub fn is_alpha(c: u16) -> bool {
    match u8::try_from(c) {
        Ok(b) => b.is_ascii_alphabetic() || matches!(b, 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0xFF),
        Err(_) => char::from_u32(u32::from(c)).is_some_and(char::is_alphabetic),
    }
}

/// Check a name's characters in a language: [`NAME_BAD`] for an empty name, or one with a unit a
/// name may not hold. When `markup_only` is set, a unit a name may not hold is let through unless
/// it is one of `%`, `@`, `[`, `]` (which the text markup reads).
// D2-14283
#[must_use]
pub fn check_characters(name: &[u16], markup_only: bool, language: u32) -> u32 {
    let mut result = if name.is_empty() { NAME_BAD } else { NAME_OK };
    for &c in name {
        if !is_name_character(c, language) {
            if !markup_only {
                return NAME_BAD;
            }
            if matches!(c, 0x25 | 0x40 | 0x5B | 0x5D) {
                return NAME_BAD;
            }
            result = NAME_OK;
        }
    }
    result
}

/// [`check_characters`] against the install's word filter: [`NAME_NO_FILTER`] when there is none.
// D2-14299
#[must_use]
pub fn check_characters_with(
    filter: Option<&WordFilter>,
    name: &[u16],
    markup_only: bool,
    language: u32,
) -> u32 {
    match filter {
        Some(_) => check_characters(name, markup_only, language),
        None => NAME_NO_FILTER,
    }
}

/// The units of a name a pattern is matched against: with mode 2 only its letters, with mode 3
/// all but its white space, else all of it.
// D2-14279
#[must_use]
pub fn check_string(name: &[u16], mode: u32) -> Vec<u16> {
    name.iter()
        .copied()
        .filter(|&c| match mode {
            2 => is_alpha(c),
            3 => !crate::crt::is_space(c),
            _ => true,
        })
        .collect()
}

/// Whether `text` matches a pattern in which `*` stands for any run of units (empty included).
/// After a failed try the match goes on from the unit where it failed, not from just after where
/// the try began.
// D2-14280
#[must_use]
pub fn matches_filter(text: &[u16], pattern: &[u16]) -> bool {
    let at = |s: &[u16], i: usize| s.get(i).copied().unwrap_or(0);
    let star = u16::from(b'*');
    let (mut ti, mut pi) = (0usize, 0usize);
    let mut last_star: Option<usize> = None;
    let mut pc = at(pattern, 0);
    let mut tc = at(text, 0);
    loop {
        while pc == star {
            pc = at(pattern, pi + 1);
            loop {
                if tc == 0 {
                    return pc == 0;
                }
                last_star = Some(pi);
                if tc == pc {
                    break;
                }
                ti += 1;
                tc = at(text, ti);
            }
            pi += 1;
        }
        if pc == tc {
            loop {
                if tc == 0 {
                    return true;
                }
                let next = pi + 1;
                ti += 1;
                tc = at(text, ti);
                pi += 1;
                if at(pattern, next) == star {
                    pc = star;
                    break;
                }
                if at(pattern, next) != tc {
                    pc = 1;
                    break;
                }
            }
            if pc == star {
                continue;
            }
        }
        match last_star {
            None => return false,
            Some(s) => {
                pi = s;
                pc = at(pattern, s);
            }
        }
    }
}

/// Check a name against an audience's banned patterns: the name lower-cased, cut down by `mode`
/// ([`check_string`]), and matched against each pattern in stored order. [`NAME_BAD`] and the
/// first pattern that matches; [`NAME_OK`] when none does or the audience has no patterns;
/// [`NAME_NO_FILTER`] without a filter.
// D2-14288, D2-14300
#[must_use]
pub fn check_censors(
    filter: Option<&WordFilter>,
    name: &[u16],
    audience: u32,
    mode: u32,
) -> (u32, Option<String>) {
    let Some(filter) = filter else {
        return (NAME_NO_FILTER, None);
    };
    let Some(patterns) = filter.patterns(audience) else {
        return (NAME_OK, None);
    };
    let lower: Vec<u16> = name.iter().map(|&c| to_lower(c)).collect();
    let text = check_string(&lower, mode);
    for p in patterns {
        let pattern: Vec<u16> = p.encode_utf16().collect();
        if matches_filter(&text, &pattern) {
            return (NAME_BAD, Some(p.clone()));
        }
    }
    (NAME_OK, None)
}

/// A name in its written form: white space trimmed from both ends, each run of spaces and tabs
/// made one, and each letter capital after a unit that is not a letter and small after one that
/// is. Answers the form and whether it differs from what was typed.
// D2-14286
#[must_use]
pub fn format_for_audience(name: &[u16]) -> (Vec<u16>, bool) {
    let mut s = name.to_vec();
    crate::crt::trim(&mut s, true, true);
    let mut changed = s.len() != name.len();
    if !changed {
        let before = s.clone();
        crate::meta::trim_excess_spaces(&mut s);
        changed = s != before;
    }
    let mut after_letter = false;
    for c in &mut s {
        let letter = is_alpha(*c);
        if letter {
            let new = if after_letter {
                to_lower(*c)
            } else {
                to_upper(*c)
            };
            changed |= new != *c;
            *c = new;
        }
        after_letter = letter;
    }
    (s, changed)
}

/// [`format_for_audience`] when the install has a word filter; without one the name is left as
/// typed.
#[must_use]
pub fn format_for_audience_with(filter: Option<&WordFilter>, name: &[u16]) -> (Vec<u16>, bool) {
    match filter {
        Some(_) => format_for_audience(name),
        None => (name.to_vec(), false),
    }
}
