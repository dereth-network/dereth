//! A run of the client that goes wrong keeps a log of its own in the settings folder's
//! `crash-logs` folder: a run that panics records the panic there before it dies, and a run that
//! fails says how it ended. The command line in it has the login's values hidden.
//!
//! Fixture: the client binary itself, started with its home folder (or its settings folder, by
//! name) pointed at a scratch folder, so the log lands there and not in the settings folder of
//! whoever runs the test. The panic and the failure both come before the client reads the dats or
//! opens a window, so no dats and no device are needed. The clean run, which needs both, is in the
//! `gpu` tier's `headless_settings`.

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
        .args([
            "--no-console",
            "-a",
            "crash-log-account",
            "-v",
            "crash-log-password",
            "--crash-test",
        ])
        // The home folder is what decides here, whatever the environment running the test names.
        .env_remove("DERETH_SETTINGS_DIR")
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
    assert!(
        !text.contains("crash-log-account") && !text.contains("crash-log-password"),
        "the account and password are not in the log:\n{text}"
    );
    assert!(
        text.contains(r#""-a", "***", "-v", "***""#),
        "the switches stay and their values are hidden:\n{text}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// Behaviour: none (tooling: which runs keep a diagnostic log, and where a harness puts it)
///
/// A run that fails -- here, given a dat folder with no dats in it -- exits with code 1 and keeps
/// its log, ending in the exit record. The run's settings folder is named by
/// `DERETH_SETTINGS_DIR`, so the log is there and nothing at all is written under the home folder.
#[test]
fn a_run_that_fails_keeps_its_crash_log_in_the_named_settings_folder() {
    let scratch =
        std::env::temp_dir().join(format!("dereth-crash-log-fail-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    let (home, settings, no_dats) = (
        scratch.join("home"),
        scratch.join("settings"),
        scratch.join("no-dats"),
    );
    for dir in [&home, &no_dats] {
        std::fs::create_dir_all(dir).expect("a scratch folder");
    }

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_dereth-client"))
        .args(["--no-console", "--headless", "--dat-dir"])
        .arg(&no_dats)
        .env("DERETH_SETTINGS_DIR", &settings)
        .env("USERPROFILE", &home)
        .env("APPDATA", &home)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &home)
        .output()
        .expect("the client binary starts");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "stderr was:\n{stderr}");

    let logs: Vec<PathBuf> = std::fs::read_dir(settings.join("crash-logs"))
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert_eq!(logs.len(), 1, "one log, in the named folder: {logs:?}");
    let text = std::fs::read_to_string(&logs[0]).expect("the log reads");
    assert!(text.contains("[START]"), "{text}");
    assert!(
        text.contains("[EXIT]") && text.contains("code=1"),
        "the failure's exit is recorded:\n{text}"
    );
    assert_eq!(
        std::fs::read_dir(&home).expect("listed").count(),
        0,
        "nothing was written under the home folder"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}
