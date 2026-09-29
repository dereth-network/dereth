//! Every run of the client keeps a log of its own in the settings folder's `crash-logs` folder,
//! and a run that panics records the panic there before it dies.
//!
//! Fixture: the client binary itself, started with the hidden deliberate-panic switch and with
//! its home folder pointed at a scratch folder, so the log lands there and not in the settings
//! folder of whoever runs the test. The panic comes before the client reads the dats or opens a
//! window, so no dats and no device are needed.

use std::path::{Path, PathBuf};

/// Where the client puts its crash logs under a home folder of `home`, per platform: the
/// settings folder's own rule, spelled out independently so a change to it breaks this test.
fn crash_log_dir_under(home: &Path) -> PathBuf {
    let root = if cfg!(windows) {
        // `APPDATA` is set to the home folder below.
        home.join("Dereth")
    } else if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("Dereth")
    } else {
        // `XDG_CONFIG_HOME` is set to the same absolute folder below, and wins over `HOME`.
        home.join("dereth")
    };
    root.join("client").join("crash-logs")
}

/// Behaviour: presentation.crash.every-run-keeps-a-log-that-records-a-panic
///
/// The run is started, panics on purpose, and dies with the panic exit code; its log, named for
/// its process id, holds the start record first and the panic with a backtrace after it.
#[test]
fn a_run_that_panics_leaves_the_panic_in_its_crash_log() {
    let home = std::env::temp_dir().join(format!("dereth-crash-log-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("a scratch home folder");

    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_dereth-client"))
        .args(["--no-console", "--crash-test"])
        .env("USERPROFILE", &home)
        .env("APPDATA", &home)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the client binary starts");
    let pid = child.id();
    let status = child.wait().expect("the client runs to its end");
    let mut stderr = String::new();
    if let Some(mut s) = child.stderr.take() {
        use std::io::Read as _;
        let _ = s.read_to_string(&mut stderr);
    }

    assert_eq!(
        status.code(),
        Some(101),
        "a panic still ends the run with the panic exit code; stderr was:\n{stderr}"
    );
    let log = crash_log_dir_under(&home).join(format!("dereth-client-{pid}.log"));
    let text = std::fs::read_to_string(&log).unwrap_or_else(|e| {
        panic!(
            "the run's crash log {} was not written: {e}\nstderr was:\n{stderr}",
            log.display()
        )
    });
    let start = text
        .find("[START]")
        .expect("the log opens with the start record");
    let panic = text.find("[PANIC]").expect("the panic is recorded");
    assert!(
        start < panic,
        "the start is recorded before the panic:\n{text}"
    );
    assert!(
        text.contains("--crash-test"),
        "the panic's message is recorded:\n{text}"
    );
    assert!(
        !text.contains("[EXIT]"),
        "a run that panicked did not reach the end of its main:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&home);
}
