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
//!   a terminal whose reader has ended the child reads the terminal; a reader that cannot be
//!   interrupted forwards each line to the child, those typed while the update ran first.

use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;

/// Takes the new server's standard input, when this process's console reader goes on reading: what
/// it reads from now on is the new server's.
pub type ForwardConsole = Box<dyn FnOnce(Box<dyn Write + Send>) + Send>;

/// Runs `exe` with `args` in place of this process, as the module documentation describes. On
/// Linux and macOS it returns only when the binary cannot be started; `forward` applies on
/// Windows.
#[must_use]
pub fn relaunch(exe: &Path, args: &[OsString], forward: Option<ForwardConsole>) -> ExitCode {
    tracing::info!("Starting {} again", exe.display());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = forward;
        let error = std::process::Command::new(exe).args(args).exec();
        tracing::error!("{} could not be started: {error}", exe.display());
        ExitCode::FAILURE
    }
    #[cfg(not(unix))]
    {
        let mut command = std::process::Command::new(exe);
        command.args(args);
        if forward.is_some() {
            command.stdin(std::process::Stdio::piped());
        }
        let waited = command.spawn().and_then(|mut child| {
            if let (Some(stdin), Some(forward)) = (child.stdin.take(), forward) {
                forward(Box::new(stdin));
            }
            child.wait()
        });
        match waited {
            Ok(status) => {
                let code = status.code().unwrap_or(1);
                ExitCode::from(u8::try_from(code).unwrap_or(1))
            }
            Err(e) => {
                tracing::error!("{} could not be started: {e}", exe.display());
                ExitCode::FAILURE
            }
        }
    }
}
