//! Text rasterised with the Windows font system into the classic interface's coverage atlas
//! ([`dereth_classic_dat::fonts`]): a height and an average character width in pixels, a weight,
//! italic or not, and a face name, each printable Windows-1252 character drawn once into a 16 by
//! 14 grid of 64-pixel cells.
//!
//! Each glyph is the system's hinted grey glyph bitmap, 65 levels of coverage, the kind of glyph
//! the game's own glyph sheets hold: the same on every machine, whatever smoothing the player's
//! desktop uses for its own text. Drawing text straight onto a surface would take the desktop's
//! choice instead, and with subpixel smoothing on, the coloured fringes read as extra coverage
//! and every stroke comes out heavier.

pub use dereth_classic_dat::fonts::{
    coverage, cp1252, measure_cells, to_cp1252, FontAtlas, FontSpec, Glyph,
};
#[cfg(windows)]
use dereth_classic_dat::fonts::{CELL, COLUMNS, PAD, ROWS};

/// The Windows font system, as a [`dereth_classic_dat::fonts::FontSource`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemFonts;

impl dereth_classic_dat::fonts::FontSource for SystemFonts {
    fn rasterize(&self, spec: &FontSpec) -> Result<FontAtlas, String> {
        rasterize(spec)
    }
}

/// Rasterise `spec` with the system's fonts.
///
/// # Errors
/// Off Windows, or when the system cannot make the font or the drawing surface.
#[cfg(windows)]
pub fn rasterize(spec: &FontSpec) -> Result<FontAtlas, String> {
    windows::rasterize(spec)
}

/// Rasterise `spec` with the system's fonts.
///
/// # Errors
/// Always: the classic interface's fonts need the Windows font system.
#[cfg(not(windows))]
pub fn rasterize(_spec: &FontSpec) -> Result<FontAtlas, String> {
    Err("the classic interface's text needs the Windows font system".into())
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows {
    use super::{coverage, cp1252, measure_cells, FontAtlas, FontSpec, CELL, COLUMNS, PAD, ROWS};
    use std::mem::zeroed;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::SIZE;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateFontIndirectA, DeleteDC, DeleteObject, GetGlyphOutlineA,
        GetTextExtentPoint32A, GetTextFaceA, GetTextMetricsA, SelectObject, FIXED, GDI_ERROR,
        GGO_GRAY8_BITMAP, GLYPHMETRICS, HDC, HFONT, HGDIOBJ, LOGFONTA, MAT2, TEXTMETRICA,
    };

    /// Everything made along the way, released in reverse on every exit.
    struct Resources {
        dc: HDC,
        font: HFONT,
        old_font: HGDIOBJ,
    }

    impl Drop for Resources {
        fn drop(&mut self) {
            // SAFETY: each handle is either null (never made, skipped) or one this function made
            // and still owns; the previous selection is restored before anything is deleted.
            unsafe {
                if !self.old_font.is_null() {
                    SelectObject(self.dc, self.old_font);
                }
                if !self.font.is_null() {
                    DeleteObject(self.font);
                }
                if !self.dc.is_null() {
                    DeleteDC(self.dc);
                }
            }
        }
    }

    pub(super) fn rasterize(spec: &FontSpec) -> Result<FontAtlas, String> {
        let (aw, ah) = (COLUMNS * CELL, ROWS * CELL);
        let pixels = usize::try_from(aw * ah).map_err(|e| e.to_string())?;
        // SAFETY: an all-zero LOGFONTA is a valid value (every field is an integer or a byte
        // array); the fields that matter are set below.
        let mut logfont: LOGFONTA = unsafe { zeroed() };
        logfont.lfHeight = spec.height;
        logfont.lfWidth = spec.width;
        logfont.lfWeight = spec.weight;
        logfont.lfItalic = u8::from(spec.italic);
        logfont.lfCharSet = 1;
        logfont.lfPitchAndFamily = 0x10;
        for (slot, byte) in logfont
            .lfFaceName
            .iter_mut()
            .zip(spec.face.bytes().take(31))
        {
            *slot = i8::from_ne_bytes([byte]);
        }
        // SAFETY: plain GDI object creation with valid arguments; every result is checked and
        // owned by `Resources`, which releases it on every path.
        let mut r = unsafe {
            Resources {
                dc: CreateCompatibleDC(null_mut()),
                font: CreateFontIndirectA(&logfont),
                old_font: null_mut(),
            }
        };
        if r.dc.is_null() || r.font.is_null() {
            return Err(format!(
                "the system would not make the font {:?}",
                spec.face
            ));
        }
        let one = FIXED { fract: 0, value: 1 };
        let none = FIXED { fract: 0, value: 0 };
        let identity = MAT2 {
            eM11: one,
            eM12: none,
            eM21: none,
            eM22: one,
        };
        let failed = u32::from_ne_bytes(GDI_ERROR.to_ne_bytes());
        let mut alpha = vec![0u8; pixels];
        let mut metrics: TEXTMETRICA;
        let mut face = [0u8; 128];
        let mut cells = Vec::new();
        let mut buffer: Vec<u8> = Vec::new();
        // SAFETY: `r.dc` is a live memory DC with the font selected into it; every string handed
        // to GDI is a one-byte slice with its length, and every glyph buffer is as long as GDI
        // said it must be.
        unsafe {
            r.old_font = SelectObject(r.dc, r.font);
            metrics = zeroed();
            if GetTextMetricsA(r.dc, &mut metrics) == 0 {
                return Err("the system would not measure the font".into());
            }
            GetTextFaceA(r.dc, 128, face.as_mut_ptr());
            for (index, byte) in (32u8..=255).enumerate() {
                if byte == 127 {
                    continue;
                }
                let Some(c) = cp1252(byte) else { continue };
                let index = i32::try_from(index).unwrap_or(0);
                let (cx, cy) = (index % COLUMNS * CELL, index / COLUMNS * CELL);
                let text = [byte];
                let mut extent = SIZE { cx: 0, cy: 0 };
                if GetTextExtentPoint32A(r.dc, text.as_ptr(), 1, &mut extent) == 0 {
                    return Err(format!(
                        "the system would not measure U+{:04X}",
                        u32::from(c)
                    ));
                }
                cells.push((u32::from(c), cx, cy, extent.cx));
                let mut glyph: GLYPHMETRICS = zeroed();
                let size = GetGlyphOutlineA(
                    r.dc,
                    u32::from(byte),
                    GGO_GRAY8_BITMAP,
                    &mut glyph,
                    0,
                    null_mut(),
                    &identity,
                );
                // A character with no ink (the space) has no bitmap at all.
                if size == failed || size == 0 {
                    continue;
                }
                buffer.clear();
                buffer.resize(size as usize, 0);
                if GetGlyphOutlineA(
                    r.dc,
                    u32::from(byte),
                    GGO_GRAY8_BITMAP,
                    &mut glyph,
                    size,
                    buffer.as_mut_ptr().cast(),
                    &identity,
                ) == failed
                {
                    return Err(format!("the system would not draw U+{:04X}", u32::from(c)));
                }
                // Rows of one byte per pixel, each row padded to four bytes, top first; the box's
                // top left sits at the glyph origin from the pen on the baseline.
                let width = i32::try_from(glyph.gmBlackBoxX).unwrap_or(0);
                let height = i32::try_from(glyph.gmBlackBoxY).unwrap_or(0);
                let pitch = (width + 3) & !3;
                let left = cx + PAD + glyph.gmptGlyphOrigin.x;
                let top = cy + PAD + metrics.tmAscent - glyph.gmptGlyphOrigin.y;
                for y in 0..height {
                    for x in 0..width {
                        let (ax, ay) = (left + x, top + y);
                        if ax < cx || ax >= cx + CELL || ay < cy || ay >= cy + CELL {
                            return Err(format!(
                                "the glyph for U+{:04X} does not fit its cell",
                                u32::from(c)
                            ));
                        }
                        let level = buffer
                            .get(usize::try_from(y * pitch + x).unwrap_or(usize::MAX))
                            .copied()
                            .unwrap_or(0);
                        if let Some(a) = alpha.get_mut(usize::try_from(ay * aw + ax).unwrap_or(0)) {
                            *a = coverage(level);
                        }
                    }
                }
            }
        }
        drop(r);
        let mut rgba = vec![0xFF; pixels * 4];
        for (px, a) in rgba.as_chunks_mut::<4>().0.iter_mut().zip(&alpha) {
            px[3] = *a;
        }
        let glyphs = measure_cells(&alpha, aw, &cells, metrics.tmAscent)?;
        let end = face.iter().position(|b| *b == 0).unwrap_or(face.len());
        Ok(FontAtlas {
            width: aw.unsigned_abs(),
            height: ah.unsigned_abs(),
            rgba,
            glyphs,
            line_height: metrics.tmHeight,
            baseline: metrics.tmAscent,
            face: face[..end].iter().map(|b| char::from(*b)).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (host font rasterisation; the pixels are this machine's fonts).
    use super::*;
    // The cell geometry, which the module imports for the Windows rasteriser only.
    use dereth_classic_dat::fonts::{CELL, PAD};

    #[test]
    fn the_western_code_page_maps_its_upper_half_and_leaves_five_bytes_undefined() {
        assert_eq!(cp1252(0x41), Some('A'));
        assert_eq!(cp1252(0x80), Some('\u{20AC}'));
        assert_eq!(cp1252(0x92), Some('\u{2019}'));
        assert_eq!(cp1252(0xE9), Some('\u{E9}'));
        let undefined: Vec<u8> = (0x80..=0x9F).filter(|b| cp1252(*b).is_none()).collect();
        assert_eq!(undefined, [0x81, 0x8D, 0x8F, 0x90, 0x9D]);
        assert_eq!(to_cp1252('\u{2019}'), Some(0x92));
        assert_eq!(to_cp1252('\u{4E00}'), None);
    }

    #[test]
    fn a_glyph_box_is_measured_from_its_ink_and_an_empty_glyph_sits_on_the_pen() {
        let width = CELL * 2;
        let mut alpha = vec![0u8; usize::try_from(width * CELL).unwrap()];
        // Ink from (20,15) to (24,30) inside the second cell.
        for y in 15..30 {
            for x in 20..24 {
                alpha[usize::try_from(y * width + CELL + x).unwrap()] = 200;
            }
        }
        let glyphs = measure_cells(&alpha, width, &[(32, 0, 0, 4), (65, CELL, 0, 7)], 11).unwrap();
        assert_eq!(
            glyphs[&32],
            Glyph {
                x: PAD,
                y: PAD + 11,
                width: 0,
                height: 0,
                bearing_x: 0,
                bearing_y: 0,
                advance: 4
            }
        );
        assert_eq!(
            glyphs[&65],
            Glyph {
                x: CELL + 20,
                y: 15,
                width: 4,
                height: 15,
                bearing_x: 8,
                bearing_y: -8,
                advance: 7
            }
        );
        alpha[0] = 1;
        assert!(measure_cells(&alpha, width, &[(32, 0, 0, 4)], 11).is_err());
    }

    #[test]
    fn grey_glyph_levels_spread_over_a_byte_as_the_game_s_glyph_sheets_spread_them() {
        assert_eq!(coverage(0), 0);
        assert_eq!(coverage(1), 3);
        assert_eq!(coverage(2), 7);
        assert_eq!(coverage(63), 251);
        assert_eq!(coverage(64), 255);
    }

    /// The atlas holds at most the 65 grey levels and no colour: every pixel is white with its
    /// coverage in alpha, whatever smoothing the desktop uses for its own text.
    #[cfg(windows)]
    #[test]
    fn the_atlas_is_grey_coverage_in_sixty_five_levels_whatever_the_desktop_smoothing() {
        let atlas = rasterize(&FontSpec {
            height: 15,
            width: 6,
            weight: 500,
            italic: false,
            face: "Times New Roman".into(),
        })
        .unwrap();
        let mut levels = std::collections::BTreeSet::new();
        for px in atlas.rgba.as_chunks::<4>().0 {
            assert_eq!(&px[..3], &[255, 255, 255]);
            levels.insert(px[3]);
        }
        assert!(levels.len() <= 65, "{} levels", levels.len());
        assert!(levels.iter().all(|a| *a == 0 || *a == 255 || a % 4 == 3));
        // The letter l's stem, halfway down: a hinted serif face's thin vertical, at most two
        // pixels of full ink across.
        let l = atlas.glyphs[&u32::from('l')];
        let row = l.y + l.height / 2;
        let full = (l.x..l.x + l.width)
            .filter(|x| atlas.rgba[usize::try_from((row * 1024 + x) * 4 + 3).unwrap()] == 255)
            .count();
        assert!(full <= 2, "{full} full pixels across the stem");
    }

    #[cfg(windows)]
    #[test]
    fn the_system_serif_font_draws_every_printable_western_character() {
        let atlas = rasterize(&FontSpec {
            height: 15,
            width: 6,
            weight: 500,
            italic: false,
            face: "Times New Roman".into(),
        })
        .unwrap();
        assert_eq!(atlas.glyphs.len(), 218);
        assert!(atlas.glyphs[&u32::from('W')].width > 0);
        assert!(atlas.line_height >= 15 && atlas.baseline > 0);
    }
}
