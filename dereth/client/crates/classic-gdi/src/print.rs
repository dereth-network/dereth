//! Printing an image: the Help viewer's Print command.
//!
//! Nothing prints without the player: the system's print dialog opens, and only the printer and
//! settings chosen there are used. The image is scaled to the printable width in physical inches
//! (so a printer with unequal horizontal and vertical resolution keeps the aspect ratio) and cut
//! into as many pages as its height needs, each page taking whole source rows.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Page {
    source_y: u32,
    source_height: u32,
    destination_width: i32,
    destination_height: i32,
}

fn pages(
    width: u32,
    height: u32,
    printable_width: i32,
    printable_height: i32,
    dpi_x: i32,
    dpi_y: i32,
) -> Result<Vec<Page>, String> {
    if width == 0
        || height == 0
        || width > i32::MAX as u32
        || height > i32::MAX as u32
        || printable_width <= 0
        || printable_height <= 0
        || dpi_x <= 0
        || dpi_y <= 0
    {
        return Err("Image or printer dimensions are invalid".into());
    }
    // Scale in physical inches, including printers with unequal X/Y resolution.
    let numerator = printable_width as u128 * dpi_y as u128;
    let denominator = width as u128 * dpi_x as u128;
    let rows = (printable_height as u128 * denominator / numerator).min(height as u128);
    if rows == 0 {
        return Err("A source row does not fit the printable page".into());
    }
    let count = (height as u128).div_ceil(rows);
    if count > u16::MAX as u128 {
        return Err("Help document exceeds the printer page limit".into());
    }
    let mut result = Vec::with_capacity(count as usize);
    let mut y = 0;
    while y < height {
        let source_height = u32::try_from(rows).unwrap_or(u32::MAX).min(height - y);
        let destination_height =
            i32::try_from((source_height as u128 * numerator).div_ceil(denominator))
                .unwrap_or(i32::MAX);
        result.push(Page {
            source_y: y,
            source_height,
            destination_width: printable_width,
            destination_height,
        });
        y += source_height;
    }
    Ok(result)
}

fn check_pixels(width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let bytes = usize::try_from(width)
        .ok()
        .and_then(|w| w.checked_mul(height as usize))
        .and_then(|n| n.checked_mul(4));
    if width == 0
        || height == 0
        || width > i32::MAX as u32
        || height > i32::MAX as u32
        || bytes != Some(rgba.len())
    {
        return Err("RGBA image dimensions do not match its buffer".into());
    }
    Ok(())
}
fn bgrx_on_white(rgba: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(rgba.len());
    for pixel in rgba.as_chunks::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for channel in [pixel[2], pixel[1], pixel[0]] {
            // A weighted mean of `channel` and white, so at most 255.
            let mixed = (u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255;
            output.push(u8::try_from(mixed).unwrap_or(u8::MAX));
        }
        output.push(0);
    }
    output
}

/// Open the print dialog and print `rgba` (rows top first) on the chosen printer. Cancelling the
/// dialog succeeds without a print job. Call it on the window's thread, only after the player's
/// Print command; `owner` is the window's native handle.
///
/// # Errors
/// When the dialog, the printer or any page fails.
#[cfg(windows)]
#[allow(unsafe_code, clippy::undocumented_unsafe_blocks)]
pub fn print_rgba(owner: isize, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    use std::{
        mem::{size_of, zeroed},
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::{GlobalFree, HWND},
        Graphics::Gdi::{
            DeleteDC, GetDeviceCaps, SetBrushOrgEx, SetStretchBltMode, StretchDIBits, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HALFTONE, HORZRES, LOGPIXELSX, LOGPIXELSY,
            RASTERCAPS, RC_STRETCHDIB, RGBQUAD, SRCCOPY, VERTRES,
        },
        Storage::Xps::{AbortDoc, EndDoc, EndPage, StartDocW, StartPage, DOCINFOW},
        UI::Controls::Dialogs::{
            CommDlgExtendedError, PrintDlgW, PD_HIDEPRINTTOFILE, PD_NOPAGENUMS, PD_NOSELECTION,
            PD_RETURNDC, PD_USEDEVMODECOPIESANDCOLLATE, PRINTDLGW,
        },
    };
    check_pixels(width, height, rgba)?;
    struct Dialog(PRINTDLGW);
    impl Drop for Dialog {
        fn drop(&mut self) {
            unsafe {
                let dc = self.0.hDC;
                if !dc.is_null() {
                    DeleteDC(dc);
                }
                let mode = self.0.hDevMode;
                if !mode.is_null() {
                    GlobalFree(mode);
                }
                let names = self.0.hDevNames;
                if !names.is_null() {
                    GlobalFree(names);
                }
            }
        }
    }
    struct Job {
        dc: windows_sys::Win32::Graphics::Gdi::HDC,
        active: bool,
    }
    impl Drop for Job {
        fn drop(&mut self) {
            if self.active {
                unsafe {
                    AbortDoc(self.dc);
                }
            }
        }
    }
    let mut dialog = Dialog(unsafe { zeroed() });
    dialog.0.lStructSize = u32::try_from(size_of::<PRINTDLGW>()).unwrap_or(u32::MAX);
    dialog.0.hwndOwner = owner as HWND;
    dialog.0.Flags = PD_RETURNDC
        | PD_NOSELECTION
        | PD_NOPAGENUMS
        | PD_USEDEVMODECOPIESANDCOLLATE
        | PD_HIDEPRINTTOFILE;
    dialog.0.nCopies = 1;
    dialog.0.nMinPage = 1;
    dialog.0.nMaxPage = u16::MAX;
    if unsafe { PrintDlgW(&mut dialog.0) } == 0 {
        let error = unsafe { CommDlgExtendedError() };
        return if error == 0 {
            Ok(())
        } else {
            Err(format!("Printer dialog failed (0x{error:08X})"))
        };
    }
    let dc = dialog.0.hDC;
    if dc.is_null() {
        return Err("The selected printer returned no device context".into());
    }
    if unsafe { GetDeviceCaps(dc, RASTERCAPS as i32) } & RC_STRETCHDIB as i32 == 0 {
        return Err("The selected printer does not support raster images".into());
    }
    let plan = pages(
        width,
        height,
        unsafe { GetDeviceCaps(dc, HORZRES as i32) },
        unsafe { GetDeviceCaps(dc, VERTRES as i32) },
        unsafe { GetDeviceCaps(dc, LOGPIXELSX as i32) },
        unsafe { GetDeviceCaps(dc, LOGPIXELSY as i32) },
    )?;
    let title: Vec<u16> = "Classic Help".encode_utf16().chain(Some(0)).collect();
    let info = DOCINFOW {
        cbSize: i32::try_from(size_of::<DOCINFOW>()).unwrap_or(i32::MAX),
        lpszDocName: title.as_ptr(),
        lpszOutput: null(),
        lpszDatatype: null(),
        fwType: 0,
    };
    if unsafe { StartDocW(dc, &info) } <= 0 {
        return Err("The printer could not start the document".into());
    }
    let mut job = Job { dc, active: true };
    for page in &plan {
        if unsafe { StartPage(dc) } <= 0 {
            return Err("The printer could not start a page".into());
        }
        if unsafe { SetStretchBltMode(dc, HALFTONE) } == 0
            || unsafe { SetBrushOrgEx(dc, 0, 0, null_mut()) } == 0
        {
            return Err("The printer could not configure image scaling".into());
        }
        let stride = width as usize * 4;
        let start = page.source_y as usize * stride;
        let end = start + page.source_height as usize * stride;
        let pixels = bgrx_on_white(&rgba[start..end]);
        let byte_count = u32::try_from(pixels.len())
            .map_err(|_| "Printer image slice exceeds the DIB size limit")?;
        let bitmap = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: u32::try_from(size_of::<BITMAPINFOHEADER>()).unwrap_or(u32::MAX),
                biWidth: width as i32,
                biHeight: -(page.source_height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: byte_count,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };
        let copied = unsafe {
            StretchDIBits(
                dc,
                0,
                0,
                page.destination_width,
                page.destination_height,
                0,
                0,
                width as i32,
                page.source_height as i32,
                pixels.as_ptr().cast(),
                &bitmap,
                DIB_RGB_COLORS,
                SRCCOPY,
            )
        };
        if copied <= 0 {
            return Err("The printer could not render the page image".into());
        }
        if unsafe { EndPage(dc) } <= 0 {
            return Err("The printer could not finish a page".into());
        }
    }
    if unsafe { EndDoc(dc) } <= 0 {
        return Err("The printer could not finish the document".into());
    }
    job.active = false;
    Ok(())
}

/// Printing needs the Windows print system.
///
/// # Errors
/// Always, off Windows.
#[cfg(not(windows))]
pub fn print_rgba(_owner: isize, _width: u32, _height: u32, _rgba: &[u8]) -> Result<(), String> {
    Err("Native help printing is available only on Windows".into())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (page planning for the system printer; no retail behaviour).
    use super::*;
    #[test]
    fn vertical_pages_cover_every_source_row_once_without_stretching_the_last_page() {
        let result = pages(800, 2401, 2400, 3000, 300, 300).unwrap();
        assert_eq!(
            result,
            [
                Page {
                    source_y: 0,
                    source_height: 1000,
                    destination_width: 2400,
                    destination_height: 3000
                },
                Page {
                    source_y: 1000,
                    source_height: 1000,
                    destination_width: 2400,
                    destination_height: 3000
                },
                Page {
                    source_y: 2000,
                    source_height: 401,
                    destination_width: 2400,
                    destination_height: 1203
                }
            ]
        );
    }
    #[test]
    fn unequal_printer_dpi_preserves_physical_aspect_ratio() {
        let result = pages(800, 800, 2400, 2000, 600, 300).unwrap();
        assert_eq!(
            result,
            [Page {
                source_y: 0,
                source_height: 800,
                destination_width: 2400,
                destination_height: 1200
            }]
        );
    }
    #[test]
    fn rounding_does_not_drop_rows_or_overrun_the_printable_height() {
        let result = pages(333, 1000, 1000, 1001, 300, 300).unwrap();
        assert_eq!(result.iter().map(|p| p.source_height).sum::<u32>(), 1000);
        assert!(result.iter().all(|p| p.destination_height <= 1001));
        for pair in result.windows(2) {
            assert_eq!(pair[0].source_y + pair[0].source_height, pair[1].source_y);
        }
        assert!(pages(0, 1, 1, 1, 1, 1).is_err());
        assert!(pages(1, 1, 100, 1, 1, 1).is_err());
        assert!(pages(1, 1, 1, 1, 0, 1).is_err());
    }
    #[test]
    fn rgba_is_composited_on_white_and_reordered_to_bgrx() {
        assert_eq!(
            bgrx_on_white(&[10, 20, 30, 255, 0, 0, 0, 0, 200, 100, 0, 128]),
            [30, 20, 10, 0, 255, 255, 255, 0, 127, 177, 227, 0]
        );
        assert!(check_pixels(3, 1, &[0; 12]).is_ok());
        assert!(check_pixels(3, 1, &[0; 11]).is_err());
    }
}
