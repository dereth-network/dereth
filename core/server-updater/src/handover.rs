//! Starting the server again after an update, in the process the operator (or the service
//! manager) started.
//!
//! - **Linux and macOS:** the process replaces itself with the installed binary (`exec`), with the
//!   same arguments. The process ID, the terminal and the service manager's view of the service
//!   stay as they were: systemd sees the same main process keep running.
//! - **Windows**, which has no `exec`: the process stays, as the one process that waits. It starts
//!   the installed binary as its child, with the same arguments and the same console, and waits for
//!   it. A server started this way that updates in its turn starts no server of its own: once it has
//!   shut down and installed the release, it exits with an exit code kept for this, and the waiting
//!   process starts the binary at the same path again, which is now the new release. Any other exit
//!   code ends the waiting process with that code. However many updates the server takes, its
//!   service runs two processes: the waiting one and the server. A server whose parent is the
//!   waiting process of an earlier release, which starts the binary only once, waits on the server
//!   it starts in the same way, and the servers after it exit to it.
//!
//!   A service wrapper that stops the service with Ctrl-C reaches the server through the shared
//!   console, and one that ends the process tree ends both; the waiting process ignores Ctrl-C
//!   itself and holds no files, listeners or databases. Console input reaches each new server: on a
//!   terminal whose reader has ended the server reads the terminal; a reader that cannot be
//!   interrupted forwards each line to the server running at the time, those typed while an update
//!   ran first. A server that exits to be started again leaves the lines it took and did not run in
//!   a file beside the binary, and the waiting process gives them to the next server before
//!   anything typed since.

use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Takes the new server's standard input, when this process's console reader goes on reading: what
/// it reads from now on is the new server's. It writes the lines it holds before it returns, since
/// the process may end right after.
pub type ForwardConsole = Box<dyn FnOnce(Box<dyn Write + Send>) + Send>;

/// Set in the environment of each server the waiting process starts: its parent starts it again
/// when it exits with [`START_AGAIN`]. The value is the file the server leaves the console lines it
/// took and did not run in, for the next server. A process started any other way does not inherit
/// it ([`only_where_set`]).
pub(crate) const PARENT_RESTARTS: &str = "DERETH_SERVER_PARENT_RESTARTS";

/// The exit code with which a server whose parent waits on it asks to be started again. A server
/// exits with it for nothing else.
const START_AGAIN: u8 = 75;

/// Runs `exe` with `args` in place of this process, as the module documentation describes. On
/// Linux and macOS it returns only when the binary cannot be started; `forward` applies on
/// Windows.
#[must_use]
pub fn relaunch(exe: &Path, args: &[OsString], forward: Option<ForwardConsole>) -> ExitCode {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = forward;
        tracing::info!("Starting {} again", exe.display());
        let error = std::process::Command::new(exe).args(args).exec();
        tracing::error!("{} could not be started: {error}", exe.display());
        ExitCode::FAILURE
    }
    #[cfg(not(unix))]
    {
        ExitCode::from(hand_over(exe, args, forward))
    }
}

/// What [`relaunch`] does on Windows; the exit code.
#[cfg(not(unix))]
fn hand_over(exe: &Path, args: &[OsString], forward: Option<ForwardConsole>) -> u8 {
    match handover(std::env::var_os(PARENT_RESTARTS)) {
        Handover::ToParent(console) => {
            if let Some(forward) = forward {
                match std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&console)
                {
                    Ok(file) => forward(Box::new(file)),
                    Err(e) => tracing::warn!(
                        "{}: {e}: what was typed on the console since the world stopped is not passed on",
                        console.display()
                    ),
                }
            }
            tracing::info!(
                "The process waiting on this one starts {} again",
                exe.display()
            );
            START_AGAIN
        }
        Handover::Wait => wait_on_servers(exe, args, forward),
    }
}

/// How a server hands over to the binary installed in its place on Windows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(unix, allow(dead_code))]
enum Handover {
    /// Its parent waits on it: it exits with [`START_AGAIN`], leaving the console lines it took and
    /// did not run in this file.
    ToParent(PathBuf),
    /// It becomes the waiting process.
    Wait,
}

/// How a server whose environment holds `marker` ([`PARENT_RESTARTS`]) hands over.
#[cfg_attr(unix, allow(dead_code))]
fn handover(marker: Option<OsString>) -> Handover {
    match marker {
        Some(console) => Handover::ToParent(PathBuf::from(console)),
        None => Handover::Wait,
    }
}

/// What the waiting process does when the server it started has exited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(unix, allow(dead_code))]
enum AfterServer {
    /// Start the binary again.
    StartAgain,
    /// End with this exit code.
    Exit(u8),
}

/// What the waiting process does when its server exited with `code` (`None`: it was ended without
/// one). A code that does not fit in a byte ends it with 1.
#[cfg_attr(unix, allow(dead_code))]
fn after_server(code: Option<i32>) -> AfterServer {
    match code {
        Some(c) if c == i32::from(START_AGAIN) => AfterServer::StartAgain,
        Some(c) => AfterServer::Exit(u8::try_from(c).unwrap_or(1)),
        None => AfterServer::Exit(1),
    }
}

/// The waiting process: starts `exe` with `args`, and again each time it asks, until it exits with
/// any other code, which is the exit code.
#[cfg(not(unix))]
fn wait_on_servers(exe: &Path, args: &[OsString], forward: Option<ForwardConsole>) -> u8 {
    let console = console_file(exe);
    let relay = forward.map(|forward| {
        let relay = Relay::default();
        forward(Box::new(relay.input()));
        relay
    });
    loop {
        tracing::info!("Starting {} again", exe.display());
        let _ = std::fs::remove_file(&console);
        let mut command = Command::new(exe);
        command.args(args).env(PARENT_RESTARTS, &console);
        if relay.is_some() {
            command.stdin(std::process::Stdio::piped());
        }
        let waited = crate::program::spawn(&mut command).and_then(|mut child| {
            if let (Some(relay), Some(stdin)) = (&relay, child.stdin.take()) {
                relay.connect(Box::new(stdin));
            }
            child.wait()
        });
        let status = match waited {
            Ok(status) => status,
            Err(e) => {
                tracing::error!("{} could not be started: {e}", exe.display());
                let _ = std::fs::remove_file(&console);
                return 1;
            }
        };
        match after_server(status.code()) {
            AfterServer::StartAgain => {
                let left = std::fs::read(&console).unwrap_or_default();
                if let Some(relay) = &relay {
                    relay.disconnect(left);
                }
            }
            AfterServer::Exit(code) => {
                let _ = std::fs::remove_file(&console);
                return code;
            }
        }
    }
}

/// The file beside `exe` that the waiting process's servers leave their console lines in.
#[cfg(not(unix))]
fn console_file(exe: &Path) -> PathBuf {
    let name = exe.file_name().unwrap_or_default().to_string_lossy();
    exe.with_file_name(format!("{name}.console-{}", std::process::id()))
}

/// Keeps [`PARENT_RESTARTS`] from a process started by this one unless `command` sets it: only the
/// servers the waiting process starts are started again by their parent, not what a server starts
/// (its health check, its other programs).
pub(crate) fn only_where_set(command: &mut Command) {
    if !command.get_envs().any(|(key, _)| key == PARENT_RESTARTS) {
        command.env_remove(PARENT_RESTARTS);
    }
}

/// The waiting process's console reader's way to the server running at the time.
#[derive(Default)]
#[cfg_attr(unix, allow(dead_code))]
struct Relay(Arc<Mutex<Relayed>>);

#[derive(Default)]
#[cfg_attr(unix, allow(dead_code))]
struct Relayed {
    /// The running server's standard input, while it takes it.
    server: Option<Box<dyn Write + Send>>,
    /// What came while no server took it, for the next one.
    kept: Vec<u8>,
    /// The console's input has ended: each server's input is closed once it has what was kept.
    ended: bool,
}

/// What the console reader writes to: the server's standard input, or what is kept for the next.
/// Dropping it ends the servers' input.
#[cfg_attr(unix, allow(dead_code))]
struct RelayInput(Arc<Mutex<Relayed>>);

#[cfg_attr(unix, allow(dead_code))]
impl Relay {
    fn lock(&self) -> MutexGuard<'_, Relayed> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The console reader's end.
    fn input(&self) -> RelayInput {
        RelayInput(Arc::clone(&self.0))
    }

    /// A server has started: what was kept is written to its input, and what comes next.
    fn connect(&self, mut server: Box<dyn Write + Send>) {
        let mut relayed = self.lock();
        let kept = std::mem::take(&mut relayed.kept);
        if server
            .write_all(&kept)
            .and_then(|()| server.flush())
            .is_err()
        {
            relayed.kept = kept;
            return;
        }
        if !relayed.ended {
            relayed.server = Some(server);
        }
    }

    /// The server has exited to be started again, leaving `left`: the next server gets it before
    /// what came since.
    fn disconnect(&self, left: Vec<u8>) {
        let mut relayed = self.lock();
        relayed.server = None;
        let mut kept = left;
        kept.append(&mut relayed.kept);
        relayed.kept = kept;
    }
}

#[cfg_attr(unix, allow(dead_code))]
impl RelayInput {
    fn lock(&self) -> MutexGuard<'_, Relayed> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Write for RelayInput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut relayed = self.lock();
        if let Some(server) = relayed.server.as_mut() {
            if server.write_all(buf).and_then(|()| server.flush()).is_ok() {
                return Ok(buf.len());
            }
            // The server is ending: the next one gets this.
            relayed.server = None;
        }
        relayed.kept.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for RelayInput {
    fn drop(&mut self) {
        let mut relayed = self.lock();
        relayed.ended = true;
        relayed.server = None;
    }
}

#[cfg(test)]
mod tests;
