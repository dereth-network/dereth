//! The movement and camera keys through the shipped default key map: the three command lists
//! (`dereth_client_runtime::actions::movement::CommandLists`) keep held commands, so releasing a
//! second key resumes the first; `DIK_Q` (0x30 Autorun) toggles the run lock and any movement key
//! cancels it; the eight world camera commands reach the camera set; the alternate camera key
//! registers input map 6 only while held; the run lock and camera commands sit behind the typing
//! barrier; and Escape in a focused chat entry drops focus without clearing its text, as retail
//! does. Assertions read command lists and input flags, not displacement. Fixture: a headless
//! `App` in the gameplay screen on the retail dats, fed Windows-style messages through the pump;
//! absent dats fail.

use crate::common::sim_app::app_in_gameplay;

use dereth_client::app::App;

use dereth_client::pump::{Pump, Win32Message};

use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::chat::window::ENTRY;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use winit::event::MouseButton;
use winit::keyboard::KeyCode;

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

/// Whether the keyboard barrier is standing, read off the map stack rather than off the helper
/// that reports it, so the test does not trust the thing it is also exercising.
fn barrier_registered(app: &mut App) -> bool {
    app.input_manager_mut()
        .expect("the input shell exists in a UI build")
        .manager
        .maps
        .entries()
        .iter()
        .any(|e| e.map == dereth_input::MAP_BLOCK_KEYBOARD)
}

/// Read alternate-camera map 6 directly from the map walk. It is registered by action 0x3E
/// while held, not by the startup map set.
fn alternate_map_registered(app: &mut App) -> bool {
    app.input_manager_mut()
        .expect("the input shell")
        .manager
        .maps
        .entries()
        .iter()
        .any(|e| e.map == dereth_input::ALTERNATE_CAMERA_MAP)
}

/// Feed Windows-style keyboard and pointer messages through the pump and input manager.
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
            time_ms: 300_000,
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

    /// Press and release with a frame after each edge. Assertions after this helper observe
    /// both edges; down()/up() below allow inspection while a key remains held.
    fn tap(&mut self, app: &mut App, code: KeyCode) {
        self.key(app, code, true);
        app.frame();
        self.key(app, code, false);
        app.frame();
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

    /// Press `code` and leave it down, one frame.
    fn down(&mut self, app: &mut App, code: KeyCode) {
        self.key(app, code, true);
        app.frame();
    }

    /// Release `code`, one frame.
    fn up(&mut self, app: &mut App, code: KeyCode) {
        self.key(app, code, false);
        app.frame();
    }
}

// -------------------------------------------------------------------------------------------
// 1. The three command lists, driven through the shipped bindings
// -------------------------------------------------------------------------------------------

/// Hold A, press D, release D: the left-turn command must resume.
/// Command removal reissues the remaining head as a start, so releasing the second
/// key becomes a renewed press of the first. A single current-command variable cannot retain
/// that history. The final release must also stop turning, rejecting a never-release mutation.
///
/// The shipped default key map binds DIK_A to 0x2F Turn Left and DIK_D to 0x2E Turn Right in map 4.
/// This test checks list order and turn flags through those bindings, not body displacement.
#[test]
fn releasing_the_second_turn_key_resumes_the_first() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    let turn = |app: &App| (app.char_input().turn_left, app.char_input().turn_right);
    let list = |app: &App| -> Vec<u32> {
        app.movement_commands()
            .lists
            .turn
            .iter()
            .map(|e| e.command)
            .collect()
    };

    // The denominator: something reached the interpreter at all.
    let before = app.movement_commands().commands_handled;

    hand.down(&mut app, KeyCode::KeyA);
    assert_eq!(turn(&app), (true, false), "DIK_A -> 0x2F Turn Left");
    assert_eq!(
        list(&app),
        vec![0x6500_000E],
        "TurnLeft is the head of the turn list"
    );

    hand.down(&mut app, KeyCode::KeyD);
    assert_eq!(turn(&app), (false, true), "the second key wins");
    assert_eq!(
        list(&app),
        vec![0x6500_000D, 0x6500_000E],
        "and the first is still on the list underneath it -- which is the whole mechanism"
    );

    hand.up(&mut app, KeyCode::KeyD);
    assert_eq!(
        turn(&app),
        (true, false),
        "releasing the second key resumes the first: this is what one bool per direction cannot do"
    );
    assert_eq!(list(&app), vec![0x6500_000E]);

    hand.up(&mut app, KeyCode::KeyA);
    assert_eq!(
        turn(&app),
        (false, false),
        "and releasing the first really does stop the turn"
    );
    assert!(list(&app).is_empty());

    assert!(
        app.movement_commands().commands_handled >= before + 4,
        "four presses and releases must all have reached the interpreter: {} -> {}",
        before,
        app.movement_commands().commands_handled
    );
    app.shutdown();
}

/// Behaviour: movement.keys.releasing-the-second-key-resumes-the-first
/// Repeat held-command resumption on the substate list: hold W, press X, then release X.
/// DIK_W maps 0x29 Move Forward to WalkForward 0x45000005; DIK_X maps 0x2A Move Backward to
/// WalkBackwards 0x45000006. Both command values carry 0x40000000 and 0x04000000, selecting the
/// substate list. Releasing the last key must clear both movement flags and the list.
#[test]
fn releasing_the_second_movement_key_resumes_the_first() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    hand.down(&mut app, KeyCode::KeyW);
    assert!(app.char_input().forward);
    hand.down(&mut app, KeyCode::KeyX);
    assert!(
        app.char_input().back && !app.char_input().forward,
        "the second key wins"
    );
    hand.up(&mut app, KeyCode::KeyX);
    assert!(
        app.char_input().forward && !app.char_input().back,
        "releasing backward resumes forward"
    );
    hand.up(&mut app, KeyCode::KeyW);
    assert!(
        !app.char_input().forward && !app.char_input().back,
        "and the last release stops"
    );
    assert!(app.movement_commands().lists.substate.is_empty());
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 2. The run lock
// -------------------------------------------------------------------------------------------

/// Behaviour: movement.run-lock.any-movement-key-cancels-the-run-lock
/// DIK_Q toggles autorun; a subsequent forward command cancels it without losing that command.
/// `CharacterInput`'s autorun view reflects the command-list latch rather than duplicating it.
///
/// The toggle reapplies movement on both transitions, which the off-direction assertion checks.
/// With autorun set, the base command is WalkForward even with an empty substate list. The
/// second Q must clear both autorun and forward input. Turning it on again and then pressing W
/// must cancel autorun but retain W's WalkForward command; releasing W must finally stop it.
///
/// Input activation cancels the run lock on starts of 0x29, 0x2A or 0x2B by releasing
/// action 0x30. This test samples W (0x29); it does not prove every movement key cancels autorun,
/// and turning while autorunning is a separate steering case. All movement readings are flags.
#[test]
fn the_run_lock_starts_on_dik_q_and_any_movement_key_cancels_it() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    assert!(!app.char_input().auto_run, "the run lock starts off");
    assert!(!app.char_input().forward);

    // 1. On.
    hand.tap(&mut app, KeyCode::KeyQ);
    assert!(app.char_input().auto_run, "DIK_Q -> 0x30 Autorun");
    assert!(
        app.char_input().forward,
        "autorun supplies WalkForward in the base slot while it is set"
    );
    assert!(
        app.movement_commands().lists.substate.is_empty(),
        "with nothing on the substate list"
    );

    // 2. Off, and it stops.
    hand.tap(&mut app, KeyCode::KeyQ);
    assert!(!app.char_input().auto_run, "the toggle alternates");
    assert!(
        !app.char_input().forward,
        "turning autorun off also clears forward input -- movement is reapplied in both directions"
    );

    // 3. On again, then a movement key.
    hand.tap(&mut app, KeyCode::KeyQ);
    assert!(app.char_input().auto_run);
    hand.down(&mut app, KeyCode::KeyW);
    assert!(
        !app.char_input().auto_run,
        "any movement key cancels the run lock"
    );
    assert!(
        app.char_input().forward,
        "and the character is still walking, on W's own WalkForward"
    );
    assert_eq!(
        app.movement_commands()
            .lists
            .substate
            .iter()
            .map(|e| e.command)
            .collect::<Vec<_>>(),
        vec![0x4500_0005]
    );
    hand.up(&mut app, KeyCode::KeyW);
    assert!(!app.char_input().forward);
    app.shutdown();
}

/// Autorun is behind the typing barrier: with the chat entry focused, Q starts no autorun while a
/// separately injected character still reaches the text, and after blur Q starts the run lock.
#[test]
fn the_run_lock_is_behind_the_typing_barrier() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);
    hand.click(&mut app, at);
    assert!(
        barrier_registered(&mut app),
        "focusing the entry put the barrier up"
    );

    hand.tap(&mut app, KeyCode::KeyQ);
    assert!(
        !app.char_input().auto_run,
        "typing `q` must not start autorun"
    );
    assert!(!app.char_input().forward);
    // Separately inject WM_CHAR: control blocking must not suppress text delivery.
    hand.character(&mut app, 'q');
    app.frame();
    assert!(
        entry_text(&mut app).contains('q'),
        "the barrier is a keyboard *control* barrier"
    );

    // Drop focus and it works again -- without this the test passes on a build that blocks for ever.
    {
        let (ui, _) = gameplay(&mut app);
        ui.set_focus_element(None);
    }
    app.frame();
    assert!(!barrier_registered(&mut app));
    hand.tap(&mut app, KeyCode::KeyQ);
    assert!(
        app.char_input().auto_run,
        "unfocused, DIK_Q starts the run lock"
    );
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 3. The camera's other eight commands
// -------------------------------------------------------------------------------------------

/// The world-dependent camera actions reach `CameraControl::on_action` from their keys. These
/// assertions read arithmetic results and flags.
///
/// | key | action | behavior checked |
/// |---|---|---|
/// | DIK_DECIMAL | 0x3A First Person | offset (0, 0.18, 0), translational stiffness 1.0 |
/// | DIK_NUMPAD0 | 0x39 Default Camera | offset (0, -2.5s, 0.75s) restored |
/// | DIK_NUMPAD5 | 0x3B Overhead View | looking_down toggles in both directions |
/// | DIK_NUMPADENTER | 0x3C Map View | in_map_mode toggles in both directions |
/// | DIK_SUBTRACT | 0x33 Zoom In | closer hold and a shorter offset |
/// | DIK_ADD | 0x34 Zoom Out | farther hold sets and clears |
/// | DIK_DIVIDE | 0x3E Alternate Mode | separate test checks map 6 and mouselook activity |
///
/// The eighth action, 0x3D Toggle Mouselook, is bound to middle button DIMOFS_BUTTON2. Its shared
/// behavior is exercised through 0x3E; this file does not independently inject the middle button.
#[test]
fn the_eight_world_camera_commands_reach_the_camera_set() {
    let mut app = app_in_gameplay(6);
    let mut hand = Hand::new();

    let offset = |app: &App| {
        app.camera_control()
            .expect("a body means a camera")
            .manager
            .viewer_offset
    };
    let scale = app.camera_control().expect("a camera").manager.scale;
    let default_offset = dereth_primitives::Vec3::new(0.0, -2.5 * scale, 0.75 * scale);

    assert_eq!(
        offset(&app),
        default_offset,
        "the shipped third-person camera, from camera-state construction's own default offsets"
    );

    // 0x3A First Person -- `DIK_DECIMAL`.
    hand.tap(&mut app, KeyCode::NumpadDecimal);
    {
        let c = app.camera_control().expect("a camera");
        assert_eq!(
            c.manager.viewer_offset,
            dereth_client::camera::IN_HEAD_OFFSET,
            "first-person offset"
        );
        assert!(
            (c.manager.t_stiffness - 1.0).abs() < 1e-6,
            "first person snaps: translational stiffness is 1"
        );
    }

    // 0x39 Move Camera to Default -- `DIK_NUMPAD0`. It is the way back out of first person.
    hand.tap(&mut app, KeyCode::Numpad0);
    assert_eq!(
        offset(&app),
        default_offset,
        "default camera offsets are restored"
    );

    // 0x3B Overhead View -- `DIK_NUMPAD5`, a toggle.
    hand.tap(&mut app, KeyCode::Numpad5);
    assert!(
        app.camera_control().expect("a camera").set.looking_down,
        "overhead view is enabled"
    );
    hand.tap(&mut app, KeyCode::Numpad5);
    assert!(
        !app.camera_control().expect("a camera").set.looking_down,
        "and back"
    );

    // 0x3C Map View -- `DIK_NUMPADENTER`, a toggle.
    hand.tap(&mut app, KeyCode::NumpadEnter);
    assert!(
        app.camera_control().expect("a camera").set.in_map_mode,
        "map view is enabled"
    );
    hand.tap(&mut app, KeyCode::NumpadEnter);
    assert!(
        !app.camera_control().expect("a camera").set.in_map_mode,
        "and back"
    );

    // 0x34 Zoom Out -- `DIK_ADD`, a hold. The flag is set before the rate gate, which is why a
    // press is enough to see it.
    hand.down(&mut app, KeyCode::NumpadAdd);
    assert!(
        app.camera_control().expect("a camera").set.farther,
        "zoom-out hold is set"
    );
    hand.up(&mut app, KeyCode::NumpadAdd);
    assert!(
        !app.camera_control().expect("a camera").set.farther,
        "zoom-out hold is cleared"
    );

    // 0x33 Zoom In -- DIK_SUBTRACT. Scaling the whole viewer offset must shorten its magnitude,
    // so a hold flag alone is insufficient; compare the squared lengths after four more frames.
    let sq = |v: dereth_primitives::Vec3| v.x * v.x + v.y * v.y + v.z * v.z;
    let before = sq(offset(&app));
    hand.down(&mut app, KeyCode::NumpadSubtract);
    assert!(
        app.camera_control().expect("a camera").set.closer,
        "zoom-in hold is set"
    );
    for _ in 0..4 {
        app.frame();
    }
    hand.up(&mut app, KeyCode::NumpadSubtract);
    let after = sq(offset(&app));
    assert!(
        after < before,
        "zooming in must shorten the viewer offset: {before} -> {after}"
    );

    app.shutdown();
}

/// Alternate-camera action 0x3E registers map 6 while held and also controls mouselook.
/// The press registers the map at unfocused-UI priority, and the release unregisters it; each
/// edge shares the corresponding 0x3D mouselook behavior. Both effects are checked on both edges.
/// Map 6 must also be absent initially: registered at start-up it would permanently outrank the
/// movement bindings.
#[test]
fn the_alternate_camera_key_registers_input_map_six_while_it_is_held() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    assert!(
        !alternate_map_registered(&mut app),
        "map 6 is not a startup registration -- it belongs to action 0x3E"
    );
    assert!(!app.camera_control().expect("a camera").set.mouselook_active);

    hand.down(&mut app, KeyCode::NumpadDivide);
    assert!(
        alternate_map_registered(&mut app),
        "0x3E's press registers map 6"
    );
    assert!(
        app.camera_control().expect("a camera").set.mouselook_active,
        "...and activates the shared 0x3D mouselook behavior"
    );

    hand.up(&mut app, KeyCode::NumpadDivide);
    assert!(
        !alternate_map_registered(&mut app),
        "0x3E's release unregisters it again"
    );
    assert!(!app.camera_control().expect("a camera").set.mouselook_active);
    app.shutdown();
}

/// Behaviour: camera.keys.the-alternate-camera-key-turns-the-arrows-into-camera-keys
/// Arrow keys normally select movement, then camera control while alternate mode is held.
/// The shipped map 4 binds Up/Down/Left/Right to forward/backward/left turn/right turn. Map 6
/// binds those keys to camera rotation at priority 2000, above map 4's 1000, and the map walk
/// keeps the first equally good match, so map 6 must be registered only while 0x3E is held.
///
/// Sample Up and Left in the ordinary state, Up with 0x3E held, and Up again after release.
/// These flag assertions establish the registration/ranking transition for the sampled keys;
/// they do not measure body or camera displacement or exercise all four arrows.
#[test]
fn the_arrow_keys_are_movement_until_the_alternate_camera_key_is_held() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    // (a) Ordinarily: map 4.
    hand.down(&mut app, KeyCode::ArrowUp);
    assert!(
        app.char_input().forward,
        "DIK_UP is 0x29 Move Forward in the shipped map 4"
    );
    assert!(!app.camera_input().look_up, "and it is not a camera key");
    hand.up(&mut app, KeyCode::ArrowUp);
    hand.down(&mut app, KeyCode::ArrowLeft);
    assert!(app.char_input().turn_left, "DIK_LEFT is 0x2F Turn Left");
    hand.up(&mut app, KeyCode::ArrowLeft);

    // (b) With 0x3E held, map 6 is registered above map 4 and the same keys become the camera --
    //     which is exactly what alternate camera mode is for.
    hand.down(&mut app, KeyCode::NumpadDivide);
    assert!(alternate_map_registered(&mut app));
    hand.down(&mut app, KeyCode::ArrowUp);
    assert!(
        app.camera_input().look_up,
        "in alternate mode DIK_UP is 0x37 Rotate Camera Up"
    );
    assert!(!app.char_input().forward, "and it is no longer movement");
    hand.up(&mut app, KeyCode::ArrowUp);
    hand.up(&mut app, KeyCode::NumpadDivide);

    // (c) ...and it goes back.
    hand.down(&mut app, KeyCode::ArrowUp);
    assert!(
        app.char_input().forward,
        "releasing 0x3E gives the arrows back to movement"
    );
    hand.up(&mut app, KeyCode::ArrowUp);
    app.shutdown();
}

/// The world-camera commands share the map-1 typing barrier. This test samples overhead-view
/// and alternate-map actions while focused, then proves overhead-view works again after blur.
#[test]
fn the_world_camera_commands_are_behind_the_typing_barrier() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

    let entry = find(&app, ENTRY);
    let at = centre(&app, entry);
    hand.click(&mut app, at);
    assert!(barrier_registered(&mut app));

    hand.tap(&mut app, KeyCode::Numpad5);
    assert!(
        !app.camera_control().expect("a camera").set.looking_down,
        "typing must not put the camera overhead"
    );
    hand.down(&mut app, KeyCode::NumpadDivide);
    assert!(
        !alternate_map_registered(&mut app),
        "nor register the alternate camera map"
    );
    hand.up(&mut app, KeyCode::NumpadDivide);

    // The other direction, in the same run.
    {
        let (ui, _) = gameplay(&mut app);
        ui.set_focus_element(None);
    }
    app.frame();
    hand.tap(&mut app, KeyCode::Numpad5);
    assert!(
        app.camera_control().expect("a camera").set.looking_down,
        "unfocused, DIK_NUMPAD5 is Overhead View"
    );
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 4. Escape in a focused chat entry
// -------------------------------------------------------------------------------------------

/// Escape drops entry focus and retains text, matching retail's chat behavior.
///
/// The chat window's child-action handler consumes Escape action 0x27 after deactivating
/// the entry. That operation relinquishes focus, deactivates the element and sends the chat-entry
/// toggle notice with false. None of these changes the glyph list. Focus relinquishment clears
/// the focus pointer/flag; deactivation clears its active flag and broadcasts element message
/// 0x2A, whose chat handler only registers for a global message. The false toggle notice does
/// not call the command interpreter; only its true branch does.
///
/// Retail's chat clears text after Enter processes a command and at the
/// endpoints of history navigation. The text element's own Escape action is preempted here:
/// parent child-action handling runs first and reports the action consumed.
///
/// Current deactivation relinquishes focus and clears the chat-entry-active state without
/// changing text. The assertions retain all four controls: focus and barrier drop; text stays;
/// W then sets forward input without entering text; a second unfocused Escape leaves the process
/// alive. The last two are input/process checks, not a physical movement measurement.
#[test]
fn escape_drops_the_focus_and_leaves_the_text_exactly_as_retail_does() {
    let mut app = app_in_gameplay(4);
    let mut hand = Hand::new();

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
    assert!(barrier_registered(&mut app), "and put the barrier up");

    for ch in "hail".chars() {
        hand.character(&mut app, ch);
    }
    app.frame();
    assert_eq!(
        entry_text(&mut app),
        "hail",
        "the four characters went into the box"
    );

    hand.tap(&mut app, KeyCode::Escape);

    // (a) The focus drops.
    {
        let (ui, _) = gameplay(&mut app);
        assert_ne!(
            ui.focus_element(),
            Some(entry),
            "Escape relinquishes the entry's focus"
        );
    }
    assert!(
        !barrier_registered(&mut app),
        "so the keyboard barrier comes down with it"
    );
    assert!(!app.device_done(), "and the process is still running");

    // (b) Deactivating the chat entry leaves its text intact.
    assert_eq!(
        entry_text(&mut app),
        "hail",
        "the Escape path relinquishes focus, deactivates and sends a notice; Enter clears text when processing the command"
    );

    // (c) The next keystroke reaches the character rather than the box -- the point of Escape.
    let before_text = entry_text(&mut app);
    hand.down(&mut app, KeyCode::KeyW);
    assert!(
        app.char_input().forward,
        "with the box blurred, W walks again"
    );
    hand.up(&mut app, KeyCode::KeyW);
    assert_eq!(
        entry_text(&mut app),
        before_text,
        "and nothing else went into the box"
    );

    // (d) A second Escape with nothing focused is inert.
    hand.tap(&mut app, KeyCode::Escape);
    assert!(!app.device_done(), "a bare Escape is not an exit path");
    app.shutdown();
}
