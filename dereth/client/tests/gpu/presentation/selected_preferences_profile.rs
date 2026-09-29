//! The `-prefs` switch selects the preferences profile: its last occurrence is both the file the
//! startup values are loaded from and the file saved to, a missing selected file does not fall
//! back to the default, and the rebuild's own command-line overrides still win after it loads.
//!
//! Fixture: profiles written under a disposable temporary directory, parsed by the real startup
//! path; one test starts a headless App on the retail dats to see the selected size reach
//! gameplay. No real preferences file is read or written and no network endpoint is attached.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::path::{Path, PathBuf};

use dereth_client::app::App;
use dereth_client::config::{Config, FORCED_LOGIN_SIZE};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let unique = format!(
            "dere-p1-13-selected-prefs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock follows the epoch")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir(&path).expect("create the disposable profile directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove the disposable profile directory");
    }
}

fn write_profile(path: &Path, resolution: &str) {
    std::fs::write(
        path,
        format!("[Display]\r\nResolution={resolution}\r\nFullScreen=False\r\n"),
    )
    .expect("write a disposable profile");
}

fn client_dir() -> PathBuf {
    let path = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "retail DAT fixture at {}",
        path.display()
    );
    path
}

fn startup_args(first: &Path, selected: &Path) -> Vec<String> {
    vec![
        "--headless".to_string(),
        "--no-connect".to_string(),
        "--no-sound".to_string(),
        "--dat-dir".to_string(),
        client_dir().display().to_string(),
        "-PrEfS".to_string(),
        first.display().to_string(),
        "/prefs".to_string(),
        selected.display().to_string(),
    ]
}

/// Behaviour: presentation.preferences.the-selected-profile-is-loaded-and-saved-to
///
/// The startup path parses `-prefs` before it initializes and loads the preferences file.
/// The final case-insensitive command-line occurrence therefore selects both the save destination
/// and the startup values.
#[test]
fn the_selected_profile_reaches_the_gameplay_presentation() {
    let dir = TempDir::new();
    let default = dir.path().join("default.ini");
    let first = dir.path().join("first.ini");
    let selected = dir.path().join("selected.ini");
    write_profile(&default, "1024x768");
    write_profile(&first, "1152x864");
    write_profile(&selected, "1280x720");

    let cfg = Config::from_args_and_prefs_at(&startup_args(&first, &selected), &default)
        .expect("the actual startup parse");
    assert_eq!(
        cfg.preferences_file, selected,
        "the final -prefs occurrence is the save path"
    );
    assert_eq!(
        cfg.display.resolution, 0x0500_02D0,
        "the same selected file supplies Display.*"
    );
    assert_eq!(
        (cfg.width, cfg.height),
        (1280, 720),
        "the display preferences' unforced answer"
    );

    let mut app = App::new(cfg).expect("a headless application");
    app.start_shell().expect("the shell starts");
    assert_eq!(
        app.renderer().size(),
        FORCED_LOGIN_SIZE,
        "the selected profile does not bypass the 800x600 pre-game force"
    );
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    app.frame();
    app.frame();
    assert_eq!(
        app.renderer().size(),
        (1280, 720),
        "gameplay lifts the force to the selected size"
    );
    assert_eq!(
        app.ui().expect("the live shell").ui.display(),
        (1280, 720),
        "the live UI uses the same physical-pixel presentation"
    );
    app.shutdown();
}

/// An explicit but absent selected profile is still the selected save path. The failed
/// preference load is ignored; it does not silently fall back to the default file.
#[test]
fn an_explicit_missing_profile_does_not_fall_back_to_the_default() {
    let dir = TempDir::new();
    let default = dir.path().join("default.ini");
    let missing = dir.path().join("missing.ini");
    write_profile(&default, "1280x720");
    let argv = vec!["-prefs".to_string(), missing.display().to_string()];

    let cfg = Config::from_args_and_prefs_at(&argv, &default).expect("missing prefs are nonfatal");
    assert_eq!(cfg.preferences_file, missing);
    assert_eq!(
        (cfg.width, cfg.height),
        (800, 600),
        "the default profile was not substituted"
    );
}

#[test]
fn no_prefs_switch_loads_and_retains_the_default_destination() {
    let dir = TempDir::new();
    let default = dir.path().join("default.ini");
    write_profile(&default, "1152x864");

    let cfg = Config::from_args_and_prefs_at(&[], &default).expect("the default profile loads");
    assert_eq!(
        cfg.preferences_file, default,
        "startup and shutdown share one destination"
    );
    assert_eq!((cfg.width, cfg.height), (1152, 864));
}

#[test]
fn prefs_still_requires_a_nonempty_value() {
    let dir = TempDir::new();
    let argv = vec!["-prefs".to_string()];
    let err = Config::from_args_and_prefs_at(&argv, &dir.path().join("default.ini"))
        .expect_err("the existing required-value rule remains in force");
    assert!(err.detail.contains("requires a value"), "{}", err.detail);
}

/// Rebuild-only command-line controls historically apply after the profile. Resolving `-prefs`
/// through the parser must not let the selected file take that precedence away.
#[test]
fn rebuild_command_line_overrides_still_win_after_the_selected_profile_loads() {
    let dir = TempDir::new();
    let selected = dir.path().join("selected.ini");
    std::fs::write(&selected, "[Render]\r\nLandscapeDrawDistance=Extreme\r\n")
        .expect("write a disposable profile");
    let argv = vec![
        "-prefs".to_string(),
        selected.display().to_string(),
        "--land-radius".to_string(),
        "7".to_string(),
    ];

    let cfg = Config::from_args_and_prefs_at(&argv, &dir.path().join("default.ini"))
        .expect("the profile and override parse");
    assert_eq!(
        cfg.render.landscape_draw_distance, 25,
        "the selected profile was loaded"
    );
    assert_eq!(
        cfg.land_radius, 7,
        "the explicit rebuild command-line override remains final"
    );
}
