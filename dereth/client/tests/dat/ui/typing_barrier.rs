//! The typing barrier: a held W walks only while the chat box lacks focus, the shipped bindings
//! drive the five other motion slots, a bare Escape does not end the process, Shift+Escape reaches
//! End Character Session, the flycam gate and the numpad camera keys follow the barrier, the intro
//! still skips on a character, an action the body does not model is put back, and the residual
//! latch moves only the flycam. Fixture: a headless `App` in gameplay on the retail dats (one test
//! without a body, one on the intro), with keyboard and pointer messages sent through `Pump` and
//! `InputShell`; nothing links to a shard, and every fixture path is an `assert!`/`expect`.
//!
//! # How the barrier works
//!
//! Chat focus registers the text maps 8,7,0x0A at priority 3000 and keyboard barrier map 1 at
//! 2990; equal-priority map insertion prepends. `App::apply_input_actions` takes dispatched
//! actions through `dereth_client_runtime::actions::movement::on_action` into `CharacterInput`, so
//! the barrier stops movement by stopping the map walk. Tests measure input flags and routing
//! counters rather than character displacement.
//!
//! # The two traps this file is written against
//!
//! 1. A tap can press and release within one frame; holding across frames avoids that
//!    cancellation. The helper sends release separately after sampling the input flag throughout
//!    the hold.
//! 2. Blocking everything would pass a focused-only check. Unfocused positive, focused negative
//!    and restored-after-blur cases distinguish that failure. `App::actions_routed` supplies an
//!    independent count that the movement path was exercised.

use crate::common::sim_app::app_in_gameplay;

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::platform::keys::Key;
use dereth_client::pump::{Pump, Win32Message};
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::chat::window::ENTRY;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

/// The same application with **no body**, which is the only configuration in which the residual
/// flycam of [`App::flycam_key`] can move anything at all: `WorldScene::update` runs
/// `FreeCamera::update` only when `character` is `None`.
fn app_in_gameplay_without_a_body(frames: u32) -> App {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    let cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        character: false,
        dat_dir: d,
        ..Config::default()
    };
    let mut app = crate::common::sim_app::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        character: false,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn gameplay(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        ui,
        any.downcast_mut::<GamePlayScreen>()
            .expect("the gameplay screen"),
    )
}

fn find(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    shell
        .ui
        .get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped layout"))
}

fn centre(app: &App, h: ElemHandle) -> (i32, i32) {
    let b = app.ui().expect("shell").ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

fn entry_text(app: &mut App) -> String {
    let entry = find(app, ENTRY);
    let (ui, _) = gameplay(app);
    ui.text_element_mut(entry)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

/// Whether a keyboard barrier is standing right now: the state
/// [`dereth_client::input::InputShell::keyboard_blocked`] reports, read here off the map stack so
/// the test does not trust the helper it is also exercising.
fn barrier_registered(app: &mut App) -> bool {
    app.input_manager_mut()
        .expect("the input shell exists in a UI build")
        .manager
        .maps
        .entries()
        .iter()
        .any(|e| e.map == dereth_input::MAP_BLOCK_KEYBOARD)
}

/// Pump/InputShell keyboard and pointer messages, with key holds across frames. No
/// operating-system window procedure is called.
struct Hand {
    pump: Pump,
    time_ms: u32,
}

impl Hand {
    fn new() -> Self {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 200_000,
        }
    }

    fn send(&mut self, app: &mut App, m: Win32Message) {
        self.pump.dispatch(m);
        if let Some(input) = app.input_manager_mut() {
            input.on_message(m);
        }
    }

    fn key(&mut self, app: &mut App, code: KeyCode, down: bool) {
        self.time_ms += 10;
        let m = self
            .pump
            .key_message_for(code, down, self.time_ms)
            .expect("winit maps this key to a virtual key and a scan code");
        self.send(app, m);
    }

    fn click(&mut self, app: &mut App, at: (i32, i32)) {
        self.time_ms += 10;
        let m = self
            .pump
            .mouse_move_message(f64::from(at.0), f64::from(at.1), self.time_ms);
        self.send(app, m);
        for pressed in [true, false] {
            self.time_ms += 10;
            let m = self
                .pump
                .mouse_button_message(MouseButton::Left, pressed, self.time_ms)
                .expect("the left button is in the 0x201 block");
            self.send(app, m);
        }
        app.frame();
    }

    fn character(&mut self, app: &mut App, ch: char) {
        self.time_ms += 10;
        let m = Win32Message::new(
            dereth_input::win32::msg::WM_CHAR,
            ch as usize,
            0,
            self.time_ms,
        );
        self.send(app, m);
    }

    /// **Hold** `code` down across `frames` frames, then release it, and answer whether the motion
    /// slot named by `read` was ever set while it was held.
    ///
    /// This is the shape a tap cannot substitute for: the press
    /// and the release are separate messages with frames between them, so a build in which the
    /// down and the up cancel inside one frame still reports the truth.
    fn hold(
        &mut self,
        app: &mut App,
        code: KeyCode,
        frames: u32,
        read: fn(dereth_client::character::CharacterInput) -> bool,
    ) -> bool {
        self.key(app, code, true);
        let mut ever = false;
        for _ in 0..frames {
            app.frame();
            ever |= read(app.char_input());
        }
        self.key(app, code, false);
        app.frame();
        ever
    }
}

// -------------------------------------------------------------------------------------------
// 1. The acceptance, both directions, with a held key and a denominator
// -------------------------------------------------------------------------------------------

/// Behaviour: ui.keyboard.a-focused-chat-box-blocks-movement-and-camera-keys
///
/// A held W sets the forward input flag without focus, never sets it with chat focus, and works
/// again after blur. A separate WM_CHAR delivery still inserts w while controls are blocked.
/// This tests routed input state, not measured character displacement.
///
/// The shipped default keymap 0x14000000 (KEYMAP group 10, entry 0x10000001) binds DIK_W 0x11
/// in map 4 to action 41/0x29, MOVE_FORWARD. Player input registers below the focused barrier;
/// movement action decoding sets the forward slot from the start/release flag.
///
/// The unfocused hold must route both press and release and clear forward afterward. With
/// focus, forward must remain false throughout and routed count must stay unchanged. The
/// character path is separate from map 1's keyboard-control stop, so text insertion still works.
#[test]
fn a_held_w_walks_only_while_the_chat_box_does_not_have_focus() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    // (a) Nothing focused. The keyboard reaches map 4 and the forward slot closes.
    assert!(
        !barrier_registered(&mut app),
        "nothing is focused, so nothing blocks the keyboard"
    );
    let before = app.actions_routed();
    let walked = hand.hold(&mut app, KeyCode::KeyW, 3, |c| c.forward);
    let routed_unfocused = app.actions_routed() - before;
    assert!(
        walked,
        "with no text box focused, a held W must close the forward slot"
    );
    assert!(
        routed_unfocused >= 2,
        "one press and one release must both reach the body: routed {routed_unfocused}"
    );
    assert!(
        !app.char_input().forward,
        "and the release must open it again"
    );

    // Chat focus registers text maps 8,7,0x0A at 3000 and keyboard barrier 1 at 2990. The barrier
    // follows the text maps; this helper checks its presence independently of keyboard_blocked.
    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);
    hand.click(&mut app, at);
    {
        let (ui, _) = gameplay(&mut app);
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "the click focused the entry box"
        );
    }
    assert!(
        barrier_registered(&mut app),
        "focusing the box put the barrier up"
    );

    let before = app.actions_routed();
    let walked = hand.hold(&mut app, KeyCode::KeyW, 3, |c| c.forward);
    let routed_focused = app.actions_routed() - before;
    assert!(
        !walked,
        "with the entry focused, a held W must not close the forward slot: typing must not walk \
         the character"
    );
    assert_eq!(
        routed_focused, 0,
        "and nothing at all may reach the body while the barrier stands"
    );

    // ...and the same key still arrives as a character.
    hand.character(&mut app, 'w');
    app.frame();
    assert!(
        entry_text(&mut app).contains('w'),
        "the barrier is a *keyboard control* barrier: the character must still be inserted"
    );

    // (c) Blur the box and movement comes back. Without this the test would also pass on a build
    //     that put the barrier up and never took it down.
    {
        let (ui, _) = gameplay(&mut app);
        ui.set_focus_element(None);
    }
    app.frame();
    assert!(
        !barrier_registered(&mut app),
        "dropping focus takes the barrier away"
    );
    let before = app.actions_routed();
    let walked = hand.hold(&mut app, KeyCode::KeyW, 3, |c| c.forward);
    assert!(walked, "unfocusing the box restores movement");
    assert!(app.actions_routed() > before);

    app.shutdown();
}

/// The other five motion slots reach the body through the same seam, and they reach it from the
/// **shipped** bindings rather than from the debug latch's key choices.
///
/// The shipped default keymap uses X->0x2A back, Z->0x2D strafe-left, C->0x2C strafe-right,
/// A->0x2F turn-left, D->0x2E turn-right and LeftShift->0x32 MovementWalkMode. S->0x2B stops
/// movement by entering Ready and releasing directional slots.
///
/// Run mode is hold_run XOR the character's default-run option. With that option enabled,
/// holding Shift walks and releasing it runs. The application copies the player option into
/// `MovementCommands::ui_toggles_run` before dispatching actions. These assertions pin both
/// polarities for this default-on fixture, with a routing-count positive so a false run flag
/// cannot masquerade as an undelivered key.
#[test]
fn the_shipped_bindings_drive_the_five_other_motion_slots() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    let cases: [(
        KeyCode,
        &str,
        fn(dereth_client::character::CharacterInput) -> bool,
    ); 5] = [
        (KeyCode::KeyX, "DIK_X -> 0x2A Move Backward", |c| c.back),
        (KeyCode::KeyZ, "DIK_Z -> 0x2D Strafe Left", |c| c.step_left),
        (KeyCode::KeyC, "DIK_C -> 0x2C Strafe Right", |c| {
            c.step_right
        }),
        (KeyCode::KeyA, "DIK_A -> 0x2F Turn Left", |c| c.turn_left),
        (KeyCode::KeyD, "DIK_D -> 0x2E Turn Right", |c| c.turn_right),
    ];
    for (code, what, read) in cases {
        assert!(
            hand.hold(&mut app, code, 2, read),
            "{what} must reach the body"
        );
    }

    // The sixth, whose polarity is the other way round. `actions_routed` is the denominator: it
    // is what tells "the key arrived and set the flag to false" from "the key never arrived",
    // which a bare `!run` cannot.
    let before = app.actions_routed();
    hand.key(&mut app, KeyCode::ShiftLeft, true);
    app.frame();
    assert!(
        app.actions_routed() > before,
        "DIK_LSHIFT -> 0x32 must reach the body"
    );
    assert!(
        !app.char_input().run,
        "0x32 is MovementWalkMode: holding it WALKS a shipped body"
    );
    hand.key(&mut app, KeyCode::ShiftLeft, false);
    app.frame();
    assert!(
        app.char_input().run,
        "and releasing it runs, because ToggleRun is default-on"
    );

    // Stop Moving enters Ready and clears directional slots; it is not another direction.
    hand.key(&mut app, KeyCode::KeyX, true);
    app.frame();
    assert!(app.char_input().back, "X is held");
    hand.key(&mut app, KeyCode::KeyS, true);
    app.frame();
    assert!(
        !app.char_input().back,
        "DIK_S is 0x2B Stop Moving -- the Ready state, all slots out"
    );
    hand.key(&mut app, KeyCode::KeyS, false);
    hand.key(&mut app, KeyCode::KeyX, false);

    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 2. Escape
// -------------------------------------------------------------------------------------------

/// Behaviour: ui.keyboard.a-bare-escape-does-not-end-the-process
///
/// Escape must leave the process alive both with and without chat focus. With chat focused,
/// Escape drops focus and retains the text. This test checks process/frame liveness and focus
/// loss; text retention is pinned elsewhere. The following test separately checks Shift+Escape's
/// character-session action.
#[test]
fn escape_no_longer_ends_the_process() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    // (a) Nothing focused.
    assert!(!app.device_done(), "the loop starts alive");
    hand.key(&mut app, KeyCode::Escape, true);
    hand.key(&mut app, KeyCode::Escape, false);
    app.frame();
    assert!(!app.device_done(), "a bare Escape is not an exit path");

    // (b) The chat entry focused.
    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);
    hand.click(&mut app, at);
    {
        let (ui, _) = gameplay(&mut app);
        assert_eq!(
            ui.focus_element(),
            Some(entry),
            "the click focused the entry box"
        );
    }
    hand.key(&mut app, KeyCode::Escape, true);
    hand.key(&mut app, KeyCode::Escape, false);
    assert!(app.frame(), "the frame after Escape still runs");
    assert!(
        !app.device_done(),
        "Escape in the chat box must clear the box, not terminate the client"
    );
    let (ui, _) = gameplay(&mut app);
    assert_ne!(
        ui.focus_element(),
        Some(entry),
        "Escape drops the entry's focus"
    );

    app.shutdown();
}

/// Shift+Escape ends the character session without quitting the process. The shipped default
/// keymap binds it to 0x10000026 in base input map 3. The gameplay key consumer confirms logout;
/// 0x10000027 is the distinct exit action.
///
/// First require at least one live-keymap binding for action 0x10000026, then deliver the actual
/// Shift/Escape messages. Accept either a gameplay screen with logout confirmed and no quit
/// request, or an already completed transition to character management; epilogue is not expected.
#[test]
fn shift_escape_reaches_end_character_session() {
    let mut app = app_in_gameplay(4);

    // 1. Is it bound at all?
    let bound = app
        .input_manager_mut()
        .expect("an input manager")
        .manager
        .keymap
        .sections
        .iter()
        .flat_map(|s| s.bindings().iter())
        .filter(|(_, a)| a.0 == 0x1000_0026)
        .count();
    eprintln!("the shipped keymap binds End Character Session to {bound} control(s)");
    assert!(
        bound > 0,
        "Shift+DIK_ESCAPE -> 0x10000026 is in the shipped default keymap"
    );

    // 2. Does a real Shift+Escape produce it?
    let mut hand = Hand::new();
    hand.key(&mut app, KeyCode::ShiftLeft, true);
    hand.key(&mut app, KeyCode::Escape, true);
    app.frame();
    hand.key(&mut app, KeyCode::Escape, false);
    hand.key(&mut app, KeyCode::ShiftLeft, false);
    app.frame();

    // 3. Did it reach the screen that ends the session? The screen may already be gone, which is
    //    the strongest form of the assertion.
    let mode_now = app.ui().expect("a shell").flow.current_mode();
    if mode_now == Some(mode::GAME_PLAY) {
        let (_ui, screen) = gameplay(&mut app);
        assert!(
            screen.logout_confirmed,
            "logout is confirmed -- Shift+Escape must reach the 0x10000026 action"
        );
        assert!(
            !screen.should_quit_on_logout,
            "0x10000026 goes to character select, not to exit -- that is 0x10000027"
        );
    } else {
        assert_eq!(
            mode_now,
            Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT),
            "the logout must leave the game screen for character select"
        );
    }
    assert!(
        !app.device_done(),
        "End Character Session logs off; it does not quit the process"
    );

    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 3. The residual flycam keys, which are the part with no home in the real seam
// -------------------------------------------------------------------------------------------

/// Debug flycam vertical translation has no corresponding shipped camera-translation action:
/// the ordinary character camera follows a body. Its Space/C latch therefore uses a separate
/// keyboard-blocked guard. C also has a normal strafe binding; this paragraph describes only
/// its bodyless flycam role.
///
/// Compare InputShell::keyboard_blocked with map-stack presence before and during focus, then
/// require it false after blur. Always-true would disable the debug camera; always-false would
/// restore the typing leak. These assertions check the guard, not camera displacement.
#[test]
fn the_flycam_gate_tracks_the_barrier_in_both_directions() {
    let mut app = app_in_gameplay(4);

    let blocked = |app: &mut App| {
        app.input_manager_mut()
            .expect("the input shell")
            .keyboard_blocked()
    };
    assert!(
        !blocked(&mut app),
        "nothing focused: the flycam keys are live"
    );
    assert_eq!(blocked(&mut app), barrier_registered(&mut app));

    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);
    let mut hand = Hand::new();
    hand.click(&mut app, at);
    assert!(
        blocked(&mut app),
        "a focused text box blocks the flycam keys too"
    );
    assert_eq!(blocked(&mut app), barrier_registered(&mut app));

    {
        let (ui, _) = gameplay(&mut app);
        ui.set_focus_element(None);
    }
    app.frame();
    assert!(!blocked(&mut app), "and dropping focus gives them back");

    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 4. The intro's own maps
// -------------------------------------------------------------------------------------------

/// The intro owns maps 9 and 3 at priority 3000/0xBB8 and registers no text-element map, so no
/// keyboard barrier stands on it. Map 9 and text map 7 both bind Return to action 0x25, so adding
/// the text maps would make the winner depend on registration order.
///
/// Require no keyboard barrier and the exact intro map set, so registering nothing cannot satisfy
/// the negative. Then send a character and require its delivery counter to advance. Character
/// input remains separate from control maps.
#[test]
fn the_intro_still_skips_on_a_character_with_the_barrier_standing() {
    let d = client_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        d.display()
    );
    let cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: d,
        ..Config::default()
    };
    let mut app = crate::common::sim_app::new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    app.queue_ui_mode(mode::INTRO);
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(
        app.ui().expect("a shell").flow.current_mode(),
        Some(mode::INTRO),
        "the intro is up"
    );
    // Intro enables character delivery but has no text element, so it owns no focused-text
    // maps and no keyboard barrier. Its two explicit control maps are asserted separately.
    assert!(
        !barrier_registered(&mut app),
        "retail's intro registers no text-element map, so no map 1 either"
    );
    // ...and the intro's own two maps ARE registered, which is what stops the assertion above
    // being satisfied by an intro that registered nothing whatsoever.
    assert_eq!(
        app.ui().expect("a shell").registered_mode_maps(),
        dereth_client::ui::INTRO_INPUT_MAPS,
        "the intro screen registers input maps 9 and 3, both at priority 0xBB8"
    );

    let before = app.ui().expect("a shell").stats.intro_characters;
    let mut hand = Hand::new();
    hand.character(&mut app, ' ');
    app.frame();
    assert!(
        app.ui().expect("a shell").stats.intro_characters > before,
        "intro character delivery uses the character path and the barrier does not gate it"
    );

    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 5. The camera half of the same seam
// -------------------------------------------------------------------------------------------

/// Numpad4/6/8/2 bind camera-left/right/up/down actions 0x35/0x36/0x37/0x38 in shipped map 5.
/// Camera input registers that map at gameplay priority through BASE_MAP_REGISTRATIONS, below
/// the focused keyboard barrier. Held rotation uses per-frame look flags rather than a single
/// event's displacement.
///
/// These checks set and clear all four flags without focus, then verify left-look remains false
/// with chat focus. Arrow keys are deliberately excluded: map 4 uses them for forward/back/turn,
/// while alternate-camera map 6 supplies rotation there.
#[test]
fn the_numpad_camera_keys_reach_the_look_flags_and_the_barrier_gates_them() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    let cases: [(
        KeyCode,
        &str,
        fn(dereth_client::camera::CameraInput) -> bool,
    ); 4] = [
        (
            KeyCode::Numpad4,
            "DIK_NUMPAD4 -> 0x35 Rotate Camera Left",
            |c| c.look_left,
        ),
        (
            KeyCode::Numpad6,
            "DIK_NUMPAD6 -> 0x36 Rotate Camera Right",
            |c| c.look_right,
        ),
        (
            KeyCode::Numpad8,
            "DIK_NUMPAD8 -> 0x37 Rotate Camera Up",
            |c| c.look_up,
        ),
        (
            KeyCode::Numpad2,
            "DIK_NUMPAD2 -> 0x38 Rotate Camera Down",
            |c| c.look_down,
        ),
    ];
    for (code, what, read) in cases {
        hand.key(&mut app, code, true);
        app.frame();
        assert!(read(app.camera_input()), "{what} must set its look flag");
        hand.key(&mut app, code, false);
        app.frame();
        assert!(
            !read(app.camera_input()),
            "{what} must clear it again on the release"
        );
    }

    // ...and with the chat box focused the same keys reach nothing, because map 1 breaks the walk
    // for every keyboard control and the camera's are keyboard controls.
    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);
    hand.click(&mut app, at);
    assert!(
        barrier_registered(&mut app),
        "focusing the box put the barrier up"
    );
    hand.key(&mut app, KeyCode::Numpad4, true);
    app.frame();
    assert!(
        !app.camera_input().look_left,
        "typing must not spin the camera either -- the barrier is not movement-specific"
    );
    hand.key(&mut app, KeyCode::Numpad4, false);

    app.shutdown();
}

/// The action-listener walk continues when a handler declines; movement decoding returns
/// MovementAction::NotHandled outside its movement/emote cases. Backquote's shipped action
/// 0x1000005A in map 0x10000002 is combat toggle, handled beyond the movement/camera seam.
/// After a W routing positive, this test requires that action not increment App::actions_routed.
/// It checks non-consumption by that seam, not the downstream combat-state change itself.
#[test]
fn an_action_the_body_does_not_model_is_put_back() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    // The denominator: the movement seam is alive in this app at all.
    let before = app.actions_routed();
    hand.key(&mut app, KeyCode::KeyW, true);
    app.frame();
    hand.key(&mut app, KeyCode::KeyW, false);
    app.frame();
    assert!(
        app.actions_routed() > before,
        "the movement seam is live in this application"
    );

    let before = app.actions_routed();
    hand.key(&mut app, KeyCode::Backquote, true);
    app.frame();
    hand.key(&mut app, KeyCode::Backquote, false);
    app.frame();
    assert_eq!(
        app.actions_routed(),
        before,
        "the combat-mode toggle is not a motion command and must reach the combat handler untouched"
    );

    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 6. The residual flycam latch
// -------------------------------------------------------------------------------------------

/// App::flycam_key exposes the small residual latch for direct tests without constructing a
/// winit KeyEvent with platform-private data. W must not write the body's forward slot, and
/// Escape must not end the process: either would bypass the barrier.
///
/// In a bodyless unfocused application, C sets/clears down and Space sets up. Focus blocks C;
/// with a body, both vertical keys remain inactive even without focus. These are flag checks,
/// not a camera-position measurement.
#[test]
fn the_residual_latch_moves_only_the_flycam_and_only_when_nothing_is_focused() {
    // (a) No body and nothing focused, which is the only state in which these two keys may act.
    let mut app = app_in_gameplay_without_a_body(4);

    assert!(!barrier_registered(&mut app), "nothing focused");
    app.flycam_key(Key::KEY_C, true);
    assert!(
        app.camera_input().down,
        "the flycam's descend key is the part with no action behind it"
    );
    app.flycam_key(Key::KEY_C, false);
    assert!(!app.camera_input().down);
    app.flycam_key(Key::SPACE, true);
    assert!(app.camera_input().up);
    app.flycam_key(Key::SPACE, false);

    // It does **not** drive the body.
    app.flycam_key(Key::KEY_W, true);
    assert!(
        !app.char_input().forward,
        "the latch must not write char_input, or the barrier would be inert"
    );

    // It does **not** end the process.
    app.flycam_key(Key::ESCAPE, true);
    assert!(!app.device_done(), "Escape is not an exit");

    // And with the chat box focused it does nothing at all.
    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);
    let mut hand = Hand::new();
    hand.click(&mut app, at);
    assert!(barrier_registered(&mut app), "the entry has focus");
    app.flycam_key(Key::KEY_C, true);
    assert!(
        !app.camera_input().down,
        "the one hand-gated control must be gated by the barrier like every other key"
    );
    app.shutdown();

    // With a body there is no debug flycam to translate. Test both vertical keys without
    // focus, so a focus-only guard cannot satisfy this embodied negative.
    let mut app = app_in_gameplay(4);
    assert!(
        !barrier_registered(&mut app),
        "nothing focused in the embodied app either"
    );
    app.flycam_key(Key::KEY_C, true);
    assert!(
        !app.camera_input().down,
        "with a body there is no free camera to lower"
    );
    app.flycam_key(Key::SPACE, true);
    assert!(!app.camera_input().up, "and none to raise");
    app.shutdown();
}
