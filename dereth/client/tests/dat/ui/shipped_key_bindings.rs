//! Shipped default key bindings reach their actions: `E` with nothing selected arms the identify
//! cursor and with a selection appraises, Escape puts the cursor away, the five `Emotes` keys and
//! the stance key `B` reach their motions, waving does not stop a run, `V` is not an emote key,
//! and the early recordings carry no emote. Fixture: a headless `App` on the gameplay screen with
//! the retail dats and no terrain or body (one test loads the default landblock with a body); keys
//! are resolved from the shipped keymap and fed as window messages through `Pump`, and the early
//! recordings are read through the shared corpus. Nothing opens a socket: assertions read the
//! client's own state or the request that *would* be sent.
//!
//! # `E` and the identify pick
//!
//! `DIK_E -> SelectionExamine 0x1000002B` reaches `Interaction::on_actions`. The UI examine
//! operation loads the selected id unconditionally and examines that value: with a non-zero id it
//! sends and returns; with zero it examines target mode, keeps that mode active, registers input
//! map `0x1000000B`, and updates the cursor. The zero case must reach the operation, so pressing
//! `E` with nothing selected arms the magnifying glass.
//!
//! # The emote and stance keys
//!
//! `B` is `Sleeping 0x10000097` in input map `4`, one of the four stance actions, and reaches the
//! body through the emote-command lookup `App::apply_input_actions` passes. `V` is not an emote key
//! in the shipped keymaps: its only input-map binding is `Ctrl+V -> PasteText 0x24`. The emote
//! keys the shipped keymap binds are `O`, `U`, `I`, `K` and `J` (Cheer, Cry, Laugh, PointState and
//! Wave), which reach dispatch because the client registers input map `0x10000006 Emotes`.
//!
//! Of the 160 shipped default bindings, 17 reach nothing; six of those are dead on purpose: the
//! two analog mouse axes have no producer at all, and the four `SystemKeys` rows are what input
//! map `0x10` exists to swallow.

use dereth_input::{ActionId, InputMapId};
use dereth_primitives::ObjectId;
use winit::keyboard::KeyCode;
use {
    dereth_client::app::App, dereth_client_runtime::config::Config,
    dereth_client_runtime::interaction::TargetMode, dereth_desktop::pump::Pump,
};

/// `UICommands`, where the shipped `ActionMap` declares `SelectionExamine` and `EscapeKey`.
const UI_COMMANDS: u32 = 0x1000_0009;
/// `MovementCommands`, where the four stance actions live.
const MOVEMENT_COMMANDS: u32 = 4;
/// `Emotes`: the map player-input setup registers last.
const EMOTES: u32 = 0x1000_0006;
/// `Wave`, the shipped `DIK_J` binding.
const WAVE: u32 = 0x1000_00E5;
/// The wave motion command value.
const MOTION_WAVE: u32 = 0x1300_0087;
/// The sleeping motion command value.
const MOTION_SLEEPING: u32 = 0x4100_0014;

// ---------------------------------------------------------------------------------------------
// Bring-up
// ---------------------------------------------------------------------------------------------

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        assert!(app.frame());
    }
}

/// The gameplay screen on the real dats. No terrain and no body: the cursor is chosen every frame
/// whether or not there is a world to point at, and the emote half below measures the command the
/// interpreter hands out rather than the animation it becomes.
fn screen_only() -> App {
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Config::default()
    })
    .expect("required retail DATs and a simulated presentation");
    app.start_shell().expect("the UI shell");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    frames(&mut app, 4);
    app
}

/// **The physical key is resolved from the shipped keymap, never written down.**
///
/// Ask the production `InputManager` which control the shipped master input map binds to this
/// action in this map, take the unmodified one, and find the `winit` key whose scan code is that
/// control's. A test that hard-codes `DIK_J = 0x24` measures its own table; this one fails loudly
/// if the dats ever say something else.
fn key_for(app: &mut App, action: u32, map: u32) -> KeyCode {
    const CANDIDATES: [KeyCode; 16] = [
        KeyCode::KeyE,
        KeyCode::KeyJ,
        KeyCode::KeyB,
        KeyCode::KeyO,
        KeyCode::KeyU,
        KeyCode::KeyI,
        KeyCode::KeyK,
        KeyCode::KeyY,
        KeyCode::KeyH,
        KeyCode::KeyG,
        KeyCode::KeyW,
        KeyCode::KeyS,
        KeyCode::KeyV,
        KeyCode::Escape,
        KeyCode::KeyR,
        KeyCode::KeyF,
    ];
    let binding = app
        .input_manager_mut()
        .expect("the production input manager")
        .keys_for_action(ActionId(action), InputMapId(map))
        .into_iter()
        .find(|b| b.meta_mode == 0)
        .unwrap_or_else(|| {
            panic!(
                "the shipped keymap binds no unmodified control to {action:#010X} in {map:#010X}"
            )
        });
    CANDIDATES
        .into_iter()
        .find(|c| {
            dereth_desktop::pump::scan_code_from_key_code(*c)
                .is_some_and(|s| u32::from(s & 0x7F) == u32::from(binding.control.offset() & 0x7F))
        })
        .unwrap_or_else(|| {
            panic!(
                "no candidate key carries scan {:#06X}",
                binding.control.offset()
            )
        })
}

/// Construct a `WM_KEYDOWN`/`WM_KEYUP` pair through `Pump`'s production key-message path and feed
/// it to the production input manager, which is why this file can say *"a key press reaches it"*.
fn press(app: &mut App, code: KeyCode, time: u32) {
    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    for (down, t) in [(true, time), (false, time + 1)] {
        let m = pump.key_message_for(code, down, t).expect("a physical key");
        app.input_manager_mut()
            .expect("the production input manager")
            .on_message(m);
    }
}

/// The current cursor `DataId`, compared against a `UICURSOR` enum key resolved through the
/// shipped mapper rather than by a literal (`dereth_desktop::cursor`: *"never hard-code a cursor's
/// DataID"*).
fn cursor_did(key: u32) -> dereth_primitives::DataId {
    let store = dereth_dat::testing::open_store().expect("required retail DAT");
    dereth_client_runtime::assets::enum_did(
        &store,
        dereth_client_shell::cursor::UICURSOR_GROUP,
        key,
    )
    .expect("the shipped UICURSOR mapper")
}

// ---------------------------------------------------------------------------------------------
// 1. `E`: the identify pick
// ---------------------------------------------------------------------------------------------

/// Nothing selected, press the shipped `E`, and ask what the pointer is.
///
/// The observable is the **cursor**, not the action firing: `Interaction::on_actions` consumes
/// `SelectionExamine` either way. What changes is target mode, which the cursor-state update's
/// fourth arm turns into `Examine` / `Examine_OverObject` (enum `10 + hovering`): the magnifying
/// glass.
#[test]
fn e_with_nothing_selected_arms_the_identify_cursor() {
    let mut app = screen_only();
    let e = key_for(
        &mut app,
        dereth_client_runtime::interaction::action::SELECTION_EXAMINE,
        UI_COMMANDS,
    );
    assert_eq!(
        e,
        KeyCode::KeyE,
        "the shipped UICommands section binds SelectionExamine to E"
    );

    app.probe_mut().objects_mut().world.set_selected_object(
        None,
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    frames(&mut app, 1);
    assert_eq!(
        app.objects().world.selected,
        None,
        "the state under test is an empty selection"
    );
    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::None,
        "and no mode is armed yet"
    );
    let before = app.current_cursor_did();
    assert_eq!(
        before,
        Some(cursor_did(
            dereth_client_shell::cursor::cursor_enum::DEFAULT
        )),
        "Default"
    );

    let armed = app.interaction().stats.target_modes_armed;
    press(&mut app, e, 400_000);
    frames(&mut app, 2);

    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::Examine,
        "the zero-id examine arm sets target mode to examine"
    );
    assert_eq!(
        app.interaction().stats.target_modes_armed,
        armed + 1,
        "and it is the arming statement that ran, not a field that happened to be set"
    );
    assert_eq!(
        app.current_cursor_did(),
        Some(cursor_did(
            dereth_client_shell::cursor::cursor_enum::EXAMINE
        )),
        "the examine target-mode cursor arm is enum 10 + hovering -- the magnifying glass"
    );
    assert_ne!(app.current_cursor_did(), before, "the pointer changed");
    assert_eq!(app.cursor_stats().failures, 0, "and the image resolved");
}

/// **The other half of the same `if`, as a negative control.** With something selected, retail
/// *sends* and arms nothing, so a client that armed the cursor unconditionally would pass the
/// test above and break appraisal.
#[test]
fn e_with_a_selection_appraises_and_arms_nothing() {
    let mut app = screen_only();
    let e = key_for(
        &mut app,
        dereth_client_runtime::interaction::action::SELECTION_EXAMINE,
        UI_COMMANDS,
    );
    let target = ObjectId(0x8000_09A4);
    app.probe_mut().objects_mut().world.set_selected_object(
        Some(target),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    frames(&mut app, 1);

    let armed = app.interaction().stats.target_modes_armed;
    press(&mut app, e, 400_000);
    for _ in 0..2 {
        assert!(app.frame());
    }

    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::None,
        "a nonzero selected id raises the examine notice and returns before the zero-id target-mode arm"
    );
    assert_eq!(
        app.interaction().stats.target_modes_armed,
        armed,
        "nothing was armed"
    );
    assert_eq!(
        app.objects().world.appraisal.examining,
        Some(target),
        "the non-zero arm examines the selected id and records what it asked about"
    );
    // No identify cursor: the pointer is the hourglass, because the examine now waits on the
    // shard's description, and nothing here answers it.
    assert_eq!(
        app.current_cursor_did(),
        Some(cursor_did(dereth_client_shell::cursor::cursor_enum::WAIT)),
        "and the pointer waits on the description rather than arming anything"
    );
}

/// **What clears it.** The UI action handler's `EscapeKey` arm sets target mode to none, and the
/// cursor goes back with it. A mode with no exit is a stuck pointer.
#[test]
fn escape_puts_the_identify_cursor_away() {
    let mut app = screen_only();
    let e = key_for(
        &mut app,
        dereth_client_runtime::interaction::action::SELECTION_EXAMINE,
        UI_COMMANDS,
    );
    let esc = key_for(
        &mut app,
        dereth_client_runtime::interaction::action::ESCAPE_KEY,
        UI_COMMANDS,
    );
    assert_eq!(esc, KeyCode::Escape);

    app.probe_mut().objects_mut().world.set_selected_object(
        None,
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    frames(&mut app, 1);
    press(&mut app, e, 400_000);
    frames(&mut app, 2);
    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::Examine,
        "armed, so there is something to clear"
    );

    press(&mut app, esc, 500_000);
    frames(&mut app, 2);
    assert_eq!(
        app.interaction().target_mode(),
        TargetMode::None,
        "clearing target mode"
    );
    assert_eq!(
        app.current_cursor_did(),
        Some(cursor_did(
            dereth_client_shell::cursor::cursor_enum::DEFAULT
        )),
        "and the no-target-mode cursor arm puts the default pointer back"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The emote and stance keys
// ---------------------------------------------------------------------------------------------

/// Behaviour: ui.bindings.the-shipped-emote-keys-reach-their-emote-actions
///
/// The `Emotes` map is registered, so its five shipped controls reach dispatch **as their own
/// actions**, in the map that declares them.
///
/// This is asserted on its own, because reaching dispatch (the `Emotes` map is registered) and
/// reaching the body (the emote-command lookup answers) have different causes and one end-to-end
/// assertion would not say which failed.
#[test]
fn the_five_shipped_emote_keys_reach_their_own_map() {
    let mut app = screen_only();
    // `Cheer O`, `Cry U`, `Laugh I`, `PointState K`, `Wave J` -- taken from the merged keymap
    // rather than listed here.
    let bound: Vec<u32> = app
        .input_manager_mut()
        .expect("the production input manager")
        .manager
        .keymap
        .section(InputMapId(EMOTES))
        .expect("the shipped Emotes keymap section")
        .bindings()
        .iter()
        .map(|(_, a)| a.0)
        .collect();
    assert_eq!(
        bound.len(),
        5,
        "the shipped keymap binds five Emotes controls"
    );
    for action in &bound {
        assert!(
            dereth_client_runtime::actions::emote::command_for_action(ActionId(*action)).is_some(),
            "{action:#010X} is in the shipped emote action-to-command table"
        );
    }

    let mut seen = Vec::new();
    let mut t = 600_000_u32;
    for action in &bound {
        let code = key_for(&mut app, *action, EMOTES);
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        let down = pump.key_message_for(code, true, t).expect("a physical key");
        let up = pump
            .key_message_for(code, false, t + 1)
            .expect("a physical key");
        let shell = app
            .input_manager_mut()
            .expect("the production input manager");
        shell.manager.take_events();
        shell.on_message(down);
        let hit = shell
            .manager
            .take_events()
            .iter()
            .find(|e| e.action.0 == *action)
            .map(|e| e.input_map.0);
        shell.on_message(up);
        shell.manager.take_events();
        seen.push((*action, hit));
        t += 10_000;
    }
    for (action, hit) in &seen {
        assert_eq!(
            *hit,
            Some(EMOTES),
            "{action:#010X} must reach dispatch in Emotes; an unregistered map answers None"
        );
    }
}

/// **`B`: `Sleeping 0x10000097` in input map `4`.**
///
/// The observable is the **motion command the body was asked for**, which is what the movement
/// sender packs into the `0xF61C Movement_MoveToState` it would send. The helper constructs the
/// key press through `Pump`'s production key-message path; it injects no action result and opens no
/// socket.
#[test]
fn b_asks_the_body_to_lie_down() {
    let mut app = screen_only();
    let b = key_for(
        &mut app,
        dereth_client_runtime::actions::movement::action::LAY_DOWN.0,
        MOVEMENT_COMMANDS,
    );
    assert_eq!(
        b,
        KeyCode::KeyB,
        "the shipped MovementCommands section binds Sleeping to B"
    );
    assert_eq!(
        dereth_client_runtime::actions::emote::command_for_action(
            dereth_client_runtime::actions::movement::action::LAY_DOWN
        ),
        Some(MOTION_SLEEPING),
        "the shipped emote action-to-command table pairs Sleeping with its motion command"
    );

    let before = app.probe().movement().transient_motions_issued;
    press(&mut app, b, 700_000);
    frames(&mut app, 2);
    assert_eq!(
        app.probe().movement().transient_motions_issued,
        before + 1,
        "the emote-command lookup answered, the action was consumed, and one motion was asked \
         for"
    );
    assert_eq!(
        app.probe().movement().last_transient_motion,
        Some((MOTION_SLEEPING, true)),
        "the emote arm of the action-command dispatcher sets the motion with a true start flag -- the start \
         flag is a literal, not the key edge, which is why one key press is one motion and not two"
    );
}

/// **`J`, Wave: the emote half, through the `Emotes` map.**
#[test]
fn j_asks_the_body_to_wave() {
    let mut app = screen_only();
    let j = key_for(&mut app, WAVE, EMOTES);
    assert_eq!(
        j,
        KeyCode::KeyJ,
        "the shipped Emotes section binds Wave to J"
    );
    assert_eq!(
        dereth_client_runtime::actions::emote::command_for_action(ActionId(WAVE)),
        Some(MOTION_WAVE),
        "Wave 0x100000E5 -> the wave motion 0x13000087"
    );

    let before = app.probe().movement().transient_motions_issued;
    press(&mut app, j, 700_000);
    frames(&mut app, 2);
    assert_eq!(
        app.probe().movement().transient_motions_issued,
        before + 1,
        "the Emotes map is registered, so the key reaches its action"
    );
    assert_eq!(
        app.probe().movement().last_transient_motion,
        Some((MOTION_WAVE, true)),
        "the command handed to the movement dispatcher is wave motion, not something that merely \
         counted"
    );
}

/// **The emote as the server would see it.** A real body on real terrain, the shipped `J`, and
/// then the queued action field the movement sender packs.
///
/// Motion handling passes the command through cancellation into the raw motion state,
/// whose action arm pushes an `ActionNode`; movement-state packing writes that queue into the
/// `0xF61C Movement_MoveToState` the position reporter would send. **Nothing is sent here**: the
/// assertion is against the state the packer reads, which is what "the request that would be sent"
/// means in a file that opens no socket.
///
/// `apply_motion` only runs when `do_interpreted_motion` returned 0, so this also proves the
/// shipped motion table has a sequence for the emote, an assertion the light tests above cannot
/// make, because they hold no body.
#[test]
fn the_wave_key_reaches_the_motion_state_the_client_would_report() {
    use dereth_animation::MotionCommand;
    use {
        dereth_client_runtime::landblock::DEFAULT_LANDBLOCK,
        dereth_client_runtime::scene::SceneConfig,
    };

    let mut app = screen_only();
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..SceneConfig::default()
    })
    .expect("real terrain and an animated body");
    frames(&mut app, 60);

    fn body(a: &App) -> std::cell::Ref<'_, dereth_animation::driver::MotionDriver> {
        a.world_state()
            .expect("a world")
            .character
            .as_ref()
            .expect("a body")
            .driver()
    }
    assert_eq!(
        body(&app).movement.interp.raw_state.actions.len(),
        0,
        "the action queue starts empty, so a hit below is this key's"
    );
    assert_eq!(
        body(&app).movement.interp.interpreted_state.current_style,
        MotionCommand::NON_COMBAT,
        "Motion dispatch refuses an emote outside NonCombat with 0x42, so the stance is the \
         precondition and it is asserted rather than assumed"
    );

    let j = key_for(&mut app, WAVE, EMOTES);
    press(&mut app, j, 900_000);
    assert!(app.frame());

    let waved = body(&app)
        .movement
        .interp
        .raw_state
        .actions
        .iter()
        .any(|n| n.action == MotionCommand(MOTION_WAVE));
    assert!(
        waved,
        "The raw motion state's action queue must hold the wave motion 0x13000087 -- this is the \
         field serialized into the 0xF61C; got {:?}",
        body(&app).movement.interp.raw_state.actions
    );
}

/// **Waving while running keeps the run.**
///
/// `move_player`'s `L::None` arm must not clear the six direction slots for *anything* on no
/// command list: that is right for `Ready` and catastrophic for an emote, since a player running
/// past somebody and pressing `J` to wave would stop dead.
/// `dereth_client_runtime::actions::movement::which_list` answers "no list" for all 91 hash
/// entries.
#[test]
fn waving_while_running_does_not_stop_the_player() {
    let mut app = screen_only();
    let w = key_for(
        &mut app,
        dereth_client_runtime::actions::movement::action::MOVE_FORWARD.0,
        MOVEMENT_COMMANDS,
    );
    let j = key_for(&mut app, WAVE, EMOTES);

    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let down = pump
        .key_message_for(w, true, 800_000)
        .expect("a physical key");
    app.input_manager_mut()
        .expect("the production input manager")
        .on_message(down);
    frames(&mut app, 2);
    assert!(
        app.probe().char_input().forward,
        "W is held, so the forward slot is set"
    );

    press(&mut app, j, 810_000);
    frames(&mut app, 2);
    assert!(
        app.probe().char_input().forward,
        "A 0x13…… emote touches no movement list and the no-list command handler declines it too \
         ((cmd & 0x40000000) is clear), so waving must not release the forward slot"
    );

    // ...and the instrument can see the slot go down, or the assertion above only means the field
    // is stuck true.
    let up = pump
        .key_message_for(w, false, 820_000)
        .expect("a physical key");
    app.input_manager_mut()
        .expect("the production input manager")
        .on_message(up);
    frames(&mut app, 2);
    assert!(!app.probe().char_input().forward, "releasing W clears it");
}

/// **`V` is not an emote key.** The only shipped input-map binding for `V` is
/// `Ctrl+V -> PasteText`, and the emote map binds no `V` at all.
#[test]
fn v_is_not_bound_to_any_emote_in_the_shipped_keymaps() {
    let mut app = screen_only();
    let scan = u32::from(
        dereth_desktop::pump::scan_code_from_key_code(KeyCode::KeyV).expect("V has a scan code")
            & 0x7F,
    );
    let shell = app
        .input_manager_mut()
        .expect("the production input manager");
    let in_emotes = shell
        .manager
        .keymap
        .section(InputMapId(EMOTES))
        .expect("the shipped Emotes keymap section")
        .bindings()
        .iter()
        .any(|(qc, _)| u32::from(qc.control.offset() & 0x7F) == scan);
    assert!(!in_emotes, "no Emotes binding uses V");
    // ...and it is bound to something, so this is a measurement rather than a keymap that failed
    // to load: `CopyAndPasteControls 8` carries `Ctrl+V -> PasteText 0x24`.
    let paste = shell.keys_for_action(ActionId(0x24), InputMapId(8));
    assert!(
        paste
            .iter()
            .any(|qc| u32::from(qc.control.offset() & 0x7F) == scan && qc.meta_mode != 0),
        "V is bound, with a modifier, to PasteText -- got {paste:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The recordings carry no emote
// ---------------------------------------------------------------------------------------------

/// **Whether the recordings carry any emote.**
///
/// They do not, and the answer decides what the corpus can and cannot be used for here. Across
/// the seven sessions named below, every `0xF61C Movement_MoveToState` body the client sends
/// carries no queued action at all: the five-bit action count in the raw motion state's flags
/// word is zero in every one. Nobody emoted while any of these sessions was being recorded. (The
/// later `requested-death-vitae-salvage` recording does queue one action, which is why the claim
/// is made over this fixed selection and not over every recording.)
///
/// So the corpus is a byte-level oracle for the *movement* half of `RawMotionState` and is
/// **silent** on the emote half. That is why
/// `the_wave_key_reaches_the_motion_state_the_client_would_report` asserts against the state the
/// packer reads rather than against a recorded body: there is no recorded body to assert against,
/// and a test that went looking for one would have found an empty set and had to guess whether
/// that meant "we do not send emotes" or "nobody waved". The non-empty columns below are what tell
/// those two apart.
///
/// The instrument is proved able to see a non-empty answer in the same selection: the bodies
/// carry forward commands and run hold keys, so a zero in the action column is the corpus
/// speaking and not the decoder failing.
#[test]
fn every_early_recorded_move_to_state_queues_no_action_so_the_wire_half_is_local() {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_protocol::actions::unpack_action;
    use dereth_protocol::movement::MovementMoveToState;
    use dereth_protocol::Message;

    const SCENARIOS: [&str; 7] = [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "login-account-booted",
        "ddd-interrogation-only",
        "long-solo-play",
        "short-play-with-training",
    ];
    const GAME_ACTION: u32 = 0xF7B1;
    const MOVE_TO_STATE: u32 = 0xF61C;

    let (mut bodies, mut with_actions, mut with_forward, mut with_hold_run) = (0, 0, 0, 0);
    let mut emotes: Vec<u32> = Vec::new();
    for name in SCENARIOS {
        let corpus = Corpus::shared(name);
        for b in corpus
            .blobs
            .iter()
            .filter(|b| b.dir == Direction::ClientToServer && b.opcode == GAME_ACTION)
        {
            let mut action = unpack_action(&b.payload).expect("a captured game action unpacks");
            if action.sub_type.0 != MOVE_TO_STATE {
                continue;
            }
            let m = MovementMoveToState::read(&mut action.body).expect("a captured body decodes");
            let s = &m.0.raw_motion_state;
            bodies += 1;
            if !s.actions.is_empty() {
                with_actions += 1;
            }
            if s.forward_command.is_some() {
                with_forward += 1;
            }
            if s.current_holdkey == Some(2) {
                with_hold_run += 1;
            }
            for a in &s.actions {
                if let Some(c) = dereth_animation::MotionCommand::from_index(a.command_index) {
                    if c.is_emote() {
                        emotes.push(c.0);
                    }
                }
            }
        }
    }

    // Calibration first: the decoder can see a non-empty answer over exactly this population.
    assert!(bodies > 0, "the seven sessions carry 0xF61C bodies");
    assert!(
        with_forward > 0,
        "some recorded 0xF61C carries a forward command"
    );
    assert!(
        with_hold_run > 0,
        "some recorded 0xF61C carries the run hold key"
    );

    // **The positive control for the column that reads zero, which is the only one that matters.**
    // The three calibrations above prove the decoder reads *some* fields; they do not prove it
    // reads the **action queue**, and a reader that ignored the five-bit count would answer zero
    // for a corpus full of emotes. So one is round-tripped through the same `read`: a state whose
    // flags word carries a count of 1 and nothing else, followed by the wave motion's wire index.
    {
        use dereth_protocol::movement::{MotionAction, RawMotionState};
        use dereth_protocol::{Reader, Writer};
        let wave = dereth_animation::MotionCommand(MOTION_WAVE)
            .to_index()
            .expect("Wave has a wire index");
        let built = RawMotionState {
            actions: vec![MotionAction {
                command_index: wave,
                stamp_and_autonomy: 0x8001,
                speed: 1.0,
            }],
            ..RawMotionState::default()
        };
        let mut w = Writer::new();
        built.write(&mut w).expect("one action encodes");
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        let back = RawMotionState::read(&mut r).expect("and decodes");
        assert_eq!(
            back.actions.len(),
            1,
            "the reader must be able to answer non-zero at all"
        );
        assert_eq!(back.actions[0].command_index, wave);
        assert!(
            dereth_animation::MotionCommand::from_index(back.actions[0].command_index)
                .is_some_and(|c| c.0 == MOTION_WAVE),
            "and the index resolves back to wave motion, which is how the loop above would have \
             recognised one"
        );
    }

    // ...and then the answer.
    assert_eq!(
        with_actions, 0,
        "no recorded 0xF61C queues an action of any kind"
    );
    assert!(
        emotes.is_empty(),
        "and therefore none of the 87 emotes: {emotes:08X?}"
    );
}
