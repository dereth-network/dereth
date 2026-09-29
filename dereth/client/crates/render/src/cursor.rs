//! The mouse cursor: a dat `RenderSurface` turned into a Win32 `HCURSOR`.
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
//! # Two halves
//!
//! [`IconBits`] and [`build`] are the **pure** half: pixels in, a 32x32 colour image and a 1-bit AND
//! mask out. They are the part that can be wrong invisibly (a mask polarity, an off-by-one row, a
//! clamp), so they are testable with no window, no device and no dat.
//!
//! The `windows` half below is the three Win32 calls that turn those bits into an `HCURSOR` and put
//! it on the window. It is behind `cfg(windows)`, which is what gates the `windows` dependency in this
//! crate's manifest.

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
/// exactly what `dereth_client::textures::TextureStore::bgra8` yields for a `0x06xxxxxx` id.
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

#[cfg(windows)]
pub use win32::{set_cursor, take_over_wm_setcursor, WinCursor};

/// The Win32 third of the path: `CreateIconIndirect`, `SetCursor`, and the one piece of glue this
/// rebuild needs that the client did not.
#[cfg(windows)]
mod win32 {
    use std::sync::atomic::{AtomicIsize, Ordering};

    use windows::core::Result as WinResult;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject, HBITMAP};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, CreateIconIndirect, DestroyIcon, GetCursor, SetCursor, SetWindowLongPtrW,
        HCURSOR, HICON, HTCLIENT, ICONINFO, WNDPROC,
    };

    use super::{IconBits, CURSOR_EXTENT};

    /// An owned `HCURSOR` built from a dat surface. Dropping it calls `DestroyIcon`.
    ///
    /// Installing a cursor destroys the *previous* class cursor rather than the one being
    /// installed, which is the same lifetime discipline expressed through the window
    /// class instead of through ownership.
    #[derive(Debug)]
    pub struct WinCursor(HCURSOR);

    // SAFETY: an HCURSOR is a process-wide handle, not thread-affine; the only operations here are
    // SetCursor and DestroyIcon, both of which accept a handle from any thread.
    unsafe impl Send for WinCursor {}

    impl WinCursor {
        /// The tail of cursor creation: two bitmaps into an `ICONINFO` with
        /// `fIcon = 0` — which is what makes it a **cursor** rather than an icon, and is the only
        /// place the hotspot is used.
        ///
        /// # Errors
        /// Any failure from `CreateBitmap` or `CreateIconIndirect`.
        pub fn new(bits: &IconBits) -> WinResult<Self> {
            let e = i32::try_from(CURSOR_EXTENT).unwrap_or(32);
            // SAFETY: both buffers are sized by `super::build` for a 32x32 bitmap of the stated
            // depth — 128 bytes at 1bpp and 4096 at 32bpp — and are read, not retained, by GDI.
            let mask: HBITMAP =
                unsafe { CreateBitmap(e, e, 1, 1, Some(bits.and_mask.as_ptr().cast())) };
            // SAFETY: as above.
            let color: HBITMAP =
                unsafe { CreateBitmap(e, e, 1, 32, Some(bits.color_bgra.as_ptr().cast())) };
            let info = ICONINFO {
                fIcon: false.into(),
                xHotspot: bits.hot_x,
                yHotspot: bits.hot_y,
                hbmMask: mask,
                hbmColor: color,
            };
            // SAFETY: `info` is fully initialised and both bitmaps outlive the call;
            // `CreateIconIndirect` copies them.
            let icon: WinResult<HICON> = unsafe { CreateIconIndirect(&info) };
            // SAFETY: the client deletes both bitmaps immediately after the same call.
            unsafe {
                let _ = DeleteObject(color.into());
                let _ = DeleteObject(mask.into());
            }
            Ok(Self(HCURSOR(icon?.0)))
        }

        /// The raw handle, for [`set_cursor`].
        #[must_use]
        pub fn handle(&self) -> HCURSOR {
            self.0
        }
    }

    impl Drop for WinCursor {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                // SAFETY: this type owns the handle and is the only thing that can free it.
                unsafe {
                    let _ = DestroyIcon(HICON(self.0 .0));
                }
            }
        }
    }

    /// The last line of installing a cursor, `SetCursor(hCursor)`.
    ///
    /// The client also does `SetClassLongA(hwnd, GCL_HCURSOR, h)` so that `DefWindowProc`'s
    /// `WM_SETCURSOR` handling re-installs it on every mouse move. See
    /// [`take_over_wm_setcursor`] for why that is not enough here.
    ///
    /// Returns whether `GetCursor()` afterwards **is** the handle that was just installed. The
    /// immediate check is necessary because `SetCursor` returns the *previous* cursor, which says
    /// nothing about whether the new one took, and a session whose pointer is hidden reports
    /// nothing to an outside observer.
    #[must_use]
    pub fn set_cursor(c: &WinCursor) -> bool {
        // SAFETY: `SetCursor` takes a handle and returns the previous one; `GetCursor` reads the
        // calling thread's current cursor. Neither has an ownership effect.
        unsafe {
            let _ = SetCursor(Some(c.handle()));
            GetCursor() == c.handle()
        }
    }

    /// The cursor this rebuild's window procedure re-installs on `WM_SETCURSOR`, and the previous
    /// window procedure to chain to. Zero means "not installed".
    static CURRENT: AtomicIsize = AtomicIsize::new(0);
    static PREV_PROC: AtomicIsize = AtomicIsize::new(0);

    /// `WM_SETCURSOR`, the message the client never had to handle.
    const WM_SETCURSOR: u32 = 0x0020;

    /// Install a window procedure that answers `WM_SETCURSOR` with the dat cursor, and record which
    /// cursor that is.
    ///
    /// **Why this exists, and why it is not in the client.** Retail owns its window class, so
    /// `SetClassLongA(GCL_HCURSOR)` plus `DefWindowProc` is the whole mechanism: Windows re-installs
    /// the class cursor on every `WM_SETCURSOR` for free. This rebuild's window belongs to `winit`,
    /// whose own procedure handles `WM_SETCURSOR` by calling
    /// `SetCursor(LoadCursorW(0, IDC_ARROW))` — unconditionally, for any position inside the client
    /// area. A bare `SetCursor` from the frame loop therefore survives only until the next pointer
    /// movement, which is precisely when a player is looking at the cursor.
    ///
    /// So the one message is intercepted ahead of `winit` and everything else is chained through
    /// `CallWindowProcW`. Passing a cursor of `None` leaves the subclass in place and hands the
    /// message back to `winit`, which is how the arrow comes back.
    ///
    /// This is a **display** decision only: `ShowCursor`, which mouse-look drives, is a separate
    /// counter and is untouched here.
    pub fn take_over_wm_setcursor(hwnd: isize, cursor: Option<&WinCursor>) {
        CURRENT.store(
            cursor.map_or(0, |c| c.handle().0 as isize),
            Ordering::Relaxed,
        );
        if PREV_PROC.load(Ordering::Relaxed) != 0 {
            return;
        }
        let h = HWND(hwnd as *mut core::ffi::c_void);
        // SAFETY: `hwnd` is the client's own window, alive for the whole run; `subclass_proc` has
        // the `WNDPROC` signature and forwards every message it does not answer to the procedure
        // it replaced.
        let prev = unsafe {
            SetWindowLongPtrW(
                h,
                windows::Win32::UI::WindowsAndMessaging::GWLP_WNDPROC,
                subclass_proc as *const () as isize,
            )
        };
        if prev != 0 {
            PREV_PROC.store(prev, Ordering::Relaxed);
        }
    }

    /// Answer `WM_SETCURSOR` in the client area with the dat cursor; chain everything else.
    unsafe extern "system" fn subclass_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let cur = CURRENT.load(Ordering::Relaxed);
        // The hit-test code `WM_NCHITTEST` returned is in the low word of `lParam`; only the
        // client area is ours. On the frame and the border Windows must keep the sizing cursors.
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        // LINT-OK: the hit-test code Windows packs into the low word of an LPARAM. The mask is the
        // truncation, and it is what `LOWORD` means.
        let in_client = ((lparam.0 as usize as u32) & 0xFFFF) == HTCLIENT;
        if msg == WM_SETCURSOR && cur != 0 && in_client {
            // SAFETY: `cur` is a live HCURSOR held by a `WinCursor` for as long as it is stored.
            unsafe {
                let _ = SetCursor(Some(HCURSOR(cur as *mut core::ffi::c_void)));
            }
            return LRESULT(1);
        }
        let prev = PREV_PROC.load(Ordering::Relaxed);
        // SAFETY: `prev` is the window procedure this one replaced, or 0 before installation.
        unsafe {
            let p: WNDPROC = if prev == 0 {
                None
            } else {
                Some(core::mem::transmute::<
                    isize,
                    unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
                >(prev))
            };
            CallWindowProcW(p, hwnd, msg, wparam, lparam)
        }
    }
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

    // Oracle: the `ICONINFO` the client's cursor builder fills — the hotspot is carried, not clamped or scaled.
    #[test]
    fn the_hotspot_is_carried_verbatim() {
        let b = build(32, 32, &checker(32, 32), 14, 14).expect("32x32 fits");
        assert_eq!((b.hot_x, b.hot_y), (14, 14));
    }
}
