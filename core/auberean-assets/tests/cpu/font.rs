//! A font's characters: how a character finds its glyph, the question mark a missing one takes,
//! and how wide a string is drawn.

use auberean_assets::ui::{font_kind, CharacterMap, Font, Glyph};
use auberean_dat::types::RecordType;

/// A font of `cells` (code point, width) glyphs, laid side by side, with its map when `mapped`.
fn font(cells: &[(u16, u32)], mapped: bool) -> Font {
    let mut offset = 0;
    let glyphs: Vec<Glyph> = cells
        .iter()
        .map(|&(code_point, width)| {
            let g = Glyph {
                width,
                code_point,
                unused: 0,
                offset,
            };
            offset += width;
            g
        })
        .collect();
    Font {
        max_height: 10,
        max_width: 9,
        average_width: 6,
        total_width: offset,
        antialiased: true,
        pixels: Vec::new(),
        character_map: if mapped {
            CharacterMap::of(&glyphs)
        } else {
            None
        },
        glyphs,
    }
}

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// A font finds a character's glyph through the map of its characters, from its least to its
/// greatest code point; a code point it holds twice is its last glyph (the first without a map);
/// a character it lacks is drawn with the question mark's glyph, or with none when it lacks that
/// too; a font id is a font of the shared files or of the language's file.
///
/// Behaviour: data.font.a-character-finds-its-glyph-or-the-question-marks
#[test]
fn a_character_finds_its_glyph_or_the_question_marks() {
    let f = font(&[(0x41, 7), (0x3F, 5), (0x42, 8), (0x41, 9)], true);
    let m = f.character_map.as_ref().expect("a map");
    assert_eq!((m.first, m.last, m.places.len()), (0x3F, 0x42, 4));
    assert_eq!(m.places, [Some(1), None, Some(3), Some(2)]);
    assert_eq!(f.char_width(0x42), 8);
    assert_eq!(f.char_width(0x41), 9, "the last of a twice-held code point");
    assert_eq!(f.char_desc(0x40).map(|g| g.code_point), Some(0x3F));
    assert_eq!(
        f.char_width(0x2603),
        5,
        "far outside the map: the question mark"
    );
    let unmapped = font(&[(0x41, 7), (0x41, 9)], false);
    assert_eq!(unmapped.char_width(0x41), 7, "without a map, the first");
    assert_eq!(unmapped.char_width(0x42), 0, "no question mark: no glyph");
    assert!(CharacterMap::of(&[]).is_none());
    let kind = |id: u32| font_kind(id).map(|k| k.0);
    assert_eq!(kind(0x4200_0001), Some(RecordType::FONT.0));
    assert_eq!(kind(0x4200_1004), Some(RecordType::FONT_LOCAL.0));
    assert_eq!(kind(0x0600_0001), None);
}

/// A string's width is its widest line, its lines broken at new lines, each character as wide
/// as the glyph it is drawn with; a font holds a string when its map has every character of it
/// but new lines, and a font without a map holds every string.
///
/// Behaviour: data.font.a-strings-width-is-its-widest-line
#[test]
fn a_strings_width_is_its_widest_line() {
    let f = font(&[(0x3F, 5), (0x61, 6), (0x62, 7)], true);
    assert_eq!(f.string_width(&units("ab")), 13);
    assert_eq!(f.string_width(&units("a\nabba")), 26);
    assert_eq!(
        f.string_width(&units("abc")),
        18,
        "c drawn as the question mark"
    );
    assert_eq!(f.string_width(&[]), 0);
    assert!(f.contains_string(&units("ab\nba")));
    assert!(!f.contains_string(&units("abc")));
    assert!(font(&[(0x61, 6)], false).contains_string(&units("xyz")));
}
