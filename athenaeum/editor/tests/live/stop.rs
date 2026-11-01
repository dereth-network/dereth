//! Behaviour: none (checks how the world builder stops the programs it starts: asked as their operator would ask, they hear it and end by themselves)
//!
//! The program asked is this test binary run again, hearing the ask as the server does
//! (Ctrl-Break on Windows, SIGTERM elsewhere). On Windows the ask is made from a program with a
//! console (this one, under the test runner) and from one with none, as the editor's release
//! build is.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use athenaeum_editor::stop;

/// Set to a file: this binary is the program asked to stop, and says there what it heard.
const PROGRAM: &str = "ATHENAEUM_TEST_STOP_PROGRAM";
/// Set to a file: this binary asks from a process with no console, and says there how it went.
const DETACHED: &str = "ATHENAEUM_TEST_STOP_DETACHED";

/// A guard against a hang, not the time a step is expected to take.
const HANG: Duration = Duration::from_secs(120);

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let began = Instant::now();
    while !done() {
        assert!(began.elapsed() < HANG, "{what}: never");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// This binary, run to play the test named `name` alone with `var` set to `file`.
fn this_binary(name: &str, var: &str, file: &Path) -> Command {
    let mut c = Command::new(std::env::current_exe().expect("the test binary"));
    c.args([name, "--exact", "--nocapture"])
        .env(var, file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    c
}

/// Start the program, ask it to stop once it listens for the ask, and answer what it said.
fn ask_one(dir: &Path) -> String {
    let heard = dir.join("heard.txt");
    let up = dir.join("heard.txt.up");
    let mut cmd = this_binary("stop::as_the_program_asked_to_stop", PROGRAM, &heard);
    stop::askable(&mut cmd);
    let mut child = cmd.spawn().expect("the program starts");
    wait_for("the program listening", || up.exists());
    assert!(stop::ask(child.id()), "the ask was sent");
    let began = Instant::now();
    let ended = loop {
        if let Ok(Some(s)) = child.try_wait() {
            break Some(s);
        }
        if began.elapsed() > HANG {
            let _ = child.kill();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let ended = ended.expect("the program ended by itself");
    assert!(ended.success(), "{ended}");
    std::fs::read_to_string(&heard).unwrap_or_default()
}

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athenaeum-stop-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Not a check of its own: the program the check below asks to stop, when this binary is run as
/// one (its variable set). It listens for the ask, says it heard it, and ends; unasked, it ends
/// past the guard, failing.
#[test]
fn as_the_program_asked_to_stop() {
    let Some(heard) = std::env::var_os(PROGRAM).map(PathBuf::from) else {
        return;
    };
    let said = heard.clone();
    ctrlc::set_handler(move || {
        let _ = std::fs::write(&said, "asked to stop");
        std::process::exit(0);
    })
    .expect("the stop handler");
    let mut up = heard.into_os_string();
    up.push(".up");
    std::fs::write(&up, "up").expect("up");
    std::thread::sleep(HANG);
    std::process::exit(1);
}

/// A program the editor started and then asks to stop hears the ask and ends by itself, again
/// and again; on Windows, from a process with a console and from one with none.
#[test]
fn a_program_asked_to_stop_hears_it_and_ends_by_itself() {
    if std::env::var_os(PROGRAM).is_some() {
        return;
    }
    let dir = scratch(if std::env::var_os(DETACHED).is_some() {
        "detached"
    } else {
        "here"
    });
    for _ in 0..2 {
        assert_eq!(ask_one(&dir), "asked to stop");
        let _ = std::fs::remove_file(dir.join("heard.txt"));
        let _ = std::fs::remove_file(dir.join("heard.txt.up"));
    }
    if let Some(report) = std::env::var_os(DETACHED) {
        std::fs::write(report, "ok").expect("the report");
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console at all, as the editor's release build has none.
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        let report = dir.join("detached.txt");
        let status = this_binary(
            "stop::a_program_asked_to_stop_hears_it_and_ends_by_itself",
            DETACHED,
            &report,
        )
        .creation_flags(DETACHED_PROCESS)
        .status()
        .expect("this binary runs with no console");
        assert!(status.success(), "{status}");
        assert_eq!(std::fs::read_to_string(&report).unwrap_or_default(), "ok");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
