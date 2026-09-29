//! **The only unsafe code in the client's console path.** One kernel32 call, in one function.
//!
//! Scope: `AttachConsole` and `GetLastError`. No memory is allocated, no handle is returned or
//! stored, no pointer crosses the boundary in either direction, and nothing here is public outside
//! the crate. `lib.rs` re-exports a plain enum.
//!
//! # Invariants this module rests on
//!
//! 1. **The call takes and returns no pointer.** `AttachConsole` takes a process id by value and
//!    returns `BOOL`. There is no lifetime, no ownership and no cleanup obligation, which is why
//!    the whole module is two lines of `unsafe` and no `Drop`.
//! 2. **It is idempotent-by-failure.** A process may have at most one console, and a second
//!    attach fails with `ERROR_ACCESS_DENIED` rather than doing damage. So calling [`attach`]
//!    twice is safe and the second call reports [`Console::Existing`].
//! 3. **No console is ever created.** `AllocConsole` is deliberately not declared: a client
//!    started from Explorer or the launcher has no console to borrow, and making one would put a
//!    console window on the screen beside the game.
//! 4. **The standard handles are the console's job, not ours.** `AttachConsole` sets the
//!    process's `STD_*` handles when it succeeds, and Rust's Windows `Stdout`/`Stderr` fetch the
//!    handle on every write rather than caching one, so a `println!` after this returns finds the
//!    console. Nothing here calls `SetStdHandle`, and redirection the caller set up with `>` is
//!    therefore whatever the Win32 console host decides it is -- see the note on [`attach`].

use super::Console;

/// `AttachConsole`'s "the console of my parent process" sentinel: `(DWORD)-1`.
const ATTACH_PARENT_PROCESS: u32 = 0xFFFF_FFFF;

/// `ERROR_ACCESS_DENIED`. `AttachConsole` returns it for "this process already has a console",
/// which is the one failure that is not a failure.
const ERROR_ACCESS_DENIED: u32 = 5;

#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
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
/// One thing this deliberately does not do is reopen `CONOUT$` and `SetStdHandle` to it. That
/// recipe exists to paper over consoles whose handles the host left unset, and it also overwrites
/// a redirection the caller asked for (`dereth-client.exe > out.txt`). Taking the call at face
/// value keeps redirection working.
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
        // Invariant 2: we already have a console.
        return Console::Existing;
    }
    // Invariant 3: nothing to borrow, and nothing is made.
    Console::None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **Invariant 2, and the only claim these declarations can be held to without side effects.**
    ///
    /// A process may have at most one console, so an [`attach`] made by a process that already has
    /// one must report [`Console::Existing`] -- i.e. `AttachConsole` failed with
    /// `ERROR_ACCESS_DENIED`. A test binary is a console-subsystem executable, so an ordinary
    /// `cargo test` run *is* that process.
    ///
    /// The skip is deliberate rather than an assertion that the runner has a console: a runner
    /// without one has nothing to prove here, and
    /// [`a_process_started_without_a_console_is_not_given_one`] covers that case.
    #[test]
    fn a_process_that_has_a_console_is_told_it_has_one() {
        if !has_console_window() {
            // Said out loud, because a test that can skip and says nothing is a test that has
            // been passing for months without running. `cargo test -- --nocapture` shows this.
            eprintln!("dereth-console: no console here -- skipped");
            return;
        }
        assert_eq!(attach(), Console::Existing);
        // Idempotent: asking again fails the same way.
        assert_eq!(attach(), Console::Existing);
    }

    /// `DETACHED_PROCESS`: the child starts with no console, and Windows does not make one for it.
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    /// What this test binary is being run as, when the test below starts it again.
    const ROLE: &str = "DERETH_CONSOLE_TEST_ROLE";
    /// The test's own path, so a re-run of this binary runs this test and nothing else.
    const THIS_TEST: &str = "windows::tests::a_process_started_without_a_console_is_not_given_one";

    /// Run this test again in a new process with no console, as `role`, and return its exit code.
    fn detached(role: &str) -> Option<i32> {
        use std::os::windows::process::CommandExt as _;
        let exe = std::env::current_exe().expect("the test binary's path");
        std::process::Command::new(exe)
            .args(["--exact", THIS_TEST, "--test-threads=1", "--nocapture"])
            .env(ROLE, role)
            .creation_flags(DETACHED_PROCESS)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("the test binary starts again")
            .code()
    }

    /// **Invariant 3: a client started from Explorer or the launcher gets no console window.**
    ///
    /// That start is a process with no console whose parent has none either, so there is nothing
    /// to borrow. It cannot be staged inside the test runner, which has a console its child would
    /// borrow, so the test starts itself twice: a detached *middle* process with no console, and
    /// from it a detached *child*, whose parent is then console-less exactly as Explorer is. The
    /// child calls [`attach`] and reports, through its exit code, whether it was left without a
    /// console. A build that made one in that case (the console window beside the game) exits 3.
    #[test]
    fn a_process_started_without_a_console_is_not_given_one() {
        match std::env::var(ROLE).as_deref() {
            Ok("child") => {
                let ok = attach() == Console::None && !has_console_window();
                std::process::exit(if ok { 0 } else { 3 });
            }
            Ok("middle") => std::process::exit(detached("child").unwrap_or(4)),
            _ => assert_eq!(
                detached("middle"),
                Some(0),
                "a process with no console to borrow is left without one"
            ),
        }
    }
}
