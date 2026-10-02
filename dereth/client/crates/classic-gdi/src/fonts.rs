//! Text rasterised with the Windows font system into the classic interface's coverage atlas
//! ([`dereth_classic_dat::fonts`]): a height and an average character width in pixels, a weight,
//! italic or not, and a face name, each printable Windows-1252 character drawn once, white on
//! black, into a 16 by 14 grid of 64-pixel cells.

pub use dereth_classic_dat::fonts::{cp1252, measure_cells, to_cp1252, FontAtlas, FontSpec, Glyph};
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
    use super::{cp1252, measure_cells, FontAtlas, FontSpec, CELL, COLUMNS, PAD, ROWS};
    use std::mem::{size_of, zeroed};
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::SIZE;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, CreateFontIndirectA, DeleteDC, DeleteObject,
        ExtTextOutA, GdiFlush, GetTextExtentPoint32A, GetTextFaceA, GetTextMetricsA, SelectObject,
        SetBkMode, SetTextAlign, SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        DIB_RGB_COLORS, HBITMAP, HDC, HFONT, HGDIOBJ, LOGFONTA, TEXTMETRICA, TRANSPARENT,
    };

    /// Everything made along the way, released in reverse on every exit.
    struct Resources {
        dc: HDC,
        bitmap: HBITMAP,
        font: HFONT,
        old_bitmap: HGDIOBJ,
        old_font: HGDIOBJ,
    }

    impl Drop for Resources {
        fn drop(&mut self) {
            // SAFETY: each handle is either null (never made, skipped) or one this function made
            // and still owns; the previous selections are restored before anything is deleted.
            unsafe {
                if !self.old_font.is_null() {
                    SelectObject(self.dc, self.old_font);
                }
                if !self.old_bitmap.is_null() {
                    SelectObject(self.dc, self.old_bitmap);
                }
                if !self.font.is_null() {
                    DeleteObject(self.font);
                }
                if !self.bitmap.is_null() {
                    DeleteObject(self.bitmap);
                }
                if !self.dc.is_null() {
                    DeleteDC(self.dc);
                }
            }
        }
    }

    pub(super) fn rasterize(spec: &FontSpec) -> Result<FontAtlas, String> {
        let (aw, ah) = (COLUMNS * CELL, ROWS * CELL);
        let size = usize::try_from(aw * ah * 4).map_err(|e| e.to_string())?;
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
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or(40),
                biWidth: aw,
                biHeight: -ah,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: u32::try_from(size).unwrap_or(0),
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            // SAFETY: RGBQUAD is plain bytes.
            bmiColors: [unsafe { zeroed() }],
        };
        let mut bits: *mut core::ffi::c_void = null_mut();
        // SAFETY: plain GDI object creation with valid arguments; every result is checked and
        // owned by `Resources`, which releases it on every path.
        let mut r = unsafe {
            let dc = CreateCompatibleDC(null_mut());
            Resources {
                dc,
                bitmap: CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0),
                font: CreateFontIndirectA(&logfont),
                old_bitmap: null_mut(),
                old_font: null_mut(),
            }
        };
        if r.dc.is_null() || r.bitmap.is_null() || r.font.is_null() || bits.is_null() {
            return Err(format!(
                "the system would not make the font {:?}",
                spec.face
            ));
        }
        let mut metrics: TEXTMETRICA;
        let mut face = [0u8; 128];
        let mut cells = Vec::new();
        // SAFETY: `r.dc` is a live memory DC with the DIB and the font selected into it; `bits`
        // points at `size` bytes the DIB owns for as long as `r` lives; every string handed to GDI
        // is a one-byte slice with its length.
        unsafe {
            r.old_bitmap = SelectObject(r.dc, r.bitmap);
            r.old_font = SelectObject(r.dc, r.font);
            std::ptr::write_bytes(bits.cast::<u8>(), 0, size);
            SetTextAlign(r.dc, 0x18);
            SetTextColor(r.dc, 0x00FF_FFFF);
            SetBkMode(r.dc, TRANSPARENT as i32);
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
                if GetTextExtentPoint32A(r.dc, text.as_ptr(), 1, &mut extent) == 0
                    || ExtTextOutA(
                        r.dc,
                        cx + PAD,
                        cy + PAD + metrics.tmAscent,
                        0,
                        std::ptr::null(),
                        text.as_ptr(),
                        1,
                        std::ptr::null(),
                    ) == 0
                {
                    return Err(format!("the system would not draw U+{:04X}", u32::from(c)));
                }
                cells.push((u32::from(c), cx, cy, extent.cx));
            }
            GdiFlush();
        }
        // SAFETY: the DIB's `size` bytes stay valid while `r` lives, and drawing has finished.
        let bgra = unsafe { std::slice::from_raw_parts(bits.cast::<u8>(), size) };
        let mut rgba = vec![0xFF; size];
        let mut alpha = vec![0u8; size / 4];
        for (i, px) in bgra.as_chunks::<4>().0.iter().enumerate() {
            let a = px[0].max(px[1]).max(px[2]);
            alpha[i] = a;
            rgba[i * 4 + 3] = a;
        }
        drop(r);
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
