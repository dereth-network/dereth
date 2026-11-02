//! Behaviour: none (checks how the world builder stops the server it starts: told `stop` at its console, it ends by itself; one that does not is ended past the guard)
//!
//! The program told to stop is this test binary run again, reading its standard input as the
//! server's console does.

use std::io::BufRead as _;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use athenaeum_editor::stop;

/// Set to a file: this binary is the program told to stop, and says there what it heard.
const PROGRAM: &str = "ATHENAEUM_TEST_STOP_PROGRAM";
/// Set: the program never reads its console.
const DEAF: &str = "ATHENAEUM_TEST_STOP_DEAF";

/// A guard against a hang, not the time a step is expected to take.
const HANG: Duration = Duration::from_secs(120);

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let began = Instant::now();
    while !done() {
        assert!(began.elapsed() < HANG, "{what}: never");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athenaeum-stop-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// This binary, started as the program that is told to stop (deaf to its console with `deaf`),
/// once it listens: it, its console, and the file it says what it heard in.
fn program(dir: &Path, deaf: bool) -> (Child, ChildStdin, PathBuf) {
    let heard = dir.join("heard.txt");
    let up = dir.join("heard.txt.up");
    let mut cmd = Command::new(std::env::current_exe().expect("the test binary"));
    cmd.args([
        "stop::as_the_program_told_to_stop",
        "--exact",
        "--nocapture",
    ])
    .env(PROGRAM, &heard)
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    if deaf {
        cmd.env(DEAF, "1");
    }
    stop::windowless(&mut cmd);
    let mut child = cmd.spawn().expect("the program starts");
    let console = child.stdin.take().expect("its console");
    wait_for("the program listening", || up.exists());
    (child, console, heard)
}

/// Not a check of its own: the program the checks below tell to stop, when this binary is run as
/// one (its variable set). It reads its console as the server does and, told `stop`, says it
/// heard and ends; deaf, or never told, it ends long past every guard, failing.
#[test]
fn as_the_program_told_to_stop() {
    let Some(heard) = std::env::var_os(PROGRAM).map(PathBuf::from) else {
        return;
    };
    if std::env::var_os(DEAF).is_none() {
        let said = heard.clone();
        std::thread::spawn(move || {
            for line in std::io::stdin().lock().lines() {
                if line.is_ok_and(|l| l.trim() == stop::STOP) {
                    let _ = std::fs::write(&said, "told to stop");
                    std::process::exit(0);
                }
            }
        });
    }
    let mut up = heard.into_os_string();
    up.push(".up");
    std::fs::write(&up, "up").expect("up");
    std::thread::sleep(HANG * 2);
    std::process::exit(1);
}

/// The server the editor started, told `stop` at its console, hears it and ends by itself, well
/// within the guard.
#[test]
fn a_server_told_stop_at_its_console_ends_by_itself() {
    if std::env::var_os(PROGRAM).is_some() {
        return;
    }
    let dir = scratch("told");
    let (mut child, mut console, heard) = program(&dir, false);
    assert!(stop::ask(&mut child, &mut console, HANG), "ended by itself");
    assert_eq!(
        std::fs::read_to_string(&heard).unwrap_or_default(),
        "told to stop"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A server that does not end when told is ended once the guard is past, so the editor never
/// waits on it for ever.
#[test]
fn a_server_that_does_not_end_is_ended_past_the_guard() {
    if std::env::var_os(PROGRAM).is_some() {
        return;
    }
    let dir = scratch("deaf");
    let (mut child, mut console, heard) = program(&dir, true);
    assert!(
        !stop::ask(&mut child, &mut console, Duration::from_millis(500)),
        "it did not end by itself"
    );
    assert!(
        matches!(child.try_wait(), Ok(Some(_))),
        "it was ended all the same"
    );
    assert!(!heard.exists());
    let _ = std::fs::remove_dir_all(&dir);
}
