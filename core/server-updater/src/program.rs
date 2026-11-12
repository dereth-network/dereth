//! Writing a program and starting processes, so that a program just written always starts.
//!
//! Linux refuses to start a program that any process holds open for writing ("Text file busy"), as
//! other Unix systems may. A process being started is, until it runs its own program, a copy of the
//! one starting it, holding every file that one has open. So a program written on one thread while
//! another thread starts a process can be held open by the new process for that moment, and a
//! program started then is refused. Writing to another name and renaming the file into place does
//! not help: the new process holds the file itself, whatever it is called.
//!
//! Here a program is written only while no process is being started, and a process is started only
//! while no program is being written. Starting a process ends once the new process runs its own
//! program, which closes what it held of this one's, so a program written after that is held by no
//! one. Every program the updater writes goes through [`write()`] or [`copy`], and every process it
//! starts through [`spawn`], [`output`] or [`status`]; a server or the hub starts no process of its
//! own but through these. Windows hands a new process only the files it is given, so there this
//! costs a moment and changes nothing.

use std::io;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// Held for reading while a process starts, and for writing while a program is written.
static STARTING: RwLock<()> = RwLock::new(());

fn writing() -> RwLockWriteGuard<'static, ()> {
    STARTING.write().unwrap_or_else(PoisonError::into_inner)
}

fn starting() -> RwLockReadGuard<'static, ()> {
    STARTING.read().unwrap_or_else(PoisonError::into_inner)
}

/// Writes `bytes` to `path` as a program, executable on Unix, while no process is being started.
///
/// # Errors
/// When the file cannot be written or marked executable.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    {
        let _writing = writing();
        std::fs::write(path, bytes)?;
    }
    make_executable(path)
}

/// Copies the file `from` to `to` as a program, executable on Unix, while no process is being
/// started.
///
/// # Errors
/// When the file cannot be read, written or marked executable.
pub fn copy(from: &Path, to: &Path) -> io::Result<()> {
    {
        let _writing = writing();
        std::fs::copy(from, to)?;
    }
    make_executable(to)
}

/// Starts `command` while no program is being written.
///
/// # Errors
/// When it cannot be started.
pub fn spawn(command: &mut Command) -> io::Result<Child> {
    let _starting = starting();
    command.spawn()
}

/// Runs `command` to its end, as [`spawn`] starts it, with its input closed and its output and
/// errors captured, as [`Command::output`] does.
///
/// # Errors
/// When it cannot be started or waited for.
pub fn output(command: &mut Command) -> io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    spawn(command)?.wait_with_output()
}

/// Runs `command` to its end, as [`spawn`] starts it, as [`Command::status`] does.
///
/// # Errors
/// When it cannot be started or waited for.
pub fn status(command: &mut Command) -> io::Result<ExitStatus> {
    spawn(command)?.wait()
}

/// Marks `path` executable (Unix; nothing to do elsewhere).
///
/// # Errors
/// When its permissions cannot be set.
pub fn make_executable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests;
