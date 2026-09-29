//! Starting the server again after an update, in the process the operator (or the service
//! manager) started.
//!
//! - **Linux and macOS:** the process replaces itself with the installed binary (`exec`), with the
//!   same arguments. The process ID, the terminal and the service manager's view of the service
//!   stay as they were: systemd sees the same main process keep running.
//! - **Windows**, which has no `exec`: the process starts the installed binary as its child, with
//!   the same arguments and the same console, waits for it, and exits with its exit code. A
//!   service wrapper that stops the service with Ctrl-C reaches the child through the shared
//!   console, and one that ends the process tree ends both; the waiting process ignores Ctrl-C
//!   itself and holds no files, listeners or databases. Console input reaches the new server: on
//!   a terminal the old process's line editor has ended and the child reads the terminal; from a
//!   pipe or a file, the old process's reader (which cannot be interrupted) forwards each line to
//!   the child, those typed while the update ran first.

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

/// Runs `exe` with `args` in place of this process, as the module documentation describes. On
/// Linux and macOS it returns only when the binary cannot be started.
#[must_use]
pub fn relaunch(exe: &Path, args: &[OsString]) -> ExitCode {
    log::info!("Starting {} again", exe.display());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = std::process::Command::new(exe).args(args).exec();
        log::error!("{} could not be started: {error}", exe.display());
        ExitCode::FAILURE
    }
    #[cfg(not(unix))]
    {
        use empyrean_command::command_manager;
        // A console reading a pipe or a file cannot stop reading, so it forwards what it reads to
        // the new process (holding what came in since the world stopped); a terminal's line editor
        // has already ended and the new process reads the terminal itself.
        let forward = command_manager::line_reader_running();
        let mut command = std::process::Command::new(exe);
        command.args(args);
        if forward {
            command.stdin(std::process::Stdio::piped());
        }
        let waited = command.spawn().and_then(|mut child| {
            if let Some(stdin) = child.stdin.take() {
                command_manager::forward_console_input(Box::new(stdin));
            }
            child.wait()
        });
        match waited {
            Ok(status) => {
                let code = status.code().unwrap_or(1);
                ExitCode::from(u8::try_from(code).unwrap_or(1))
            }
            Err(e) => {
                log::error!("{} could not be started: {e}", exe.display());
                ExitCode::FAILURE
            }
        }
    }
}
