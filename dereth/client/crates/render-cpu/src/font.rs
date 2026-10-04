//! Bitmap fonts: the `Font` dat object, the texture-based font atlas and the batched text path.
//!
//! This module preserves the font byte layout and all eight pixel-positioning rules.
//!
//! **Why [`Font`] is declared here.** `dereth-primitives` has no font type, so [`Font`] is
//! declared in this crate, with the field names of the decoded font record, for the same reason
//! [`crate::surface::Surface`] is.
//!
//! The font path has its **own** half-pixel formula (`2·(p/size) − 1 ∓ 1/size`) — related to, but
//! not identical to, the UI's [`crate::ui::pixel_rules`]. Keep both; do not unify them.

use crate::RenderError;

/// `FontCharDesc` — 11 bytes, packed, no padding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FontCharDesc {
    /// The UTF-16 code unit this glyph draws.
    pub unicode: u16,
    /// Left edge of the glyph inside the atlas, in pixels.
    pub offset_x: u16,
    /// Top edge inside the atlas.
    pub offset_y: u16,
    pub width: u8,
    pub height: u8,
    /// Left side bearing, added to the pen before drawing.
    pub horizontal_offset_before: i8,
    /// Right side bearing, added after.
    pub horizontal_offset_after: i8,
    /// Vertical offset from the top of the line to the glyph top.
    pub vertical_offset_before: i8,
}

impl FontCharDesc {
    /// The packed fields occupy 2+2+2+1+1+1+1+1 = 11 bytes.
    pub const SERIALISED_SIZE: usize = 11;

    /// Read one record from its 11 packed bytes, little-endian.
    #[must_use]
    pub fn from_bytes(b: &[u8; Self::SERIALISED_SIZE]) -> Self {
        Self {
            unicode: u16::from_le_bytes([b[0], b[1]]),
            offset_x: u16::from_le_bytes([b[2], b[3]]),
            offset_y: u16::from_le_bytes([b[4], b[5]]),
            width: b[6],
            height: b[7],
            #[allow(clippy::cast_possible_wrap)]
            horizontal_offset_before: b[8] as i8,
            #[allow(clippy::cast_possible_wrap)]
            horizontal_offset_after: b[9] as i8,
            #[allow(clippy::cast_possible_wrap)]
            vertical_offset_before: b[10] as i8,
        }
    }

    /// Rule 1 — **advance** = `horizontal_offset_before + width + horizontal_offset_after`
    /// Integer, accumulated in integer pixels; no
    /// kerning, no sub-pixel positioning, no hinting. **There is no kerning table** — pair kerning
    /// does not exist in this engine.
    #[must_use]
    pub fn advance(&self) -> i32 {
        i32::from(self.horizontal_offset_before)
            + i32::from(self.width)
            + i32::from(self.horizontal_offset_after)
    }
}

/// The `Font` dat object (type 0x0D `.font`), metrics only: the pixels live in two ordinary
/// `0x06xxxxxx` `RenderSurface`s.
#[derive(Debug, Clone, Default)]
pub struct Font {
    /// The line height in pixels.
    pub max_char_height: u32,
    /// Widest glyph.
    pub max_char_width: u32,
    /// Sorted ascending by `unicode` in every retail font.
    pub char_descs: Vec<FontCharDesc>,
    pub num_horizontal_border_pixels: u32,
    pub num_vertical_border_pixels: u32,
    /// Serialised and never read by any traced draw path; its intended use is not established.
    pub baseline_offset: i32,
    pub foreground_surface_data_id: u32,
    /// 0 when the font has no shadow sheet.
    pub background_surface_data_id: u32,
}

/// The `'?'` fallback the client uses for any character outside the map.
pub const MISSING_GLYPH: u16 = 0x3F;

impl Font {
    /// the client falls back to `'?' (0x3F)` for any character
    /// outside the map or with an out-of-range index; if `'?'` is also missing it returns NULL.
    ///
    /// "The `'?'` fallback means a missing glyph renders as a question mark, not as a box or
    /// nothing."
    #[must_use]
    pub fn get_char_desc(&self, ch: u16) -> Option<&FontCharDesc> {
        self.find(ch).or_else(|| self.find(MISSING_GLYPH))
    }

    fn find(&self, ch: u16) -> Option<&FontCharDesc> {
        // Records are stored sorted ascending by unicode in every retail font, so a binary search
        // is the intended lookup; the client builds a dense character map for O(1) and falls back
        // to a linear scan when it is absent. Either way the answer is the same record.
        self.char_descs
            .binary_search_by_key(&ch, |d| d.unicode)
            .ok()
            .map(|i| &self.char_descs[i])
            .or_else(|| self.char_descs.iter().find(|d| d.unicode == ch))
    }

    /// True only when the character is in *this* font's map. It
    /// is what drives per-character font fallback.
    #[must_use]
    pub fn contains_char(&self, ch: u16) -> bool {
        self.find(ch).is_some()
    }

    /// The character's advance width.
    #[must_use]
    pub fn get_char_width_a(&self, ch: u16) -> i32 {
        self.get_char_desc(ch).map_or(0, FontCharDesc::advance)
    }

    /// The lowest and highest `unicode` present.
    #[must_use]
    pub fn unicode_range(&self) -> Option<(u16, u16)> {
        let min = self.char_descs.iter().map(|d| d.unicode).min()?;
        let max = self.char_descs.iter().map(|d| d.unicode).max()?;
        Some((min, max))
    }
}

/// One baked glyph in the atlas.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GlyphUv {
    pub u0: f32,
    pub v0: f32,
    /// Rule 7 — `(x + width)/256`, **exclusive**, with no half-texel inset (the sampler is `POINT`,
    /// so no inset is needed). Divided by the *glyph texture's* width, which is 256 only for a
    /// [`FontAtlas::build`] bake; [`FontAtlas::from_glyph_sheet`] divides by the sheet's own width.
    pub u1: f32,
    /// `(y + max_character_height)/256`, likewise exclusive — and `(y + height)/sheetHeight` for
    /// a sheet, because the client's source rectangle is `height` tall.
    pub v1: f32,
    pub width: u8,
    /// A baked cell is `max_char_height` tall — that is what the font-texture setup records and what
    /// the text draw uses. A **sheet** entry is `height` tall, the glyph's own, because
    /// the rectangle builder builds both rectangles from `height`.
    pub height: u32,
    pub horizontal_offset_before: i8,
    pub horizontal_offset_after: i8,
    pub vertical_offset_before: i8,
}

/// The eight `(dx, dy)` offsets of the text element's **`0x9000`** outline arm, in the client's
/// own iteration order.
///
/// The client loops `dy` from -1 to 1 on the outside and `dx` from -1 to 1 on the inside,
/// skipping `(0, 0)`, and draws the character at `(x + dx, y + dy)` in the outline colour with
/// flags `0x9000`.
///
/// The order is submission order and a text batch has no other ordering, so
/// it is pinned rather than left to a nested loop's convenience. The centre is skipped because
/// the foreground pass draws it.
pub const OUTLINE_NEIGHBOURHOOD: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// The atlas edge, in texels: the client creates a 256×256 `PFID_A8R8G8B8` texture, one level.
pub const ATLAS_SIZE: u32 = 256;
/// `0.00390625 = 1/256` is the texel scale.
pub const TEXEL_SCALE: f32 = 1.0 / 256.0;
/// Rule 7 — the atlas gutter is 1 pixel horizontally and 1 pixel vertically
/// (`row height = max_char_height + 1`).
pub const ATLAS_GUTTER: u32 = 1;
/// the font-texture setup bakes only the printable-ASCII range.
pub const FIRST_BAKED_CHAR: u16 = 0x20;
/// The clamp is `<= 0x7F`, one past the last printable character.
pub const LAST_BAKED_CHAR: u16 = 0x7F;

/// A glyph sheet: one of the `Font`'s two `RenderSurface`s, as BGRA8.
#[derive(Debug, Clone, Copy)]
pub struct GlyphSheet<'a> {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub bgra: &'a [u8],
}

/// A font's glyph texture plus the per-glyph metrics — built either of the two ways the client
/// draws text.
///
/// [`FontAtlas::build`] is the 256×256 texture-font bake: the **GPU path**, used by the debug
/// overlay, which refuses outright any font whose printable-ASCII range will not fit
/// the 256×256 atlas. [`FontAtlas::from_glyph_sheet`] is the **CPU path**'s
/// source -- the font's own foreground `RenderSurface`, addressed by the client's own source
/// rectangle. It bakes nothing, so it has no size limit and no ASCII requirement, and it is what
/// the UI text path reads.
#[derive(Debug, Clone)]
pub struct FontAtlas {
    /// The glyph texture, BGRA8: `texture_size.0 * texture_size.1 * 4` bytes.
    pub pixels: Vec<u8>,
    /// The glyph texture's size in texels — `(256, 256)` for a bake, the sheet's own size for
    /// [`FontAtlas::from_glyph_sheet`]. Every `u`/`v` in [`GlyphUv`] is a fraction of it.
    pub texture_size: (u32, u32),
    /// `(unicode, its cell)`, sorted ascending by the code unit. The list is already in the order
    /// the atlas lookup expects, so a lookup is the client's binary search.
    characters: Vec<(u16, GlyphUv)>,
    /// `max_character_height` = `font.max_char_height`.
    pub max_character_height: u32,
    /// `vertical_spacing`, defaulted to `font.max_char_height`.
    pub vertical_spacing: u32,
    /// `horizontal_spacing`, defaulted to the maximum `GetCharWidthA` over the baked range. Used
    /// only in fixed-pitch mode.
    pub horizontal_spacing: i32,
    /// Horizontal dilation — how far the **outline** blit's rectangles
    /// are dilated horizontally. Carried here because a caller holding only the atlas cannot
    /// otherwise build the client's dilated rectangle.
    pub num_horizontal_border_pixels: u32,
    /// Vertical dilation. See
    /// [`Self::num_horizontal_border_pixels`], and [`TextBatch::draw_ui_outline_glyph`] for the
    /// one place the two are *not* used symmetrically.
    pub num_vertical_border_pixels: u32,
    /// The font's **background** (outline) sheet, BGRA8 at [`Self::texture_size`], when it has
    /// one.
    ///
    /// It is a second glyph texture addressed by the *same* [`GlyphUv`] metrics: the retail
    /// sheets are authored with the glyph pre-fattened in place, so the outline blit reads the
    /// same `(offset_x, offset_y)` origin, only dilated. Measured over the shipped set: **37 of
    /// 49** fonts have one, every one of them is the *same size* as its foreground sheet, and for
    /// every one of them the dilated source rectangle of every glyph stays inside the sheet.
    ///
    /// `None` for the other 12, which is the `0x9000` arm's precondition — see
    /// [`OUTLINE_NEIGHBOURHOOD`].
    ///
    /// **This is the payload, not the fact.** A caller that uploads the sheet to the GPU wants to
    /// *move* the pixels out rather than clone 8 MiB, so [`FontAtlas::take_outline_pixels`] exists
    /// and [`Self::has_outline_sheet`] is answered by [`Self::outline_sheet`] instead. Reading the
    /// fact off `outline_pixels.is_some()` after such a move silently selects the wrong
    /// the character draw arm, which is exactly what happened the first time this was written.
    outline_pixels: Option<Vec<u8>>,
    /// Whether the font named a background sheet — **independent of whether the pixels are still
    /// here**. See [`Self::outline_pixels`].
    outline_sheet: bool,
}

impl FontAtlas {
    /// The client's font-texture setup.
    ///
    /// ```text
    /// require min_unicode_char <= 0x20 and max_unicode_char >= 0x7E   // must cover printable ASCII
    /// clamp first_char to >= 0x20 and last_char to <= 0x7F
    /// scratch = 256x256 A8R8G8B8, filled RGBA(1,1,1,0)            // white, fully transparent
    /// x = 0; y = 0
    /// for ch = first .. last:
    ///     d = font.get_char_desc(ch)
    ///     if (x + d->width > 255) { y += max_character_height + 1; x = 0; if (y > 255) fail }
    ///     draw ch into scratch at (x, y), colours 0xFFFFFFFF / 0xFF000000, flags 0x100
    /// .. record U0/V0/U1/V1 ...
    ///     x += d->width + 1                                      // 1 px gutter horizontally
    /// ```
    ///
    /// # Errors
    /// [`RenderError::FontAtlasOverflow`] when the glyphs do not fit, which is what the client
    /// returns false for — "which is why the GPU path is only used for the small debug font and for
    /// UI text drawn in Latin scripts".
    pub fn build(font: &Font, foreground: GlyphSheet<'_>) -> Result<Self, RenderError> {
        let (min_u, max_u) = font
            .unicode_range()
            .ok_or(RenderError::Unsupported("font has no glyphs"))?;
        // The client's `require`: the font must cover printable ASCII.
        if min_u > FIRST_BAKED_CHAR || max_u < 0x7E {
            return Err(RenderError::Unsupported(
                "bitmap font requires a font covering printable ASCII (0x20..=0x7E)",
            ));
        }
        let first = min_u.max(FIRST_BAKED_CHAR);
        let last = max_u.min(LAST_BAKED_CHAR);

        // Fill white and fully transparent, as the client does: RGBA(1,1,1,0).
        let mut pixels = vec![0xFFu8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
        for px in pixels.as_chunks_mut::<4>().0 {
            px[3] = 0;
        }

        let max_h = font.max_char_height;
        let mut horizontal_spacing = 0i32;
        let mut characters: Vec<(u16, GlyphUv)> = Vec::with_capacity(usize::from(last - first) + 1);
        let mut x = 0u32;
        let mut y = 0u32;
        for ch in first..=last {
            let d = font.get_char_desc(ch).copied().unwrap_or_default();
            horizontal_spacing = horizontal_spacing.max(d.advance());
            let w = u32::from(d.width);
            // `if (x + d->width > 255)` -- note the 255, not 256: a glyph ending exactly on the
            // last column still wraps.
            if x + w > ATLAS_SIZE - 1 {
                y += max_h + ATLAS_GUTTER;
                x = 0;
                if y > ATLAS_SIZE - 1 {
                    return Err(RenderError::FontAtlasOverflow);
                }
            }
            blit_glyph(&mut pixels, &d, foreground, x, y);
            #[allow(clippy::cast_precision_loss)]
            characters.push((
                ch,
                GlyphUv {
                    u0: x as f32 * TEXEL_SCALE,
                    v0: y as f32 * TEXEL_SCALE,
                    // U1 = (x + width - 1 + 1)/256 -- the exclusive right edge.
                    u1: (x + w) as f32 * TEXEL_SCALE,
                    // V1 = (y + max_character_height - 1 + 1)/256.
                    v1: (y + max_h) as f32 * TEXEL_SCALE,
                    width: d.width,
                    height: max_h,
                    horizontal_offset_before: d.horizontal_offset_before,
                    horizontal_offset_after: d.horizontal_offset_after,
                    vertical_offset_before: d.vertical_offset_before,
                },
            ));
            x += w + ATLAS_GUTTER;
        }

        Ok(Self {
            pixels,
            texture_size: (ATLAS_SIZE, ATLAS_SIZE),
            characters,
            max_character_height: max_h,
            vertical_spacing: max_h,
            horizontal_spacing,
            num_horizontal_border_pixels: font.num_horizontal_border_pixels,
            num_vertical_border_pixels: font.num_vertical_border_pixels,
            // The client bakes with flags `0x100`, which is the copy-alpha flag and
            // has no `0x4000`, so the bake never reads the background sheet. The debug overlay
            // therefore has no outline, and this stays `None` by transcription rather than by
            // omission.
            outline_pixels: None,
            outline_sheet: false,
        })
    }

    /// The **CPU path**: the font's own foreground glyph sheet, used as the glyph texture directly.
    ///
    /// The client draws UI text by blitting out of the
    /// `Font`'s two `RenderSurface`s rather than out of a baked atlas, and its rectangle builder
    /// builds the pair of rectangles it blits between:
    ///
    /// ```text
    /// src = (offset_x - hBorder, offset_y - vBorder,
    ///        offset_x + width + hBorder, offset_y + height + vBorder)
    /// dst = (penX - hBorder, penY - vBorder, penX + width + hBorder, penY + height + hBorder)
    /// ```
    ///
    /// and the character draw passes **0** for the border on the foreground call — only the background
    /// (shadow) call passes `num_vertical_border_pixels`. So the foreground source rectangle is
    /// exactly `(offset_x, offset_y, width, height)`, which is what this records, and the
    /// entry's height is the glyph's own `height` rather than `max_char_height`.
    ///
    /// **This is the one that has no 256×256 limit**, and the reason the client can draw a 48-pixel
    /// display font that the font-texture setup refuses: three retail fonts — `0x40000013`,
    /// `0x40000014` and `0x40000024` — need more than 256 rows for printable ASCII alone, and the
    /// shipped char-gen layouts name two of them.
    ///
    /// # Errors
    /// [`RenderError::Unsupported`] when the font has no glyphs at all, or when the sheet's byte
    /// count does not match its stated size.
    pub fn from_glyph_sheet(font: &Font, sheet: GlyphSheet<'_>) -> Result<Self, RenderError> {
        Self::from_glyph_sheets(font, sheet, None)
    }

    /// [`Self::from_glyph_sheet`] plus the font's **background** (outline) sheet, which is the
    /// second texture the client blits from when its flags carry
    /// `0x4000` — the `0x7000` outline pass of the text element's glyph loop.
    ///
    /// The outline sheet must be the same size as the foreground one, because the two are
    /// addressed by the same `(offset_x, offset_y)`; that is true of all 37 shipped fonts that
    /// have one, and a mismatch is refused rather than silently mis-sampled.
    ///
    /// # Errors
    /// [`RenderError::Unsupported`] as [`Self::from_glyph_sheet`], and additionally when the
    /// outline sheet's size differs from the foreground sheet's.
    pub fn from_glyph_sheets(
        font: &Font,
        sheet: GlyphSheet<'_>,
        outline: Option<GlyphSheet<'_>>,
    ) -> Result<Self, RenderError> {
        if font.char_descs.is_empty() {
            return Err(RenderError::Unsupported("font has no glyphs"));
        }
        let want = (sheet.width as usize) * (sheet.height as usize) * 4;
        if sheet.bgra.len() < want {
            return Err(RenderError::Unsupported(
                "the glyph sheet is smaller than its stated size",
            ));
        }
        let outline_pixels = match outline {
            None => None,
            Some(o) => {
                if o.width != sheet.width || o.height != sheet.height {
                    return Err(RenderError::Unsupported(
                        "the outline sheet is not the same size as the foreground sheet",
                    ));
                }
                if o.bgra.len() < want {
                    return Err(RenderError::Unsupported(
                        "the outline sheet is smaller than its stated size",
                    ));
                }
                Some(o.bgra[..want].to_vec())
            }
        };
        #[allow(clippy::cast_precision_loss)]
        let (sw, sh) = (
            1.0f32 / sheet.width.max(1) as f32,
            1.0f32 / sheet.height.max(1) as f32,
        );
        let max_h = font.max_char_height;
        let mut horizontal_spacing = 0i32;
        let mut characters: Vec<(u16, GlyphUv)> = Vec::with_capacity(font.char_descs.len());
        for d in &font.char_descs {
            horizontal_spacing = horizontal_spacing.max(d.advance());
            let (x, y) = (u32::from(d.offset_x), u32::from(d.offset_y));
            let (w, h) = (u32::from(d.width), u32::from(d.height));
            #[allow(clippy::cast_precision_loss)]
            characters.push((
                d.unicode,
                GlyphUv {
                    u0: x as f32 * sw,
                    v0: y as f32 * sh,
                    // Exclusive at the far edge and no half-texel inset, exactly as rule 7 says --
                    // the rule is about the sampler, not about the atlas.
                    u1: (x + w) as f32 * sw,
                    v1: (y + h) as f32 * sh,
                    width: d.width,
                    height: h,
                    horizontal_offset_before: d.horizontal_offset_before,
                    horizontal_offset_after: d.horizontal_offset_after,
                    vertical_offset_before: d.vertical_offset_before,
                },
            ));
        }
        // The client uses a dense-map lookup; here it is a binary search, so
        // the records must be sorted. They are in every retail font; this makes it true.
        characters.sort_unstable_by_key(|e| e.0);
        characters.dedup_by_key(|e| e.0);
        Ok(Self {
            pixels: sheet.bgra[..want].to_vec(),
            texture_size: (sheet.width, sheet.height),
            characters,
            max_character_height: max_h,
            vertical_spacing: max_h,
            horizontal_spacing,
            num_horizontal_border_pixels: font.num_horizontal_border_pixels,
            num_vertical_border_pixels: font.num_vertical_border_pixels,
            outline_sheet: outline_pixels.is_some(),
            outline_pixels,
        })
    }

    /// Whether this font has a background sheet, i.e. which arm of the text element's outline
    /// pass applies: `true` selects the single dilated `0x7000` draw
    /// ([`TextBatch::draw_ui_outline_glyph`]), `false` the eight `0x9000` draws over
    /// [`OUTLINE_NEIGHBOURHOOD`].
    ///
    /// Stays true after [`Self::take_outline_pixels`] has moved the pixels to a GPU texture.
    #[must_use]
    pub fn has_outline_sheet(&self) -> bool {
        self.outline_sheet
    }

    /// Move the outline sheet's pixels out, for a caller uploading them as a texture.
    ///
    /// [`Self::has_outline_sheet`] keeps answering `true` afterwards: the atlas still *describes*
    /// a font with an outline sheet, it simply no longer holds a resident copy of it. Returns
    /// `None` on the second call and for a font with no background sheet.
    pub fn take_outline_pixels(&mut self) -> Option<Vec<u8>> {
        self.outline_pixels.take()
    }

    /// The outline sheet's pixels, while they are still resident.
    #[must_use]
    pub fn outline_pixels(&self) -> Option<&[u8]> {
        self.outline_pixels.as_deref()
    }

    /// `1/width`, `1/height` of the glyph texture — the texel step each `u`/`v` moves by.
    #[must_use]
    pub fn texel_scale(&self) -> (f32, f32) {
        #[allow(clippy::cast_precision_loss)]
        (
            1.0 / self.texture_size.0.max(1) as f32,
            1.0 / self.texture_size.1.max(1) as f32,
        )
    }

    /// Every code unit this glyph texture can draw, ascending.
    #[must_use]
    pub fn baked_chars(&self) -> Vec<u16> {
        self.characters.iter().map(|e| e.0).collect()
    }

    /// The debug-font setup overrides `horizontal_spacing = 8` and
    /// `vertical_spacing = 14` — the fixed-pitch debug console metrics.
    pub fn set_debug_console_metrics(&mut self) {
        self.horizontal_spacing = 8;
        self.vertical_spacing = 14;
    }

    /// The baked glyph for a character, with the `'?'` fallback.
    #[must_use]
    pub fn glyph(&self, ch: char) -> Option<GlyphUv> {
        let code = u16::try_from(u32::from(ch)).ok()?;
        self.lookup(code).or_else(|| self.lookup(MISSING_GLYPH))
    }

    fn lookup(&self, code: u16) -> Option<GlyphUv> {
        self.characters
            .binary_search_by_key(&code, |e| e.0)
            .ok()
            .map(|i| self.characters[i].1)
    }

    /// Text width: the sum of `horizontal_spacing` per character
    /// when `flags & 1` (fixed pitch), otherwise the sum of `before + width + after`.
    #[must_use]
    pub fn compute_text_width(&self, text: &str, flags: TextFlags) -> i32 {
        text.chars()
            .filter_map(|c| self.glyph(c))
            .map(|g| {
                if flags.fixed_pitch() {
                    self.horizontal_spacing
                } else {
                    i32::from(g.horizontal_offset_before)
                        + i32::from(g.width)
                        + i32::from(g.horizontal_offset_after)
                }
            })
            .sum()
    }
}

/// Copy one glyph's source rectangle out of the foreground sheet into the atlas.
///
/// The destination rectangle is `(x, y, width, height)` sampled from
/// `(offset_x, offset_y, width, height)` in the sheet.
/// The background (shadow) sheet is not composited here.
fn blit_glyph(atlas: &mut [u8], d: &FontCharDesc, sheet: GlyphSheet<'_>, x: u32, y: u32) {
    for row in 0..u32::from(d.height) {
        let sy = u32::from(d.offset_y) + row;
        let dy = y + row;
        if sy >= sheet.height || dy >= ATLAS_SIZE {
            break;
        }
        for col in 0..u32::from(d.width) {
            let sx = u32::from(d.offset_x) + col;
            let dx = x + col;
            if sx >= sheet.width || dx >= ATLAS_SIZE {
                break;
            }
            let s = ((sy * sheet.width + sx) * 4) as usize;
            let t = ((dy * ATLAS_SIZE + dx) * 4) as usize;
            if s + 4 <= sheet.bgra.len() {
                atlas[t..t + 4].copy_from_slice(&sheet.bgra[s..s + 4]);
            }
        }
    }
}

/// The text draw's alignment flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TextFlags(pub u32);

impl TextFlags {
    /// Rule 4 — fixed pitch: advance by `horizontal_spacing` and ignore the side bearings.
    pub const FIXED_PITCH: u32 = 0x01;
    /// Right-align: `x -= compute_text_width(text) * scale`.
    pub const RIGHT_ALIGN: u32 = 0x08;
    /// Centre horizontally: `x -= compute_text_width(text) * scale * 0.5`.
    pub const CENTRE_H: u32 = 0x10;
    /// Bottom-align: `y -= max_character_height * scale`.
    pub const BOTTOM_ALIGN: u32 = 0x40;
    /// Centre vertically: `y -= max_character_height * scale * 0.5`.
    pub const CENTRE_V: u32 = 0x80;

    #[must_use]
    pub const fn fixed_pitch(self) -> bool {
        self.0 & Self::FIXED_PITCH != 0
    }
}

/// The texture-font vertex layout is 24 bytes: `Vector3 Origin; ulong Diffuse; float u, v` — exactly
/// FVF `0x142` ([`crate::vertex::VertexFormat::XyzDiffuseTex1`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextVertex {
    /// Already in clip space; the matrices are identity for the duration of the batch.
    pub origin: [f32; 3],
    pub diffuse: u32,
    pub u: f32,
    pub v: f32,
}

/// The client's global text vertex batch: **6 vertices per character**, two triangles, unindexed,
/// `D3DPT_TRIANGLELIST`.
///
/// "All text drawn between a Begin/End pair shares one draw call and one material, so ordering
/// within a batch is submission order." That is the one batching the renderer allows, because
/// it provably cannot reorder.
#[derive(Debug, Clone, Default)]
pub struct TextBatch {
    pub vertices: Vec<TextVertex>,
    /// `ready`.
    ready: bool,
    /// `uses_scaling` — selects `TEXFILTER_LINEAR` over `TEXFILTER_POINT` at flush time.
    uses_scaling: bool,
}

impl TextBatch {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin a text batch -- sets `ready = true`.
    pub fn begin(&mut self) {
        self.ready = true;
    }

    /// Render one run of text.
    ///
    /// Appends six vertices per character to the batch. `viewport` is the viewport size, whose
    /// reciprocals are the `iw`/`ih` of the half-pixel rule.
    ///
    /// The parameter list follows the retail client's; keeping it recognisable is worth more than bundling
    /// it into a struct nothing else would use.
    #[allow(clippy::too_many_arguments)]
    pub fn render_text(
        &mut self,
        atlas: &FontAtlas,
        x: f32,
        y: f32,
        scale: f32,
        text: &str,
        colour: u32,
        flags: TextFlags,
        viewport: (f32, f32),
    ) {
        if !self.ready {
            return;
        }
        if scale != 1.0 {
            self.uses_scaling = true;
        }
        let (iw, ih) = (1.0 / viewport.0, 1.0 / viewport.1);

        // Alignment. The text width is measured in unscaled pixels and then scaled.
        let mut origin_x = x;
        let mut origin_y = y;
        #[allow(clippy::cast_precision_loss)]
        let text_width = atlas.compute_text_width(text, flags) as f32;
        #[allow(clippy::cast_precision_loss)]
        let line_height = atlas.max_character_height as f32;
        if flags.0 & TextFlags::RIGHT_ALIGN != 0 {
            origin_x -= text_width * scale;
        }
        if flags.0 & TextFlags::CENTRE_H != 0 {
            origin_x -= text_width * scale * 0.5;
        }
        if flags.0 & TextFlags::BOTTOM_ALIGN != 0 {
            origin_y -= line_height * scale;
        }
        if flags.0 & TextFlags::CENTRE_V != 0 {
            origin_y -= line_height * scale * 0.5;
        }

        let mut pen = 0.0f32;
        for ch in text.chars() {
            let Some(g) = atlas.glyph(ch) else { continue };
            let (advance, dx, dy) = if flags.fixed_pitch() {
                #[allow(clippy::cast_precision_loss)]
                (atlas.horizontal_spacing as f32, 0.0f32, 0.0f32)
            } else {
                #[allow(clippy::cast_precision_loss)]
                let a = (i32::from(g.width)
                    + i32::from(g.horizontal_offset_before)
                    + i32::from(g.horizontal_offset_after)) as f32;
                (
                    a,
                    f32::from(g.horizontal_offset_before),
                    f32::from(g.vertical_offset_before),
                )
            };

            // Rule 5 -- the half-pixel rule expressed per edge. The near edge is pulled half a
            // pixel outward (`- iw`) and the far edge pushed half a pixel outward (`+ iw`), so a
            // w-pixel-wide glyph covers exactly w pixel centres. Note the quad is one pixel
            // narrower than the glyph in source units (`width - 1`) before the two expansions.
            let left = (dx + pen) * scale + origin_x;
            let x0 = 2.0 * (left * iw) - 1.0 - iw;
            let x1 = 2.0 * (((f32::from(g.width) - 1.0) * scale + left) * iw) - 1.0 + iw;
            let top = dy * scale + origin_y;
            let y0 = -(2.0 * (top * ih) - 1.0 - ih);
            #[allow(clippy::cast_precision_loss)]
            let y1 = -(2.0 * (((g.height as f32 - 1.0) * scale + top) * ih) - 1.0 + ih);

            let v = |px: f32, py: f32, u: f32, vv: f32| TextVertex {
                origin: [px, py, 0.0],
                diffuse: colour,
                u,
                v: vv,
            };
            // (x0,y0) (x0,y1) (x1,y1) | (x1,y1) (x1,y0) (x0,y0)
            self.vertices.push(v(x0, y0, g.u0, g.v0));
            self.vertices.push(v(x0, y1, g.u0, g.v1));
            self.vertices.push(v(x1, y1, g.u1, g.v1));
            self.vertices.push(v(x1, y1, g.u1, g.v1));
            self.vertices.push(v(x1, y0, g.u1, g.v0));
            self.vertices.push(v(x0, y0, g.u0, g.v0));

            pen += advance;
        }
    }

    /// One glyph of the **UI** text path: the client's character blit, drawn
    /// as a quad instead of composed into a UI surface.
    ///
    /// The text draw above is the *debug/overlay* path — a NUL-terminated `char*` at a pen with one
    /// colour. The UI text path needs the other one: each glyph carries its own colour, the pen
    /// comes from `GlyphList`'s layout rather than from a running sum here, and the whole thing is
    /// **clipped to the element's box**. The client clips in software while composing surfaces,
    /// without a scissor test; a glyph quad has no composed surface to perform that clipping.
    ///
    /// `pen_x`/`pen_y` are the destination pixel the character draw is given; the client's
    /// rectangle builder builds the destination rectangle "from the pen position plus
    /// `horizontal_offset_before` / `vertical_offset_before`", which is rule 2. `clip` is an
    /// **inclusive** box, the UI region convention.
    ///
    /// Returns false when the glyph is not in the atlas or is clipped away entirely.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_ui_glyph(
        &mut self,
        atlas: &FontAtlas,
        ch: u16,
        pen_x: i32,
        pen_y: i32,
        colour: u32,
        clip: (i32, i32, i32, i32),
        viewport: (u32, u32),
    ) -> bool {
        let Some(g) = char::from_u32(u32::from(ch)).and_then(|c| atlas.glyph(c)) else {
            return false;
        };
        let w = i32::from(g.width);
        let h = i32::try_from(g.height).unwrap_or(0);
        if w <= 0 || h <= 0 {
            return false;
        }
        // Rule 2: pen + the two bearings.
        let left = pen_x + i32::from(g.horizontal_offset_before);
        let top = pen_y + i32::from(g.vertical_offset_before);
        // The intersection with the element's clip box, still inclusive.
        let (cx0, cy0) = (left.max(clip.0), top.max(clip.1));
        let (cx1, cy1) = ((left + w - 1).min(clip.2), (top + h - 1).min(clip.3));
        if cx1 < cx0 || cy1 < cy0 {
            return false;
        }
        // The cell is exactly `width` x `height` texels, so one destination pixel is one texel
        // and a clipped edge moves the UV by whole texels. Rule 7's UVs are exclusive at the far
        // edge and have no half-texel inset, and this keeps that. The step is the *glyph
        // texture's* texel, which is 1/256 only when the source is a bake.
        let (tw, th) = atlas.texel_scale();
        #[allow(clippy::cast_precision_loss)]
        let u0 = g.u0 + (cx0 - left) as f32 * tw;
        #[allow(clippy::cast_precision_loss)]
        let u1 = g.u0 + (cx1 + 1 - left) as f32 * tw;
        #[allow(clippy::cast_precision_loss)]
        let v0 = g.v0 + (cy0 - top) as f32 * th;
        #[allow(clippy::cast_precision_loss)]
        let v1 = g.v0 + (cy1 + 1 - top) as f32 * th;

        let rect = ui_glyph_clip_rect(cx0, cy0, cx1 - cx0 + 1, cy1 - cy0 + 1, viewport);
        // **The half-pixel compensation, applied exactly once**, through the renderer's one
        // implementation of it. Rule 5 is D3D9's convention; `compensate_for_d3d12` gives back the
        // `+1/W`, `-1/H` a pixel-centre rasteriser needs. See `crate::ui::compensate_for_d3d12`.
        let rect = crate::ui::compensate_for_d3d12(rect, viewport);

        let (l, r, b, t) = (rect.x, rect.right(), rect.y, rect.top());
        let v = |px: f32, py: f32, u: f32, vv: f32| TextVertex {
            origin: [px, py, 0.0],
            diffuse: colour,
            u,
            v: vv,
        };
        // (x0,y0) (x0,y1) (x1,y1) | (x1,y1) (x1,y0) (x0,y0), the same winding the text draw emits.
        self.vertices.push(v(l, t, u0, v0));
        self.vertices.push(v(l, b, u0, v1));
        self.vertices.push(v(r, b, u1, v1));
        self.vertices.push(v(r, b, u1, v1));
        self.vertices.push(v(r, t, u1, v0));
        self.vertices.push(v(l, t, u0, v0));
        true
    }

    /// One glyph of the **outline** pass, `0x7000` arm: the font's *background* sheet blitted
    /// through the client's **dilated** rectangle in the outline colour.
    ///
    /// The text element's glyph loop runs twice when its outline attribute is set, so an element
    /// carrying attribute `0x21` makes this pass first and the
    /// ordinary [`Self::draw_ui_glyph`] pass (flags `0x1000`) second, over the same pen. Flags
    /// `0x7000` is `0x1000 | 0x2000 | 0x4000`: `0x4000` draws the background sheet, `0x2000`
    /// suppresses the foreground, and `0x1000` keeps the side bearings, so **the pen advances by
    /// the same amount in both passes** and the two stay in register.
    ///
    /// # The rectangles
    ///
    /// A font carries **two** separate border counts, a horizontal one and a vertical one, and
    /// the rectangle builder takes both: the background (outline) call passes the font's values
    /// and the foreground call passes zero for each. So:
    ///
    /// ```text
    /// src = (offset_x - hb, offset_y - vb, offset_x + width + hb, offset_y + height + vb)
    /// dst = (penX     - hb, penY     - vb, penX     + width + hb, penY     + height + hb)
    /// ```
    ///
    /// **`dst.bottom` uses the *horizontal* border**, where the
    /// matching source edge used the vertical one. That is retail's own asymmetry
    /// and it is transcribed rather than tidied. It is unobservable in shipped data: all 49
    /// retail fonts have `hb == vb`, so this only ever differs for a synthetic font, and
    /// the outline-pixel tests pin that case.
    ///
    /// Returns false when the glyph is not in the atlas, the font has no outline sheet, or the
    /// dilated rectangle is clipped away entirely.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_ui_outline_glyph(
        &mut self,
        atlas: &FontAtlas,
        ch: u16,
        pen_x: i32,
        pen_y: i32,
        colour: u32,
        clip: (i32, i32, i32, i32),
        viewport: (u32, u32),
    ) -> bool {
        if !atlas.has_outline_sheet() {
            return false;
        }
        let Some(g) = char::from_u32(u32::from(ch)).and_then(|c| atlas.glyph(c)) else {
            return false;
        };
        let w = i32::from(g.width);
        let h = i32::try_from(g.height).unwrap_or(0);
        if w <= 0 || h <= 0 {
            return false;
        }
        let hb = i32::try_from(atlas.num_horizontal_border_pixels).unwrap_or(0);
        let vb = i32::try_from(atlas.num_vertical_border_pixels).unwrap_or(0);
        // Rule 2's pen + bearings, then the dilation. `left`/`top` are the *dilated* corner, which
        // is also where the source rectangle's own corner is, so one destination pixel is still
        // one source texel and the UV walk below is the foreground one shifted whole texels.
        let left = pen_x + i32::from(g.horizontal_offset_before) - hb;
        let top = pen_y + i32::from(g.vertical_offset_before) - vb;
        // `dst.right` dilates by `hb`; `dst.bottom` dilates by `hb` as well — see the note above.
        let right = pen_x + i32::from(g.horizontal_offset_before) + w + hb - 1;
        let bottom = pen_y + i32::from(g.vertical_offset_before) + h + hb - 1;
        let (cx0, cy0) = (left.max(clip.0), top.max(clip.1));
        let (cx1, cy1) = (right.min(clip.2), bottom.min(clip.3));
        if cx1 < cx0 || cy1 < cy0 {
            return false;
        }
        let (tw, th) = atlas.texel_scale();
        // The source origin is the glyph cell's own corner moved out by the border, in texels.
        #[allow(clippy::cast_precision_loss)]
        let su0 = g.u0 - hb as f32 * tw;
        #[allow(clippy::cast_precision_loss)]
        let sv0 = g.v0 - vb as f32 * th;
        #[allow(clippy::cast_precision_loss)]
        let u0 = su0 + (cx0 - left) as f32 * tw;
        #[allow(clippy::cast_precision_loss)]
        let u1 = su0 + (cx1 + 1 - left) as f32 * tw;
        #[allow(clippy::cast_precision_loss)]
        let v0 = sv0 + (cy0 - top) as f32 * th;
        #[allow(clippy::cast_precision_loss)]
        let v1 = sv0 + (cy1 + 1 - top) as f32 * th;

        let rect = ui_glyph_clip_rect(cx0, cy0, cx1 - cx0 + 1, cy1 - cy0 + 1, viewport);
        let rect = crate::ui::compensate_for_d3d12(rect, viewport);
        let (l, r, b, t) = (rect.x, rect.right(), rect.y, rect.top());
        let v = |px: f32, py: f32, u: f32, vv: f32| TextVertex {
            origin: [px, py, 0.0],
            diffuse: colour,
            u,
            v: vv,
        };
        self.vertices.push(v(l, t, u0, v0));
        self.vertices.push(v(l, b, u0, v1));
        self.vertices.push(v(r, b, u1, v1));
        self.vertices.push(v(r, b, u1, v1));
        self.vertices.push(v(r, t, u1, v0));
        self.vertices.push(v(l, t, u0, v0));
        true
    }

    /// One glyph of the **outline** pass, `0x9000` arm: the *foreground* sheet drawn eight times
    /// over [`OUTLINE_NEIGHBOURHOOD`] in the outline colour, which is what the text element does
    /// for a font with **no** background sheet.
    ///
    /// Flags `0x9000` is `0x1000 | 0x8000`. The high flag byte is interpreted as signed;
    /// `0x90` is negative and forces the
    /// colourising arm, i.e. colourise the glyph with the passed colour whatever
    /// the surface format is. There is no `0x4000`, so no background sheet is read, and no
    /// `0x2000`, so it is the ordinary foreground blit; only the offset and the colour differ.
    ///
    /// Returns how many of the eight draws produced geometry — 8 when the glyph is wholly inside
    /// the clip box, fewer at an edge, 0 when the glyph is absent or clipped away.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_ui_outline_neighbourhood(
        &mut self,
        atlas: &FontAtlas,
        ch: u16,
        pen_x: i32,
        pen_y: i32,
        colour: u32,
        clip: (i32, i32, i32, i32),
        viewport: (u32, u32),
    ) -> u32 {
        let mut drawn = 0;
        for (dx, dy) in OUTLINE_NEIGHBOURHOOD {
            if self.draw_ui_glyph(atlas, ch, pen_x + dx, pen_y + dy, colour, clip, viewport) {
                drawn += 1;
            }
        }
        drawn
    }

    /// End the batch -- hands it over and resets.
    ///
    /// Returns the vertices and whether the sampler should be `LINEAR` (scaled text) rather than
    /// `POINT` (rule 6: point filtering for unscaled text, linear only when a scale is applied).
    pub fn end(&mut self) -> (Vec<TextVertex>, bool) {
        self.ready = false;
        let scaling = std::mem::replace(&mut self.uses_scaling, false);
        (std::mem::take(&mut self.vertices), scaling)
    }

    /// Rule 8's state, as data: `SRCALPHA / INVSRCALPHA`, `ZFUNC ALWAYS`, no z-write, no culling,
    /// and identity matrices for the duration of the batch, as the client's own font setup
    /// does.
    #[must_use]
    pub fn pipeline_key(&self) -> crate::pso::PipelineKey {
        use crate::pso::{Blend, Cull, PipelineKey, StageOps, ZFunc};
        PipelineKey {
            vertex_format: crate::vertex::VertexFormat::XyzDiffuseTex1,
            src_blend: Blend::SrcAlpha,
            dst_blend: Blend::InvSrcAlpha,
            alpha_blend: true,
            alpha_test: false,
            z_write: false,
            z_func: ZFunc::Always,
            cull: Cull::None,
            stage_ops: StageOps::TEXT,
            fog: false,
            lighting: false,
        }
    }
}

/// Rule 5, as a [`crate::ui::ClipRect`]: the glyph quad for a `w` x `h` pixel rectangle at
/// `(x, y)`, in **D3D9** clip coordinates.
///
/// ```text
/// x0 = 2*(left * iw) - 1 - iw
/// x1 = 2*(((w - 1) + left) * iw) - 1 + iw
/// y0 = -(2*(top * ih) - 1 - ih)
/// y1 = -(2*(((h - 1) + top) * ih) - 1 + ih)
/// ```
///
/// "the left/top edge is pulled half a pixel *outward* and the right/bottom edge pushed half a
/// pixel outward, so a `w`-pixel-wide glyph covers exactly `w` pixel centres." This is **not**
/// `ui::update_transform`: the UI quad's own rule inserts a `- 0.25` size inset that the font path
/// does not have. Keep both; unifying them moves every glyph.
#[must_use]
pub fn ui_glyph_clip_rect(
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    viewport: (u32, u32),
) -> crate::ui::ClipRect {
    #[allow(clippy::cast_precision_loss)]
    let (vw, vh) = (viewport.0.max(1) as f32, viewport.1.max(1) as f32);
    let (iw, ih) = (1.0 / vw, 1.0 / vh);
    #[allow(clippy::cast_precision_loss)]
    let (left, top) = (x as f32, y as f32);
    #[allow(clippy::cast_precision_loss)]
    let (fw, fh) = (w as f32, h as f32);
    let x0 = 2.0 * (left * iw) - 1.0 - iw;
    let x1 = 2.0 * (((fw - 1.0) + left) * iw) - 1.0 + iw;
    let y0 = -(2.0 * (top * ih) - 1.0 - ih);
    let y1 = -(2.0 * (((fh - 1.0) + top) * ih) - 1.0 + ih);
    // `ClipRect::y` is the **bottom** edge in clip space, which is the larger screen y, i.e. `y1`.
    crate::ui::ClipRect {
        x: x0,
        y: y1,
        sx: x1 - x0,
        sy: y0 - y1,
    }
}

/// Rule 3 — **line height** is the maximum over the glyphs on the line, so a
/// line of one small glyph is short.
#[must_use]
pub fn line_height(fonts_on_line: &[&Font]) -> u32 {
    fonts_on_line
        .iter()
        .map(|f| f.max_char_height)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic font covering printable ASCII with a 6x8 cell, laid out left to right in a
    /// 640x8 sheet. Metrics are chosen so every rule can be distinguished from every other.
    fn test_font() -> Font {
        let mut char_descs = Vec::new();
        for (i, ch) in (0x20u16..=0x7Eu16).enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let x = (i * 6) as u16;
            char_descs.push(FontCharDesc {
                unicode: ch,
                offset_x: x,
                offset_y: 0,
                width: 5,
                height: 8,
                horizontal_offset_before: 1,
                horizontal_offset_after: 2,
                vertical_offset_before: 1,
            });
        }
        Font {
            max_char_height: 8,
            max_char_width: 5,
            char_descs,
            num_horizontal_border_pixels: 0,
            num_vertical_border_pixels: 0,
            baseline_offset: 6,
            foreground_surface_data_id: 0x0600_0001,
            background_surface_data_id: 0,
        }
    }

    fn test_sheet(font: &Font) -> (u32, u32, Vec<u8>) {
        let w = 6 * 96u32;
        let h = 8u32;
        let mut bgra = vec![0u8; (w * h * 4) as usize];
        // Paint each glyph's cell with its index so a blit can be traced back to its source.
        for (i, d) in font.char_descs.iter().enumerate() {
            for row in 0..u32::from(d.height) {
                for col in 0..u32::from(d.width) {
                    let x = u32::from(d.offset_x) + col;
                    let p = ((row * w + x) * 4) as usize;
                    #[allow(clippy::cast_possible_truncation)]
                    {
                        bgra[p] = i as u8;
                        bgra[p + 1] = col as u8;
                        bgra[p + 2] = row as u8;
                        bgra[p + 3] = 0xFF;
                    }
                }
            }
        }
        (w, h, bgra)
    }

    fn build() -> FontAtlas {
        let f = test_font();
        let (w, h, bits) = test_sheet(&f);
        FontAtlas::build(
            &f,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bits,
            },
        )
        .unwrap()
    }

    // ---- Rule 1 -------------------------------------------------------------------------------
    // Rule 1 uses the verified integer advance `before + width + after`, with no
    // kerning table at all.
    #[test]
    fn rule_1_advance_is_before_plus_width_plus_after() {
        let f = test_font();
        assert_eq!(f.get_char_width_a(u16::from(b'A')), 1 + 5 + 2);
        // Negative bearings are signed and must not be read as unsigned.
        let d = FontCharDesc {
            width: 5,
            horizontal_offset_before: -2,
            horizontal_offset_after: -1,
            ..FontCharDesc::default()
        };
        assert_eq!(d.advance(), 2);
        // ..and the packed record decodes them as i8.
        let raw = [0x41u8, 0x00, 0x0A, 0x00, 0x14, 0x00, 5, 8, 0xFE, 0xFF, 0xFD];
        let d = FontCharDesc::from_bytes(&raw);
        assert_eq!(d.unicode, 0x41);
        assert_eq!((d.offset_x, d.offset_y), (10, 20));
        assert_eq!(d.horizontal_offset_before, -2);
        assert_eq!(d.horizontal_offset_after, -1);
        assert_eq!(d.vertical_offset_before, -3);
        assert_eq!(FontCharDesc::SERIALISED_SIZE, 11);
    }

    // ---- Rule 2 -------------------------------------------------------------------------------
    // Oracle: rule #2 -- "Glyph placement = pen + horizontal_offset_before horizontally, line top +
    // vertical_offset_before vertically." Checked by reading the emitted quad back out of clip
    // space, which is the only place the placement is observable.
    #[test]
    fn rule_2_placement_is_pen_plus_before_and_line_top_plus_vertical_before() {
        let atlas = build();
        let mut b = TextBatch::new();
        b.begin();
        b.render_text(
            &atlas,
            100.0,
            50.0,
            1.0,
            "A",
            0xFFFF_FFFF,
            TextFlags(0),
            (800.0, 600.0),
        );
        let (v, _) = b.end();
        assert_eq!(v.len(), 6, "six vertices per character");
        // Recover the left edge in pixels: x0 = 2*(left/W) - 1 - 1/W  =>  left = (x0 + 1 + iw)*W/2.
        let iw = 1.0f32 / 800.0;
        let left = (v[0].origin[0] + 1.0 + iw) * 800.0 * 0.5;
        assert!(
            (left - 101.0).abs() < 1e-2,
            "pen 100 + before 1 = 101, got {left}"
        );
        let ih = 1.0f32 / 600.0;
        let top = (1.0 - v[0].origin[1] + ih) * 600.0 * 0.5;
        assert!(
            (top - 51.0).abs() < 1e-2,
            "origin 50 + vertical before 1 = 51, got {top}"
        );
    }

    // ---- Rule 3 -------------------------------------------------------------------------------
    // Oracle: rule #3 -- "Line height = the maximum over the glyphs on the line
    // (a line of one small glyph is short)."
    #[test]
    fn rule_3_line_height_is_the_maximum_over_the_line() {
        let small = Font {
            max_char_height: 8,
            ..Font::default()
        };
        let big = Font {
            max_char_height: 20,
            ..Font::default()
        };
        assert_eq!(line_height(&[&small]), 8);
        assert_eq!(line_height(&[&small, &big]), 20);
        assert_eq!(line_height(&[&big, &small]), 20);
        assert_eq!(line_height(&[]), 0);
    }

    // ---- Rule 4 -------------------------------------------------------------------------------
    // Oracle: rule #4 -- "Fixed-pitch mode (flags & 1) replaces the advance with horizontal_spacing
    // and drops both bearings", plus the debug font's overrides
    // (horizontal_spacing = 8, vertical_spacing = 14).
    #[test]
    fn rule_4_fixed_pitch_replaces_the_advance_and_drops_the_bearings() {
        let mut atlas = build();
        // The default spacing is the maximum advance over the baked range.
        assert_eq!(atlas.horizontal_spacing, 8);
        atlas.set_debug_console_metrics();
        assert_eq!((atlas.horizontal_spacing, atlas.vertical_spacing), (8, 14));

        let fixed = TextFlags(TextFlags::FIXED_PITCH);
        assert_eq!(atlas.compute_text_width("AB", fixed), 16);
        assert_eq!(
            atlas.compute_text_width("AB", TextFlags(0)),
            2 * (1 + 5 + 2)
        );

        // In fixed pitch the horizontal bearing is dropped, so the first glyph starts at the pen.
        let mut b = TextBatch::new();
        b.begin();
        b.render_text(
            &atlas,
            100.0,
            50.0,
            1.0,
            "A",
            0xFFFF_FFFF,
            fixed,
            (800.0, 600.0),
        );
        let (v, _) = b.end();
        let iw = 1.0f32 / 800.0;
        let left = (v[0].origin[0] + 1.0 + iw) * 800.0 * 0.5;
        assert!(
            (left - 100.0).abs() < 1e-2,
            "fixed pitch drops the before bearing, got {left}"
        );
    }

    // ---- Rule 5 -------------------------------------------------------------------------------
    // Oracle: rule #5 -- "the quad's clip coordinates are 2*(p/size) - 1 -/+ 1/size for the
    // near/far edge", with the note that "the quad is one pixel narrower than the glyph in source
    // units (width - 1) before the two half-pixel expansions -- the net width is exactly width
    // pixels". Checked as a net width in pixels, which is the claim that matters.
    #[test]
    fn rule_5_the_half_pixel_rule_gives_a_glyph_its_exact_pixel_width() {
        let atlas = build();
        let mut b = TextBatch::new();
        b.begin();
        b.render_text(
            &atlas,
            0.0,
            0.0,
            1.0,
            "A",
            0xFFFF_FFFF,
            TextFlags(0),
            (800.0, 600.0),
        );
        let (v, _) = b.end();
        let (iw, ih) = (1.0f32 / 800.0, 1.0f32 / 600.0);
        let to_px_x = |c: f32| (c + 1.0 + iw) * 800.0 * 0.5;
        let to_px_y = |c: f32| (1.0 - c + ih) * 600.0 * 0.5;
        // Vertex 0 is the top-left, vertex 2 the bottom-right.
        let x0 = to_px_x(v[0].origin[0]);
        let x1 = to_px_x(v[2].origin[0]);
        assert!(
            (x1 - x0 - 5.0).abs() < 1e-2,
            "net width must be width = 5, got {}",
            x1 - x0
        );
        let y0 = to_px_y(v[0].origin[1]);
        let y1 = to_px_y(v[1].origin[1]);
        assert!(
            (y1 - y0 - 8.0).abs() < 1e-2,
            "net height must be max_char_height = 8, got {}",
            y1 - y0
        );

        // The near edge really is pulled outward: x0's clip value is *below* the naive
        // 2*(p/W) - 1, and the far edge above it. Dropping the terms would move text half a pixel
        // left and up and blur point-sampled glyphs.
        let naive_left = 2.0 * (1.0 * iw) - 1.0;
        assert!(v[0].origin[0] < naive_left);
    }

    // ---- Rule 6 -------------------------------------------------------------------------------
    // Oracle: rule #6 -- "Point filtering for unscaled text, linear only when a scale factor is
    // applied", and the batch end's `uses_scaling ? LINEAR : POINT`.
    #[test]
    fn rule_6_unscaled_text_is_point_filtered_and_scaled_text_is_not() {
        let atlas = build();
        let mut b = TextBatch::new();
        b.begin();
        b.render_text(&atlas, 0.0, 0.0, 1.0, "A", 0, TextFlags(0), (800.0, 600.0));
        let (_, scaling) = b.end();
        assert!(!scaling, "unscaled text must stay on POINT");

        b.begin();
        b.render_text(&atlas, 0.0, 0.0, 2.0, "A", 0, TextFlags(0), (800.0, 600.0));
        let (_, scaling) = b.end();
        assert!(scaling, "scaled text switches the sampler to LINEAR");

        // ..and the flag resets with the batch.
        b.begin();
        b.render_text(&atlas, 0.0, 0.0, 1.0, "A", 0, TextFlags(0), (800.0, 600.0));
        let (_, scaling) = b.end();
        assert!(!scaling);
    }

    // ---- Rule 7 -------------------------------------------------------------------------------
    // Oracle: rule #7 -- "The atlas gutter is 1 pixel horizontally and 1 pixel vertically (row
    // height = max_char_height + 1), and U1/V1 are (x + width)/256 -- exclusive, no inset."
    #[test]
    fn rule_7_the_atlas_has_a_one_pixel_gutter_and_exclusive_far_edges() {
        let atlas = build();
        assert_eq!(atlas.pixels.len(), (256 * 256 * 4) as usize);
        let space = atlas.glyph(' ').unwrap();
        let bang = atlas.glyph('!').unwrap();
        // The first glyph starts at (0,0); the next starts one gutter pixel past its width.
        assert!((space.u0 - 0.0).abs() < 1e-7);
        assert!(
            (space.u1 - 5.0 / 256.0).abs() < 1e-7,
            "U1 = (x + width)/256"
        );
        assert!(
            (bang.u0 - 6.0 / 256.0).abs() < 1e-7,
            "x advances by width + 1"
        );
        // V1 uses max_char_height, not the glyph's own height.
        assert!((space.v1 - 8.0 / 256.0).abs() < 1e-7);
        assert_eq!(space.height, 8);
        // The row height is max_char_height + 1: 256/6 = 42 glyphs per row, so glyph 42 wraps.
        let wrapped = atlas.glyph((0x20u8 + 42) as char).unwrap();
        assert!(
            (wrapped.v0 - 9.0 / 256.0).abs() < 1e-7,
            "row height is max_char_height + 1"
        );
        assert!((wrapped.u0 - 0.0).abs() < 1e-7, "and x restarts at zero");
        assert!((TEXEL_SCALE - 0.003_906_25).abs() < 1e-9);

        // The glyph pixels really were blitted, and from the right place: the sheet paints
        // (index, col, row) into (B, G, R).
        let p = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * 256 + x) * 4) as usize;
            [
                atlas.pixels[i],
                atlas.pixels[i + 1],
                atlas.pixels[i + 2],
                atlas.pixels[i + 3],
            ]
        };
        assert_eq!(p(0, 0), [0, 0, 0, 0xFF], "glyph 0 (space), col 0, row 0");
        assert_eq!(p(4, 7), [0, 4, 7, 0xFF], "glyph 0, col 4, row 7");
        assert_eq!(
            p(6, 0),
            [1, 0, 0, 0xFF],
            "glyph 1 begins one gutter pixel later"
        );
        // The gutter column itself is untouched: white and fully transparent.
        assert_eq!(p(5, 0), [0xFF, 0xFF, 0xFF, 0x00]);
    }

    // ---- Rule 8 -------------------------------------------------------------------------------
    // Oracle: rule #8 -- "Text is drawn with SRCALPHA / INVSRCALPHA, ZFUNC ALWAYS, no z-write, no
    // culling, and with the world/view/projection matrices set to identity for the duration of the
    // batch", plus the material table built during font initialization.
    #[test]
    fn rule_8_the_text_pipeline_state_is_the_documented_one() {
        use crate::pso::{Blend, Cull, PixelShader, StageOps, ZFunc};
        let b = TextBatch::new();
        let k = b.pipeline_key();
        assert_eq!(
            (k.src_blend, k.dst_blend),
            (Blend::SrcAlpha, Blend::InvSrcAlpha)
        );
        assert!(k.alpha_blend);
        assert_eq!(k.z_func, ZFunc::Always);
        assert!(!k.z_write);
        assert_eq!(k.cull, Cull::None);
        // The colour comes entirely from the vertex (SELECTARG2 -> DIFFUSE); alpha is glyph
        // coverage times vertex alpha (MODULATE).
        assert_eq!(k.stage_ops, StageOps::TEXT);
        assert_eq!(k.stage_ops.pixel_shader(), PixelShader::SelectArg2);
        // The batch is FVF 0x142, which is the 24-byte font vertex.
        assert_eq!(k.vertex_format.fvf(), 0x142);
        assert_eq!(k.vertex_format.stride(), 24);
    }

    // ---- The batch --------------------------------------------------------------------------
    // The observed text path batches vertices globally and flushes at the end of text rendering.
    // All text within a begin/end pair shares one draw call and material; order within the batch
    // is submission order. This is the supported batching boundary.
    #[test]
    fn the_batch_preserves_submission_order_and_flushes_once() {
        let atlas = build();
        let mut b = TextBatch::new();
        b.begin();
        b.render_text(
            &atlas,
            0.0,
            0.0,
            1.0,
            "A",
            0x1111_1111,
            TextFlags(0),
            (800.0, 600.0),
        );
        b.render_text(
            &atlas,
            0.0,
            20.0,
            1.0,
            "B",
            0x2222_2222,
            TextFlags(0),
            (800.0, 600.0),
        );
        let (v, _) = b.end();
        assert_eq!(v.len(), 12);
        assert_eq!(v[0].diffuse, 0x1111_1111);
        assert_eq!(
            v[6].diffuse, 0x2222_2222,
            "the second string follows the first"
        );
        // The batch is emptied by the flush.
        let (v2, _) = b.end();
        assert!(v2.is_empty());
        // Text submitted without a Begin is dropped, as the ready flag gates the text draw.
        b.render_text(&atlas, 0.0, 0.0, 1.0, "C", 0, TextFlags(0), (800.0, 600.0));
        assert!(b.vertices.is_empty());
    }

    // Oracle: the text renderer's alignment table (flags 0x08, 0x10, 0x40, 0x80) and its text
    // width.
    #[test]
    fn the_alignment_flags_shift_the_origin_as_documented() {
        let atlas = build();
        let iw = 1.0f32 / 800.0;
        let to_px = |c: f32| (c + 1.0 + iw) * 800.0 * 0.5;
        let run = |flags: u32| {
            let mut b = TextBatch::new();
            b.begin();
            b.render_text(
                &atlas,
                400.0,
                300.0,
                1.0,
                "AB",
                0,
                TextFlags(flags),
                (800.0, 600.0),
            );
            let (v, _) = b.end();
            to_px(v[0].origin[0])
        };
        let width = atlas.compute_text_width("AB", TextFlags(0));
        assert_eq!(width, 16);
        let base = run(0);
        assert!((run(TextFlags::RIGHT_ALIGN) - (base - 16.0)).abs() < 1e-2);
        assert!((run(TextFlags::CENTRE_H) - (base - 8.0)).abs() < 1e-2);
        // Vertical alignment moves y, not x.
        assert!((run(TextFlags::BOTTOM_ALIGN) - base).abs() < 1e-2);
    }

    // The observed lookup falls back to '?' (0x3F) for any character outside the map.
    // A missing glyph therefore renders as a question mark.
    #[test]
    fn a_missing_glyph_falls_back_to_a_question_mark() {
        let f = test_font();
        let q = f.get_char_desc(u16::from(b'?')).copied().unwrap();
        assert_eq!(
            f.get_char_desc(0x4E00).copied(),
            Some(q),
            "a CJK character falls back"
        );
        assert!(
            !f.contains_char(0x4E00),
            "...but the character-presence test still says no, which drives fallback"
        );
        assert!(f.contains_char(u16::from(b'A')));

        let atlas = build();
        assert_eq!(atlas.glyph('\u{4E00}'), atlas.glyph('?'));
        // A font with no '?' returns nothing rather than inventing a glyph.
        let empty = Font {
            char_descs: vec![],
            ..Font::default()
        };
        assert!(empty.get_char_desc(u16::from(b'A')).is_none());
        assert_eq!(empty.get_char_width_a(u16::from(b'A')), 0);
    }

    // Oracle: the atlas setup -- "require min_unicode_char <= 0x20 and
    // max_unicode_char >= 0x7E", and "if the glyphs do not fit in 256x256 the function returns false
    // and the font is unusable -- which is why the GPU path is only used for the small debug font
    // and for UI text drawn in Latin scripts".
    #[test]
    fn the_atlas_refuses_a_font_it_cannot_bake() {
        // A font that does not cover printable ASCII.
        let f = Font {
            max_char_height: 8,
            char_descs: vec![FontCharDesc {
                unicode: 0x41,
                width: 5,
                height: 8,
                ..FontCharDesc::default()
            }],
            ..Font::default()
        };
        let bits = vec![0u8; 4];
        let err = FontAtlas::build(
            &f,
            GlyphSheet {
                width: 1,
                height: 1,
                bgra: &bits,
            },
        )
        .unwrap_err();
        assert!(matches!(err, RenderError::Unsupported(_)));

        // A font whose glyphs are far too tall to fit 256 rows.
        let mut f = test_font();
        f.max_char_height = 200;
        for d in &mut f.char_descs {
            d.width = 200;
        }
        let bits = vec![0u8; 4];
        let err = FontAtlas::build(
            &f,
            GlyphSheet {
                width: 1,
                height: 1,
                bgra: &bits,
            },
        )
        .unwrap_err();
        assert!(matches!(err, RenderError::FontAtlasOverflow), "{err:?}");
    }

    // The baked ASCII sheet is deterministic and self-consistent. There is no golden font image
    // to compare it with, so the claim stops there.
    #[test]
    fn the_baked_ascii_sheet_is_deterministic() {
        let a = build();
        let b = build();
        assert_eq!(a.pixels, b.pixels);
        // Every printable character has a distinct atlas slot.
        let mut slots: Vec<(u32, u32)> = Vec::new();
        for ch in 0x20u8..=0x7E {
            let g = a.glyph(ch as char).unwrap();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            slots.push(((g.u0 * 256.0).round() as u32, (g.v0 * 256.0).round() as u32));
        }
        let mut sorted = slots.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), slots.len(), "every glyph has its own slot");
    }

    /// Which destination pixels a glyph quad covers on a D3D12 rasteriser, and which atlas texel
    /// each one samples. Same method as `ui::coverage`: walk the pixel centres.
    /// One axis of a glyph-coverage report: (destination pixel, sampled uv) per covered pixel.
    type UvCoverage = Vec<(i64, f32)>;

    fn glyph_coverage(v: &[TextVertex], fb: (u32, u32)) -> (UvCoverage, UvCoverage) {
        use crate::ui::{clip_x_to_viewport, clip_y_to_viewport, PixelCentre};
        #[allow(clippy::cast_precision_loss)]
        let (fw, fh) = (fb.0 as f32, fb.1 as f32);
        let (l, t) = (v[0].origin[0], v[0].origin[1]);
        let (r, b) = (v[2].origin[0], v[1].origin[1]);
        let (x_lo, x_hi) = (clip_x_to_viewport(l, fw), clip_x_to_viewport(r, fw));
        let (y_lo, y_hi) = (clip_y_to_viewport(t, fh), clip_y_to_viewport(b, fh));
        let axis = |lo: f32, hi: f32, uv0: f32, uv1: f32| -> UvCoverage {
            let (lo, hi) = (f64::from(lo), f64::from(hi));
            let mut out = Vec::new();
            for k in -4i64..600 {
                #[allow(clippy::cast_precision_loss)]
                let sample = PixelCentre::D3D12.sample_point(k as f64);
                if sample < lo || sample >= hi {
                    continue;
                }
                let f = (sample - lo) / (hi - lo);
                #[allow(clippy::cast_possible_truncation)]
                let uv = (f64::from(uv0) + f * f64::from(uv1 - uv0)) as f32;
                out.push((k, uv));
            }
            out
        };
        (
            axis(x_lo, x_hi, v[0].u, v[2].u),
            axis(y_lo, y_hi, v[0].v, v[1].v),
        )
    }

    /// A `w`-pixel-wide glyph must cover exactly `w` pixel centres, positioned by the pen
    /// plus bearings. Measured in pixels, which is the
    /// only level at which "exactly" means anything.
    #[test]
    fn a_ui_glyph_covers_exactly_its_own_pixel_rectangle_at_the_pen_plus_bearings() {
        let font = test_font();
        let (w, h, bgra) = test_sheet(&font);
        let atlas = FontAtlas::build(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap();
        let fb = (800u32, 600u32);
        let mut batch = TextBatch::new();
        batch.begin();
        // Pen at (100, 50); the test font has before = 1, vertical before = 1, width 5, cell 8.
        assert!(batch.draw_ui_glyph(
            &atlas,
            u16::from(b'A'),
            100,
            50,
            0xFFFF_FFFF,
            (0, 0, 799, 599),
            fb
        ));
        let (verts, _) = batch.end();
        assert_eq!(verts.len(), 6);
        let (cols, rows) = glyph_coverage(&verts, fb);
        assert_eq!(
            cols.iter().map(|c| c.0).collect::<Vec<_>>(),
            (101..106).collect::<Vec<_>>()
        );
        assert_eq!(
            rows.iter().map(|r| r.0).collect::<Vec<_>>(),
            (51..59).collect::<Vec<_>>()
        );
        // ..and each destination pixel samples its own texel: the first column sits inside texel
        // `offset_x` of the atlas row, i.e. u in [u0, u0 + 1/256).
        let g = atlas.glyph('A').unwrap();
        assert!(
            cols[0].1 >= g.u0 && cols[0].1 < g.u0 + TEXEL_SCALE,
            "u = {}",
            cols[0].1
        );
        assert!(cols[4].1 >= g.u1 - TEXEL_SCALE && cols[4].1 < g.u1);
    }

    /// The client clips in software during surface composition, without a scissor test.
    /// A glyph quad has no composed surface, so its clip is
    /// arithmetic: the covered pixels must stop at the element's box and the UV must move with
    /// them rather than the glyph being squashed.
    #[test]
    fn a_ui_glyph_is_clipped_to_the_element_box_by_moving_its_uvs() {
        let font = test_font();
        let (w, h, bgra) = test_sheet(&font);
        let atlas = FontAtlas::build(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap();
        let fb = (800u32, 600u32);
        let mut batch = TextBatch::new();
        batch.begin();
        // The glyph covers columns 101..=105; clip it at 103.
        assert!(batch.draw_ui_glyph(
            &atlas,
            u16::from(b'A'),
            100,
            50,
            0xFFFF_FFFF,
            (0, 0, 103, 599),
            fb
        ));
        let (verts, _) = batch.end();
        let (cols, _) = glyph_coverage(&verts, fb);
        assert_eq!(
            cols.iter().map(|c| c.0).collect::<Vec<_>>(),
            (101..104).collect::<Vec<_>>()
        );
        let g = atlas.glyph('A').unwrap();
        // Three of the five texels, starting at the same one: the picture is cut, not scaled.
        assert!((verts[0].u - g.u0).abs() < 1e-6);
        assert!(
            (verts[2].u - (g.u0 + 3.0 * TEXEL_SCALE)).abs() < 1e-6,
            "u1 = {}",
            verts[2].u
        );

        // Entirely outside: nothing at all.
        batch.begin();
        assert!(!batch.draw_ui_glyph(
            &atlas,
            u16::from(b'A'),
            100,
            50,
            0xFFFF_FFFF,
            (0, 0, 50, 599),
            fb
        ));
        assert!(batch.end().0.is_empty());
    }
    // ---- The CPU path's source: the font's own glyph sheet -------------------------------------
    // Oracle: the client's character blit and its rectangle builder. The
    // foreground call passes 0 for the border, so the source rectangle is exactly
    // `(offset_x, offset_y, width, height)` and the destination rectangle is the same size at
    // the pen -- `height`, not `max_char_height`.

    /// A font whose line box is taller than its glyphs, so `max_char_height` and `height` can be
    /// told apart. Cells stay 5x8 in the 576x8 sheet [`test_sheet`] paints.
    fn tall_line_font() -> Font {
        Font {
            max_char_height: 12,
            ..test_font()
        }
    }

    #[test]
    fn the_sheet_path_addresses_the_fonts_own_source_rectangle() {
        let font = tall_line_font();
        let (w, h, bgra) = test_sheet(&font);
        let atlas = FontAtlas::from_glyph_sheet(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap();
        assert_eq!(
            atlas.texture_size,
            (w, h),
            "the glyph texture is the sheet itself"
        );
        assert_eq!(atlas.pixels.len(), (w * h * 4) as usize);
        assert_eq!(atlas.max_character_height, 12);

        // 'A' is the 33rd printable character, so its cell starts at x = 33*6 = 198.
        let d = font.get_char_desc(u16::from(b'A')).copied().unwrap();
        assert_eq!((d.offset_x, d.offset_y, d.width, d.height), (198, 0, 5, 8));
        let g = atlas.glyph('A').unwrap();
        #[allow(clippy::cast_precision_loss)]
        let (sw, sh) = (1.0f32 / w as f32, 1.0f32 / h as f32);
        assert!(
            (g.u0 - 198.0 * sw).abs() < 1e-7,
            "U0 = offset_x / sheetWidth"
        );
        assert!((g.v0 - 0.0).abs() < 1e-7);
        // Exclusive at the far edge and no half-texel inset -- rule 7 is about the POINT sampler,
        // not about the 256x256 atlas, so it survives the change of denominator.
        assert!(
            (g.u1 - 203.0 * sw).abs() < 1e-7,
            "U1 = (offset_x + width) / sheetWidth"
        );
        assert!(
            (g.v1 - 8.0 * sh).abs() < 1e-7,
            "V1 = (offset_y + height) / sheetHeight"
        );
        // The cell is the glyph's own height, which is what retail's character-rectangle builder measures; a baked
        // cell is max_char_height instead, and the two differ for this font.
        assert_eq!(g.height, 8, "height, not max_char_height");
        assert_eq!(g.width, 5);
        // The bearings are the font's own, untouched.
        assert_eq!(
            (
                g.horizontal_offset_before,
                g.horizontal_offset_after,
                g.vertical_offset_before
            ),
            (1, 2, 1)
        );
        // Every character of the font is addressable, not just a baked ASCII window.
        assert_eq!(atlas.baked_chars().len(), font.char_descs.len());
        assert_eq!(atlas.texel_scale(), (sw, sh));
    }

    /// Oracle: rules 5 and 2 again, measured in pixels -- but against the *sheet*, where the
    /// destination rectangle is `height` tall. A baked cell would make this glyph 12 rows tall.
    #[test]
    fn a_sheet_backed_glyph_covers_exactly_its_own_pixel_rectangle_at_the_pen_plus_bearings() {
        let font = tall_line_font();
        let (w, h, bgra) = test_sheet(&font);
        let atlas = FontAtlas::from_glyph_sheet(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap();
        let fb = (800u32, 600u32);
        let mut batch = TextBatch::new();
        batch.begin();
        assert!(batch.draw_ui_glyph(
            &atlas,
            u16::from(b'A'),
            100,
            50,
            0xFFFF_FFFF,
            (0, 0, 799, 599),
            fb
        ));
        let (verts, _) = batch.end();
        let (cols, rows) = glyph_coverage(&verts, fb);
        assert_eq!(
            cols.iter().map(|c| c.0).collect::<Vec<_>>(),
            (101..106).collect::<Vec<_>>()
        );
        assert_eq!(
            rows.iter().map(|r| r.0).collect::<Vec<_>>(),
            (51..59).collect::<Vec<_>>(),
            "height rows at line top + vertical_offset_before, not max_char_height rows"
        );
        // Each destination pixel samples its own texel of the *sheet*, whose texel is 1/576.
        let g = atlas.glyph('A').unwrap();
        let (sw, _) = atlas.texel_scale();
        assert!(
            cols[0].1 >= g.u0 && cols[0].1 < g.u0 + sw,
            "u = {}",
            cols[0].1
        );
        assert!(cols[4].1 >= g.u1 - sw && cols[4].1 < g.u1);
    }

    /// Software clipping must step by the sheet's texel size.
    /// Using 1/256 here would cut the wrong number of columns out of a 576-wide sheet.
    #[test]
    fn a_sheet_backed_glyph_is_clipped_by_the_sheets_own_texel() {
        let font = tall_line_font();
        let (w, h, bgra) = test_sheet(&font);
        let atlas = FontAtlas::from_glyph_sheet(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap();
        let fb = (800u32, 600u32);
        let mut batch = TextBatch::new();
        batch.begin();
        // The glyph covers columns 101..=105; clip it at 103.
        assert!(batch.draw_ui_glyph(
            &atlas,
            u16::from(b'A'),
            100,
            50,
            0xFFFF_FFFF,
            (0, 0, 103, 599),
            fb
        ));
        let (verts, _) = batch.end();
        let (cols, _) = glyph_coverage(&verts, fb);
        assert_eq!(
            cols.iter().map(|c| c.0).collect::<Vec<_>>(),
            (101..104).collect::<Vec<_>>()
        );
        let g = atlas.glyph('A').unwrap();
        let (sw, _) = atlas.texel_scale();
        assert!((verts[0].u - g.u0).abs() < 1e-6);
        assert!(
            (verts[2].u - (g.u0 + 3.0 * sw)).abs() < 1e-6,
            "u1 = {}",
            verts[2].u
        );
    }

    /// **The atlas setup's `return false` is faithful and stays; UI text does not go through
    /// it.** A font too tall for 256 rows is
    /// refused by the bake and drawn by the sheet.
    #[test]
    fn the_sheet_path_takes_a_font_the_256x256_bake_refuses() {
        // The shape of the shipped `0x40000013`: max_char_height 46, glyphs up to 50 wide. Five of
        // those fit one 256-pixel row, so 95 printable characters need 19 rows of 47 pixels --
        // 893, well past the atlas's 256.
        let mut font = test_font();
        font.max_char_height = 46;
        font.max_char_width = 50;
        for (i, d) in font.char_descs.iter_mut().enumerate() {
            d.width = 50;
            d.height = 46;
            #[allow(clippy::cast_possible_truncation)]
            {
                d.offset_x = (i * 51) as u16;
            }
            d.offset_y = 0;
        }
        let (w, h) = (51u32 * 96, 46u32);
        let bgra = vec![0xFFu8; (w * h * 4) as usize];
        let err = FontAtlas::build(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap_err();
        assert!(matches!(err, RenderError::FontAtlasOverflow), "{err:?}");

        let atlas = FontAtlas::from_glyph_sheet(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap();
        for ch in 0x20u8..=0x7E {
            assert!(
                atlas.glyph(ch as char).is_some(),
                "{ch:#04X} draws from the sheet"
            );
        }
        // ..and a font with no glyphs at all is still refused, rather than yielding a texture
        // nothing can be drawn from.
        let empty = Font::default();
        assert!(matches!(
            FontAtlas::from_glyph_sheet(
                &empty,
                GlyphSheet {
                    width: 1,
                    height: 1,
                    bgra: &[0; 4]
                }
            ),
            Err(RenderError::Unsupported(_))
        ));
        // ..as is a sheet that does not carry the pixels it claims to.
        assert!(matches!(
            FontAtlas::from_glyph_sheet(
                &font,
                GlyphSheet {
                    width: 64,
                    height: 64,
                    bgra: &[0; 4]
                }
            ),
            Err(RenderError::Unsupported(_))
        ));
    }

    /// Oracle: the lookup -- the `'?'` fallback is the sheet path's too, and a
    /// character the font does have is found however sparse its map is. The shipped unicode fonts
    /// carry 20 609 records spread over 0x0000..0xFFFC, which a dense array indexed by
    /// `ch - first` would answer wrongly.
    #[test]
    fn the_sheet_path_keeps_the_question_mark_fallback_over_a_sparse_map() {
        let mut font = test_font();
        font.char_descs.push(FontCharDesc {
            unicode: 0x4E00,
            offset_x: 0,
            offset_y: 0,
            width: 5,
            height: 8,
            ..FontCharDesc::default()
        });
        let (w, h, bgra) = test_sheet(&font);
        let atlas = FontAtlas::from_glyph_sheet(
            &font,
            GlyphSheet {
                width: w,
                height: h,
                bgra: &bgra,
            },
        )
        .unwrap();
        assert!(
            atlas.glyph('\u{4E00}').is_some(),
            "a sparse high code unit is addressable"
        );
        assert_ne!(atlas.glyph('\u{4E00}'), atlas.glyph('?'));
        // ..and one the font does not have still falls back to '?'.
        assert_eq!(atlas.glyph('\u{4E01}'), atlas.glyph('?'));
    }
}
