//! The shipped screenshot key reaches `CaptureScreenshot 0x55`, and the action writes the next
//! free `ScreenShot#####` file beside the preferences file and reports the path in channel `0x1A`.
//!
//! Fixture: the retail keymap for the binding; a headless App in gameplay on the retail dats,
//! with the resolved action injected into its input manager, for the file. `Interaction` sets
//! `screenshot_requested` and `App::take_action_screenshot` performs it through
//! `Gpu::capture_png`.
//!
//! # The file-naming rule
//!
//! The action requests a screenshot with an empty name. On success it reports
//! `"Screenshot saved to file '%hs'"` in channel `0x1A`. The save routine behaves as follows:
//!
//! ```text
//! if no render device: return false
//! directory = directory_of(default_preferences_file)
//! for index in 0..100000:
//!     name = directory + format("ScreenShot%05d.jpg", index)
//!     if name does not exist: save JPEG to name and return result
//! overwrite ScreenShot99999.jpg
//! ```
//!
//! The same default-preferences path also supplies the keymap directory, which is why the
//! screenshot lands beside the keymap rather than beside the DATs. The rule is:
//!
//! > **`<directory of the preferences file>` + `ScreenShot` + five-digit zero-padded index +
//! > extension, at the lowest index whose file does not already exist**, and if all 100 000 are
//! > taken overwrite `ScreenShot99999`.
//!
//! This test exercises the low-index gap behavior. It does not fill all 100,000 names or exercise
//! the fallback overwrite.
//!
//! **One declared deviation: the extension.** The original client writes JPEG. This workspace has
//! no JPEG encoder, so the file is a PNG through the `Gpu::capture_png` read-back and is named
//! `.png`. Everything else about the name is retail's.
//!
//! # No DAT and no user directory is written
//!
//! Every test below points `Config::preferences_file` at a directory under the system temp
//! directory and cleans up after itself. Nothing is written under `DERETH_TEST_DAT_DIR`.

#![cfg(gpu)]

use crate::common::client_dir;

use std::path::{Path, PathBuf};

use dereth_client_contract::actions::mapped as ia;

/// A disposable directory standing in for the default preferences file's directory.
fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dereth-screenshot-file-{tag}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("a disposable preferences directory");
    d
}

fn app_in_gameplay(prefs_dir: &Path) -> dereth_client::app::App {
    let cfg = dereth_client_runtime::config::Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        // The default preferences path is the *file*; the screenshot takes its directory.
        preferences_file: prefs_dir.join("dereth-client.ini"),
        ..dereth_client_runtime::config::Config::default()
    };
    let mut app = dereth_client::app::App::new(cfg).expect("an application");
    app.start_shell().expect("the shell starts");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..3 {
        app.frame();
    }
    app
}

/// Inject the resolved screenshot `InputEvent` into the application input manager and run one
/// frame. This exercises the production action/drain/device path after resolution; it does not
/// synthesize an OS key press.
fn press_screenshot(app: &mut dereth_client::app::App) {
    let e = dereth_input::InputEvent {
        action: dereth_input::ActionId(ia::CAPTURE_SCREENSHOT.0),
        input_map: dereth_client_shell::ui::UI_INPUT_MAP,
        toggle: dereth_input::ToggleType::OneShot,
        extent: 1.0,
        start: true,
        repeat_delta: 1,
        repeat_total: 0,
        from_key_down: false,
    };
    app.input_manager_mut()
        .expect("an input manager")
        .inject_action(e);
    app.frame();
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .expect("the scratch directory")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    v.sort();
    v
}

// ---------------------------------------------------------------------------------------------
// 1. The key reaches the action
// ---------------------------------------------------------------------------------------------

/// Resolve the shipped `UICommands` binding for `CaptureScreenshot 0x55`. The test asserts one
/// unmodified keyboard binding and walks that returned control through the production map. It does
/// not hard-code a particular numeric-keypad scan code.
#[test]
fn the_shipped_numpad_star_resolves_to_capture_screenshot() {
    use dereth_input::fire::walk_input_maps;
    use dereth_input::spec::{activation, ControlChord, DeviceType};
    use dereth_input::{ActionId, InputMapId};

    let store = dereth_dat::testing::open_store().expect("the retail dats");
    let shell =
        dereth_client_shell::input::InputShell::new(&store, None).expect("the input tables");
    let km = &shell.manager.keymap;
    let ui_commands = InputMapId(0x1000_0009);
    let section = km.section(ui_commands).expect("the UICommands section");
    let mut keys = section.keys_for_action(ActionId(ia::CAPTURE_SCREENSHOT.0));
    assert_eq!(keys.len(), 1, "one shipped default binding");
    let qc = keys.pop().expect("one");
    assert_eq!(
        qc.meta_mode, 0,
        "the shipped screenshot binding is unmodified"
    );
    assert_eq!(km.device_type_of(qc.control), Some(DeviceType::Keyboard));

    let live = ControlChord::new(
        qc.control,
        qc.meta_mode,
        (qc.activation & !activation::UP) | activation::LIVE,
    );
    let stack = shell.manager.maps.entries().to_vec();
    let hit = walk_input_maps(&stack, &live, true, |m| km.section(m)).expect("reaches dispatch");
    assert_eq!(
        (hit.input_map, hit.action),
        (ui_commands, ActionId(ia::CAPTURE_SCREENSHOT.0)),
        "the resolved shipped binding must reach action 0x55 through UICommands, not a shadow"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The file, where retail puts it
// ---------------------------------------------------------------------------------------------

/// Behaviour: presentation.screenshot.the-shipped-key-writes-the-next-free-file-beside-preferences
///
/// The observable: the injected action writes `ScreenShot00000.png` **into the preferences file's
/// own directory**, and the pending scroll message names it. The file check covers only the
/// eight-byte PNG signature; it does not decode pixels.
#[test]
fn the_screenshot_lands_beside_the_preferences_file_as_screenshot00000() {
    let dir = scratch("first");
    let mut app = app_in_gameplay(&dir);
    assert!(names_in(&dir).is_empty(), "premise: the directory is empty");
    let before = app.objects().world.scroll.added;

    press_screenshot(&mut app);

    let (.., saved, failed) = app.probe().action_arm_host_stats();
    assert_eq!(
        failed, 0,
        "the screenshot action reports a successful save on the created GPU"
    );
    assert_eq!(saved, 1, "one save");
    assert_eq!(
        names_in(&dir),
        vec!["ScreenShot00000.png".to_owned()],
        "the first free five-digit screenshot name is under the preferences-file directory, \
         with this build's PNG extension"
    );
    let png = std::fs::read(dir.join("ScreenShot00000.png")).expect("the file");
    assert_eq!(
        &png[..8],
        b"\x89PNG\r\n\x1a\n",
        "and it is a real image, not an empty file"
    );

    assert_eq!(
        app.objects().world.scroll.added,
        before + 1,
        "one pending screenshot-result message"
    );
    let want = format!(
        "Screenshot saved to file '{}'",
        dir.join("ScreenShot00000.png").display()
    );
    let line = app
        .objects()
        .world
        .scroll
        .pending()
        .iter()
        .find(|l| l.body.starts_with("Screenshot saved to file '"))
        .unwrap_or_else(|| panic!("the `%hs` line should name the file"))
        .clone();
    assert_eq!(
        line.body, want,
        "the success message contains the resolved path, not a bare name"
    );
    assert_eq!(
        line.chat_type,
        dereth_client_model::scroll::LOCAL_ERROR_TYPE,
        "the screenshot-result message uses local-error channel 0x1A"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The existence-check loop chooses the **lowest free index**, not a session counter —
/// so a second press writes `00001`, and a name that already exists on disk is skipped even though
/// this process did not write it. The fixture creates squatters at `00000` and `00002`, checks the
/// preserved contents of `00000`, then checks `00001`, the existence of `00003`, and a saved count
/// of two. It does not compare the complete directory after the second save.
#[test]
fn the_index_is_the_lowest_name_that_does_not_already_exist() {
    let dir = scratch("index");
    // Two files this process did not write: `00000` and `00002`. Retail's loop takes `00001`.
    std::fs::write(dir.join("ScreenShot00000.png"), b"not ours").expect("a squatter");
    std::fs::write(dir.join("ScreenShot00002.png"), b"not ours").expect("a squatter");

    let mut app = app_in_gameplay(&dir);
    press_screenshot(&mut app);
    assert_eq!(
        names_in(&dir),
        vec![
            "ScreenShot00000.png".to_owned(),
            "ScreenShot00001.png".to_owned(),
            "ScreenShot00002.png".to_owned(),
        ],
        "the first press filled the gap at 00001 and left both squatters alone"
    );
    assert_eq!(
        std::fs::read(dir.join("ScreenShot00000.png")).expect("00000"),
        b"not ours",
        "…and did not overwrite the one that was already there"
    );

    press_screenshot(&mut app);
    assert!(
        dir.join("ScreenShot00003.png").exists(),
        "the second press went to 00003, the next free index -- not to 'the second screenshot'"
    );
    assert_eq!(app.probe().action_arm_host_stats().3, 2, "two saves");

    let _ = std::fs::remove_dir_all(&dir);
}
