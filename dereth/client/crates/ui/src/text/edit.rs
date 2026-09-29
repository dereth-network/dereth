//! Editing: the twelve cursor travel modes, the input filters, selection and the clipboard.
//!
//! Editing filters and character handling live here.
//!
//! There is no Win32 edit control anywhere: editing is driven entirely by named **input actions**
//! 0x16–0x28 plus `WM_CHAR`.

use crate::text::glyph::GlyphList;

/// `CursorTravelMode`. The text action handler uses twelve values. Numeric values beyond
/// those twelve were not recovered, so only the observed values are implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorTravelMode {
    /// action 0x16
    Left,
    /// action 0x17
    Right,
    /// action 0x18
    WordLeft,
    /// action 0x19
    WordRight,
    /// action 0x1A
    LineHome,
    /// action 0x1B
    LineEnd,
    /// action 0x1C
    Home,
    /// action 0x1D
    End,
    /// action 0x1E
    Up,
    /// action 0x1F
    Down,
    /// action 0x20
    PageUp,
    /// action 0x21
    PageDown,
}

impl CursorTravelMode {
    /// The action → cursor travel mode table.
    #[must_use]
    pub const fn from_action(a: u32) -> Option<Self> {
        Some(match a {
            0x16 => Self::Left,
            0x17 => Self::Right,
            0x18 => Self::WordLeft,
            0x19 => Self::WordRight,
            0x1A => Self::LineHome,
            0x1B => Self::LineEnd,
            0x1C => Self::Home,
            0x1D => Self::End,
            0x1E => Self::Up,
            0x1F => Self::Down,
            0x20 => Self::PageUp,
            0x21 => Self::PageDown,
            _ => return None,
        })
    }
}

/// `CursorMovementFlags` — the third argument of caret movement and the whole of the decision
/// "does moving the caret drag a selection with it".
///
/// The three retail values are 0 (default), 1 (select text) and 2 (do not select text).
///
/// **`Default` is not "do nothing special"** — it is *conditional*: it selects when a mouse drag
/// is live (the edit's state bit `0x40`) or shift is held, and does not otherwise. That is why `MouseMove`
/// and `MouseUp` pass `Default` rather than `SelectText`: the same call has to extend a sweep and
/// leave a plain click alone, and the bit is what tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorMovementFlags {
    /// The default movement, `0`.
    #[default]
    Default,
    /// `1` — extend unconditionally.
    SelectText,
    /// `2` — never extend, and drop a live selection.
    DontSelectText,
}

/// Move the text cursor.
///
/// `lines_per_page` is the scrollable area's height in lines; the client reads it from the
/// scrollable rectangle, and a caller with no viewport should pass 1.
#[must_use]
pub fn move_cursor(
    g: &GlyphList,
    pos: usize,
    mode: CursorTravelMode,
    lines_per_page: usize,
) -> usize {
    let n = g.len();
    let line = g.find_current_line(pos);
    let x = g.find_pixels_from_pos(pos);
    match mode {
        CursorTravelMode::Left => pos.saturating_sub(1),
        CursorTravelMode::Right => (pos + 1).min(n),
        CursorTravelMode::WordLeft => g.find_prev_word(pos),
        CursorTravelMode::WordRight => g.find_next_word(pos),
        CursorTravelMode::LineHome => g.lines.get(line).map_or(0, |l| l.start),
        CursorTravelMode::LineEnd => g.lines.get(line).map_or(n, |l| l.end),
        CursorTravelMode::Home => 0,
        CursorTravelMode::End => n,
        CursorTravelMode::Up => {
            if line == 0 {
                0
            } else {
                g.find_pos_from_line_and_pixels(line - 1, x)
            }
        }
        CursorTravelMode::Down => {
            if line + 1 >= g.lines.len() {
                n
            } else {
                g.find_pos_from_line_and_pixels(line + 1, x)
            }
        }
        CursorTravelMode::PageUp => {
            let target = line.saturating_sub(lines_per_page.max(1));
            g.find_pos_from_line_and_pixels(target, x)
        }
        CursorTravelMode::PageDown => {
            let target = (line + lines_per_page.max(1)).min(g.lines.len().saturating_sub(1));
            g.find_pos_from_line_and_pixels(target, x)
        }
    }
}

/// A per-character input filter: returns whether the character is accepted.
pub type InputFilter = fn(u16) -> bool;

/// The character-name rule: `isalpha(c)` for `c < 0x100`, plus
/// `'` (0x27), space (0x20) and `-` (0x2D).
///
/// `isalpha` in the era's CRT is locale-dependent; for `c < 0x80` it is exactly A–Z and a–z, and
/// the 0x80..0xFF range depends on the code page. The Latin-1 letters are accepted here, which is
/// what CP1252 — the client's narrow code page — gives.
#[must_use]
pub fn name_input_filter(c: u16) -> bool {
    if c == 0x27 || c == 0x20 || c == 0x2D {
        return true;
    }
    if c >= 0x100 {
        return false;
    }
    // LINT-OK: the `c >= 0x100` guard above is exactly the original's `c < 0x100` test, so this
    // conversion cannot lose anything.
    let Ok(b) = u8::try_from(c) else { return false };
    b.is_ascii_alphabetic() || matches!(b, 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0xFF)
}

/// The number filter — `isdigit(c)`.
#[must_use]
pub fn number_input_filter(c: u16) -> bool {
    (0x30..=0x39).contains(&c)
}

/// The selection range and the clipboard operations.
///
/// Selection is the half-open range `[start, end)`; equal means no
/// selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Selection {
    pub start: usize,
    pub end: usize,
}

impl Selection {
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// The range in ascending order, since a drag can run backwards.
    #[must_use]
    pub fn ordered(&self) -> (usize, usize) {
        if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::glyph::{FixedMetrics, GlyphList};

    const M: FixedMetrics = FixedMetrics {
        advance: 10,
        height: 16,
    };

    fn list(s: &str, width: i32) -> GlyphList {
        let mut g = GlyphList::default();
        g.add_text(0, s, &M, 0, 0, None);
        g.recalculate(width);
        g
    }

    /// Oracle: all twelve rows of the action-to-cursor-movement table.
    #[test]
    fn every_editing_action_maps_to_its_documented_travel_mode() {
        use CursorTravelMode as C;
        let expect = [
            (0x16, C::Left),
            (0x17, C::Right),
            (0x18, C::WordLeft),
            (0x19, C::WordRight),
            (0x1A, C::LineHome),
            (0x1B, C::LineEnd),
            (0x1C, C::Home),
            (0x1D, C::End),
            (0x1E, C::Up),
            (0x1F, C::Down),
            (0x20, C::PageUp),
            (0x21, C::PageDown),
        ];
        assert_eq!(expect.len(), 12);
        for (a, m) in expect {
            assert_eq!(CursorTravelMode::from_action(a), Some(m), "action {a:#x}");
        }
        // The actions either side of the range are not cursor moves: 0x22 is Copy, 0x15 is nothing.
        assert_eq!(CursorTravelMode::from_action(0x22), None);
        assert_eq!(CursorTravelMode::from_action(0x15), None);
    }

    /// Oracle: the client's cursor move, with its find-current-line and find-position helpers.
    #[test]
    fn cursor_travel_walks_the_line_index() {
        // three lines of five glyphs, 50px wide
        let g = list("abcdefghijklmno", 50);
        assert_eq!(g.lines.len(), 3);
        use CursorTravelMode as C;
        assert_eq!(move_cursor(&g, 7, C::Left, 1), 6);
        assert_eq!(move_cursor(&g, 0, C::Left, 1), 0, "clamps at the start");
        assert_eq!(move_cursor(&g, 15, C::Right, 1), 15, "clamps at the end");
        assert_eq!(move_cursor(&g, 7, C::LineHome, 1), 5);
        assert_eq!(move_cursor(&g, 7, C::LineEnd, 1), 10);
        assert_eq!(move_cursor(&g, 7, C::Home, 1), 0);
        assert_eq!(move_cursor(&g, 7, C::End, 1), 15);
        assert_eq!(move_cursor(&g, 7, C::Up, 1), 2, "same x, one line up");
        assert_eq!(move_cursor(&g, 7, C::Down, 1), 12);
        assert_eq!(move_cursor(&g, 2, C::Up, 1), 0, "already on the first line");
        assert_eq!(
            move_cursor(&g, 12, C::Down, 1),
            15,
            "already on the last line"
        );
        assert_eq!(move_cursor(&g, 12, C::PageUp, 2), 2);
    }

    /// Pinned behavior: `isalpha(c)` for `c < 0x100`, plus `'`, space and `-`.
    #[test]
    fn the_name_filter_accepts_exactly_the_character_name_rule() {
        for c in "abcXYZ".chars() {
            assert!(name_input_filter(c as u16), "{c}");
        }
        for c in "' -".chars() {
            assert!(name_input_filter(c as u16), "{c:?}");
        }
        for c in "0123456789!@#".chars() {
            assert!(!name_input_filter(c as u16), "{c}");
        }
        assert!(
            !name_input_filter(0x4E00),
            "no ideographs in a character name"
        );
    }

    /// Oracle: the client's number filter — `isdigit`.
    #[test]
    fn the_number_filter_is_isdigit() {
        for c in '0'..='9' {
            assert!(number_input_filter(c as u16));
        }
        assert!(!number_input_filter(b'/' as u16));
        assert!(!number_input_filter(b':' as u16));
        assert!(!number_input_filter(b'a' as u16));
    }

    /// Oracle: §3.4 — "Selection is the half-open range `[start, end)`".
    #[test]
    fn a_backwards_drag_still_yields_an_ordered_range() {
        assert!(Selection { start: 3, end: 3 }.is_empty());
        assert_eq!(Selection { start: 7, end: 2 }.ordered(), (2, 7));
        assert_eq!(Selection { start: 2, end: 7 }.ordered(), (2, 7));
    }
}
