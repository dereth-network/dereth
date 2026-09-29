//! The non-Windows leg of both clipboard calls, over `arboard`.
//!
//! No `unsafe` and no `#[allow(unsafe_code)]`: arboard's API is safe, so this module sits under
//! the crate's `deny` like any other. `lib.rs` still re-exports only `&str` in and `String` out.
//!
//! # One handle for the process, not one per call
//!
//! Retail opens and closes the Windows clipboard per call, and a per-call `arboard::Clipboard`
//! would mirror that -- except on X11. There the copied text lives in the process that copied it
//! (arboard serves it from a background thread), and dropping the *last* `Clipboard` tears that
//! thread down after offering the text to a clipboard manager; with no manager running (GNOME
//! ships none) the text would vanish the moment `set_text` returned. So the handle is created on
//! first use and kept in `CLIPBOARD` for the life of the process. Wayland serves the copy from a
//! detached thread and macOS hands it to the pasteboard server, so the handle's lifetime does not
//! matter there; the one policy covers all three.
//!
//! # The caveat arboard documents (Linux, X11 and Wayland alike)
//!
//! The clipboard is owned by the process that last copied to it, so text this client copied is
//! **gone when the client exits** unless a clipboard manager snapshotted it. That is the platform
//! and not this crate: Windows keeps the `HGLOBAL` after the writer dies; X11 and Wayland have
//! nothing to keep it. arboard's `SetExtLinux::wait` would block until the next copy, which a
//! game client cannot do.

use super::Error;
use std::sync::{Mutex, PoisonError};

/// The process-wide handle; see the module docs. `None` until the first call.
static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// Runs `op` against the process-wide handle, creating it on first use.
///
/// Serialised by the mutex: arboard reports `ClipboardOccupied` for concurrent operations from one
/// process, and the retail contract is one clipboard per task anyway.
fn with_clipboard<T>(
    op: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
) -> Result<T, Error> {
    // A panic inside `op` cannot leave the handle half-written, so a poisoned lock still guards a
    // usable handle.
    let mut guard = CLIPBOARD.lock().unwrap_or_else(PoisonError::into_inner);
    let clipboard = match &mut *guard {
        Some(clipboard) => clipboard,
        slot @ None => slot.insert(arboard::Clipboard::new().map_err(map_error)?),
    };
    op(clipboard).map_err(map_error)
}

/// arboard's variants onto this crate's. `ContentNotAvailable` is the read path's `Ok(None)` and
/// is matched there before this runs.
fn map_error(error: arboard::Error) -> Error {
    match error {
        // No clipboard to reach in this session -- arboard raises it for selections the
        // compositor lacks, which for the regular clipboard should not happen.
        arboard::Error::ClipboardNotSupported => Error::UnsupportedPlatform,
        // Text the backend offered as UTF-8 that was not: the Win32 case one encoding over.
        arboard::Error::ConversionFailure => Error::InvalidUtf16,
        // `ClipboardOccupied`, `Unknown { description }` and whatever a later arboard adds (the
        // enum is `#[non_exhaustive]`). The `Display` text is the only diagnostic there is.
        other => Error::Backend(other.to_string()),
    }
}

/// Put a string on the clipboard, non-Windows leg.
///
/// Takes the `&str` and not [`super::unicode_payload`]'s units: there is no `HGLOBAL` to fill,
/// arboard wants UTF-8, and the terminator the `GMEM_ZEROINIT` tail supplied has no counterpart.
pub fn set_text(text: &str) -> Result<(), Error> {
    with_clipboard(|clipboard| clipboard.set_text(text))
}

/// Take a string off the clipboard, non-Windows leg.
///
/// `Ok(None)` is arboard's `ContentNotAvailable` -- empty, or holding something that is not text --
/// and stands in for the `IsClipboardFormatAvailable` query the Windows leg makes first. arboard
/// has no separate query, so unlike retail this asks by reading.
pub fn get_text() -> Result<Option<String>, Error> {
    with_clipboard(|clipboard| match clipboard.get_text() {
        Ok(text) => Ok(Some(text)),
        Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(error),
    })
}
