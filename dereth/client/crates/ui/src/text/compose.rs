//! Composition: turning a laid-out `GlyphList` into positioned glyphs, and the two services the
//! text element needs from outside this crate.
//!
//! This module applies the client's exact glyph,
//! selection, and caret positioning rules.
//!
//! ## Why this is a separate step
//!
//! In the client the text element **composes into its own UI surface** with
//! the surface's character blit, one glyph at a time, and the surface is then
//! drawn as a single quad. A rebuild that keeps a CPU surface per
//! element would reproduce that literally; this one emits the same glyph *placements* as draw data
//! and lets the renderer's atlas path rasterise them. The two agree pixel for pixel because both take
//! the placement from the same three rules:
//!
//! 1. **Advance** = `HorizontalOffsetBefore + Width + HorizontalOffsetAfter`, accumulated in
//!    integer pixels, with no kerning and no sub-pixel positioning
//!    (the font's character-width query);
//! 2. **Glyph placement** = pen + `HorizontalOffsetBefore` horizontally, line top +
//!    `VerticalOffsetBefore` vertically — the bearings are added by whoever holds the font, which
//!    is why a [`PlacedGlyph`] carries the *pen* position and not the glyph's corner;
//! 3. **Line height** = the maximum character height over the glyphs on the line.
//!
//! Justification is the text element's own, already transcribed as
//! [`crate::text::calc_justification`].

use dereth_primitives::DataId;

use crate::region::Box2D;
use crate::text::glyph::{calc_justification, justification_extent, Glyph, GlyphLine};

/// One glyph, placed. The position is the **pen**, i.e. the top-left of the glyph's line cell
/// before `HorizontalOffsetBefore` / `VerticalOffsetBefore` are added — exactly the `(x, y)`
/// passed to the character draw operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedGlyph {
    /// Absolute screen x of the pen.
    pub x: i32,
    /// Absolute screen y of the line top.
    pub y: i32,
    /// The UTF-16 code unit. The font falls back to `'?'` for a missing glyph.
    pub ch: u16,
    /// The glyph color packed as ARGB.
    pub color: u32,
    /// Which `Font` measured and draws it. The client stores a font pointer; here it is
    /// the DataID of the same object.
    pub font: DataId,
}

/// What a text element needs from the host to turn a `StringInfo` into characters.
///
/// Text set or appended from a `StringInfo` (attribute 0x17)
/// is resolved through the string tables **at the moment it is set**, using the
/// current language. The tables live in `client_local_<Language>.dat`, decoded by the asset crate, and this
/// crate has no asset source, so the host installs one of these.
///
/// # The escape pass is the contract's, not the implementor's
///
/// A shipped row is stored **escaped** — a line break is a literal backslash and an `n` — and
/// every retail path out of a string table ends in the meta-language's unescape: the string
/// info's query, its literal-value getter and the meta-language's render
/// alike. The unescape therefore belongs to the *lookup*, and an implementor that forgets it
/// composes a screen the client does not draw.
///
/// So the two methods an implementor writes are the **raw** ones, which hand back the row exactly
/// as the dat stores it, and the two a caller uses are provided here and run
/// [`crate::text::unescape`] over the answer. **Do not override [`StringResolver::resolve`] or
/// [`StringResolver::resolve_variants`]**; overriding one is the only way back to the defect.
pub trait StringResolver: std::fmt::Debug {
    /// `StringTable`'s `(table DataID, string id)` → variant 0, the singular/default form,
    /// **escaped** — byte for byte what the row holds in the dat.
    fn resolve_raw(&self, table: DataId, string_id: u32) -> Option<String>;

    /// Every variant of the same row, **escaped**. A row with substitutions stores the literal
    /// pieces *around* its variables, so rebuilding one needs the whole list.
    fn resolve_variants_raw(&self, table: DataId, string_id: u32) -> Option<Vec<String>> {
        self.resolve_raw(table, string_id).map(|s| vec![s])
    }

    /// The row's **own** variable list: one
    /// string-hash id per substitution, in the order the row substitutes them.
    ///
    /// # Why the fragments alone are not enough
    ///
    /// The string table's lookup never takes a value by position. Both its arms walk
    /// **this** list and ask the caller's table for each id:
    ///
    /// ```text
    /// both arms:
    ///   look up the row's variable id in the caller's id-keyed value table
    /// ```
    ///
    /// and the caller's `vars` is itself keyed by the same id —
    /// the string info's own internal query walks its variable table, filled by its add-variable
    /// call, and copies
    /// it key for key. So the caller's *order* is meaningless and the row's is everything: the
    /// two shipped rows `ID_ActionKeyMap_OverwriteExistingBinding` (`KEY, ACTION`) and
    /// `ID_ActionKeyMap_Binding` (`ACTION, KEY`) take the same two values the other way round,
    /// and a localised dat is free to reorder either.
    ///
    /// `None` means *this resolver cannot say*, not "the row has no variables": a fixture that
    /// holds only fragments answers `None`, and [`crate::UiSystem::resolve_string_named`] then
    /// falls back to the positional order the caller supplied. `Some(vec![])` is a row that
    /// really substitutes nothing.
    fn resolve_variables(&self, table: DataId, string_id: u32) -> Option<Vec<u32>> {
        let _ = (table, string_id);
        None
    }

    /// Variant 0 as a text element receives it, after.
    fn resolve(&self, table: DataId, string_id: u32) -> Option<String> {
        self.resolve_raw(table, string_id)
            .map(crate::text::unescape)
    }

    /// Every variant as a text element receives them, after. Each
    /// piece is unescaped on its own, which is what the client does: the pieces are separate
    /// `StringInfo` literals and the substituted values are never rescanned.
    fn resolve_variants(&self, table: DataId, string_id: u32) -> Option<Vec<String>> {
        self.resolve_variants_raw(table, string_id)
            .map(|v| v.into_iter().map(crate::text::unescape).collect())
    }
}

/// What a text element needs from the host to measure a `Font`.
///
/// Layout depends on the shipped bitmap fonts' exact advances, so the metrics must come from the dat
/// and cannot be approximated here.
pub trait FontProvider: std::fmt::Debug {
    /// The metrics for a font DataID.
    fn metrics(&self, did: DataId) -> Option<std::sync::Arc<dyn crate::text::FontMetrics>>;
}

/// Place every glyph of a laid-out list inside `content`, honouring the two justifications.
///
/// `content` is the element's box **after** the four margins have been removed, in absolute screen
/// coordinates. `lines` must be [`crate::text::glyph::wrap`]'s answer at `content.width()`.
#[must_use]
pub fn place(
    glyphs: &[Glyph],
    lines: &[GlyphLine],
    fonts: &[DataId],
    content: Box2D,
    h_justify: u32,
    v_justify: u32,
) -> Vec<PlacedGlyph> {
    let mut out = Vec::with_capacity(glyphs.len());
    walk_cells(
        glyphs,
        lines,
        content,
        h_justify,
        v_justify,
        |_, g, cell| {
            // A newline occupies the end of its line and draws nothing; the font lookup would
            // otherwise answer `'?'` for it and the interface would grow question marks.
            if !g.is_new_line() {
                out.push(PlacedGlyph {
                    x: cell.x,
                    y: cell.y,
                    ch: g.data,
                    color: g.color,
                    font: fonts.get(g.font as usize).copied().unwrap_or(DataId(0)),
                });
            }
        },
    );
    out
}

/// One glyph's **cell** in the composed layout: the pen, the advance the pen takes, and the height
/// of the line the glyph sits on.
///
/// This is the rectangle the text element's draw builds for a selected glyph —
/// the box constructor is `(x, y, w, h)` with an **inclusive** far edge — and it is the only
/// thing the selection needs that a [`PlacedGlyph`] does not carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlyphCell {
    /// Absolute screen x of the pen, the same value [`PlacedGlyph::x`] carries.
    pub x: i32,
    /// Absolute screen y of the line top, the same value [`PlacedGlyph::y`] carries.
    pub y: i32,
    /// The pen's advance for this glyph — the same width returned by the character draw operation.
    pub advance: i32,
    /// The line height ([`GlyphLine::height`]) for the line the glyph is on.
    pub line_height: i32,
}

/// The one walk both [`place`] and [`place_selection`] are built from, so a composed glyph and its
/// selection rectangle can never be laid out by two different pieces of arithmetic.
///
/// `f` is called once per glyph **including newlines**, with the glyph's absolute index, because
/// the text element's selection endpoints count every glyph. Drawing advances the absolute
/// glyph index after each loop iteration, including iterations that encounter a newline.
fn walk_cells(
    glyphs: &[Glyph],
    lines: &[GlyphLine],
    content: Box2D,
    h_justify: u32,
    v_justify: u32,
    mut f: impl FnMut(usize, &Glyph, GlyphCell),
) {
    let text_h: i32 = lines.iter().map(|l| l.height).sum();
    // The margins are already gone, so the justification is computed with zero near/far margins and
    // the content extent; that is the same arithmetic with the margin terms cancelled.
    //
    // **The extent it justifies inside is `max(box, content)`, not the box** — see
    // [`justification_extent`]. The scrollable height is the top and bottom margins plus the sum
    // of line heights and the scrollable width is the left and right margins plus the widest line,
    // so with the margins
    // cancelled the two maxima are exactly the ones below. A block that overflows its box is
    // therefore drawn from the corner and clipped, which is what a player sees in retail: one
    // line of a two-line caption, not both halves of a centred block.
    let max_w: i32 = lines.iter().map(|l| l.width).max().unwrap_or(0);
    let avail_h = justification_extent(content.height(), text_h);
    let avail_w = justification_extent(content.width(), max_w);
    let mut y = content.y0 + calc_justification(avail_h, text_h, 0, 0, v_justify);
    let n = glyphs.len();
    for line in lines {
        let mut x = content.x0 + calc_justification(avail_w, line.width, 0, 0, h_justify);
        let start = line.start.min(n);
        for (i, g) in glyphs[start..line.end.min(n)].iter().enumerate() {
            f(
                start + i,
                g,
                GlyphCell {
                    x,
                    y,
                    advance: g.width,
                    line_height: line.height,
                },
            );
            x += g.width;
        }
        y += line.height;
    }
}

/// The text element's draw, selection arm: the **inclusive** screen rectangle to
/// invert for every glyph whose index is in `[a, b)`.
///
/// When a selection exists, the client inverts each glyph cell in the half-open selected range
/// after intersecting that cell with the surface box.
///
/// The arm sits in the *foreground* one of the element's **two** glyph passes, so a
/// text element carrying an outline (the other arm) inverts once and
/// not twice. \[verified\]
///
/// A newline glyph draws nothing and is therefore never inverted, but it still occupies an index.
#[must_use]
pub fn place_selection(
    glyphs: &[Glyph],
    lines: &[GlyphLine],
    content: Box2D,
    h_justify: u32,
    v_justify: u32,
    range: (usize, usize),
) -> Vec<Box2D> {
    let (a, b) = range;
    let mut out = Vec::new();
    walk_cells(
        glyphs,
        lines,
        content,
        h_justify,
        v_justify,
        |i, g, cell| {
            if g.is_new_line() || i < a || i >= b || cell.advance <= 0 || cell.line_height <= 0 {
                return;
            }
            // The client's inclusive box constructor uses `x1 = x + w - 1`.
            out.push(Box2D::from_xywh(
                cell.x,
                cell.y,
                cell.advance,
                cell.line_height,
            ));
        },
    );
    out
}

/// The text element's draw, **caret** arm: the inclusive screen rectangle the
/// client fills with the current font colour — one pixel wide, one glyph high, at the cursor
/// position.
///
/// The glyph walk records the pen position, line top, and font height before the glyph at the
/// cursor. At the end position it uses the pen after the final glyph and that line's height. It
/// fills an inclusive rectangle with equal left and right coordinates, making the caret one pixel
/// wide and one glyph high.
///
/// The fill builds `RECT{x0, y0, x1 + 1, y1 + 1}`, so `x0 == x1` really is one
/// column and not an empty rectangle.
///
/// Two cases have no glyph at the cursor and are decided by the glyph list's recalculation.
/// Its ordinary tail gives an empty box one zero-height line; its separate
/// trailing-newline arm gives text ending in a newline another empty line whose stored height is
/// inherited from the newline glyph. With no glyph cell at either cursor position, the draw
/// still uses the element font's maximum character height for the caret itself. `font_height`
/// is that caret fallback.
///
/// The same [`walk_cells`] as [`place`] and [`place_selection`], so the caret can never land
/// beside a glyph laid out by different arithmetic.
#[must_use]
pub fn place_caret(
    glyphs: &[Glyph],
    lines: &[GlyphLine],
    content: Box2D,
    h_justify: u32,
    v_justify: u32,
    cursor: usize,
    font_height: i32,
) -> Box2D {
    let mut at_cursor: Option<(i32, i32, i32)> = None;
    // The pen after the last glyph, its line top and line height, and whether it was a newline.
    let mut after_last: Option<(i32, i32, i32, bool)> = None;
    // The same available extent [`walk_cells`] justifies inside, for the two branches below that
    // re-justify an empty line rather than reading a cell back.
    let avail_w = justification_extent(
        content.width(),
        lines.iter().map(|l| l.width).max().unwrap_or(0),
    );
    walk_cells(
        glyphs,
        lines,
        content,
        h_justify,
        v_justify,
        |i, g, cell| {
            if i == cursor && at_cursor.is_none() {
                at_cursor = Some((cell.x, cell.y, g.height));
            }
            after_last = Some((
                cell.x + cell.advance,
                cell.y,
                cell.line_height,
                g.is_new_line(),
            ));
        },
    );
    let (x, y, h) = match (at_cursor, after_last) {
        (Some((x, y, h)), _) => (x, y, if h > 0 { h } else { font_height }),
        (None, Some((pen, top, line_h, false))) => {
            (pen, top, if line_h > 0 { line_h } else { font_height })
        }
        (None, Some((_, top, line_h, true))) => {
            // The empty line after a trailing newline: the recalculation pushed it with index ==
            // glyph count, and the draw steps down onto it by the previous line's height and
            // re-justifies an empty width.
            (
                content.x0 + calc_justification(avail_w, 0, 0, 0, h_justify),
                top + line_h,
                font_height,
            )
        }
        (None, None) => {
            // No glyphs at all: one empty line. The draw justifies vertically against
            // `max(max character height, scrollable height)`, which with nothing scrollable is
            // the font height.
            (
                content.x0 + calc_justification(avail_w, 0, 0, 0, h_justify),
                content.y0
                    + calc_justification(
                        justification_extent(content.height(), font_height),
                        font_height,
                        0,
                        0,
                        v_justify,
                    ),
                font_height,
            )
        }
    };
    Box2D::new(x, y, x, y + h.max(1) - 1)
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
        g.add_text(0, s, &M, 0xFFFF_FFFF, 0, None);
        g.recalculate(width);
        g
    }

    /// Oracle: (1 = centre, 3/5 = far) and the
    /// advance rule — the pen moves by the glyph's own advance and by nothing else.
    #[test]
    fn glyphs_are_placed_at_the_pen_and_advance_by_their_own_width() {
        let g = list("ab", 1000);
        let box_ = Box2D::new(100, 50, 299, 99);
        let p = place(&g.glyphs, &g.lines, &[DataId(0x4000_0001)], box_, 0, 0);
        assert_eq!(p.len(), 2);
        assert_eq!((p[0].x, p[0].y), (100, 50));
        assert_eq!((p[1].x, p[1].y), (110, 50));
        assert_eq!(p[0].font, DataId(0x4000_0001));
    }

    /// Justification centres each line horizontally and the block vertically.
    #[test]
    fn justification_centres_each_line_horizontally_and_the_block_vertically() {
        let g = list("ab\nc", 1000);
        let box_ = Box2D::new(0, 0, 99, 99);
        let p = place(&g.glyphs, &g.lines, &[DataId(1)], box_, 1, 1);
        assert_eq!(p[0].x, 40);
        // The newline itself draws nothing.
        assert_eq!(p.len(), 3);
        // Two lines of 16 = 32 tall, centred in 100 -> (100>>1) - (32>>1) = 34, second 16 below.
        assert_eq!(p[0].y, 34);
        assert_eq!(p[2].y, 50);
    }

    /// A block taller than its box is drawn from the top.
    #[test]
    fn a_block_taller_than_its_box_is_drawn_from_the_top() {
        let g = list("ab\ncd", 1000);
        let box_ = Box2D::new(0, 0, 99, 19);
        let p = place(&g.glyphs, &g.lines, &[DataId(1)], box_, 1, 1);
        assert_eq!(p[0].y, 0, "line 0 sits at the top of the box, not at -6");
        assert_eq!(p[2].y, 16, "line 1 sits below it, and outside a 20 px box");
        assert_eq!(p[0].x, 40);
    }

    /// Oracle: the same rule on the **horizontal** axis, over
    /// the scrollable width. A one-line field whose text is wider than its box shows the *start*
    /// of the text, not the middle of it, however it is justified.
    #[test]
    fn a_line_wider_than_its_box_is_drawn_from_the_left_under_every_justification() {
        let g = list("abcdef", 1000); // one line, 60 px, `one_line` is irrelevant at this width
        let box_ = Box2D::new(0, 0, 39, 99); // 40 px wide
        for h in [0u32, 1, 2, 3, 5] {
            let p = place(&g.glyphs, &g.lines, &[DataId(1)], box_, h, 0);
            assert_eq!(p[0].x, 0, "h={h}: max(40, 60) = 60, so every arm is zero");
        }
        // …and the moment it fits, justification returns.
        let wide = Box2D::new(0, 0, 99, 99);
        assert_eq!(
            place(&g.glyphs, &g.lines, &[DataId(1)], wide, 1, 0)[0].x,
            20
        );
        assert_eq!(
            place(&g.glyphs, &g.lines, &[DataId(1)], wide, 3, 0)[0].x,
            40
        );
    }

    /// Centring halves the two extents separately as the image does.
    #[test]
    fn centring_halves_the_two_extents_separately_as_the_image_does() {
        use crate::text::glyph::calc_justification;
        assert_eq!(calc_justification(18, 15, 0, 0, 1), 2);
        assert_eq!((18 - 15) / 2, 1, "and the arithmetic it is not");
        // The even/even and odd/odd cases agree, which is why this went unnoticed.
        assert_eq!(calc_justification(18, 14, 0, 0, 1), 2);
        assert_eq!(calc_justification(19, 15, 0, 0, 1), 2);
    }

    /// Pinned behavior: `\n` is a glyph in the list and is not a character
    /// any font contains, so it must never reach the character lookup's `'?'` fallback.
    #[test]
    fn a_newline_glyph_is_never_placed() {
        let g = list("a\nb", 1000);
        let p = place(
            &g.glyphs,
            &g.lines,
            &[DataId(1)],
            Box2D::new(0, 0, 99, 99),
            0,
            0,
        );
        assert_eq!(
            p.iter().map(|q| q.ch).collect::<Vec<_>>(),
            vec![97u16, 98u16]
        );
    }

    /// Selection indices count the newline the placement drops.
    #[test]
    fn selection_indices_count_the_newline_the_placement_drops() {
        let g = list("a\nb", 1000);
        let box_ = Box2D::new(0, 0, 99, 99);
        assert_eq!(g.glyphs.len(), 3, "the newline is a glyph in the list");
        assert_eq!(
            place(&g.glyphs, &g.lines, &[DataId(1)], box_, 0, 0).len(),
            2,
            "and is not placed"
        );

        // Select the whole thing: two cells, one per drawable glyph, on two different lines.
        let all = place_selection(&g.glyphs, &g.lines, box_, 0, 0, (0, 3));
        assert_eq!(
            all.len(),
            2,
            "the newline occupies an index and inverts nothing"
        );
        assert_eq!((all[0].x0, all[0].y0), (0, 0));
        assert_eq!(
            (all[1].x0, all[1].y0),
            (0, 16),
            "the second line, one line height down"
        );

        // Select index 2 alone -- the 'b'. Off the placed list that index does not exist.
        let b = place_selection(&g.glyphs, &g.lines, box_, 0, 0, (2, 3));
        assert_eq!(b.len(), 1);
        assert_eq!(
            (b[0].x0, b[0].y0, b[0].x1, b[0].y1),
            (0, 16, 9, 31),
            "the 'b' cell, 10x16"
        );

        // And index 1 alone is the newline: an index inside the range, nothing inverted.
        assert!(place_selection(&g.glyphs, &g.lines, box_, 0, 0, (1, 2)).is_empty());
    }

    /// Every selected cell starts at its own glyphs pen under every justification.
    #[test]
    fn every_selected_cell_starts_at_its_own_glyphs_pen_under_every_justification() {
        let g = list("ab\ncd", 1000);
        let box_ = Box2D::new(7, 11, 106, 110);
        for h in [0u32, 1, 3, 5] {
            for v in [0u32, 1, 3, 5] {
                let placed = place(&g.glyphs, &g.lines, &[DataId(1)], box_, h, v);
                let cells = place_selection(&g.glyphs, &g.lines, box_, h, v, (0, g.glyphs.len()));
                assert_eq!(
                    cells.len(),
                    placed.len(),
                    "h={h} v={v}: one cell per drawn glyph"
                );
                for (c, p) in cells.iter().zip(&placed) {
                    assert_eq!((c.x0, c.y0), (p.x, p.y), "h={h} v={v}");
                }
            }
        }
    }
}
