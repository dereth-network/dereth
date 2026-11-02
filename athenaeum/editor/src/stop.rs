//! Starting the programs the editor runs without a console window of their own, and stopping the
//! server as its operator does: `stop` typed at its console (its standard input, which the editor
//! holds), after which it keeps every character still in the world and ends. Ending its process
//! would lose what they did since they were last kept, so that is done only past a guard, against
//! a hang.

use std::io::Write;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// What the server is told at its console to stop at once, keeping its characters.
pub const STOP: &str = "stop";

/// Start `cmd` with no console window of its own. On Windows a console program started by a
/// program with no console (the editor's release build) would open one; its input and output are
/// the editor's pipes and files, so it needs none. Elsewhere nothing changes.
pub fn windowless(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        /// A console program run without a console window (a windowed program ignores it).
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Tell `child` to stop by typing [`STOP`] at its `console`, and wait for it to end; one still
/// running after `guard` is ended. Answers whether it ended by itself (one that had ended already
/// did).
pub fn ask(child: &mut Child, console: &mut impl Write, guard: Duration) -> bool {
    if let Ok(Some(_)) = child.try_wait() {
        return true;
    }
    let asked = writeln!(console, "{STOP}")
        .and_then(|()| console.flush())
        .is_ok();
    let began = Instant::now();
    while asked && began.elapsed() < guard {
        if let Ok(Some(_)) = child.try_wait() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    false
}
