//! Text copy, paste and clipboard sequence queries, kept in their own crate because on Windows
//! they need unsafe code.
//!
//! **Depends on** no other workspace crate (`windows-sys` on Windows, `arboard` elsewhere). **Used
//! by** the desktop host (`dereth-desktop`), which hands it the text the UI's selection produced.
//!
//! **Must never** hold unsafe code outside its one Windows module: the workspace forbids
//! `unsafe_code`, nothing in the dependency set offers a safe clipboard, and this crate is the hop,
//! with the exception scoped to this case alone. It never reaches the client runtime (`cargo xtask
//! seams`, `seam: client crates`).
//!
//! Both retail calls pick their format with one branchless expression, `((os_version != 2 ? -1 : 0)
//! & 0xFFFFFFF4) + 0x0D`: version code 2 (NT) gives `CF_UNICODETEXT`, anything else `CF_TEXT`. The
//! NT leg copies the UTF-16 code units verbatim; the ANSI leg truncates each to its low byte. A
//! rebuild targets NT, so this crate implements the Unicode leg and [`clipboard_format`] records
//! the other. Two deviations, neither observable: the clipboard is opened with a null owner (retail
//! passes its window, which matters only for delayed rendering, which it does not use), and a
//! failed open is checked and its allocation freed (retail leaks it).
//!
//! Off Windows the same two calls go through `arboard` (X11 and Wayland, or the macOS pasteboard):
//! `&str` in, `String` out, `Ok(None)` for a clipboard with no text. On X11 and Wayland the
//! clipboard belongs to the process that last copied, so text copied here is gone when the client
//! exits unless a clipboard manager kept it, and `sequence_number` is `None`.

#![deny(unsafe_code)]

/// The only module in this crate permitted to call Win32. Every other module here, and every
/// consumer of this crate, stays safe -- `dereth-desktop` in particular keeps `#![forbid(unsafe_code)]`.
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

/// The same two calls off Windows, over `arboard`. No `#[allow(unsafe_code)]`: arboard's API is
/// safe, so this module sits under the crate's `deny` like any other.
#[cfg(not(windows))]
mod unix;

/// `CF_UNICODETEXT` -- `0 + 0xd` when the OS version is 2.
pub const CF_UNICODETEXT: u32 = 13;
/// `CF_TEXT` -- `0xfffffff4 + 0xd`, the pre-NT leg, recorded and not implemented.
pub const CF_TEXT: u32 = 1;

/// `VER_PLATFORM_WIN32_NT`, the value the selector compares against.
pub const VER_PLATFORM_WIN32_NT: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// There is no clipboard to reach. Off Windows only: arboard's `ClipboardNotSupported`.
    UnsupportedPlatform,
    /// `OpenClipboard` failed -- another process holds it. Carries `GetLastError`.
    OpenFailed(u32),
    /// `GlobalAlloc` or `GlobalLock` failed. Carries `GetLastError`.
    AllocFailed(u32),
    /// `SetClipboardData` failed; the block was freed. Carries `GetLastError`.
    SetFailed(u32),
    /// The clipboard held `CF_UNICODETEXT` that was not valid UTF-16 -- or, off Windows, text the
    /// backend offered as UTF-8 that was not (arboard's `ConversionFailure`).
    InvalidUtf16,
    /// Off Windows only: arboard reported an error with no Win32 counterpart -- the X server or
    /// compositor could not be reached, another party holds the clipboard, or the pasteboard
    /// refused the write. Carries arboard's own description, the only diagnostic there is. Never
    /// constructed on Windows.
    Backend(String),
}

/// `SetClipboardData`'s first argument, exactly as the client computes it.
///
/// Written the client's way -- as the branchless arithmetic and not as an `if` -- because the
/// constant `0xfffffff4` is the only thing in the client that says which formats are in play, and
/// an `if` would discard it. Both arms are asserted in `tests/cpu/format.rs`.
#[must_use]
pub const fn clipboard_format(os_version: u32) -> u32 {
    // `os_version != 2`, widened to a full-width mask.
    let mask = if os_version != VER_PLATFORM_WIN32_NT {
        0xFFFF_FFFFu32
    } else {
        0
    };
    (mask & 0xFFFF_FFF4).wrapping_add(0x0D)
}

/// The bytes that go in the `HGLOBAL`, as UTF-16 code units.
///
/// `GlobalAlloc(0x42, len * 2 + 2)` is `GMEM_MOVEABLE | GMEM_ZEROINIT`, and the copy loop at
/// The copy loop writes exactly `len` code units -- so the terminator is not written by it, and
/// all, it is the zero the allocation flag left there. That is why this returns `len + 1` units
/// with a trailing `0` and why an interior NUL is *preserved* rather than truncating: the client
/// copies by length, not by terminator.
#[must_use]
pub fn unicode_payload(text: &str) -> Vec<u16> {
    let mut units: Vec<u16> = text.encode_utf16().collect();
    units.push(0); // the GMEM_ZEROINIT tail, not a loop store
    units
}

/// Put a string on the clipboard, Unicode leg.
///
/// # Errors
/// Whichever Win32 step failed; off Windows, whatever arboard reported, as [`Error::Backend`]
/// unless a retail variant fits.
pub fn set_text(text: &str) -> Result<(), Error> {
    #[cfg(windows)]
    {
        windows::set_unicode_text(&unicode_payload(text))
    }
    #[cfg(not(windows))]
    {
        unix::set_text(text)
    }
}

/// Take a string off the clipboard, Unicode leg.
///
/// `Ok(None)` is the `IsClipboardFormatAvailable` arm: the format is not on the clipboard, and
/// the client returns false **without opening the clipboard**. The caller's editable gate is
/// the text element's own business and is enforced in `dereth-ui`, not here.
///
/// Off Windows `Ok(None)` is arboard's `ContentNotAvailable`: the same answer, found by reading
/// rather than by asking first, because arboard has no query.
///
/// # Errors
/// Whichever Win32 step failed; off Windows, whatever arboard reported, as [`Error::Backend`]
/// unless a retail variant fits.
pub fn get_text() -> Result<Option<String>, Error> {
    #[cfg(windows)]
    {
        windows::get_unicode_text()
    }
    #[cfg(not(windows))]
    {
        unix::get_text()
    }
}

/// Decode what `GetClipboardData` handed back.
///
/// `GlobalSize` counts bytes including the terminator and the client halves it with an integer shift, so the
/// buffer the client copies ends in the NUL the writer left. Trailing NULs are therefore not text.
/// Kept out of the Windows module so that it is exercised by an ordinary test.
///
/// # Errors
/// [`Error::InvalidUtf16`] for an unpaired surrogate.
pub fn decode_units(units: &[u16]) -> Result<String, Error> {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16(&units[..end]).map_err(|_| Error::InvalidUtf16)
}

/// `GetClipboardSequenceNumber`, or `None` off Windows.
///
/// Every successful `SetClipboardData` on the desktop bumps it. A host that mirrors the clipboard
/// into [`get_text`] can poll this instead of opening the clipboard, which is what keeps the
/// per-frame refresh from contending with every other application on the machine.
///
/// `None` off Windows, and not a stub: X11 and Wayland have no such counter at all (ownership
/// changes are events, not state), and macOS's `NSPasteboard.changeCount` is not surfaced by
/// arboard -- so a host there refreshes on the paste, as retail did, instead of once a frame.
#[must_use]
pub fn sequence_number() -> Option<u32> {
    #[cfg(windows)]
    {
        Some(windows::sequence_number())
    }
    #[cfg(not(windows))]
    {
        None
    }
}
