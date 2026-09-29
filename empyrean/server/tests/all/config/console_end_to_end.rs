//! Divergence: V378
//! Booted binary writes a console reply at log level error; non-interactive console prints no
//! prompt.
//! Fixture: the built server, retail dat files and world.pack.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The dat folder and the content pack, when both are present.
fn content() -> Option<(PathBuf, PathBuf)> {
    if !cfg!(feature = "real-content") {
        return None;
    }
    let client = dereth_dat::testing::dat_dir();
    let pack = empyrean_common::test_paths::world_pack();
    if dereth_dat::testing::have_dats() && pack.is_file() {
        Some((client, pack))
    } else {
        eprintln!(
            "(no retail dats at {} or no world.pack at {}: the console check did not run)",
            client.display(),
            pack.display()
        );
        None
    }
}

/// Boots the server in `dir` with `extra` added to `[server]`, types `input` on its console, and
/// lets it run for two seconds.
fn run(dir: &Path, client: &Path, pack: &Path, port: u16, extra: &str, input: &str) -> Output {
    std::fs::write(
        dir.join("empyrean.toml"),
        format!(
            "[server]\ndat_files_directory = '{}'\nworld_pack_path = '{}'\nlandblock_preloading = false\n{extra}\n\
             [server.network]\nhost = \"127.0.0.1\"\nport = {port}\n",
            client.display(),
            pack.display()
        ),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_empyrean-server"))
        .args(["--run-for", "2"])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("empyrean-server runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn a_console_reply_is_written_at_log_level_error() {
    let Some((client, pack)) = content() else {
        return;
    };
    let dir = scratch("console-reply-at-error");
    let out = run(
        &dir,
        &client,
        &pack,
        19_410,
        "log_level = \"error\"",
        "serverstatus\n",
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stdout.contains("Empyrean command prompt ready.") && stdout.contains("empyrean>> "),
        "{stdout}"
    );
    assert!(
        stdout.contains("Server Status:\n"),
        "the reply is written although Info is off:\n{stdout}"
    );
    assert!(
        !stderr.contains("Server Status:"),
        "and it is not logged:\n{stderr}"
    );
    assert!(
        !stderr.contains("Initializing CommandManager"),
        "Info is off once the configuration is read:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_non_interactive_console_prints_no_prompt() {
    let Some((client, pack)) = content() else {
        return;
    };
    let dir = scratch("console-non-interactive");
    let out = run(
        &dir,
        &client,
        &pack,
        19_420,
        "interactive_console = false",
        "serverstatus\n",
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        !stdout.contains("empyrean>>") && !stdout.contains("command prompt ready"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("Server Status:"),
        "the console reads nothing:\n{stdout}"
    );
    assert!(
        stderr.contains("command prompt disabled - server.interactive_console is false"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
