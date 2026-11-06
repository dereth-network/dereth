//! Text: glyphs, each a character in a font and a color, kept in order in a glyph list; the list
//! broken into lines no wider than its element; and the glyphs drawn into the interface's picture.

use std::sync::Arc;

use auberean_assets::ui::Font;

use crate::draw::Surface;
use crate::Rect;

/// A color as four channels from 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    /// The color as blue, green, red, alpha bytes, each channel scaled to 255 and truncated.
    #[must_use]
    // Truncation is the original's conversion; channels are kept within 0 to 1.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn bgra(self) -> [u8; 4] {
        let q = |v: f32| (v * 255.0) as u8;
        [q(self.b), q(self.g), q(self.r), q(self.a)]
    }

    fn from_bgra(p: [u8; 4]) -> Self {
        let f = |v: u8| f32::from(v) / 255.0;
        Self {
            r: f(p[2]),
            g: f(p[1]),
            b: f(p[0]),
            a: f(p[3]),
        }
    }
}

/// Where a line sits across its element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Justify {
    #[default]
    Left,
    Center,
    Right,
}

/// Where a line of `line` pixels starts across `avail`.
// D2-11748
#[must_use]
pub const fn justify(avail: i32, line: i32, j: Justify) -> i32 {
    match j {
        Justify::Left => 0,
        Justify::Center => avail / 2 - line / 2,
        Justify::Right => avail - line,
    }
}

// ---------------------------------------------------------------------------------------------
// Glyphs
// ---------------------------------------------------------------------------------------------

/// The new line character.
pub const NEW_LINE: u16 = 0x000A;

/// One character of a text: its font (by data id, 0 for none), its color and outline, and its
/// width and height in that font. Dropped, it lets its font go.
// D2-11700
#[derive(Clone, Debug)]
pub struct Glyph {
    pub ch: u16,
    pub width: i32,
    pub height: i32,
    pub font: u32,
    pub color: Color,
    pub outline: bool,
    /// The font's record, to draw with.
    pub(crate) face: Option<Arc<Font>>,
}

impl Glyph {
    /// A glyph of `ch` in `font`: a new line one pixel wide, any other character its width in
    /// the font (a question mark's when the font lacks it); every glyph the font's greatest
    /// height. Without a font it has no size.
    // D2-11699, D2-11698, D2-11704, D2-11701
    #[must_use]
    pub fn new(ch: u16, color: Color, font: Option<(u32, Arc<Font>)>, outline: bool) -> Self {
        let (font_id, face) = font.map_or((0, None), |(id, f)| (id, Some(f)));
        let (width, height) = face.as_ref().map_or((0, 0), |f| {
            let w = if ch == NEW_LINE {
                1
            } else {
                i32::try_from(f.char_width(ch)).unwrap_or(0)
            };
            (w, i32::try_from(f.max_height).unwrap_or(0))
        });
        Self {
            ch,
            width,
            height,
            font: font_id,
            color,
            outline,
            face,
        }
    }

    /// Tab, carriage return and space.
    // D2-11702
    #[must_use]
    pub const fn is_white_space(&self) -> bool {
        is_white_space(self.ch)
    }

    // D2-11703
    #[must_use]
    pub const fn is_new_line(&self) -> bool {
        self.ch == NEW_LINE
    }
}

/// Whether a font holds every character of a text up to its end (a new line needs no glyph); a
/// font without a map of its characters holds every one.
#[must_use]
pub fn font_contains(font: &Font, text: &[u16]) -> bool {
    let end = text.iter().position(|c| *c == 0).unwrap_or(text.len());
    font.contains_string(&text[..end])
}

// ---------------------------------------------------------------------------------------------
// Line breaking
// ---------------------------------------------------------------------------------------------

// D2-11697
const fn is_white_space(c: u16) -> bool {
    matches!(c, 0x0009 | 0x000D | 0x0020)
}

/// Characters a line never begins with: closing marks and stops.
// D2-11705
const fn is_non_beginning_char(c: u16) -> bool {
    matches!(
        c,
        0x0021
            | 0x0029
            | 0x002C
            | 0x002E
            | 0x003F
            | 0x3001
            | 0x3002
            | 0x30FC
            | 0xFF01
            | 0xFF09
            | 0xFF1F
            | 0xFF70
            | 0xFF9E
            | 0xFF9F
    )
}

/// Characters a line never ends with: opening brackets.
// D2-11706
const fn is_non_ending_char(c: u16) -> bool {
    matches!(c, 0x0028 | 0xFF08)
}

// D2-11707
const fn is_east_asian_char(c: u16) -> bool {
    matches!(c, 0x1100..=0x11FF | 0x3000..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFDC)
}

/// Whether a line may break before `next`: always after a new line; otherwise only before white
/// space, after a hyphen or beside an East Asian character, and never before a closing mark or
/// after an opening one. (The white space starts the next line.)
// D2-11708
const fn can_break_line_at(prev: Option<u16>, next: u16) -> bool {
    let Some(p) = prev else { return false };
    if p == NEW_LINE {
        return true;
    }
    if !is_white_space(next) && p != 0x002D && !is_east_asian_char(next) && !is_east_asian_char(p) {
        return false;
    }
    !is_non_beginning_char(next) && !is_non_ending_char(p)
}

// ---------------------------------------------------------------------------------------------
// The glyph list
// ---------------------------------------------------------------------------------------------

/// One line of a glyph list: its first glyph, its width and its height.
// D2-11718
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub start: usize,
    pub width: i32,
    pub height: i32,
}

/// A text's glyphs in order, the lines they last broke into, and how many glyphs it holds at most
/// (dropping the oldest to make room, when it trims from the top). Dropped with its glyphs and
/// lines.
// D2-11733, D2-11731
#[derive(Clone, Debug)]
pub struct GlyphRun {
    pub glyphs: Vec<Glyph>,
    pub lines: Vec<Line>,
    pub max_chars: i32,
    pub trim_from_top: bool,
}

impl Default for GlyphRun {
    // D2-11732
    fn default() -> Self {
        Self {
            glyphs: Vec::new(),
            lines: Vec::new(),
            max_chars: 0xFFFF,
            trim_from_top: false,
        }
    }
}

fn idx(v: i32) -> usize {
    usize::try_from(v).unwrap_or(0)
}

fn int(v: usize) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

impl GlyphRun {
    /// How many glyphs it holds.
    #[must_use]
    pub fn count(&self) -> i32 {
        int(self.glyphs.len())
    }

    /// The line holding position `pos` (a position is a place between glyphs, 0 to the count) and
    /// that line's first glyph; none for a position outside the text. With no lines yet, line 0
    /// at 0.
    // D2-11711
    #[must_use]
    pub fn find_current_line(&self, pos: i32) -> Option<(i32, i32)> {
        if pos < 0 || pos > self.count() {
            return None;
        }
        let n = self.lines.len();
        if n == 0 {
            return Some((0, 0));
        }
        let mut i = 0;
        while i != n - 1 && int(self.lines[i + 1].start) <= pos {
            i += 1;
        }
        Some((int(i), int(self.lines[i].start)))
    }

    /// The line `y` pixels down: the first whose foot is at or below `y`; the last when `y` is
    /// below every line; line 0 above the first.
    // D2-11712
    #[must_use]
    pub fn find_line_from_y(&self, y: i32) -> i32 {
        if y < 1 {
            return 0;
        }
        let mut total = 0;
        for (i, l) in self.lines.iter().enumerate() {
            total += l.height;
            if y <= total {
                return int(i);
            }
        }
        (int(self.lines.len()) - 1).max(0)
    }

    /// The line after the one holding `pos` (the last stays the last) and its first glyph.
    // D2-11713
    #[must_use]
    pub fn find_next_line(&self, pos: i32) -> Option<(i32, i32)> {
        let (line, _) = self.find_current_line(pos)?;
        let next = (line + 1).min(int(self.lines.len()) - 1).max(0);
        Some((next, self.line_start(next)))
    }

    /// The line before the one holding `pos` (the first stays the first) and its first glyph.
    // D2-11714
    #[must_use]
    pub fn find_prev_line(&self, pos: i32) -> Option<(i32, i32)> {
        let (line, _) = self.find_current_line(pos)?;
        let prev = (line - 1).max(0);
        Some((prev, self.line_start(prev)))
    }

    fn line_start(&self, line: i32) -> i32 {
        self.lines.get(idx(line)).map_or(0, |l| int(l.start))
    }

    /// A line's height.
    // D2-11715
    #[must_use]
    pub fn line_height(&self, line: i32) -> Option<i32> {
        if line < 0 {
            return None;
        }
        self.lines.get(idx(line)).map(|l| l.height)
    }

    /// A line's width.
    // D2-11716
    #[must_use]
    pub fn line_width(&self, line: i32) -> Option<i32> {
        if line < 0 {
            return None;
        }
        self.lines.get(idx(line)).map(|l| l.width)
    }

    /// The position after the white space that follows `pos` (the next word's start, or the
    /// end).
    // D2-11719
    #[must_use]
    pub fn find_next_word(&self, pos: i32) -> Option<i32> {
        if pos < 0 || pos > self.count() {
            return None;
        }
        let mut at = pos;
        let mut seen_space = false;
        for g in self.glyphs.iter().skip(idx(pos)) {
            if g.is_white_space() {
                seen_space = true;
            } else if seen_space {
                return Some(at);
            }
            at += 1;
        }
        Some(at)
    }

    /// The start of the word before `pos` (0 when none).
    // D2-11720
    #[must_use]
    pub fn find_prev_word(&self, pos: i32) -> Option<i32> {
        if pos < 0 || pos > self.count() {
            return None;
        }
        let mut start = 0;
        let mut after_space = true;
        for (i, g) in self.glyphs.iter().enumerate().take(idx(pos)) {
            if g.is_white_space() {
                after_space = true;
            } else if after_space {
                after_space = false;
                start = int(i);
            }
        }
        Some(start)
    }

    /// How far `pos` lies across its line, in pixels.
    // D2-11721
    #[must_use]
    pub fn find_pixels_from_pos(&self, pos: i32) -> Option<i32> {
        if pos < 0 || pos > self.count() {
            return None;
        }
        let (_, start) = self.find_current_line(pos)?;
        Some(
            self.glyphs
                .iter()
                .take(idx(pos))
                .skip(idx(start))
                .map(|g| g.width)
                .sum(),
        )
    }

    /// The position on `line` nearest `pixels` across it: a glyph is passed once the pixels
    /// reach past its middle; past the end of the last line, the end.
    // D2-11722
    #[must_use]
    pub fn find_pos_from_line_and_pixels(&self, line: i32, pixels: i32) -> Option<i32> {
        if line < 0 || line >= int(self.lines.len()) {
            return None;
        }
        let mut pos = int(self.lines[idx(line)].start);
        let last = int(self.lines.len()) - 1;
        let next = (line + 1).min(last);
        let end = if next == line {
            self.count()
        } else {
            int(self.lines[idx(next)].start)
        };
        let mut left = pixels;
        for (i, g) in self.glyphs.iter().enumerate().take(idx(end)) {
            let i = int(i);
            if pos <= i {
                #[allow(clippy::cast_precision_loss)]
                let half = g.width as f32 * 0.5;
                #[allow(clippy::cast_precision_loss)]
                let past = (left as f32) <= half;
                if past || i + 1 == end {
                    left -= g.width;
                    break;
                }
                left -= g.width;
                pos += 1;
            }
        }
        if left > 0 && end == self.count() {
            pos = self.count();
        }
        Some(pos)
    }

    /// Where `pos` is: its pixels across its line, and the heights of the lines above it.
    // D2-11726
    #[must_use]
    pub fn find_xy_from_position(&self, pos: i32) -> Option<(i32, i32)> {
        let (line, _) = self.find_current_line(pos)?;
        let y = self.lines.iter().take(idx(line)).map(|l| l.height).sum();
        Some((self.find_pixels_from_pos(pos)?, y))
    }

    /// Drop every glyph and line.
    // D2-11728
    pub fn flush(&mut self) {
        self.glyphs.clear();
        self.lines.clear();
    }

    /// Drop the glyphs from `from` up to `to`; returns how many went.
    // D2-11729
    pub fn delete(&mut self, from: i32, to: i32) -> i32 {
        let count = self.count();
        let from = from.max(0).min(count - 1);
        let to = to.max(0).min(count);
        if to <= from {
            return 0;
        }
        self.glyphs.drain(idx(from)..idx(to));
        to - from
    }

    /// Add `text` at `pos`, each character a glyph in `font`, `color` and `outline`. A full list
    /// drops its first glyph for each new one when it trims from the top, and otherwise takes no
    /// more. Returns how far the position moved: the glyphs added less the glyphs dropped.
    // D2-11730
    pub fn add_text(
        &mut self,
        pos: i32,
        text: &[u16],
        color: Color,
        font: Option<(u32, Arc<Font>)>,
        outline: bool,
    ) -> i32 {
        let (mut at, mut added, mut dropped) = (pos, 0, 0);
        for &ch in text {
            if self.count() >= self.max_chars {
                if !self.trim_from_top {
                    continue;
                }
                let gone = self.delete(0, 1);
                dropped -= gone;
                at -= gone;
            }
            let g = Glyph::new(ch, color, font.clone(), outline);
            if at >= 0 && at <= self.count() {
                self.glyphs.insert(idx(at), g);
                added += 1;
                at += 1;
            }
        }
        added + dropped
    }

    /// The characters from `from` up to `to`.
    // D2-11734
    #[must_use]
    pub fn inq_text_range(&self, from: i32, to: i32) -> Vec<u16> {
        let count = self.count();
        let from = from.max(0).min(count - 1);
        let to = to.max(0).min(count);
        self.glyphs
            .iter()
            .enumerate()
            .take(idx(to))
            .filter(|(i, _)| int(*i) >= from)
            .map(|(_, g)| g.ch)
            .collect()
    }

    /// Every character.
    // D2-11737
    #[must_use]
    pub fn inq_text(&self) -> Vec<u16> {
        self.inq_text_range(0, self.count())
    }

    /// Break the glyphs into lines no wider than `wrap` (unless one line is asked for); returns
    /// the widest line's width and the lines' total height.
    // D2-11735
    pub fn recalculate(&mut self, wrap: i32, one_line: bool) -> (i32, i32) {
        let (lines, w, h) = lay_out(&self.glyphs, wrap, one_line);
        self.lines = lines;
        (w, h)
    }
}

/// Break glyphs into lines no wider than `wrap` (unless one line is asked for); returns the
/// lines, the widest line's width and the lines' total height.
pub(crate) fn lay_out(gs: &[Glyph], wrap: i32, one_line: bool) -> (Vec<Line>, i32, i32) {
    let mut lines = Vec::new();
    let (mut paper_w, mut paper_h) = (0, 0);
    let mut prev: Option<u16> = None;
    let mut can_break = false;
    let (mut line_start, mut break_at) = (0usize, 0usize);
    let (mut width, mut since_break) = (0, 0);
    let (mut height, mut height_at_break) = (0, 0);
    for (i, g) in gs.iter().enumerate() {
        if !one_line && can_break_line_at(prev, g.ch) {
            break_at = i;
            height_at_break = height;
            since_break = 0;
            can_break = true;
        }
        width += g.width;
        since_break += g.width;
        if (can_break && wrap < width) || prev == Some(NEW_LINE) {
            let w = width - since_break;
            lines.push(Line {
                start: line_start,
                width: w,
                height: height_at_break,
            });
            paper_w = paper_w.max(w);
            paper_h += height_at_break;
            height = 0;
            line_start = break_at;
            width = since_break;
            can_break = false;
        }
        height = height.max(g.height);
        prev = Some(g.ch);
    }
    lines.push(Line {
        start: line_start,
        width,
        height,
    });
    paper_w = paper_w.max(width);
    paper_h += height;
    (lines, paper_w, paper_h)
}

/// The lines `text` breaks into in `font` at most `wrap` pixels wide: each line's first character
/// (in UTF-16 units) and width.
#[must_use]
pub fn measure(font: &Font, text: &str, wrap: i32, one_line: bool) -> Vec<Line> {
    let face = Arc::new(font.clone());
    let gs: Vec<Glyph> = text
        .encode_utf16()
        .map(|c| Glyph::new(c, Color::WHITE, Some((1, Arc::clone(&face))), false))
        .collect();
    lay_out(&gs, wrap, one_line).0
}

// ---------------------------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------------------------

/// Draw one glyph with its top left at `(x, y)` (a character the font lacks drawn as its question
/// mark): every column but the last of its cell, each covered texel blended toward `color` by its
/// coverage (or set outright when `blend` is off or the coverage is full); a font of one bit a
/// texel sets its texels. Returns how far the next glyph starts.
// D2-09958
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_glyph(
    s: &mut Surface,
    font: &Font,
    ch: u16,
    x: i32,
    y: i32,
    clip: Rect,
    color: Color,
    blend: bool,
) -> i32 {
    let Some(fg) = font.char_desc(ch) else {
        return 0;
    };
    let width = i32::try_from(fg.width).unwrap_or(0);
    let offset = fg.offset;
    let rows = i32::try_from(font.max_height).unwrap_or(0);
    let stride = i32::try_from(font.total_width).unwrap_or(0);
    let packed = color.bgra();
    // The rows and columns of the glyph that lie inside the clip.
    let (first_row, last_row) = (0.max(clip.y0 - y), (rows - 1).min(clip.y1 - y));
    let (first_col, last_col) = (0.max(clip.x0 - x), (width - 2).min(clip.x1 - x));
    for row in first_row..=last_row {
        let py = y + row;
        let row_at = i64::from(row) * i64::from(stride);
        for col in first_col..=last_col {
            let px = x + col;
            let column = i64::from(offset) + i64::from(col);
            let coverage = if font.antialiased {
                usize::try_from(row_at + column)
                    .ok()
                    .and_then(|i| font.pixels.get(i))
                    .copied()
                    .unwrap_or(0)
            } else {
                let byte = usize::try_from(i64::from(row) * i64::from(stride / 8) + column / 8)
                    .ok()
                    .and_then(|i| font.pixels.get(i))
                    .copied()
                    .unwrap_or(0);
                if (byte >> (7 - (column & 7))) & 1 != 0 {
                    0xFF
                } else {
                    0
                }
            };
            if coverage == 0 {
                continue;
            }
            if !blend || coverage == 0xFF || !font.antialiased {
                s.put(px, py, packed);
                continue;
            }
            let a = f32::from(coverage) * 0.003_921_569;
            let d = Color::from_bgra(s.get(px, py));
            let mut c = Color {
                r: color.r * a + d.r * (1.0 - a),
                g: color.g * a + d.g * (1.0 - a),
                b: color.b * a + d.b * (1.0 - a),
                a: color.a * a * 1.25 + d.a,
            };
            c.r = c.r.clamp(0.0, 1.0);
            c.g = c.g.clamp(0.0, 1.0);
            c.b = c.b.clamp(0.0, 1.0);
            c.a = c.a.clamp(0.0, 1.0);
            s.put(px, py, c.bgra());
        }
    }
    width
}

/// Turn over the color bits of the texels from `(x0, y0)` to `(x1, y1)` inside `clip` (a
/// selection shown).
pub(crate) fn not_color_bits(s: &mut Surface, from: (i32, i32), to: (i32, i32), clip: Rect) {
    for y in from.1..=to.1 {
        for x in from.0..=to.0 {
            if clip.contains(x, y) {
                let p = s.get(x, y);
                s.put(x, y, [!p[0], !p[1], !p[2], p[3]]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (line layout and glyph list arithmetic on synthetic glyphs)
    use super::*;

    fn g(ch: char, width: i32) -> Glyph {
        Glyph {
            ch: ch as u16,
            width,
            height: 10,
            font: 0,
            color: Color::WHITE,
            outline: false,
            face: None,
        }
    }

    fn list(text: &str, width: i32) -> GlyphRun {
        GlyphRun {
            glyphs: text.chars().map(|c| g(c, width)).collect(),
            ..GlyphRun::default()
        }
    }

    #[test]
    fn words_wrap_before_the_space_and_the_space_starts_the_next_line() {
        let text: Vec<Glyph> = "ab cd".chars().map(|c| g(c, 5)).collect();
        let (lines, w, h) = lay_out(&text, 18, false);
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[0].start, lines[0].width), (0, 10));
        assert_eq!((lines[1].start, lines[1].width), (2, 15));
        assert_eq!((w, h), (15, 20));
    }

    #[test]
    fn one_line_text_never_wraps() {
        let text: Vec<Glyph> = "ab cd".chars().map(|c| g(c, 5)).collect();
        let (lines, w, _) = lay_out(&text, 18, true);
        assert_eq!(lines.len(), 1);
        assert_eq!(w, 25);
    }

    #[test]
    fn centered_lines_split_the_spare_width() {
        assert_eq!(justify(100, 41, Justify::Center), 30);
        assert_eq!(justify(100, 41, Justify::Right), 59);
    }

    #[test]
    fn a_full_list_trims_from_the_top_or_takes_no_more() {
        let mut l = list("abc", 5);
        l.max_chars = 3;
        let moved = l.add_text(3, &[u16::from(b'd')], Color::WHITE, None, false);
        assert_eq!((moved, l.count()), (0, 3), "no room: nothing taken");
        l.trim_from_top = true;
        let moved = l.add_text(3, &[u16::from(b'd')], Color::WHITE, None, false);
        assert_eq!(moved, 0, "one added, one dropped");
        assert_eq!(String::from_utf16_lossy(&l.inq_text()), "bcd");
    }

    #[test]
    fn positions_map_to_lines_pixels_and_words() {
        let mut l = list("ab cd", 5);
        l.recalculate(18, false);
        assert_eq!(l.find_current_line(3), Some((1, 2)));
        assert_eq!(l.find_pixels_from_pos(4), Some(10));
        assert_eq!(l.find_xy_from_position(4), Some((10, 10)));
        assert_eq!(l.find_next_word(0), Some(3));
        assert_eq!(l.find_prev_word(5), Some(3));
        assert_eq!(l.find_line_from_y(15), 1);
        assert_eq!(l.find_pos_from_line_and_pixels(1, 7), Some(3));
        assert_eq!(l.find_pos_from_line_and_pixels(1, 99), Some(5));
    }
}
