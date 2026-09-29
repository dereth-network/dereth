//! `Glyph` and `GlyphList` — the text model.
//!
//! Because colour, font and tag are **per glyph**, one text element can mix fonts, colours and
//! links freely — which is exactly what the chat window, item appraisals and the journal need. A
//! paragraph-level styling model cannot reproduce them.

use std::sync::Arc;

use crate::text::linebreak::{can_break_line_at, is_new_line};
use crate::text::tag::TextTag;

/// What the glyph list needs from a font. The renderer owns the atlases, the rasterisation and the
/// pixel rules; the only thing layout needs is the advance, and the shipped bitmap fonts' exact
/// advances are what every line break in the interface depends on.
pub trait FontMetrics: std::fmt::Debug {
    /// Return this character's advance width.
    fn advance(&self, ch: u16) -> i32;
    /// The line box height for this font.
    fn height(&self) -> i32;
}

/// A fixed-advance font, for tests and for the debug console.
#[derive(Debug, Clone, Copy)]
pub struct FixedMetrics {
    pub advance: i32,
    pub height: i32,
}

impl FontMetrics for FixedMetrics {
    fn advance(&self, _ch: u16) -> i32 {
        self.advance
    }
    fn height(&self) -> i32 {
        self.height
    }
}

/// One measured UTF-16 code unit and its drawing attributes.
#[derive(Debug, Clone)]
pub struct Glyph {
    /// The UTF-16 code unit.
    pub data: u16,
    /// `width` / `height`, measured from the font.
    pub width: i32,
    pub height: i32,
    /// Packed glyph color.
    pub color: u32,
    /// Which font measured it. An index into the element's font set.
    pub font: u32,
    /// The glyph's tag — ref-counted in the original, shared here through an `Arc`.
    pub tag: Option<Arc<TextTag>>,
}

impl Glyph {
    #[must_use]
    pub fn new(data: u16, m: &dyn FontMetrics, color: u32, font: u32) -> Self {
        Self {
            data,
            width: m.advance(data),
            height: m.height(),
            color,
            font,
            tag: None,
        }
    }

    /// Whether this glyph is whitespace.
    #[must_use]
    pub const fn is_white_space(&self) -> bool {
        crate::text::linebreak::is_white_space(self.data)
    }

    /// Whether this glyph is a newline.
    #[must_use]
    pub const fn is_new_line(&self) -> bool {
        is_new_line(self.data)
    }
}

/// One entry of [`GlyphList::lines`], rebuilt by the wrap pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphLine {
    /// First glyph index on the line.
    pub start: usize,
    /// One past the last glyph index.
    pub end: usize,
    pub width: i32,
    pub height: i32,
}

/// A sequence of measured glyphs.
#[derive(Debug, Default, Clone)]
pub struct GlyphList {
    /// The glyphs, in text order.
    pub glyphs: Vec<Glyph>,
    /// The wrapped lines.
    pub lines: Vec<GlyphLine>,
    /// The character limit (attribute 0x1E); 0 = unlimited.
    pub max_characters: usize,
    /// Trim-from-top (attribute 0x28): when the limit is hit, drop from the **front** — this is
    /// how the chat scrollback stays bounded.
    pub trim_from_top: bool,
    /// The width the lines were last wrapped to.
    pub last_recalc_width: i32,
    /// Whether the element is one-line (no wrapping).
    pub one_line: bool,
}

impl GlyphList {
    #[must_use]
    pub fn len(&self) -> usize {
        self.glyphs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    /// Flush the list.
    pub fn flush(&mut self) {
        self.glyphs.clear();
        self.lines.clear();
    }

    /// Change the font — **re-measure every glyph against a new font, in
    /// place.**
    ///
    /// The client clears the line table, then assigns the new font to every glyph in list order.
    ///
    /// **It is not re-setting the text from its tagged source**, which is what
    /// [`super::TextElement::do_font_reset`] does: that rebuilds the list from the tagged string
    /// and every glyph takes the element's single font colour. The chat log's glyphs each carry
    /// their *channel's* colour (the append-with-font's colour argument, and the chat colour table
    /// behind it), so rebuilding would repaint the whole backlog white. This keeps colour, font
    /// index and tag and touches only the two measurements, which is why the chat interface's
    /// font-settings-changed notice uses it.
    pub fn change_font(&mut self, m: &dyn FontMetrics) {
        self.lines.clear();
        for g in &mut self.glyphs {
            g.width = m.advance(g.data);
            g.height = m.height();
        }
    }

    /// Behavior: insert a string at a position, applying font, colour and
    /// tag.
    ///
    /// Honours the character limit: with trim-from-top the oldest glyphs are dropped, otherwise
    /// the surplus is refused.
    pub fn add_text(
        &mut self,
        at: usize,
        s: &str,
        m: &dyn FontMetrics,
        color: u32,
        font: u32,
        tag: Option<&Arc<TextTag>>,
    ) {
        self.add_units(at, s.encode_utf16(), m, color, font, tag);
    }

    /// The same insertion for a native UTF-16 character-handler code unit. Do not convert a
    /// surrogate independently through a Rust string: two successive units form one character.
    pub(crate) fn add_units(
        &mut self,
        at: usize,
        units: impl IntoIterator<Item = u16>,
        m: &dyn FontMetrics,
        color: u32,
        font: u32,
        tag: Option<&Arc<TextTag>>,
    ) {
        let at = at.min(self.glyphs.len());
        let mut new: Vec<Glyph> = Vec::new();
        for u in units {
            let mut g = Glyph::new(u, m, color, font);
            g.tag = tag.cloned();
            new.push(g);
        }
        if self.max_characters > 0 && !self.trim_from_top {
            let room = self.max_characters.saturating_sub(self.glyphs.len());
            new.truncate(room);
        }
        let n = new.len();
        self.glyphs.splice(at..at, new);
        if self.max_characters > 0 && self.trim_from_top && self.glyphs.len() > self.max_characters
        {
            self.behead(self.glyphs.len() - self.max_characters);
        }
        if n > 0 {
            self.lines.clear();
        }
    }

    /// Behavior: drop `n` glyphs from the front.
    pub fn behead(&mut self, n: usize) {
        let n = n.min(self.glyphs.len());
        self.glyphs.drain(..n);
        self.lines.clear();
    }

    /// Delete a half-open range.
    pub fn delete(&mut self, start: usize, end: usize) {
        let start = start.min(self.glyphs.len());
        let end = end.min(self.glyphs.len()).max(start);
        self.glyphs.drain(start..end);
        self.lines.clear();
    }

    /// Reconstruct the string.
    ///
    /// `with_tags` re-emits the markup; the clipboard copy uses the **without**-tags
    /// form, so links are copied as their display text.
    #[must_use]
    pub fn inq_text(&self, with_tags: bool) -> String {
        if !with_tags {
            return String::from_utf16_lossy(
                &self.glyphs.iter().map(|g| g.data).collect::<Vec<_>>(),
            );
        }
        let mut out = String::new();
        let mut open: Option<Arc<TextTag>> = None;
        for g in &self.glyphs {
            let same = match (&open, &g.tag) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            };
            if !same {
                if let Some(t) = &open {
                    out.push_str(&crate::text::tag::build_end_tag(t));
                }
                if let Some(t) = &g.tag {
                    out.push_str(&crate::text::tag::build_start_tag(t));
                }
                open = g.tag.clone();
            }
            if let Some(c) = char::from_u32(u32::from(g.data)) {
                out.push(c);
            }
        }
        if let Some(t) = &open {
            out.push_str(&crate::text::tag::build_end_tag(t));
        }
        out
    }

    /// Behavior: the word-wrap pass; rebuilds [`Self::lines`].
    ///
    /// A break is taken at the last position the line-break test allows before the width runs
    /// out; if no such position exists the line breaks hard at the overflowing glyph, because a
    /// word longer than the box still has to go somewhere.
    pub fn recalculate(&mut self, width: i32) {
        self.last_recalc_width = width;
        self.lines = wrap(&self.glyphs, width, self.one_line);
    }

    /// Returns which line contains a glyph index.
    #[must_use]
    pub fn find_current_line(&self, pos: usize) -> usize {
        for (i, l) in self.lines.iter().enumerate() {
            if pos < l.end {
                return i;
            }
        }
        self.lines.len().saturating_sub(1)
    }

    /// Behavior: one wrapped line's height.
    ///
    /// `None` when the line index is past the end, which is the client's own `false` return and
    /// what stops the scroll-delta query from stepping an empty element.
    #[must_use]
    pub fn glyph_line_height(&self, line: usize) -> Option<i32> {
        self.lines.get(line).map(|l| l.height)
    }

    /// The width of one laid-out line.
    #[must_use]
    pub fn glyph_line_width(&self, line: usize) -> Option<i32> {
        self.lines.get(line).map(|l| l.width)
    }

    /// Find the complete line from a y coordinate — the last line that fits **entirely** above
    /// `y`, as retail computes it.
    ///
    /// The running total starts at line 0's height; from line 1 on each line's height is added,
    /// and the first time the total exceeds `y` the answer is the index *before* that line. When
    /// no line exceeds it the answer is the line count — **not** the count minus one.
    ///
    /// **The fall-through is retail's off-by-one and it is transcribed, not fixed.** When every
    /// line fits, the client hands back the line count and the truncation recalculation then loops
    /// `for (i = 0; i <= last_line; ++i)`, i.e. one line past the end of the array. This build
    /// reaches it when the truncate bit is armed, so the *loop* is written to stop at the end of the array while the *returned index*
    /// stays `len()`. Changing the return would be inventing a client we do not have.
    #[must_use]
    pub fn find_complete_line_from_y(&self, y: i32) -> Option<usize> {
        let num = self.lines.len();
        if num == 0 {
            return Some(0);
        }
        let mut acc = self.lines[0].height;
        let mut i = 1usize;
        while i < num {
            acc += self.lines[i].height;
            if acc > y {
                return Some(i - 1);
            }
            i += 1;
        }
        Some(num)
    }

    /// The glyph index `pixels` into a line, in full — four arguments and both
    /// arms.
    ///
    /// A line past the end fails. When `pixels` exceeds the line's width the whole line fits, and
    /// the answer is the end of the list on the last line and the next line's start minus one
    /// (dropping the break glyph) on any other. Otherwise the walk goes glyph by glyph from the
    /// line start, up to the line end: it stops when the remaining budget is no more than the
    /// glyph's width (halved when rounding) or at a newline that is the line's last glyph, and
    /// otherwise subtracts the glyph's full width and moves on.
    ///
    /// The **`round` argument is the whole difference between the callers.** The caret's
    /// up/down movement and the click-to-position lookup both pass `true`, which halves each
    /// glyph so a click lands on the nearer edge; the truncation passes `false`, which means
    /// *"how many whole glyphs fit"*. [`Self::find_pos_from_line_and_pixels`] is the first with
    /// its own `None` folded into the end of the list, and it delegates here.
    ///
    /// `pixels` is compared **unsigned** in retail, so a negative budget — a box narrower than
    /// its own margins plus the trailer — reads as enormous there and takes the whole-line arm.
    /// Clamped to 0 here and stated rather than reproduced: a caller asking to fit text into a
    /// negative width is a layout bug, not a rendering rule.
    #[must_use]
    pub fn find_pos_from_line_and_pixels_rounded(
        &self,
        line: usize,
        pixels: i32,
        round: bool,
    ) -> Option<usize> {
        let l = *self.lines.get(line)?;
        let next = self.lines.len().saturating_sub(1).min(line + 1);
        let end = if next == line {
            self.glyphs.len()
        } else {
            self.lines[next].start
        };
        let pixels = pixels.max(0);
        if pixels > l.width {
            // The whole line fits: the answer is the end of the line, and for every line but the
            // last that is `next_start - 1` — *"everything up to the break, minus the break
            // character"*, the trailing space the wrap pass left there.
            if line + 1 >= self.lines.len() {
                return Some(self.glyphs.len());
            }
            return Some(self.lines[line + 1].start.saturating_sub(1));
        }
        let mut pos = l.start;
        let mut left = pixels;
        while pos < end {
            let Some(g) = self.glyphs.get(pos) else { break };
            let step = if round { g.width / 2 } else { g.width };
            if left <= step {
                break;
            }
            if g.is_new_line() && pos + 1 == end {
                break;
            }
            left -= g.width;
            pos += 1;
        }
        Some(pos)
    }

    /// Behavior: where one glyph sits inside the wrapped text,
    /// in the text's own coordinates (before any scroll offset or justification).
    ///
    /// `None` when the list is empty, which is the in-view test's early-out.
    #[must_use]
    pub fn find_xy_from_position(&self, pos: usize) -> Option<(i32, i32)> {
        if self.lines.is_empty() {
            return None;
        }
        let line = self.find_current_line(pos);
        let y = self.lines[..line].iter().map(|l| l.height).sum();
        Some((self.find_pixels_from_pos(pos), y))
    }

    /// Behavior: the x offset of a glyph index within its line.
    #[must_use]
    pub fn find_pixels_from_pos(&self, pos: usize) -> i32 {
        let Some(l) = self.lines.get(self.find_current_line(pos)) else {
            return 0;
        };
        self.glyphs[l.start..pos.min(l.end)]
            .iter()
            .map(|g| g.width)
            .sum()
    }

    /// The line-and-pixels search with `round = true` — the glyph index
    /// **nearest** an x offset, which is what the caret wants. A line index past the end answers
    /// the end of the list, which is the client's `false` return folded into the caret's own
    /// clamp.
    ///
    /// This calls [`Self::find_pos_from_line_and_pixels_rounded`] rather than keeping a second
    /// copy of the scan, so the two cannot disagree about the *whole-line-fits* arm.
    #[must_use]
    pub fn find_pos_from_line_and_pixels(&self, line: usize, x: i32) -> usize {
        self.find_pos_from_line_and_pixels_rounded(line, x, true)
            .unwrap_or(self.glyphs.len())
    }

    /// Find the next word boundary.
    #[must_use]
    pub fn find_next_word(&self, pos: usize) -> usize {
        let n = self.glyphs.len();
        let mut i = pos;
        while i < n && !self.glyphs[i].is_white_space() {
            i += 1;
        }
        while i < n && self.glyphs[i].is_white_space() {
            i += 1;
        }
        i
    }

    /// Find the previous word boundary.
    #[must_use]
    pub fn find_prev_word(&self, pos: usize) -> usize {
        let mut i = pos;
        while i > 0 && self.glyphs[i - 1].is_white_space() {
            i -= 1;
        }
        while i > 0 && !self.glyphs[i - 1].is_white_space() {
            i -= 1;
        }
        i
    }

    /// The whole text's extent, for (attribute 0x29, fit to text).
    #[must_use]
    pub fn extent(&self) -> (i32, i32) {
        let w = self.lines.iter().map(|l| l.width).max().unwrap_or(0);
        let h = self.lines.iter().map(|l| l.height).sum();
        (w, h)
    }
}

/// The client's wrap loop, as a function of the glyphs alone.
///
/// Split out of the glyph-list recalculation so that composition can lay a list out against a width
/// it does not own without mutating it: the client re-flows into its line table because it composes
/// into a surface it also owns, and a draw pass that borrows the element immutably needs the same
/// answer without the write. The two share this one implementation, so they cannot drift.
#[must_use]
pub fn wrap(glyphs: &[Glyph], width: i32, one_line: bool) -> Vec<GlyphLine> {
    let mut lines = Vec::new();
    if glyphs.is_empty() {
        // The recalculation always emits its ordinary tail, even when the glyph list is
        // empty. Its four accumulators are all still zero in that case.
        lines.push(GlyphLine {
            start: 0,
            end: 0,
            width: 0,
            height: 0,
        });
        return lines;
    }
    if one_line {
        let w: i32 = glyphs.iter().map(|g| g.width).sum();
        let h = glyphs.iter().map(|g| g.height).max().unwrap_or(0);
        lines.push(GlyphLine {
            start: 0,
            end: glyphs.len(),
            width: w,
            height: h,
        });
        if let Some(last) = glyphs.last().filter(|g| g.is_new_line()) {
            // The final-newline arm is after the one-line/wrap loop in retail too.
            lines.push(GlyphLine {
                start: glyphs.len(),
                end: glyphs.len(),
                width: 0,
                height: last.height,
            });
        }
        return lines;
    }
    let n = glyphs.len();
    let mut start = 0;
    while start < n {
        let mut x = 0;
        let mut last_break: Option<usize> = None;
        let mut i = start;
        let mut hard = None;
        while i < n {
            if glyphs[i].is_new_line() {
                hard = Some(i + 1);
                break;
            }
            let w = glyphs[i].width;
            // The wrap test asks whether the current glyph is whitespace or a newline and then
            // subtracts the current glyph's width: the width measured against the box is the
            // running total **less the current glyph when that glyph is whitespace**. A space that tips the
            // line over the edge therefore does not wrap -- it stays where it is and the break
            // is taken at the next glyph instead.
            let measured = if glyphs[i].is_white_space() { 0 } else { w };
            if width > 0 && x + measured > width && i > start {
                break;
            }
            x += w;
            i += 1;
            if i < n
                && can_break_line_at(Some((i - 1, glyphs[i - 1].data)), Some((i, glyphs[i].data)))
            {
                last_break = Some(i);
            }
        }
        let end = if let Some(e) = hard {
            e
        } else if i >= n {
            n
        } else {
            last_break.unwrap_or(i)
        };
        let end = end.max(start + 1).min(n);
        let mut w: i32 = glyphs[start..end].iter().map(|g| g.width).sum();
        // At a break, the client tests the glyph before the break index and subtracts its width
        // when it is whitespace or a newline.
        //
        // **The break character stays on the line and is not part of the line's width.** The
        // glyph is still there -- the *next* line's start is the index after it, so the space is
        // still drawn -- but the line's width excludes it, and that is
        // the number the justification works with. A right- or centre-justified
        // wrapped label is therefore flush, where counting the space leaves a gap of exactly the
        // space's advance. The **last** line is emitted by the ordinary tail, which stores the
        // running width unmodified, so a trailing space at the very end of the text *is* counted.
        if end < n {
            if let Some(g) = glyphs.get(end - 1) {
                if g.is_white_space() || g.is_new_line() {
                    w -= g.width;
                }
            }
        }
        let h = glyphs[start..end]
            .iter()
            .map(|g| g.height)
            .max()
            .unwrap_or(0);
        lines.push(GlyphLine {
            start,
            end,
            width: w,
            height: h,
        });
        start = end;
    }
    // After the ordinary tail, the client asks whether the last processed
    // glyph is a newline and, if so, appends an empty line at the glyph count. Its height is the
    // last glyph's own height, not zero or the tallest preceding glyph.
    if let Some(last) = glyphs.last().filter(|g| g.is_new_line()) {
        lines.push(GlyphLine {
            start: glyphs.len(),
            end: glyphs.len(),
            width: 0,
            height: last.height,
        });
    }
    lines
}

/// Calculate the text offset inside the available extent after subtracting both margins. Center
/// mode halves the remaining gap, far modes 3 and 5 use the whole gap, and other modes use zero;
/// the near margin is then added back.
///
/// 1 = centre, 3 or 5 = far (right/bottom), anything else = near (left/top).
#[must_use]
pub fn calc_justification(
    available: i32,
    text_extent: i32,
    near_margin: i32,
    far_margin: i32,
    justification: u32,
) -> i32 {
    let extent = available - far_margin - near_margin;
    let offset = match justification {
        // The centre arm halves the two extents **separately** and subtracts, which is not
        // `(extent - text) / 2` — the
        // two differ by a pixel whenever `extent` is even and `text_extent` odd. The halving is
        // an unsigned shift, not a signed one, which only matters with margins wider than the
        // box.
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        // LINT-OK: the two unsigned halvings above, as retail performs them.
        1 => (((extent as u32) >> 1) as i32) - (((text_extent as u32) >> 1) as i32),
        3 | 5 => extent - text_extent,
        _ => 0,
    };
    near_margin + offset
}

/// The justification's **available extent**: the larger of the
/// element's own box and the scrollable extent its wrapped text needs.
///
/// The client takes the maximum of the box height and measured scrollable height; the horizontal
/// arm does the same over measured scrollable width and the
/// box's width. The scrollable-area resize stores what the glyph-list recalculation
/// measured and **never clamps it to the box**, so the moment
/// the text is bigger than the box the available extent *becomes* the text extent and every
/// justification collapses to `near`: a block that does not fit is drawn from the top-left corner
/// and the overflow is clipped away by the element's own box.
///
/// That — not the truncation path, which needs attribute `0xC7` and which no
/// shipped overflowing label carries — is why retail's Squelch caption reads `Character` and its
/// rendering-quality row reads `Environment Texture`.
#[must_use]
pub fn justification_extent(box_extent: i32, text_extent: i32) -> i32 {
    box_extent.max(text_extent)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: FixedMetrics = FixedMetrics {
        advance: 10,
        height: 16,
    };

    fn list(s: &str, width: i32) -> GlyphList {
        let mut g = GlyphList::default();
        g.add_text(0, s, &M, 0xFFFF_FFFF, 0, None);
        g.recalculate(width);
        g
    }

    /// Oracle: the glyph list's recalculation driving the line-break test — Western text breaks
    /// only at whitespace.
    #[test]
    fn wrapping_breaks_after_whitespace_at_the_documented_width() {
        // 10px per glyph, 100px wide = ten glyphs per line.
        let g = list("hello world again", 100);
        let texts: Vec<String> = g
            .lines
            .iter()
            .map(|l| {
                String::from_utf16_lossy(
                    &g.glyphs[l.start..l.end]
                        .iter()
                        .map(|x| x.data)
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        assert_eq!(texts, vec!["hello ", "world ", "again"]);
    }

    /// Oracle: the same function — a word longer than the box has to go somewhere, so the line
    /// breaks hard at the overflowing glyph.
    #[test]
    fn a_word_longer_than_the_box_breaks_hard() {
        let g = list("abcdefghijklmno", 50);
        assert_eq!(g.lines.len(), 3);
        assert_eq!(
            g.lines[0],
            GlyphLine {
                start: 0,
                end: 5,
                width: 50,
                height: 16
            }
        );
    }

    /// An explicit newline ends the line.
    #[test]
    fn an_explicit_newline_ends_the_line() {
        let g = list("ab\ncd", 1000);
        assert_eq!(g.lines.len(), 2);
        assert_eq!(
            g.lines[0],
            GlyphLine {
                start: 0,
                end: 3,
                width: 20,
                height: 16
            }
        );
        assert_eq!(g.lines[1].start, 3);
    }

    /// Oracle: the recalculation always emits the ordinary tail, then
    /// adds a zero-width line at the glyph count only when the last glyph is a
    /// newline. The added line copies that glyph's height.
    #[test]
    fn the_final_tail_distinguishes_empty_ordinary_and_trailing_newline_text() {
        let empty = list("", 1000);
        assert_eq!(
            empty.lines,
            [GlyphLine {
                start: 0,
                end: 0,
                width: 0,
                height: 0
            }],
            "the unconditional ordinary tail exists for an empty list",
        );

        let ordinary = list("ab", 1000);
        assert_eq!(
            ordinary.lines.len(),
            1,
            "ordinary text gets no additional empty line"
        );
        assert_eq!(ordinary.lines[0].end, ordinary.glyphs.len());

        let trailing = list("ab\n", 1000);
        assert_eq!(
            trailing.lines.last().copied(),
            Some(GlyphLine {
                start: trailing.glyphs.len(),
                end: trailing.glyphs.len(),
                width: 0,
                height: M.height,
            }),
        );

        let short = FixedMetrics {
            advance: 10,
            height: 8,
        };
        let mut one_line = GlyphList {
            one_line: true,
            ..GlyphList::default()
        };
        one_line.add_text(0, "a", &M, 0, 0, None);
        one_line.add_text(1, "\n", &short, 0, 0, None);
        one_line.recalculate(1000);
        assert_eq!(
            one_line.lines.last().map(|line| line.height),
            Some(short.height),
            "the empty tail copies the newline glyph, not the tallest glyph on the line",
        );
    }

    /// Oracle: East Asian text breaks between any two ideographs.
    #[test]
    fn cjk_wraps_without_spaces() {
        let g = list("\u{4E00}\u{4E8C}\u{4E09}\u{56DB}", 20);
        assert_eq!(g.lines.len(), 2);
        assert_eq!(g.lines[0].end, 2);
    }

    /// Oracle: the character limit (0x1E) + trim-from-top (0x28) —
    /// "this is how the chat scrollback stays bounded ".
    #[test]
    fn the_length_limit_either_refuses_or_beheads() {
        let mut g = GlyphList {
            max_characters: 5,
            ..GlyphList::default()
        };
        g.add_text(0, "abcdefgh", &M, 0, 0, None);
        assert_eq!(
            g.inq_text(false),
            "abcde",
            "without trim_from_top the surplus is refused"
        );

        let mut g = GlyphList {
            max_characters: 5,
            trim_from_top: true,
            ..GlyphList::default()
        };
        g.add_text(0, "abcdefgh", &M, 0, 0, None);
        assert_eq!(g.inq_text(false), "defgh", "with it the oldest glyphs go");
    }

    /// Oracle: transcribed in §2.4. "1 = centre,
    /// 3 or 5 = far, anything else = near."
    #[test]
    fn justification_matches_the_transcribed_formula() {
        assert_eq!(calc_justification(100, 40, 5, 5, 0), 5, "near");
        assert_eq!(calc_justification(100, 40, 5, 5, 1), 5 + 25, "centre");
        assert_eq!(calc_justification(100, 40, 5, 5, 3), 5 + 50, "far");
        assert_eq!(
            calc_justification(100, 40, 5, 5, 5),
            5 + 50,
            "far, the other value"
        );
        assert_eq!(
            calc_justification(100, 40, 5, 5, 2),
            5,
            "anything else is near"
        );
    }

    /// Oracle: text extraction without tags copies links
    /// as their display text".
    #[test]
    fn inq_text_can_re_emit_or_drop_the_markup() {
        let tag = std::sync::Arc::new(crate::text::tag::TextTag {
            kind: crate::text::tag::TagKind::Iid,
            type_keyword: "IID".into(),
            format: "Name".into(),
            data: "0x50001234".into(),
        });
        let mut g = GlyphList::default();
        g.add_text(0, "hi ", &M, 0, 0, None);
        g.add_text(3, "Bob", &M, 0, 0, Some(&tag));
        assert_eq!(g.inq_text(false), "hi Bob");
        assert_eq!(g.inq_text(true), "hi <IID:Name:0x50001234>Bob<\\IID>");
    }

    /// Oracle: the Ctrl+arrow navigation.
    #[test]
    fn word_navigation_skips_the_run_then_the_spaces() {
        let g = list("one two  three", 1000);
        assert_eq!(g.find_next_word(0), 4);
        assert_eq!(g.find_next_word(4), 9);
        assert_eq!(g.find_prev_word(9), 4);
        assert_eq!(g.find_prev_word(4), 0);
        assert_eq!(g.find_prev_word(0), 0);
    }
}
