//! The mouse cursor: a dat `RenderSurface` turned into the image a window system installs as the
//! pointer.
//!
//! The client builds a cursor from an RGBA surface with one monochrome mask bitmap and one
//! screen-compatible colour bitmap, installs it as the window class cursor, and shows or hides
//! it with a separate counter.
//!
//! **The client's cursors are dat images, not Win32 resources.** Setting a cursor pulls a
//! `RenderSurface` from the dat as image type `0xC` and hands it
//! here; `LoadCursorA(NULL, IDC_ARROW)` supplies the system arrow when the cursor-install
//! routine receives no image. A client drawing that fallback arrow is therefore not
//! a client with the feature switched off, it is a client permanently on the error path.
//!
//! # What is here
//!
//! [`IconBits`] and [`build`] are pixels in, a 32x32 colour image and a 1-bit AND mask out, and
//! [`IconBits::rgba`] is the same picture as straight RGBA with the mask folded into the alpha.
//! They are the part that can be wrong invisibly (a mask polarity, an off-by-one row, a clamp), so
//! they are testable with no window, no device and no dat. Turning the picture into a system
//! cursor and putting it on the window is the host's (`dereth_desktop::cursor`).

/// The fixed size of a Windows cursor, and what the icon is rendered into regardless of how
/// small the source surface is.
///
/// The client asks the render device for a `0x20 x 0x20` surface, fills it with transparent black,
/// and blits the source into its top-left corner. A source larger than this in **either** axis is
/// refused outright (`width < 0x21 && height < 0x21`), not scaled.
pub const CURSOR_EXTENT: u32 = 32;

/// The alpha at or above which a pixel is opaque, when the mask bitmap is built:
/// `(pixel & 0xFF000000) < 0x40000000` selects the transparent bit, so `0x40` is the first opaque
/// alpha.
pub const ALPHA_OPAQUE_MIN: u8 = 0x40;

/// One cursor, in the two bitmaps `CreateIconIndirect` wants.
///
/// Both are 32x32 and **top-down** (row 0 is the top row), which is the order `CreateBitmap` reads
/// its bits in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconBits {
    /// The AND mask, 1 bit per pixel, 4 bytes per row, MSB is the leftmost pixel: **1 means
    /// transparent** (leave the screen alone), which is Windows' convention and the client's.
    ///
    /// The client's mask-bitmap builder fills the whole buffer with `0xFF` and only clears the bits it
    /// computes, so every pixel outside the source surface, and every bit of padding at the end of
    /// a row, stays transparent.
    pub and_mask: Vec<u8>,
    /// The colour image, 32x32 BGRA, top-down, 4096 bytes.
    ///
    /// The client's colour-bitmap builder writes `0` for any source pixel whose alpha is below
    /// [`ALPHA_OPAQUE_MIN`] and `PatBlt(..., BLACKNESS)` for everything outside the source, so a
    /// masked-out pixel is black rather than whatever the dat happened to store there.
    pub color_bgra: Vec<u8>,
    /// Horizontal cursor hotspot passed when the cursor is installed.
    pub hot_x: u32,
    /// `ICONINFO::yHotspot`.
    pub hot_y: u32,
}

impl IconBits {
    /// The cursor as 32x32 straight (not premultiplied) RGBA, top-down: the form a window system
    /// that takes a cursor as one colour image reads.
    ///
    /// The AND mask becomes the alpha and nothing else does: a pixel the mask shows is opaque
    /// (`0xFF`) in its colour, and a pixel the mask leaves transparent is `[0, 0, 0, 0]`. So every
    /// pixel is either drawn or not, exactly as the mask-and-colour pair draws it, and no pixel is
    /// blended, whatever alpha the dat surface stored between [`ALPHA_OPAQUE_MIN`] and `0xFF`.
    #[must_use]
    pub fn rgba(&self) -> Vec<u8> {
        let e = CURSOR_EXTENT as usize;
        let mut rgba = vec![0u8; e * e * 4];
        for y in 0..e {
            for x in 0..e {
                let transparent = self.and_mask[y * (e / 8) + x / 8] & (0x80 >> (x % 8)) != 0;
                if transparent {
                    continue;
                }
                let o = (y * e + x) * 4;
                let bgra = &self.color_bgra[o..o + 4];
                rgba[o..o + 4].copy_from_slice(&[bgra[2], bgra[1], bgra[0], 0xFF]);
            }
        }
        rgba
    }
}

/// Why a surface could not become a cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CursorError {
    /// The client's first cursor-size test: `width < 0x21 && height < 0x21`.
    #[error("a {0}x{1} surface is too large for a cursor (the client's limit is 32x32)")]
    TooLarge(u32, u32),
    /// The pixel buffer is shorter than `width * height`.
    #[error("the surface says {0}x{1} but carries only {2} pixels")]
    Truncated(u32, u32, usize),
}

/// The client's icon-from-surface path, without the GDI.
///
/// `pixels` is the decoded surface in **BGRA** order, top-down, `width * height` entries — which is
/// exactly what `dereth_scene::textures::TextureStore::bgra8` yields for a `0x06xxxxxx` id.
///
/// The client's own sequence, for the record: refuse anything wider or taller than 32 pixels,
/// make a transparent 32x32 A8R8G8B8 scratch surface, blit the source into its top-left corner,
/// and build the icon from that. The refusal is what makes [`CURSOR_EXTENT`] a hard limit rather
/// than a scaling hint.
///
/// The client also refuses a surface that is not a device surface; that test has no counterpart
/// here, because everything this rebuild decodes is system memory.
///
/// # Errors
/// [`CursorError`] when the surface is larger than 32x32 in either axis, or carries fewer pixels
/// than its own dimensions claim.
pub fn build(
    width: u32,
    height: u32,
    pixels: &[[u8; 4]],
    hot_x: u32,
    hot_y: u32,
) -> Result<IconBits, CursorError> {
    if width >= 0x21 || height >= 0x21 {
        return Err(CursorError::TooLarge(width, height));
    }
    let need = (width as usize) * (height as usize);
    if pixels.len() < need {
        return Err(CursorError::Truncated(width, height, pixels.len()));
    }

    let e = CURSOR_EXTENT as usize;
    // A fresh buffer filled with 0xFF: every bit starts transparent.
    let mut and_mask = vec![0xFFu8; (e / 8) * e];
    // `PatBlt(hdc, 0, 0, 32, 32, BLACKNESS)`.
    let mut color_bgra = vec![0u8; e * e * 4];

    for y in 0..(height as usize).min(e) {
        for x in 0..(width as usize).min(e) {
            let p = pixels[y * width as usize + x];
            // BGRA in memory: `[0]=B, [1]=G, [2]=R, [3]=A`. The client reads the same word as
            // `0xAARRGGBB` and tests `(px & 0xFF000000) < 0x40000000`.
            let opaque = p[3] >= ALPHA_OPAQUE_MIN;
            if opaque {
                let o = (y * e + x) * 4;
                color_bgra[o] = p[0];
                color_bgra[o + 1] = p[1];
                color_bgra[o + 2] = p[2];
                // The client's colour bitmap is a screen-compatible DDB and carries no alpha at
                // all; the AND mask is the only transparency. Writing 0xFF here is the same
                // picture whether the platform honours the alpha channel or ignores it, whereas
                // writing the surface's own alpha would not be.
                color_bgra[o + 3] = 0xFF;
            }
            if opaque {
                and_mask[y * (e / 8) + x / 8] &= !(0x80 >> (x % 8));
            }
        }
    }
    Ok(IconBits {
        and_mask,
        color_bgra,
        hot_x,
        hot_y,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w x h` block of opaque white with a fully transparent right-hand column.
    fn checker(w: u32, h: u32) -> Vec<[u8; 4]> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let a = if x + 1 == w { 0x00 } else { 0xFF };
                v.push([0xFF, 0xFF, 0xFF, a]);
                let _ = y;
            }
        }
        v
    }

    // Oracle: the client's first test on the source surface is `width < 0x21 && height < 0x21`,
    // so a surface of 32x32 or smaller is accepted and anything larger is refused.
    #[test]
    fn a_surface_larger_than_thirty_two_in_either_axis_is_refused_not_scaled() {
        assert!(build(32, 32, &checker(32, 32), 0, 0).is_ok());
        assert_eq!(
            build(33, 32, &checker(33, 32), 0, 0),
            Err(CursorError::TooLarge(33, 32))
        );
        assert_eq!(
            build(32, 33, &checker(32, 33), 0, 0),
            Err(CursorError::TooLarge(32, 33))
        );
    }

    // Oracle: the mask buffer is filled with 0xFF and only the source's own pixels are cleared,
    // so everything outside a small surface is transparent.
    #[test]
    fn everything_outside_the_source_stays_transparent_in_the_and_mask() {
        let b = build(16, 8, &checker(16, 8), 0, 0).expect("16x8 fits");
        assert_eq!(b.and_mask.len(), 128, "4 bytes per row, 32 rows");
        assert_eq!(b.color_bgra.len(), 32 * 32 * 4);
        // Rows 8..32 were never written.
        for y in 8..32 {
            assert_eq!(&b.and_mask[y * 4..y * 4 + 4], &[0xFF; 4], "row {y}");
        }
        // In row 0 the first 15 pixels are opaque (mask 0), the 16th is transparent, and columns
        // 16..32 were never reached.
        assert_eq!(b.and_mask[0], 0x00, "columns 0-7 opaque");
        assert_eq!(
            b.and_mask[1], 0x01,
            "columns 8-14 opaque, column 15 transparent"
        );
        assert_eq!(
            &b.and_mask[2..4],
            &[0xFF, 0xFF],
            "columns 16-31 outside the surface"
        );
    }

    // Oracle: `(pixel & 0xFF000000) < 0x40000000` in both the client's mask-bitmap and
    // colour-bitmap builders. 0x3F is transparent; 0x40 is not.
    #[test]
    fn the_alpha_threshold_is_0x40_and_a_masked_pixel_is_black() {
        let px = vec![
            [0x11, 0x22, 0x33, 0x3F], // just under
            [0x44, 0x55, 0x66, 0x40], // exactly at
        ];
        let b = build(2, 1, &px, 0, 0).expect("2x1 fits");
        // Pixel 0 transparent -> mask bit set, colour left black by the PatBlt.
        assert_eq!(b.and_mask[0] & 0x80, 0x80);
        assert_eq!(&b.color_bgra[0..4], &[0, 0, 0, 0]);
        // Pixel 1 opaque -> mask bit cleared, colour carried through.
        assert_eq!(b.and_mask[0] & 0x40, 0x00);
        assert_eq!(&b.color_bgra[4..8], &[0x44, 0x55, 0x66, 0xFF]);
    }

    // A surface whose payload is shorter than its own dimensions is refused rather than read past.
    // The client cannot hit this (its source is a real `RenderSurface`); a rebuild whose decoder
    // returned a short buffer would, and a panic in the frame loop is the worst way to find out.
    #[test]
    fn a_short_pixel_buffer_is_an_error_and_not_a_panic() {
        assert_eq!(
            build(8, 8, &checker(8, 7), 0, 0),
            Err(CursorError::Truncated(8, 8, 56))
        );
        assert!(build(8, 8, &checker(8, 8), 0, 0).is_ok());
    }

    // The one-image form carries the mask as the alpha and nothing else: shown pixels are opaque in
    // their own colour whatever alpha the surface stored, and every other pixel -- masked out, or
    // outside the source -- is fully transparent black.
    #[test]
    fn the_rgba_form_is_the_mask_as_alpha_with_every_pixel_shown_or_transparent() {
        let px = vec![
            [0x11, 0x22, 0x33, 0x3F], // under the threshold: transparent
            [0x44, 0x55, 0x66, 0x40], // at it: shown, and opaque
            [0x77, 0x88, 0x99, 0xFF],
        ];
        let rgba = build(3, 1, &px, 0, 0).expect("3x1 fits").rgba();
        assert_eq!(rgba.len(), 32 * 32 * 4);
        assert_eq!(&rgba[0..4], &[0, 0, 0, 0]);
        assert_eq!(&rgba[4..8], &[0x66, 0x55, 0x44, 0xFF], "BGRA read as RGBA");
        assert_eq!(&rgba[8..12], &[0x99, 0x88, 0x77, 0xFF]);
        assert!(
            rgba[12..].iter().all(|b| *b == 0),
            "outside the source is transparent"
        );
        assert!(
            rgba.as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[3] == 0 || p[3] == 0xFF),
            "no pixel is blended"
        );
    }

    // Oracle: the `ICONINFO` the client's cursor builder fills — the hotspot is carried, not clamped or scaled.
    #[test]
    fn the_hotspot_is_carried_verbatim() {
        let b = build(32, 32, &checker(32, 32), 14, 14).expect("32x32 fits");
        assert_eq!((b.hot_x, b.hot_y), (14, 14));
    }
}
