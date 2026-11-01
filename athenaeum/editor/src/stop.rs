//! Starting a program so that it can be asked to stop, and asking it as its operator would: the
//! server then keeps every character still in the world before it ends, where ending its process
//! would lose what they did since they were last kept.
//!
//! On Windows the ask is Ctrl-Break, sent to the program's own process group on its console: the
//! program is started in a group of its own, on the editor's console when the editor has one (a
//! build run from a terminal, or started by a program that gave it one without a window), else on
//! a console of its own with no window, which the editor borrows for the moment it takes to send
//! the break. Elsewhere it is SIGTERM.

use std::process::Command;

/// Start `cmd` so that [`ask`] can reach it, with no console window of its own.
pub fn askable(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(win::creation_flags());
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Start `cmd` with no console window of its own (a client: it has its own window).
pub fn windowless(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if !win::has_console() {
            cmd.creation_flags(win::CREATE_NO_WINDOW);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Ask the program `pid`, started by [`askable`], to stop. Answers whether the ask was sent.
#[must_use]
pub fn ask(pid: u32) -> bool {
    #[cfg(windows)]
    {
        win::ctrl_break(pid)
    }
    #[cfg(not(windows))]
    {
        Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .is_ok_and(|s| s.success())
    }
}

/// **The only unsafe code in the editor.** Four kernel32 calls, each taking and returning plain
/// numbers, none holding anything after it returns.
#[cfg(windows)]
#[allow(unsafe_code)]
mod win {
    /// A new process group: the break reaches the program and nothing else.
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    /// A console with no window.
    pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CTRL_BREAK_EVENT: u32 = 1;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleCP() -> u32;
        fn AttachConsole(process_id: u32) -> i32;
        fn FreeConsole() -> i32;
        fn GenerateConsoleCtrlEvent(event: u32, process_group: u32) -> i32;
    }

    /// Whether the editor has a console: a console's code page is never 0, and asking with none
    /// answers 0. (Whether it has a console *window* is another question: a console given without
    /// one is a console all the same, and a program started on it shares it.)
    pub fn has_console() -> bool {
        // SAFETY: no arguments; a number out.
        let code_page = unsafe { GetConsoleCP() };
        code_page != 0
    }

    /// The creation flags of a program [`ctrl_break`] can reach.
    pub fn creation_flags() -> u32 {
        if has_console() {
            CREATE_NEW_PROCESS_GROUP
        } else {
            CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
        }
    }

    /// Ctrl-Break to the group of `pid`: on the editor's own console when it has one, else on the
    /// program's, borrowed while the break is sent. The break goes to the program's group alone,
    /// never to the editor's.
    pub fn ctrl_break(pid: u32) -> bool {
        if has_console() {
            // SAFETY: two numbers in, a number out.
            return unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) } != 0;
        }
        // SAFETY: a process id in, a number out.
        if unsafe { AttachConsole(pid) } == 0 {
            return false;
        }
        // SAFETY: numbers in and out; the console borrowed above is let go.
        unsafe {
            let sent = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid) != 0;
            FreeConsole();
            sent
        }
    }
}
