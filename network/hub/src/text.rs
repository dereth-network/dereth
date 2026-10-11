//! The rules for text the hub keeps from operators, worlds and reporters, and for text it writes
//! into a document another program reads.
//!
//! A world's name, description and links reach every launcher and player on the network, so none
//! may hide what it says: no control characters (a description may hold line feeds), no
//! bidirectional overrides, embeddings, isolates or marks that reorder what a reader sees, and no
//! zero-width or other invisible characters that make two names look one.

/// Whether `c` is refused in any text the hub keeps: a control character, a line or paragraph
/// separator, a bidirectional control, a zero-width or invisible formatting character, or one of
/// the two noncharacters XML 1.0 forbids.
#[must_use]
pub fn is_hidden(c: char) -> bool {
    matches!(
        u32::from(c),
        0x00..=0x1F
            | 0x7F..=0x9F
            | 0x061C
            | 0x180E
            | 0x200B..=0x200F
            | 0x2028..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x2069
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0xFFFE
            | 0xFFFF
    )
}

/// Whether `s` holds no hidden character, a line feed aside where `lines` allows them.
#[must_use]
pub fn is_plain(s: &str, lines: bool) -> bool {
    s.chars().all(|c| !is_hidden(c) || (lines && c == '\n'))
}

/// `s` with every hidden character taken out, for text the hub is given rather than sent (a
/// Discord display name, a world's note on who verified it).
#[must_use]
pub fn without_hidden(s: &str) -> String {
    s.chars().filter(|&c| !is_hidden(c)).collect()
}

/// `s` with every character XML 1.0 forbids taken out, for a document a launcher parses: a row
/// kept before these rules, or imported, never breaks it.
#[must_use]
pub fn xml_safe(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            matches!(
                u32::from(c),
                0x09 | 0x0A | 0x0D | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x1_0000..=0x10_FFFF
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec: hub: text a world or an operator sends holds no control, bidirectional, zero-width
    /// or invisible character, a description's line feeds aside.
    #[test]
    fn every_kind_of_hidden_character_is_refused_and_ordinary_text_is_not() {
        for c in [
            '\0', '\u{7}', '\t', '\r', '\n', '\u{7f}', '\u{85}', '\u{9f}', '\u{61c}', '\u{180e}',
            '\u{200b}', '\u{200c}', '\u{200d}', '\u{200e}', '\u{200f}', '\u{2028}', '\u{2029}',
            '\u{202a}', '\u{202e}', '\u{2060}', '\u{2064}', '\u{2066}', '\u{2069}', '\u{feff}',
            '\u{fff9}', '\u{fffb}', '\u{fffe}', '\u{ffff}',
        ] {
            assert!(is_hidden(c), "{:04x}", u32::from(c));
            assert!(!is_plain(&format!("a{c}b"), false), "{:04x}", u32::from(c));
        }
        for s in [
            "Coldeve",
            "Ærfalle — the north",
            "Dereth’s «best»",
            "ナイト",
            "🐉 Drake",
        ] {
            assert!(is_plain(s, false), "{s}");
        }
        assert!(is_plain("one\ntwo", true));
        assert!(!is_plain("one\ntwo", false));
        assert!(!is_plain("one\r\ntwo", true));
    }

    /// Spec: hub: the community list the hub writes holds no character XML 1.0 forbids.
    #[test]
    fn xml_keeps_what_it_allows_and_nothing_else() {
        assert_eq!(xml_safe("a\u{0}b\u{1}c\u{b}d\u{fffe}e\u{ffff}f"), "abcdef");
        assert_eq!(xml_safe("tab\tlf\ncr\r🐉"), "tab\tlf\ncr\r🐉");
        assert_eq!(without_hidden("a\u{202e}b\u{200b}c\nd"), "abcd");
    }
}
