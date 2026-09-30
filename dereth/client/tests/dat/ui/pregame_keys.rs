//! Before the player is in the world, no key reaches an in-world command: on the character screen
//! `E` does not start an examine, `R` does not start a use, `W` does not walk, and no gameplay input
//! map is registered at all, while Escape still reaches the screen's own map. Entering the gameplay
//! screen brings every one of them back, and leaving it takes them away again.
//! Fixture: the retail dats' keymaps; a headless `App` with its UI up and no network, driven with
//! synthetic key messages.

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::input::{InputShell, BASE_MAP_REGISTRATIONS, WHOLE_RUN_INPUT_MAPS};
use dereth_client::platform::keys::Key;
use dereth_client::platform::window::HostEvent;
use dereth_client::present::NullPresentation;
use dereth_client_runtime::interaction::TargetMode;
use dereth_input::{ActionId, InputMapId};
use dereth_ui::framework::mode;

/// `SelectionExamine` and `UseSelected`, both in the UI-commands map.
const SELECTION_EXAMINE: ActionId = ActionId(0x1000_002B);
const USE: ActionId = ActionId(0x1000_0025);
const UI_COMMANDS_MAP: InputMapId = InputMapId(0x1000_0009);
/// The pre-game screens' own map and its Escape action.
const DIALOG_BOXES_MAP: InputMapId = InputMapId(9);
const ESCAPE_KEY: ActionId = ActionId(0x27);

const KEY_E: Key = Key::new(0x45, 0x12);
const KEY_R: Key = Key::new(0x52, 0x13);

/// The maps a character session registers: every base row the whole run does not keep.
fn session_maps() -> Vec<u32> {
    BASE_MAP_REGISTRATIONS
        .iter()
        .map(|(_, map, _)| *map)
        .filter(|m| !WHOLE_RUN_INPUT_MAPS.contains(m))
        .collect()
}

fn registered_maps(input: &InputShell) -> Vec<u32> {
    input
        .manager
        .maps
        .entries()
        .iter()
        .map(|e| e.map.0)
        .collect()
}

/// Run frames until the UI flow has switched to `m`.
fn settle_on(app: &mut App, m: dereth_ui::UiMode) {
    for _ in 0..10 {
        if app.ui().and_then(|s| s.flow.current_mode()) == Some(m) {
            return;
        }
        app.frame();
    }
    panic!(
        "the flow never reached {m:?}: {:?}",
        app.ui().and_then(|s| s.flow.current_mode())
    );
}

/// Feed one key press and release straight into the input manager and return what it resolved to.
fn resolve(app: &mut App, key: Key, t: &mut u32) -> Vec<(u32, u32, bool)> {
    let mut pump = dereth_client::pump::Pump::new();
    let input = app.input_manager_mut().expect("the input manager is up");
    input.manager.take_events();
    for pressed in [true, false] {
        *t += 10;
        input.on_message(pump.key_message_for_key(key, pressed, *t));
    }
    input
        .manager
        .take_events()
        .iter()
        .map(|e| (e.input_map.0, e.action.0, e.start))
        .collect()
}

/// Press and release a key through the window, as the event loop hands it on, and run a frame.
fn press_through_the_window(app: &mut App, key: Key, text: &str, t: &mut u32) {
    for pressed in [true, false] {
        *t += 10;
        app.handle_window_event(
            &HostEvent::KeyboardInput {
                key,
                pressed,
                text: pressed.then(|| text.to_owned()),
            },
            *t,
        );
        app.frame();
    }
}

/// Behaviour: ui.keyboard.in-world-keys-do-nothing-before-the-player-is-in-the-world
#[test]
fn e_and_r_start_no_examine_or_use_on_the_character_screen_and_do_in_the_world() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the shipped keymaps are this test's oracle: no dats at {}",
        client_dir().display()
    );
    let cfg = Config {
        headless: true,
        ui: true,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::with_presentation(cfg, Box::new(NullPresentation::new(800, 600)))
        .expect("a headless app");
    app.start_shell().expect("the UI shell comes up");

    // The report's two keys are the shipped keymap's examine and use keys.
    let input = app.input_manager().expect("the input manager is up");
    let offsets = |a| {
        input
            .keys_for_action(a, UI_COMMANDS_MAP)
            .iter()
            .filter(|k| k.meta_mode == 0)
            .map(|k| k.control.offset())
            .collect::<Vec<_>>()
    };
    assert!(
        offsets(SELECTION_EXAMINE).contains(&0x12),
        "E examines: {:x?} {:?}",
        offsets(SELECTION_EXAMINE),
        input.keys_for_action(SELECTION_EXAMINE, UI_COMMANDS_MAP)
    );
    assert!(offsets(USE).contains(&0x13), "R uses: {:x?}", offsets(USE));

    // ---- the character screen ----
    app.queue_ui_mode(mode::CHARACTER_MANAGEMENT);
    settle_on(&mut app, mode::CHARACTER_MANAGEMENT);
    app.frame();
    let registered = registered_maps(app.input_manager().unwrap());
    for m in session_maps() {
        assert!(
            !registered.contains(&m),
            "map {m:#x} is an in-world map and must not be registered on the character screen: \
             {registered:x?}"
        );
    }
    assert!(registered.contains(&DIALOG_BOXES_MAP.0), "{registered:x?}");

    let mut t = 1_000;
    assert_eq!(
        resolve(&mut app, KEY_E, &mut t),
        vec![],
        "E resolves to nothing"
    );
    assert_eq!(
        resolve(&mut app, KEY_R, &mut t),
        vec![],
        "R resolves to nothing"
    );
    assert_eq!(
        resolve(&mut app, Key::KEY_W, &mut t),
        vec![],
        "W resolves to nothing"
    );
    // A pre-game key still reaches the screen's own map. Escape there is a click: one start.
    assert_eq!(
        resolve(&mut app, Key::ESCAPE, &mut t),
        vec![(DIALOG_BOXES_MAP.0, ESCAPE_KEY.0, true)],
        "Escape is the character screen's"
    );

    // The whole path, window to cursor: neither key leaves a target mode or a changed cursor.
    let cursor = app.current_cursor_did();
    assert!(cursor.is_some(), "a cursor is installed");
    press_through_the_window(&mut app, KEY_E, "e", &mut t);
    assert_eq!(app.interaction().target_mode(), TargetMode::None, "after E");
    assert_eq!(app.current_cursor_did(), cursor, "after E");
    press_through_the_window(&mut app, KEY_R, "r", &mut t);
    assert_eq!(app.interaction().target_mode(), TargetMode::None, "after R");
    assert_eq!(app.current_cursor_did(), cursor, "after R");

    // ---- the gameplay screen: every in-world map is back, and E examines ----
    app.queue_ui_mode(mode::GAME_PLAY);
    settle_on(&mut app, mode::GAME_PLAY);
    app.frame();
    let registered = registered_maps(app.input_manager().unwrap());
    for m in session_maps() {
        assert!(
            registered.contains(&m),
            "map {m:#x} is registered in the world: {registered:x?}"
        );
    }
    assert!(
        resolve(&mut app, KEY_E, &mut t).contains(&(UI_COMMANDS_MAP.0, SELECTION_EXAMINE.0, true)),
        "E examines in the world"
    );

    // ---- and back to the character screen, as a log-off does ----
    app.queue_ui_mode(mode::CHARACTER_MANAGEMENT);
    settle_on(&mut app, mode::CHARACTER_MANAGEMENT);
    app.frame();
    assert_eq!(resolve(&mut app, KEY_E, &mut t), vec![], "E after leaving");
    assert_eq!(resolve(&mut app, KEY_R, &mut t), vec![], "R after leaving");

    app.shutdown();
}
