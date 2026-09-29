//! The client's internationalized line-breaking rules.
//!
//! This module implements the client's line-break classification.
//!
//! > Western text breaks only at whitespace, CJK text breaks between any two ideographs subject to
//! > the kinsoku rules above.
//!
//! Keep this exactly: the character sets are load-bearing for the Japanese and Korean
//! clients and are cheap to port.

/// The whitespace test. Note what is **not** here: `\n` (U+000A) is not
/// whitespace by this test, which is why [`can_break_line_at`] special-cases it first.
#[must_use]
pub const fn is_white_space(c: u16) -> bool {
    matches!(c, 0x0009 | 0x000D | 0x0020)
}

/// The new-line test.
#[must_use]
pub const fn is_new_line(c: u16) -> bool {
    c == 0x000A
}

/// Characters that may not start a line.
///
/// `!` `)` `,` `.` `?` `、` `。` `ー` `！` `）` `？` `ｰ` `ﾞ` `ﾟ`.
#[must_use]
pub const fn is_non_beginning_char(c: u16) -> bool {
    matches!(
        c,
        0x0021 // !
            | 0x0029 // )
            | 0x002C // ,
            | 0x002E // .
            | 0x003F // ?
            | 0x3001 // 、
            | 0x3002 // 。
            | 0x30FC // ー
            | 0xFF01 // ！
            | 0xFF09 // ）
            | 0xFF1F // ？
            | 0xFF70 // ｰ
            | 0xFF9E // ﾞ
            | 0xFF9F // ﾟ
    )
}

/// Characters that may not end a line: `(` and `（`.
#[must_use]
pub const fn is_non_ending_char(c: u16) -> bool {
    matches!(c, 0x0028 | 0xFF08)
}

/// The East-Asian character test.
///
/// `U+1100`–`U+11FF` (Hangul Jamo), `U+3000`–`U+D7AF` (CJK, Kana, Hangul),
/// `U+F900`–`U+FAFF` (CJK compatibility), `U+FF00`–`U+FFDC` (halfwidth and fullwidth forms).
#[must_use]
pub const fn is_east_asian_char(c: u16) -> bool {
    matches!(c, 0x1100..=0x11FF | 0x3000..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFDC)
}

/// Decide whether a line may break between `prev` and `next`. Retail's rules, in order:
///
/// ```text
/// if (!next || !prev || prev == next) return false
/// if (prev.ch == '\n') return true
/// if (!is_white_space(prev) && !is_east_asian_char(next) && !is_east_asian_char(prev)) return false
/// if (is_non_beginning_char(next) || is_non_ending_char(prev)) return false
/// return true
/// ```
///
/// `prev == next` in the original is a *pointer* comparison — the same glyph twice — so the
/// arguments here are the glyph **positions**, not their characters.
#[must_use]
pub fn can_break_line_at(prev: Option<(usize, u16)>, next: Option<(usize, u16)>) -> bool {
    let (Some((pi, p)), Some((ni, n))) = (prev, next) else {
        return false;
    };
    if pi == ni {
        return false;
    }
    if is_new_line(p) {
        return true;
    }
    if !is_white_space(p) && !is_east_asian_char(n) && !is_east_asian_char(p) {
        return false;
    }
    if is_non_beginning_char(n) || is_non_ending_char(p) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `u16`, so a character-class test enumerates rather than spot-checks.
    fn all_u16() -> impl Iterator<Item = u16> {
        0..=u16::MAX
    }

    /// Oracle: the four fixed line-breaking tables, which name every
    /// character in each set.
    #[test]
    fn the_character_classes_are_exactly_the_documented_sets() {
        assert_eq!(
            all_u16()
                .filter(|c| is_white_space(*c))
                .map(u32::from)
                .collect::<Vec<_>>(),
            vec![0x09, 0x0D, 0x20]
        );
        assert_eq!(
            all_u16()
                .filter(|c| is_non_ending_char(*c))
                .map(u32::from)
                .collect::<Vec<_>>(),
            vec![0x28, 0xFF08]
        );
        let nb: Vec<u32> = all_u16()
            .filter(|c| is_non_beginning_char(*c))
            .map(u32::from)
            .collect();
        assert_eq!(
            nb,
            vec![
                0x21, 0x29, 0x2C, 0x2E, 0x3F, 0x3001, 0x3002, 0x30FC, 0xFF01, 0xFF09, 0xFF1F,
                0xFF70, 0xFF9E, 0xFF9F
            ]
        );
        // The East Asian ranges, at their boundaries.
        for (lo, hi) in [
            (0x1100_u16, 0x11FF_u16),
            (0x3000, 0xD7AF),
            (0xF900, 0xFAFF),
            (0xFF00, 0xFFDC),
        ] {
            assert!(is_east_asian_char(lo), "{lo:#X}");
            assert!(is_east_asian_char(hi), "{hi:#X}");
            assert!(!is_east_asian_char(lo - 1), "{:#X}", lo - 1);
            assert!(!is_east_asian_char(hi + 1), "{:#X}", hi + 1);
        }
        assert!(!is_east_asian_char(0x0041), "Latin A is not East Asian");
    }

    /// Oracle: the five-line body.
    #[test]
    fn western_text_breaks_only_after_whitespace() {
        let b = |p: u16, n: u16| can_break_line_at(Some((0, p)), Some((1, n)));
        assert!(b(b' ' as u16, b'a' as u16), "after a space");
        assert!(!b(b'a' as u16, b'b' as u16), "never inside a Latin word");
        assert!(b(0x000A, b'a' as u16), "always after a newline");
        // A space followed by a non-beginning character still refuses.
        assert!(!b(b' ' as u16, b',' as u16));
        assert!(!b(b'(' as u16, b'a' as u16), "( may not end a line");
        // Degenerate arguments.
        assert!(!can_break_line_at(None, Some((1, b'a' as u16))));
        assert!(!can_break_line_at(Some((0, b' ' as u16)), None));
        assert!(!can_break_line_at(
            Some((0, b' ' as u16)),
            Some((0, b'a' as u16))
        ));
    }

    /// Oracle: the same function's third line — an East Asian character on **either** side lifts
    /// the whitespace requirement, subject to the kinsoku rules on the fourth line.
    #[test]
    fn cjk_text_breaks_between_ideographs_subject_to_kinsoku() {
        let b = |p: u16, n: u16| can_break_line_at(Some((0, p)), Some((1, n)));
        assert!(b(0x4E00, 0x4E8C), "between two kanji");
        assert!(b(0x4E00, b'a' as u16), "kanji then Latin");
        assert!(b(b'a' as u16, 0x4E00), "Latin then kanji");
        assert!(!b(0x4E00, 0x3002), "。 may not start a line");
        assert!(!b(0x4E00, 0x30FC), "ー may not start a line");
        assert!(!b(0xFF08, 0x4E00), "（ may not end a line");
    }
}
