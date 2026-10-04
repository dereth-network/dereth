//! The UI, input and audio shell: the documented opening mode transitions run in order, a UI
//! element draws over the world, a key reaches the input manager and moves the body, and a
//! button press plays a sound.
//!
//! Fixture: the retail dats; the shell driven frame by frame through its modes, a headless App on
//! a software device for the pixels and the key, and window messages synthesised by the real
//! message pump. Four oracles, one per claim:
//!
//! | claim | oracle |
//! |---|---|
//! | the mode transitions | the original state diagram and the eight registered ids of its factory table |
//! | a UI element draws over the world | the retail dats and the frame itself: two runs of the same scene, one with the UI and one without, compared pixel by pixel |
//! | a key reaches `dereth-input` | the retail default keymap `0x14000000`: input map 4 binds `DIK_W` to action 41, which `dereth_client_runtime::actions::movement::action` names `MOVE_FORWARD` |
//! | a sound plays | the retail `client_portal.dat`: the UI sound table and the `0x0A` wave its button-press row names |
//!
//! **Injected keystrokes are not evidence.** Nothing here
//! touches the desktop: the window message is synthesised by `crate::pump`, which is the same code
//! the real pump runs, and it is fed to the application's own per-frame path.
//!
//! A machine without the retail dats fails.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::ui::{HostState, UiShell};
use dereth_ui::framework::mode;
use dereth_ui::UiMode;

/// The retail store, or a failed test: a skipped test and a passing test would be the same
/// green line.
fn store() -> std::sync::Arc<dereth_dat::RetailDatStore> {
    crate::common::dats()
}

/// A headless App with its UI shell up, or a failed test: the gpu tier needs a software device
/// and the retail dats, and a skip would read as a pass.
fn headless(cfg: Config) -> App {
    let mut app = App::new(cfg)
        .unwrap_or_else(|e| panic!("the gpu tier needs a headless App on a software device: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the UI shell must come up over the retail dats: {e}"));
    app
}

fn base_config() -> Config {
    Config {
        headless: true,
        // No device: a regression must not take the machine's audio endpoint.
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// One local-time value, advanced the way the headless clock advances it.
fn at(frame: u32) -> dereth_primitives::LocalTime {
    dereth_primitives::LocalTime(f64::from(frame) / 30.0)
}

/// Step the shell until `want` is the current mode, and return the next frame number.
///
/// The intro really plays: a movie media step and two splash states whose media
/// lists are 0.1 + 0.75 + 2.75 + 0.5 seconds each, so the opening run is hundreds of frames rather
/// than three. What this file asserts is the **order** of the transitions and that every one of them
/// produced a live screen.
fn run_until(
    shell: &mut UiShell,
    host: &HostState,
    from: u32,
    want: dereth_ui::UiMode,
    limit: u32,
) -> u32 {
    let mut pump = dereth_ui::NullInputPump;
    let mut f = from;
    while f < from + limit {
        shell.frame(at(f), host, &mut pump);
        f += 1;
        if shell.flow.current_mode() == Some(want) {
            return f;
        }
    }
    panic!(
        "the flow did not reach {want:?} in {limit} frames: {:?}",
        shell.flow.current_mode()
    );
}

// ---------------------------------------------------------------------------------------------
// 1. The mode state machine
// ---------------------------------------------------------------------------------------------

/// Behaviour: presentation.shell.the-opening-mode-transitions-run-in-order
///
/// Oracle: the original UI-flow state diagram. The opening run is
///
/// ```text
/// [*] --> DataPatch : UI-flow construction queues mode 0x10000003
/// DataPatch --> Intro : connect=1.0 AND patch=1.0 AND charset received
/// Intro --> CharSel : movie list exhausted
/// CharSel --> InGame : the begin-enter-world notice arrives
/// ```
///
/// and every one of those edges is a *deferred* switch: the mode queue records the next mode and
/// UI-flow mode application performs it once per frame, last, inside global message 3. So the
/// recording is one mode per frame, in order, and that ordering is the thing being asserted.
#[test]
fn the_documented_opening_mode_transitions_run_in_the_documented_order() {
    let store = store();
    let mut shell = UiShell::new(&store, (800, 600))
        .unwrap_or_else(|e| panic!("the UI shell must come up over the retail dats: {e}"));

    // UI-flow construction has run: the data-patch mode is queued and no screen exists yet.
    assert_eq!(shell.flow.queued_mode(), Some(mode::DATA_PATCH));
    assert_eq!(
        shell.flow.current_mode(),
        None,
        "the switch happens on the first message-3 tick"
    );

    // What the client reads out of globals. `has_packet_controller` is false because this run has
    // no network, which is the documented `!has_packet_controller` branch.
    let mut host = HostState {
        connected: true,
        patch_finished: true,
        ..HostState::default()
    };

    let mut pump = dereth_ui::NullInputPump;
    let f = run_until(&mut shell, &host, 0, mode::CHARACTER_MANAGEMENT, 600);
    assert_eq!(
        shell.transitions(),
        &[mode::DATA_PATCH, mode::INTRO, mode::CHARACTER_MANAGEMENT],
        "the documented opening run"
    );

    // The begin-enter-world notice says the session became
    // playable, so the character-select screen hands over to the in-game one.
    host.in_world = true;
    shell.frame(at(f), &host, &mut pump);
    assert_eq!(shell.flow.current_mode(), Some(mode::GAME_PLAY));

    // Every switch produced a live screen. A mode switch swallows a screen `create` failure and
    // leaves no current screen, which would otherwise be a blank window and no error.
    assert_eq!(
        shell.stats.screen_create_failures, 0,
        "a screen failed to build"
    );
    assert_eq!(shell.stats.unregistered_mode_requests, 0);
    assert_eq!(shell.stats.mode_switches, 4);
}

/// Oracle: the same diagram's two error edges.
///
/// ```text
/// DataPatch/Intro/CharSel/CharGen/InGame/Credits --> Disc : any character error / server died
/// Disc --> Epilogue : OK button (0x10000418 msg 1)
/// Epilogue --> [*] : global message 1 ends the device loop
/// ```
///
/// plus the original flow's suppression: a second disconnect while
/// already on `0x10000002` is ignored.
#[test]
fn the_disconnect_edge_carries_its_message_and_ends_at_device_done() {
    let store = store();
    let mut shell = UiShell::new(&store, (800, 600))
        .unwrap_or_else(|e| panic!("the UI shell must come up over the retail dats: {e}"));
    let mut pump = dereth_ui::NullInputPump;
    let mut host = HostState {
        connected: true,
        patch_finished: true,
        ..HostState::default()
    };
    let mut f = run_until(&mut shell, &host, 0, mode::CHARACTER_MANAGEMENT, 600);

    // The server-death notice queues mode 0x10000002 with `ID_NetErr_ConnectionLost`.
    host.error = Some("ID_NetErr_ConnectionLost".into());
    shell.frame(at(f), &host, &mut pump);
    f += 1;
    assert_eq!(shell.flow.current_mode(), Some(mode::DISCONNECTED));

    // The error-text hand-off is "the only way a mode receives a parameter", and it reached the
    // screen before `Show(true)`.
    let screen = shell.flow.current().expect("a screen");
    let any = screen as &dyn std::any::Any;
    let d = any
        .downcast_ref::<dereth_ui_screens::screens::disconnected::DisconnectedScreen>()
        .expect("the disconnected screen");
    assert_eq!(d.error_text.as_deref(), Some("ID_NetErr_ConnectionLost"));

    // "…unless the current mode is already `0x10000002`, so a second disconnect while on the
    // disconnected screen is ignored."
    let switches = shell.stats.mode_switches;
    host.error = Some("ID_NetErr_ConnectionLost again".into());
    shell.frame(at(f), &host, &mut pump);
    f += 1;
    assert_eq!(
        shell.stats.mode_switches, switches,
        "a second disconnect must not re-enter"
    );

    // The OK button. Element-message broadcast is how a real click arrives; the message is the
    // same either way.
    let ok = shell
        .ui
        .get_element(dereth_ui_screens::screens::disconnected::OK_BUTTON)
        .expect("the OK button is in the shipped layout");
    shell
        .ui
        .broadcast_element_message(ok, dereth_ui::MessageId(1), 0, 0);
    shell.frame(at(f), &host, &mut pump);
    f += 1;
    assert_eq!(shell.flow.current_mode(), Some(mode::EPILOGUE));
    assert!(
        !shell.stats.device_done,
        "the epilogue has not been dismissed yet"
    );

    // The epilogue's global-message handler ends the device loop on message 1 (any key).
    shell
        .ui
        .broadcast_global(dereth_ui::msg::global::KEY_DOWN_UNCONSUMED, 0x1B);
    shell.frame(at(f), &host, &mut pump);
    assert!(
        shell.stats.device_done,
        "the epilogue must end the main loop"
    );

    assert_eq!(shell.stats.screen_create_failures, 0);
    assert_eq!(shell.stats.unregistered_mode_requests, 0);
}

/// Oracle: in the original switch behavior, an unregistered mode id does nothing and the queued
/// request **stays set**, so it is retried every frame forever. This build instead validates the
/// id when `UiShell::queue` receives it, increments the rejection counter, and returns before the
/// request reaches the flow queue.
///
/// The client's silent infinite retry is exactly the shape of failure this project has twice let
/// hide, so the shell counts it. This asserts on the counter.
#[test]
fn an_unregistered_mode_is_refused_and_counted_rather_than_retried_for_ever() {
    let store = store();
    let mut shell = UiShell::new(&store, (800, 600))
        .unwrap_or_else(|e| panic!("the UI shell must come up over the retail dats: {e}"));
    let mut pump = dereth_ui::NullInputPump;
    let host = HostState {
        connected: true,
        patch_finished: true,
        ..HostState::default()
    };
    shell.frame(at(0), &host, &mut pump);
    let before = shell.flow.current_mode();

    for m in dereth_ui_screens::UNREGISTERED_MODES {
        shell.queue(m);
    }
    shell.frame(at(1), &host, &mut pump);
    assert_eq!(shell.stats.unregistered_mode_requests, 3);
    assert_ne!(shell.flow.current_mode(), Some(UiMode(0x1000_0004)));
    assert!(
        shell.flow.current_mode().is_some(),
        "the flow still has a screen"
    );
    let _ = before;
}

/// A character set with one character, as `0xF658` leaves it.
fn one_character() -> dereth_ui::persist::CharacterSet {
    dereth_ui::persist::CharacterSet {
        set: vec![dereth_ui::persist::CharacterIdentity {
            id: dereth_primitives::ObjectId(0x5000_0001),
            name: "+Aldis".into(),
            seconds_grace_period: 0,
        }],
        num_allowed_characters: 11,
        account: "ac01".into(),
        ..dereth_ui::persist::CharacterSet::default()
    }
}

/// Behaviour: presentation.interface.a-switch-back-to-the-retail-interface-shows-the-screen-the-game-is-at
///
/// Oracle: the game state the other interface left. The retail interface is not framed while the
/// classic one is shown, so it comes back either never having been framed at all (the classic
/// interface was chosen before it started) or on the screen it last showed.
#[test]
fn a_retail_interface_shown_again_comes_up_on_the_screen_the_game_is_at() {
    let store = store();
    let mut pump = dereth_ui::NullInputPump;
    let in_world = HostState {
        connected: true,
        patch_finished: true,
        received_set: true,
        character_set: Some(one_character()),
        character_set_notices: 1,
        in_world: true,
        ..HostState::default()
    };

    // Never framed: the classic interface was shown from the start and the player went into the
    // world with it. The retail flow goes straight to the gameplay screen, not through the
    // data-patch screen, the intro and character selection.
    let mut shell = UiShell::new(&store, (800, 600))
        .unwrap_or_else(|e| panic!("the UI shell must come up over the retail dats: {e}"));
    shell.catch_up(&in_world);
    for f in 0..4 {
        shell.frame(at(f), &in_world, &mut pump);
    }
    assert_eq!(shell.flow.current_mode(), Some(mode::GAME_PLAY));
    assert_eq!(shell.transitions(), &[mode::GAME_PLAY], "nothing before it");

    // The control: framed with no catch-up, the same shell state restarts the opening run.
    let mut control = UiShell::new(&store, (800, 600))
        .unwrap_or_else(|e| panic!("the UI shell must come up over the retail dats: {e}"));
    for f in 0..4 {
        control.frame(at(f), &in_world, &mut pump);
    }
    assert_ne!(control.flow.current_mode(), Some(mode::GAME_PLAY));

    // Left in the world, shown again after a log-off and a new entry made with the classic
    // interface: the character list it missed does not send it back to character selection.
    let mut shell = UiShell::new(&store, (800, 600))
        .unwrap_or_else(|e| panic!("the UI shell must come up over the retail dats: {e}"));
    let at_select = HostState {
        in_world: false,
        ..in_world.clone()
    };
    let f = run_until(&mut shell, &at_select, 0, mode::CHARACTER_MANAGEMENT, 600);
    shell.frame(at(f), &in_world, &mut pump);
    assert_eq!(shell.flow.current_mode(), Some(mode::GAME_PLAY));
    let again = HostState {
        character_set_notices: 2,
        ..in_world.clone()
    };
    shell.catch_up(&again);
    for g in 1..4 {
        shell.frame(at(f + g), &again, &mut pump);
    }
    assert_eq!(
        shell.flow.current_mode(),
        Some(mode::GAME_PLAY),
        "still in the world"
    );

    // At character selection in the other interface: the character screen, without the intro.
    let mut shell = UiShell::new(&store, (800, 600))
        .unwrap_or_else(|e| panic!("the UI shell must come up over the retail dats: {e}"));
    shell.catch_up(&at_select);
    for f in 0..4 {
        shell.frame(at(f), &at_select, &mut pump);
    }
    assert_eq!(shell.transitions(), &[mode::CHARACTER_MANAGEMENT]);
    assert_eq!(shell.stats.screen_create_failures, 0);
}

// ---------------------------------------------------------------------------------------------
// 2. A UI element draws over the world
// ---------------------------------------------------------------------------------------------

/// One rendered frame plus what the UI pass said it did: (width, height, BGRA, blit list,
/// texture counters).
type Capture = (
    u32,
    u32,
    Vec<u8>,
    Vec<dereth_ui::UiDrawCmd>,
    dereth_client::ui_draw::UiTextureStats,
);

/// Oracle: the frame itself. Two runs of the *same* scene from the *same* dats, one with `--ui` and
/// one without, compared pixel by pixel: the difference is the UI and nothing else, because the
/// headless capture is byte-identical across runs (proved separately by the headless capture
/// test).
///
/// This is a content assertion, not a null draw: it requires that the changed pixels lie **inside**
/// the rectangles the UI said it would blit and that nothing outside them moved. A UI pass that
/// drew nothing, drew everywhere, or drew in the wrong place all fail it.
#[test]
fn a_ui_element_rasterises_over_the_world_and_only_where_it_said_it_would() {
    // Missing dats fail here rather than return early: a green line inside a red run would read
    // as "that one is fine".
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this test's oracle and there are none at {} -- \
         set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    const FRAMES: u32 = 8;

    let scene = |ui: bool| -> Option<Capture> {
        let cfg = Config {
            ui,
            character: false,
            ..base_config()
        };
        let mut app = headless(cfg);
        let s = dereth_client::world::SceneConfig {
            landblock: app.config().landblock,
            land_radius: app.config().land_radius,
            scenery_radius: app.config().scenery_radius,
            character: false,
            ..dereth_client::world::SceneConfig::default()
        };
        if let Err(e) = app.load_static_scene(s) {
            panic!("the static scene must load: {e}");
        }
        for _ in 0..FRAMES {
            app.frame();
        }
        let list = app.ui_draw_list().to_vec();
        let stats = app.renderer_mut().ui_stats;
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((w, h, bgra, list, stats))
    };

    let (w, h, without, _, _) =
        scene(false).expect("a rendered frame: retail dats and a WARP device");
    let (w2, h2, with, list, stats) =
        scene(true).expect("a rendered frame: retail dats and a WARP device");
    assert_eq!((w, h), (w2, h2));

    // The pass actually ran: quads submitted, images uploaded, and nothing silently skipped.
    assert!(stats.quads_drawn > 0, "the UI pass submitted no geometry");
    assert!(stats.uploaded > 0, "no UI image was uploaded");
    assert_eq!(stats.decode_failures, 0, "a UI image would not decode");
    assert_eq!(
        stats.skipped_draws, 0,
        "a draw was skipped for want of a texture"
    );

    // The world was drawn: without the UI the frame is not black.
    let lit = without
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0] | p[1] | p[2] != 0)
        .count();
    assert!(
        lit > (w * h / 4) as usize,
        "the world drew only {lit} lit pixels"
    );

    // Where the UI said it would blit, on the visible part of the back buffer. A command claims
    // its rectangle when it has an image **or** glyphs: a text element with a
    // transparent background emits a command with no image at all and only a glyph run, and its
    // pixels are still inside the same visible rectangle, because the client composes text into
    // the element's own surface and a surface cannot be escaped.
    let mut covered = vec![false; (w * h) as usize];
    for cmd in &list {
        if cmd.image.is_none() && cmd.glyphs.is_empty() {
            continue;
        }
        let x0 = cmd.screen.x0.max(cmd.clip.x0).max(0);
        let y0 = cmd.screen.y0.max(cmd.clip.y0).max(0);
        let x1 = cmd.screen.x1.min(cmd.clip.x1).min(w as i32 - 1);
        let y1 = cmd.screen.y1.min(cmd.clip.y1).min(h as i32 - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                covered[(y as u32 * w + x as u32) as usize] = true;
            }
        }
    }
    let claimed = covered.iter().filter(|c| **c).count();
    assert!(claimed > 0, "the UI claimed no pixels");

    let mut changed_inside = 0usize;
    let mut changed_outside = 0usize;
    for (i, c) in covered.iter().enumerate() {
        let a = &without[i * 4..i * 4 + 4];
        let b = &with[i * 4..i * 4 + 4];
        if a != b {
            if *c {
                changed_inside += 1;
            } else {
                changed_outside += 1;
            }
        }
    }
    assert_eq!(
        changed_outside, 0,
        "{changed_outside} pixels changed outside every UI rectangle"
    );
    assert!(
        changed_inside > 1_000,
        "only {changed_inside} of {claimed} claimed pixels changed; the UI drew nothing visible"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. A key reaches dereth-input, through the application's own update path
// ---------------------------------------------------------------------------------------------

/// Oracle: the retail default key map (`0x14000000`, KEYMAP group 10 entry `0x10000001`). Input map
/// 5 — the camera's, registered for the whole run, so on the pre-game screen the client starts on
/// too — binds `DIK_NUMPAD4` (0x4B) with meta-mode 0 to action **0x35**, which
/// `dereth_client_runtime::actions::camera::action` names `ROTATE_LEFT`. (The character session's
/// maps, movement among them, are not registered until the player is in the world, so a movement
/// key here would reach no map at all.)
///
/// The `MSG` is built by `crate::pump`, which is the same code the real pump runs, and it is fed
/// through `App::frame` so the whole per-frame path — window-message forwarding, input-manager
/// message handling, then its time update inside UI time — is what produces
/// the action. Nothing is injected into the desktop.
#[test]
fn a_window_message_reaches_the_input_manager_through_the_frame_and_fires_its_action() {
    let mut app = headless(base_config());
    assert!(
        app.input_manager_mut().is_some(),
        "the input tables must load from the retail dats"
    );
    app.load_first_pixel_scene()
        .expect("the first-pixel surface decodes");

    // The `MSG` a `winit` `Numpad4` press produces, with the `GetMessageTime()` the pump carries.
    let mut pump = dereth_client::pump::Pump::new();
    let down = pump
        .key_message_for(winit::keyboard::KeyCode::Numpad4, true, 12_345)
        .expect("winit maps Numpad4 to a virtual key and a scan code");
    assert_eq!(down.wparam, 0x64, "VK_NUMPAD4");
    assert_eq!(
        dereth_input::win32::keyboard_offset(down.lparam),
        Some(0x4B),
        "DIK_NUMPAD4 must be in the lParam; the input pipeline never reads the wParam"
    );

    let input = app.input_manager_mut().expect("present");
    assert!(
        input.on_message(down),
        "input message handling consumes the synthesized key message"
    );

    // Now run the application's own frame. The input manager's time update is step 6 of UI time,
    // and it is what drains the pipeline.
    assert!(app.frame());

    // The assertion is made **where the action went**: `App::apply_input_actions`, the
    // command-handler leg of the listener dispatch, consumes the queued camera action, so the
    // queue is empty after a frame and the camera's turn-left input is held. A still-queued action
    // would mean nothing anywhere was listening, and it would be re-offered to the UI on every
    // frame.
    assert!(
        app.camera_input().look_left,
        "the camera must be turning left while the key is down"
    );
    let input = app.input_manager_mut().expect("present");
    assert!(
        input.take_events().is_empty(),
        "and it was consumed rather than left in the queue to be re-offered on every frame"
    );
    assert!(
        input.is_action_in_progress(dereth_client_runtime::actions::camera::action::ROTATE_LEFT)
    );
    assert_eq!(input.stats.messages_offered, 1);
    assert_eq!(input.stats.messages_handled, 1);
    // A keyboard message with a zero scan code (a zero `lParam`) is counted here.
    assert_eq!(input.stats.keyboard_without_scan_code, 0);
    assert!(input.stats.actions_fired >= 1);

    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 4. A sound plays
// ---------------------------------------------------------------------------------------------

/// Oracle: the retail `client_portal.dat`. The UI sound table is whatever the original
/// enum-keyed lookup for `(0x10000003, 7, 0x22)` resolves
/// to — nothing is hard-coded — and the wave is whatever that table's button-press row
/// names. The assertion is on the **samples**: a path that resolved, decoded and started a voice
/// and then mixed silence would pass every count and still be inaudible.
#[test]
fn the_ui_sound_the_shell_starts_produces_samples_from_the_mixer() {
    let mut app = headless(base_config());
    {
        let audio = app
            .audio_mut()
            .expect("the shell builds its sound subsystem even with no audio device");
        // `--no-sound` in `base_config`, so there is no device and the original mixer's CRT seed
        // stays 1 — which is the client's own behaviour on a machine with no sound card.
        assert!(!audio.has_device());
        assert!(
            audio.stats.waves_created > 0,
            "no wave was created from the UI sound table"
        );
        assert_eq!(
            audio.stats.wave_decode_failures, 0,
            "a wave would not decode"
        );
        assert!(
            audio.stats.tables_loaded > 0,
            "the UI sound table did not load"
        );
        assert_eq!(
            audio.active_voices(),
            0,
            "nothing is playing before the sound is started"
        );
    }

    app.play_startup_sound();

    let audio = app.audio_mut().expect("the audio engine is up");
    assert_eq!(audio.stats.sounds_started, 1);
    assert_eq!(
        audio.active_voices(),
        1,
        "the startup sound leaves one active voice"
    );

    // One block at the client's own mix rate: 11 025 Hz, interleaved stereo,
    // matching original sound initialization.
    let mut block = vec![0.0f32; 2 * 2048];
    audio.mix(&mut block);
    let peak = block.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    let nonzero = block.iter().filter(|s| **s != 0.0).count();
    assert!(peak > 0.0, "the mixer produced 2048 frames of silence");
    // The button click is a short wave, so the assertion is on the *shape*: a run of non-zero
    // samples at the head of the block and silence after it. Nothing loops, so a
    // second block finds the voice finished.
    assert!(
        nonzero > 100,
        "only {nonzero} of {} samples are non-zero",
        block.len()
    );
    let last_nonzero = block.iter().rposition(|s| *s != 0.0).expect("some signal");
    assert!(
        block[..=last_nonzero].iter().filter(|s| **s != 0.0).count() * 2 >= last_nonzero,
        "the signal is sparse; that is noise, not a decoded wave"
    );
    // Stereo, and the original startup sound pans to 0, so both channels carry the same signal.
    let left: Vec<f32> = block.as_chunks::<2>().0.iter().map(|f| f[0]).collect();
    let right: Vec<f32> = block.as_chunks::<2>().0.iter().map(|f| f[1]).collect();
    assert_eq!(
        left, right,
        "a centre-panned sound must be identical in both channels"
    );

    // "Nothing loops; continuous ambience is a re-trigger on a timer". The voice
    // ends when the sample does, and a rebuild that looped it would keep this at 1 for ever.
    for _ in 0..64 {
        audio.mix(&mut block);
    }
    assert_eq!(
        audio.active_voices(),
        0,
        "the voice must end with the sample; nothing loops"
    );

    app.shutdown();
}
