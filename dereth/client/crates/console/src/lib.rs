//! The console the Windows client has to ask for, because its binary is linked under the windows
//! subsystem.
//!
//! **Depends on** no other workspace crate (`windows-sys`, on Windows). **Used by** the client
//! (`dereth-client`).
//!
//! **Must never** hold, dereference or close a handle, or pass a buffer: its unsafe is exactly two
//! kernel32 calls (`AttachConsole`, `AllocConsole`) plus `GetLastError`, and everything else about
//! the console is what `std` already does. It never reaches the client runtime (`cargo xtask
//! seams`, `seam: client crates`).
//!
//! Under the windows subsystem Windows creates no console, which is what lets `--no-console` avoid
//! even a flash of one; the cost is that output goes nowhere by default, and [`attach`] buys it
//! back:
//!
//! | how the client was started | what [`attach`] does | what the user sees |
//! |---|---|---|
//! | from a terminal | `AttachConsole(ATTACH_PARENT_PROCESS)` | output in that terminal |
//! | from Explorer, or from the launcher | `AllocConsole` | a console window |
//! | with `--no-console` | nothing is called | no console |
//!
//! The middle row keeps a plain start unchanged: the client's stderr is the only place several
//! acceptance measurements are printed.

#![deny(unsafe_code)]

#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

#[cfg(windows)]
#[allow(unsafe_code)]
mod message_box;

/// Show the operating system's modal error box and return when the player has closed it.
///
/// This is the other thing the client says outside its own window: the box it shows when its
/// first connection fails, before it has a screen to say it on. `style` is the box's style bits
/// (the buttons, the icon, top-most); the box has no owner window.
///
/// Off Windows nothing is shown: the caller has already logged the same text (on stderr), which
/// is the only surface such a build has.
pub fn error_box(caption: &str, text: &str, style: u32) {
    #[cfg(windows)]
    {
        message_box::error_box(caption, text, style);
    }
    #[cfg(not(windows))]
    {
        let _ = (caption, text, style);
    }
}

/// What [`attach`] found, or made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Console {
    /// `AttachConsole(ATTACH_PARENT_PROCESS)` succeeded: the client is writing to the terminal it
    /// was started from. Note that the shell does **not** wait for a windows-subsystem process, so
    /// output arrives after the prompt has come back. That is the shell's behaviour and not
    /// something the client can change from this side; a launcher that waits on the process
    /// (`Start-Process -PassThru` + `WaitForExit`) rather than on a pipe sees all of it.
    Parent,
    /// There was no parent console -- the Explorer and launcher case -- and `AllocConsole` made
    /// one. This is the row that preserves what a console-subsystem binary does.
    Allocated,
    /// The process already had a console, so neither call was needed. Unreachable while the binary
    /// is linked for the windows subsystem, and correct if it ever is not.
    Existing,
    /// Both calls failed, or this is not Windows. Output goes nowhere; the run is otherwise
    /// unaffected, which is why this is a value and not an error.
    None,
}

impl Console {
    /// Whether anything can be written. For a caller that wants to say so in a log.
    #[must_use]
    pub fn is_open(self) -> bool {
        self != Self::None
    }
}

/// Borrow the console of whatever started us, or make one.
///
/// Call this **first thing in `main`**, before anything writes to stdout or stderr: `std` resolves
/// the standard handles when it writes, and a write that happens first goes nowhere.
///
/// Never called when `--no-console` was given -- that switch is precisely "do not call this".
#[must_use]
pub fn attach() -> Console {
    #[cfg(windows)]
    {
        windows::attach()
    }
    #[cfg(not(windows))]
    {
        // The non-Windows build of the client exists to keep `cargo check` honest on a non-Windows
        // host; it cannot open a device or a window either. The process keeps whatever streams it
        // was started with, which on a Unix shell is the terminal, so there is nothing to do and
        // nothing is lost.
        Console::None
    }
}
