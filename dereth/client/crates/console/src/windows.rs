//! **The only unsafe code in the client's console path.** Two kernel32 calls, in one function.
//!
//! Scope: `AttachConsole`, `AllocConsole` and `GetLastError`. No memory is allocated, no handle is
//! returned or stored, no pointer crosses the boundary in either direction, and nothing here is
//! public outside the crate. `lib.rs` re-exports a plain enum.
//!
//! # Invariants this module rests on
//!
//! 1. **Neither call takes or returns a pointer.** `AttachConsole` takes a process id by value and
//!    `AllocConsole` takes nothing; both return `BOOL`. There is no lifetime, no ownership and no
//!    cleanup obligation, which is why the whole module is four lines of `unsafe` and no `Drop`.
//! 2. **They are both idempotent-by-failure.** A process may have at most one console, and a
//!    second attach or alloc fails with `ERROR_ACCESS_DENIED` rather than doing damage. So calling
//!    [`attach`] twice is safe and the second call reports [`Console::Existing`].
//! 3. **The standard handles are the console's job, not ours.** Both functions set the process's
//!    `STD_*` handles when they succeed, and Rust's Windows `Stdout`/`Stderr` fetch the handle on
//!    every write rather than caching one, so a `println!` after this returns finds the console.
//!    Nothing here calls `SetStdHandle`, and redirection the caller set up with `>` is therefore
//!    whatever the Win32 console host decides it is -- see the note on [`attach`].

use super::Console;

/// `AttachConsole`'s "the console of my parent process" sentinel: `(DWORD)-1`.
const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;

/// `ERROR_ACCESS_DENIED`. `AttachConsole` returns it for "this process already has a console",
/// which is the one failure that is not a failure.
const ERROR_ACCESS_DENIED: u32 = 5;

#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
    fn AllocConsole() -> i32;
    fn GetLastError() -> u32;
    #[cfg(test)]
    fn GetConsoleWindow() -> *mut core::ffi::c_void;
}

/// Whether this process already has a console, for the test below and nothing else.
///
/// Not public, and not used by [`attach`]: `GetConsoleWindow` answers "is there a console
/// *window*", which is not quite "is there a console" -- a pseudoconsole has one and may hand back
/// a hidden dummy, and a service may have neither. `AttachConsole`'s own `ERROR_ACCESS_DENIED` is
/// the authoritative answer and is what [`attach`] uses. This is only ever asked in the direction
/// where it is reliable: a non-null handle means there is certainly a console.
///
/// The returned handle is compared against null and never dereferenced, passed on or closed.
#[cfg(test)]
fn has_console_window() -> bool {
    // SAFETY: no arguments, and the returned `HWND` is only compared against null.
    !unsafe { GetConsoleWindow() }.is_null()
}

/// See [`super::attach`].
///
/// The order matters: attach first, allocate only if there was nothing to attach to. Doing it the
/// other way round would give a client started from a terminal its own second window and leave the
/// terminal it was started from silent.
///
/// One thing this deliberately does not do is reopen `CONOUT$` and `SetStdHandle` to it. That
/// recipe exists to paper over consoles whose handles the host left unset, and it also overwrites
/// a redirection the caller asked for (`dereth-client.exe > out.txt`). Taking the two calls at face
/// value keeps redirection working and matches what `dereth-launcher` has been doing since it was
/// written.
pub fn attach() -> Console {
    // SAFETY: `AttachConsole` takes a process id by value and returns `BOOL`. There are no
    // preconditions; it fails harmlessly when the parent has no console, which is the Explorer and
    // launcher case, and when this process already has one.
    if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) } != 0 {
        return Console::Parent;
    }
    // SAFETY: reading the calling thread's last-error code. No arguments, no pointers. Valid here
    // because the call above is the most recent Win32 call on this thread and it failed.
    if unsafe { GetLastError() } == ERROR_ACCESS_DENIED {
        // Invariant 2: we already have a console. Allocating would fail the same way.
        return Console::Existing;
    }
    // SAFETY: `AllocConsole` takes no arguments and returns `BOOL`. It fails rather than doing
    // damage when a console already exists, which invariant 2 has just ruled out anyway.
    if unsafe { AllocConsole() } != 0 {
        Console::Allocated
    } else {
        Console::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Invariant 2, and the only claim these declarations can be held to without side effects.**
    ///
    /// A process may have at most one console, so an [`attach`] made by a process that already has
    /// one must report [`Console::Existing`] -- i.e. `AttachConsole` failed with
    /// `ERROR_ACCESS_DENIED` and `AllocConsole` was never reached. A test binary is a
    /// console-subsystem executable, so an ordinary `cargo test` run *is* that process.
    ///
    /// The skip is deliberate rather than an assertion that the runner has a console: a runner
    /// without one would make this test *create* one, which is a console window appearing during
    /// `cargo test`. There is nothing to prove in that case and the test declines to prove it.
    #[test]
    fn a_process_that_has_a_console_is_told_it_has_one() {
        if !has_console_window() {
            // Said out loud, because a test that can skip and says nothing is a test that has
            // been passing for months without running. `cargo test -- --nocapture` shows this.
            eprintln!("dereth-console: no console here -- skipped, and nothing was allocated");
            return;
        }
        assert_eq!(attach(), Console::Existing);
        // Idempotent: asking again neither allocates nor fails differently.
        assert_eq!(attach(), Console::Existing);
    }
}
