//! A headless run keeps out of the player's settings folder: started with its home folder pointed
//! at a scratch folder that holds a player's preferences and key map, the client binary runs a few
//! headless frames and exits, and both files are byte for byte what they were, with nothing added
//! beside them. Naming a file with `--prefs` is the way to ask for one, and the same run given one
//! writes it on exit. A run that ends cleanly leaves no crash log either.
//!
//! Fixture: the client binary, the retail dats and a graphics device; a missing install, or a
//! device the client cannot open, fails.

use crate::common::client_dir;

use std::path::{Path, PathBuf};
use std::process::Command;

/// A folder under the system temporary folder, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let unique = format!(
            "dereth-headless-settings-{what}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock follows the epoch")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("a scratch folder");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The settings folder under a home folder of `home`, per platform, spelled out independently of
/// the client's own rule so that a change to that rule breaks this test rather than moving it.
fn settings_dir_under(home: &Path) -> PathBuf {
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
    root.join("client")
}

/// A player's preferences, shaped like a file the client wrote: a renderer choice and two
/// registered options set away from their defaults, so a save would change the text.
const PLAYER_PREFERENCES: &str = "[Render]\r\nRenderer=vulkan\r\nFieldOfView=75.00\r\n\
[Input]\r\nKeymapFile=dereth.keymap\r\nMouseLookSensitivity=0.90\r\n";

/// A key map the client's writer would not produce, so a save would change it.
const PLAYER_KEYMAP: &str = "; the player's own key map\r\n";

/// Run the client headless for three frames with `home` as its home folder.
fn run_headless(home: &Path, dat_dir: &Path, extra: &[&str]) {
    let exe = env!("CARGO_BIN_EXE_dereth-client");
    let output = Command::new(exe)
        .args([
            "--no-console",
            "--headless",
            "--no-connect",
            "--no-sound",
            "--frames",
            "3",
        ])
        .arg("--dat-dir")
        .arg(dat_dir)
        .args(extra)
        // The home folder is what decides here, whatever the environment running the test names.
        .env_remove("DERETH_SETTINGS_DIR")
        .env("USERPROFILE", home)
        .env("APPDATA", home)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .output()
        .unwrap_or_else(|e| panic!("spawning {exe}: {e}"));
    assert!(
        output.status.success(),
        "the headless run failed with {:?}:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Every file under `dir` but the crash logs, with its bytes, in name order.
fn files_under(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if e.file_name() == "crash-logs" {
            continue;
        }
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            let bytes = std::fs::read(&path).expect("a file just listed reads");
            out.push((path, bytes));
        }
    }
    out.sort();
    out
}

/// Behaviour: presentation.settings.a-headless-run-leaves-the-players-settings-alone-unless-given-a-file
#[test]
fn a_headless_run_leaves_the_players_preferences_and_key_map_untouched() {
    let dat_dir = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's fixture and there are none at {} -- \
         set DERETH_TEST_DAT_DIR",
        dat_dir.display()
    );
    let home = Scratch::new("home");
    let settings = settings_dir_under(home.path());
    std::fs::create_dir_all(&settings).expect("the player's settings folder");
    std::fs::write(settings.join("UserPreferences.ini"), PLAYER_PREFERENCES)
        .expect("the player's preferences");
    std::fs::write(settings.join("dereth.keymap"), PLAYER_KEYMAP).expect("the player's key map");
    let before = files_under(&settings);
    assert_eq!(before.len(), 2, "the fixture is the two files");

    run_headless(home.path(), &dat_dir, &[]);
    let after = files_under(&settings);
    assert_eq!(
        after, before,
        "a headless run changed or added a file in the player's settings folder"
    );

    // The control: asked for a file, the same run writes it on exit, so the quiet run above is a
    // run that would have written and did not, rather than one that never saves at all.
    let scratch = Scratch::new("named");
    let named = scratch.path().join("UserPreferences.ini");
    run_headless(
        home.path(),
        &dat_dir,
        &["--prefs", named.to_str().expect("a UTF-8 path")],
    );
    let written = std::fs::read_to_string(&named).unwrap_or_else(|e| {
        panic!(
            "the run named {} with --prefs and wrote nothing there: {e}",
            named.display()
        )
    });
    assert!(
        written.contains("FieldOfView="),
        "the named file holds the saved options:\n{written}"
    );
    assert_eq!(
        files_under(&settings),
        before,
        "a run given its own file still left the player's folder alone"
    );
}

/// Behaviour: none (tooling: which runs keep a diagnostic log)
///
/// A run that ends cleanly removes the crash log it opened at its start: the folder the log was
/// written into is there, and empty.
#[test]
fn a_clean_run_leaves_no_crash_log() {
    let dat_dir = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's fixture and there are none at {} -- \
         set DERETH_TEST_DAT_DIR",
        dat_dir.display()
    );
    let home = Scratch::new("clean");
    run_headless(home.path(), &dat_dir, &[]);
    let logs = settings_dir_under(home.path()).join("crash-logs");
    let left: Vec<PathBuf> = std::fs::read_dir(&logs)
        .unwrap_or_else(|e| panic!("the run never opened its log in {}: {e}", logs.display()))
        .flatten()
        .map(|e| e.path())
        .collect();
    assert!(left.is_empty(), "a clean run left {left:?}");
}
