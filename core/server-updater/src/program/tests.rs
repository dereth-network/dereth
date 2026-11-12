//! Behaviour: none (the updater's own plumbing: a program written while other threads start
//! processes still starts; checked with many programs written and started while other threads
//! start processes without a pause)
//! Each test writes programs into a scratch folder of its own and removes it after. On Linux and
//! macOS a written program is a shell script; on Windows, a copy of the system's `hostname.exe`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use super::*;

/// The tests here start processes on threads of their own, so they run one at a time: one test's
/// processes, started as anything else in a test binary may start them, would otherwise be started
/// while the other writes its programs.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

/// Threads that write programs, each writing and starting this many (fewer on Windows, where a
/// new program takes long to start and none is ever refused as busy).
const WRITERS: usize = 4;
const PROGRAMS: usize = if cfg!(windows) { 10 } else { 100 };
/// Threads that start processes over and over while the programs are written.
const STARTERS: usize = 4;

/// What a written program exits with, where it can be told so.
const EXIT: i32 = 3;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "dereth-server-updater-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

/// A program: a script exiting with [`EXIT`] behind a long comment, so that writing it takes a
/// while (Unix); the system's `hostname.exe` (Windows).
fn program_bytes() -> Vec<u8> {
    if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        std::fs::read(Path::new(&root).join("System32").join("hostname.exe"))
            .expect("the system's hostname.exe")
    } else {
        let mut text = format!("#!/bin/sh\nexit {EXIT}\n#");
        text.push_str(&"-".repeat(256 * 1024));
        text.push('\n');
        text.into_bytes()
    }
}

/// A process to start over and over: this test program listing no test.
fn busy_work() -> Command {
    let mut command = Command::new(std::env::current_exe().expect("this test program"));
    command
        .args(["--list", "--exact", "no such test"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Starts [`STARTERS`] threads starting [`busy_work`] with `start` until `writers` have all ended;
/// then has [`WRITERS`] threads each write [`PROGRAMS`] programs into `dir` with `write` and run
/// each with `run`. Every program must run and say it is the one written.
fn written_while_starting(
    dir: &Path,
    start: fn(&mut Command) -> io::Result<ExitStatus>,
    write: fn(&Path, &[u8]),
    run: fn(&mut Command) -> io::Result<ExitStatus>,
) {
    let bytes = program_bytes();
    let done = AtomicBool::new(false);
    std::thread::scope(|s| {
        let starters: Vec<_> = (0..STARTERS)
            .map(|_| {
                s.spawn(|| {
                    while !done.load(Ordering::Relaxed) {
                        let status = start(&mut busy_work()).expect("busy work starts");
                        assert!(status.success(), "busy work: {status}");
                    }
                })
            })
            .collect();
        let writers: Vec<_> = (0..WRITERS)
            .map(|w| {
                let bytes = &bytes;
                s.spawn(move || {
                    for p in 0..PROGRAMS {
                        let path =
                            dir.join(format!("program-{w}-{p}{}", std::env::consts::EXE_SUFFIX));
                        write(&path, bytes);
                        let status = run(Command::new(&path).stdout(Stdio::null()))
                            .unwrap_or_else(|e| panic!("{} starts: {e}", path.display()));
                        if cfg!(windows) {
                            assert!(status.success(), "{}: {status}", path.display());
                        } else {
                            assert_eq!(status.code(), Some(EXIT), "{}", path.display());
                        }
                    }
                })
            })
            .collect();
        let mut failed = None;
        for w in writers {
            if let Err(e) = w.join() {
                failed.get_or_insert(e);
            }
        }
        done.store(true, Ordering::Relaxed);
        for st in starters {
            if let Err(e) = st.join() {
                failed.get_or_insert(e);
            }
        }
        if let Some(e) = failed {
            std::panic::resume_unwind(e);
        }
    });
}

/// A program the updater writes starts, however many processes the updater starts on other
/// threads while it writes.
#[test]
fn a_program_written_while_processes_start_starts() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = scratch("written");
    written_while_starting(
        &dir,
        status,
        |path, bytes| write(path, bytes).expect("written"),
        status,
    );
    let dir_copied = scratch("copied");
    std::fs::write(dir_copied.join("source"), program_bytes()).expect("the source");
    written_while_starting(
        &dir_copied,
        status,
        |path, _| copy(&path.with_file_name("source"), path).expect("copied"),
        status,
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir_copied);
}

/// A program a test writes with the test helpers starts, however many processes other threads
/// start meanwhile without going through this module, as other tests in a test binary do.
#[cfg(feature = "testing")]
#[test]
fn a_program_a_test_writes_starts_while_other_tests_start_processes() {
    let _one = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
    let dir = scratch("test-written");
    written_while_starting(
        &dir,
        Command::status,
        crate::testing::write_program,
        Command::status,
    );
    let dir_copied = scratch("test-copied");
    std::fs::write(dir_copied.join("source"), program_bytes()).expect("the source");
    written_while_starting(
        &dir_copied,
        Command::status,
        |path, _| crate::testing::copy_program(&path.with_file_name("source"), path),
        Command::status,
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&dir_copied);
}
