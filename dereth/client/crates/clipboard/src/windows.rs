//! **The only unsafe code in the client's clipboard path.** This crate opts out of the workspace's
//! `unsafe_code = "forbid"` for it -- as `dereth-primitives`' `text` module does for kernel32 NLS and `dereth-render` for
//! D3D12 -- and confines it to this one file.
//!
//! Scope: six kernel32 memory functions and seven user32 clipboard functions, called from two
//! non-generic functions in this file. No registry, file, network, console, locale setter, loader,
//! original-client DLL, callback or window is touched. Nothing here is public outside the crate;
//! `lib.rs` re-exports only `&str` in and `String` out.
//!
//! # Invariants this module rests on
//!
//! 1. **`GlobalAlloc(GMEM_MOVEABLE, n)` returns an `HGLOBAL`, not a pointer**, so it may only be
//!    dereferenced between a `GlobalLock` and its `GlobalUnlock`. Every write below is inside such
//!    a pair, and the pointer never escapes it.
//! 2. **The block is sized from the same slice that fills it.** `n = units.len() * 2` is computed
//!    once by `size_of_val`, and the copy writes exactly `units.len()` code units. No pointer
//!    arithmetic is performed by hand.
//! 3. **`SetClipboardData` takes ownership on success.** After it returns non-null the block must
//!    not be freed, unlocked again, or read; on failure this module still owns it and frees it.
//!    This is the invariant a leak-free transcription depends on, and it is why the `GlobalFree`
//!    calls are all on error paths.
//! 4. **The clipboard is a process-wide lock.** Every path that reaches a successful
//!    `OpenClipboard` also reaches `CloseClipboard`, including early returns and a panic -- which
//!    is what the `Close` guard exists to guarantee. Holding it open would freeze copy and paste
//!    for every other application on the desktop.
//! 5. **`GetClipboardData`'s handle belongs to the clipboard, not to us.** It is valid only until
//!    `CloseClipboard`, so the code units are copied into a `Vec` *before* the guard drops, and
//!    that handle is never freed here.
//! 6. **A `CF_UNICODETEXT` block is not necessarily NUL-terminated, or even-sized.** The length
//!    used is `GlobalSize / 2` -- the client's own halving -- and never a scan for a
//!    terminator, so a truncated or hostile block cannot walk off the end. `decode_units` trims at
//!    the first NUL afterwards, in safe code.

use super::Error;
use core::ffi::c_void;

/// `GMEM_MOVEABLE | GMEM_ZEROINIT` -- the `push 42h` in both legs. The zero-init is
/// load-bearing: it is what terminates the string, because the copy loop does not.
const GMEM_MOVEABLE_ZEROINIT: u32 = 0x42;

#[link(name = "kernel32")]
extern "system" {
    fn GetLastError() -> u32;
    fn GlobalAlloc(flags: u32, bytes: usize) -> *mut c_void;
    fn GlobalFree(handle: *mut c_void) -> *mut c_void;
    fn GlobalLock(handle: *mut c_void) -> *mut c_void;
    fn GlobalUnlock(handle: *mut c_void) -> i32;
    fn GlobalSize(handle: *mut c_void) -> usize;
}

#[link(name = "user32")]
extern "system" {
    fn OpenClipboard(owner: *mut c_void) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, handle: *mut c_void) -> *mut c_void;
    fn GetClipboardData(format: u32) -> *mut c_void;
    fn IsClipboardFormatAvailable(format: u32) -> i32;
    fn GetClipboardSequenceNumber() -> u32;
}

/// `GetClipboardSequenceNumber` -- bumped by every successful `SetClipboardData` anywhere on the
/// desktop.
///
/// Not a retail call: retail reads the clipboard only inside `Paste`, on the keystroke. This build
/// keeps `UiSystem::clipboard` mirrored so that stays synchronous, and this is
/// how that mirror is refreshed **without** opening the clipboard once a frame -- which would take
/// the desktop-wide clipboard lock 60 times a second and fight every other application for it.
pub fn sequence_number() -> u32 {
    // SAFETY: no pointers, no lock, no clipboard open. A monotonic counter read.
    unsafe { GetClipboardSequenceNumber() }
}

fn last_error() -> u32 {
    // SAFETY: no pointers; reads this thread's error word immediately after a failed call.
    unsafe { GetLastError() }
}

/// Closes the clipboard on every exit path, including a panic unwinding through the copy.
///
/// Invariant 4. Constructed *only* after `OpenClipboard` returned non-zero, so `CloseClipboard` is
/// never called without a matching open.
struct Close;
impl Drop for Close {
    fn drop(&mut self) {
        // SAFETY: no pointers. This value exists only on a path where `OpenClipboard` succeeded
        // and has not yet been closed, so the open/close pairing is one-to-one.
        unsafe { CloseClipboard() };
    }
}

/// A locked `HGLOBAL`. Unlocks on every exit path so that invariant 1 holds even under a panic.
///
/// Deliberately does **not** free the handle: ownership differs between the write path
/// (transferred to the clipboard on success) and the read path (never ours at all).
struct Locked {
    handle: *mut c_void,
    ptr: *mut c_void,
}
impl Locked {
    /// `None` if the block could not be locked. `handle` must be a live, non-null `HGLOBAL`.
    fn new(handle: *mut c_void) -> Option<Self> {
        // SAFETY: `handle` came from `GlobalAlloc` or `GetClipboardData` and was checked non-null
        // by the caller; it has not been freed or unlocked since.
        let ptr = unsafe { GlobalLock(handle) };
        if ptr.is_null() {
            None
        } else {
            Some(Self { handle, ptr })
        }
    }
}
impl Drop for Locked {
    fn drop(&mut self) {
        // SAFETY: `self.handle` is the handle `GlobalLock` succeeded on in `new`, still live
        // because no path frees a block while a `Locked` for it is in scope.
        unsafe { GlobalUnlock(self.handle) };
    }
}

/// Put a string on the clipboard, NT leg.
///
/// `units` is [`super::unicode_payload`]'s output: the text's code units plus the terminator.
pub fn set_unicode_text(units: &[u16]) -> Result<(), Error> {
    // Invariant 2: one size, used for both the allocation and the copy. This is retail's
    // `len * 2 + 2` -- because `units` already carries the
    // terminator that the `+ 2` accounts for.
    let bytes = core::mem::size_of_val(units);

    // SAFETY: no pointer arguments. Returns a movable, zero-filled `HGLOBAL` of `bytes` bytes, or
    // null on failure. Retail: `push 42h` / `call GlobalAlloc`.
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE_ZEROINIT, bytes) };
    if handle.is_null() {
        return Err(Error::AllocFailed(last_error()));
    }

    {
        let Some(locked) = Locked::new(handle) else {
            let e = last_error();
            // SAFETY: the lock failed, so no pointer into the block exists and nothing else owns
            // it -- `SetClipboardData` has not been reached, so invariant 3 does not apply yet.
            unsafe { GlobalFree(handle) };
            return Err(Error::AllocFailed(e));
        };
        // SAFETY: invariants 1 and 2. `locked.ptr` is the start of a live `bytes`-byte block that
        // nothing else aliases -- it was allocated two statements ago and no other thread has the
        // handle. `units` is a distinct immutable slice of exactly `bytes` bytes. `HGLOBAL` blocks
        // are aligned for any type, so the `u16` stores are aligned. This is retail's word-copy
        // loop, done as one move.
        unsafe {
            core::ptr::copy_nonoverlapping(units.as_ptr(), locked.ptr.cast::<u16>(), units.len());
        }
        // `locked` drops here: `GlobalUnlock` *before* the handle is handed to the clipboard, which
        // is retail's order too (GlobalUnlock, then OpenClipboard).
    }

    // SAFETY: no pointers -- retail passes, this passes null for the current task
    // (see the crate docs). Non-zero means this process now holds the clipboard lock.
    if unsafe { OpenClipboard(core::ptr::null_mut()) } == 0 {
        let e = last_error();
        // SAFETY: the block was never handed anywhere; invariant 3's transfer has not happened.
        unsafe { GlobalFree(handle) };
        return Err(Error::OpenFailed(e));
    }
    let _close = Close; // invariant 4

    // SAFETY: no pointers. Frees whatever the previous owner left on the clipboard; it is required
    // before `SetClipboardData` and it is what makes this process the owner. Retail does the same.
    unsafe { EmptyClipboard() };

    // SAFETY: `handle` is a live, unlocked `HGLOBAL` whose contents are exactly the format named.
    // Invariant 3: on a non-null return the system owns `handle` and this function must not touch
    // it again -- and below, it does not.
    let placed = unsafe { SetClipboardData(super::CF_UNICODETEXT, handle) };
    if placed.is_null() {
        let e = last_error();
        // SAFETY: the call failed, so ownership did not transfer and the block is still ours.
        unsafe { GlobalFree(handle) };
        return Err(Error::SetFailed(e));
    }
    Ok(())
}

/// Take a string off the clipboard, NT leg.
///
/// `Ok(None)` is `IsClipboardFormatAvailable` returning zero, which the client tests
/// **before** opening the clipboard.
pub fn get_unicode_text() -> Result<Option<String>, Error> {
    // SAFETY: no pointers. A pure query; it does not open the clipboard or take any lock.
    if unsafe { IsClipboardFormatAvailable(super::CF_UNICODETEXT) } == 0 {
        return Ok(None);
    }

    // SAFETY: no pointers; see the write path.
    if unsafe { OpenClipboard(core::ptr::null_mut()) } == 0 {
        return Err(Error::OpenFailed(last_error()));
    }
    let _close = Close; // invariant 4

    // SAFETY: no pointers. The returned `HGLOBAL` belongs to the clipboard (invariant 5): it is
    // read-only to us, must not be freed, and dies at `CloseClipboard`.
    let handle = unsafe { GetClipboardData(super::CF_UNICODETEXT) };
    if handle.is_null() {
        // A format available a moment ago can be gone: another process may have emptied the
        // clipboard between the query and the open. Retail does not check; treat it as absent.
        return Ok(None);
    }

    // SAFETY: `handle` is the live clipboard block; `GlobalSize` reads its header only.
    let size = unsafe { GlobalSize(handle) };
    // Invariant 6: the client's own halving. Integer division discards a trailing
    // odd byte rather than reading past the end.
    let count = size / 2;
    if count == 0 {
        return Ok(Some(String::new()));
    }

    let Some(locked) = Locked::new(handle) else {
        return Err(Error::AllocFailed(last_error()));
    };
    // SAFETY: invariants 1, 5 and 6. `locked.ptr` is the start of a block of at least `size` bytes,
    // so `count = size / 2` `u16`s lie entirely within it; `HGLOBAL` alignment satisfies `u16`. The
    // slice is copied out immediately and does not outlive this scope, and therefore does not
    // outlive `CloseClipboard`, which is what would invalidate it.
    let units = unsafe { core::slice::from_raw_parts(locked.ptr.cast::<u16>(), count) }.to_vec();
    drop(locked);

    super::decode_units(&units).map(Some)
}
