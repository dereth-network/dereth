//! Losing window focus stops a walk the player drives with the keyboard but not a MoveTo the
//! server drives, and does not cancel the run lock; one conditional decides the first two.
//! Fixture: a headless `App` on the retail `DEFAULT_LANDBLOCK` with a body built from
//! `early-inventory-and-casting`'s recorded player, a constructed non-autonomous `0xF74C
//! Movement_SetObjectMovement` `MoveToPosition`, and focus loss fed through the pump as the host
//! window delivers it. The claims are positions, not flags.
//!
//! # The chain, from `WM_KILLFOCUS` to the movement state
//!
//! Window deactivation (`WM_ACTIVATEAPP`, `0x1C`, with `wParam == 0`) deactivates input,
//! unacquires devices and releases pressed keys. `WM_KILLFOCUS` (`8`) also releases pressed keys.
//! That release reaches the movement command interpreter's keyboard-focus loss, whose order and
//! gate are:
//!
//! ```text
//! clear keyboard commands in substate, turn, sidestep order; retain the mouse command
//! reset hold-run to false
//! reset hold-sidestep to false
//! finish the jump
//! if autonomy_level != 0 && controlled_by_server == 0:
//!     apply current movement
//!     send the movement event
//! ```
//!
//! Keyboard clearing differs from clearing all commands: it stops at the
//! mouse command and retains it, so mouse-driven movement can survive focus loss.
//!
//! Applying movement with three empty lists and no run lock issues `Ready` (`0x41000003`).
//! That reaches motion execution, which cancels a move-to when `params.bitfield & 0x8000` is
//! set. Applying current movement therefore really does end a MoveTo — which is why retail
//! refuses to run it while the **server** owns the body: player movement application accepts the
//! non-autonomous `0xF74C`, `App::frame` answers it with `lose_control_to_server`, and
//! `controlled_by_server` is then `true` for the whole approach.

use crate::common::sim_app::{app_with_recorded_body as setup, frames, position};

use dereth_client::app::App;

use dereth_client::pump::Pump;

use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, Position};
use dereth_protocol::movement::{
    MoveToArm, MovementBody, MovementBuffer, MovementParameters, MovementSetObjectMovement,
};

use dereth_protocol::Opcode;

fn moved(a: &Position, b: &Position) -> f32 {
    dereth_animation::motion::moveto::distance(a, b)
}

/// A **non**-autonomous `0xF74C Movement_SetObjectMovement` carrying `MoveToPosition`, addressed
/// to the player: the server taking the body over, which is what a MoveTo is.
fn server_move_to(app: &mut App, metres: f32) -> Position {
    let start = position(app);
    let mut goal = start;
    goal.frame.origin.x += metres;
    let player = app.objects().player().unwrap();
    let presence = app.objects().presence(player).unwrap();
    let params = dereth_animation::motion::MovementParameters::default();
    let arm = MoveToArm::MoveToPosition {
        origin: dereth_protocol::types::Origin {
            objcell_id: goal.cell.0,
            origin: dereth_protocol::types::Vec3 {
                x: goal.frame.origin.x,
                y: goal.frame.origin.y,
                z: goal.frame.origin.z,
            },
        },
        params: MovementParameters::MoveTo {
            bitfield: params.flags,
            distance_to_object: 0.6,
            min_distance: params.min_distance,
            fail_distance: params.fail_distance,
            speed: params.speed,
            walk_run_threshold: params.walk_run_threshold,
            desired_heading: params.desired_heading,
        },
        run_rate: 1.0,
    };
    let message = MovementSetObjectMovement {
        id: player,
        instance_sequence: presence.instance,
        movement: MovementSetObjectMovement::encode_movement(&MovementBuffer {
            movement_timestamp: presence.movement_ts.wrapping_add(1),
            server_control_timestamp: presence.server_control_ts.wrapping_add(1),
            autonomous: false,
            body: MovementBody {
                current_style: 61, // NonCombat in the retail command_ids table.
                movement_type: dereth_protocol::movement::movement_type::MOVE_TO_POSITION,
                unhandled: MovementBody::encode_move_to(&arm),
                ..Default::default()
            },
        })
        .expect("source-derived movement"),
    };
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
            body: dereth_protocol::write_body(&message).unwrap(),
        },
        LocalTime(3.0),
    );
    frames(app, 1);
    assert!(
        app.movement_commands().lists.controlled_by_server,
        "a non-autonomous 0xF74C for the player is a transfer of control to the server"
    );
    assert!(app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .is_moving_to());
    goal
}

/// The focus-input pair in `App::handle_window_event`: the direct arm first,
/// then every `Win32Message` the pump maps it to, fed to the input manager. `Focused(false)` maps
/// to `WM_ACTIVATEAPP(0)`, `WM_ACTIVATE(0)` and `WM_KILLFOCUS`, which is the retail triple.
fn lose_focus(app: &mut App, time: u32) {
    let event = dereth_client::platform::window::HostEvent::Focused(false);
    app.note_flycam_input(&event);
    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    for m in pump.map_window_event(&event, time) {
        app.input_manager_mut().unwrap().on_message(m);
    }
}

/// A real key-down/up through the pump, on whichever physical key the shipped map-4 binding for
/// `action` names. `inject_action` would not do: a player holds a key, and focus loss must walk
/// and release the actual pressed-key entries.
fn movement_key(app: &mut App, action: dereth_input::ActionId, down: bool, time: u32) {
    use winit::keyboard::KeyCode::*;
    let binding = app
        .input_manager_mut()
        .unwrap()
        .keys_for_action(action, dereth_input::InputMapId(4))
        .into_iter()
        .find(|b| b.meta_mode == 0)
        .expect("shipped unmodified movement key");
    let keycode = [
        KeyW, KeyE, ArrowUp, KeyA, KeyD, ArrowLeft, ArrowRight, KeyS, KeyQ, KeyZ, KeyX, KeyC,
    ]
    .into_iter()
    .find(|keycode| {
        dereth_client::pump::scan_code_from_key_code(*keycode)
            .is_some_and(|scan| (scan & 0xff) as u32 == (binding.control.offset() & 0x7f) as u32)
    })
    .expect("known physical movement key for this retail DAT");
    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let message = pump.key_message_for(keycode, down, time).unwrap();
    app.input_manager_mut().unwrap().on_message(message);
    frames(app, 1);
}

fn forward(app: &mut App, start: bool, time: u32) {
    movement_key(
        app,
        dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
        start,
        time,
    );
}

// -------------------------------------------------------------------------------------------
// Station 1 — the server-ownership gate.
// -------------------------------------------------------------------------------------------

/// Behaviour: movement.focus.losing-focus-leaves-a-server-approach-and-the-run-lock-running
/// **Losing keyboard focus must not stop movement the server owns.**
///
/// The assertion is the character's world position 150 frames after the alt-tab, because that
/// is what a player sees; a `controlled` or `is_moving_to` flag could stay set on a body that
/// had already stopped.
#[test]
fn unfocusing_the_client_during_a_server_move_to_leaves_the_approach_running() {
    let mut app = setup();
    let start = position(&app);
    // Thirty metres, not six: `MoveToPosition`'s `distance_to_object` is 0.6 m, and a six-metre
    // approach *arrives* inside 150 frames, which would read as a stop.
    let goal = server_move_to(&mut app, 30.0);

    frames(&mut app, 150);
    let before = position(&app);
    assert!(
        moved(&start, &before) > 0.3,
        "the approach must be under way before the window is taken away: {:?} -> {:?}",
        start.frame.origin,
        before.frame.origin
    );

    lose_focus(&mut app, 5_000);
    frames(&mut app, 150);
    let after = position(&app);
    let err = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .stats
        .last_move_to_error;

    assert!(
        moved(&before, &after) > 0.5,
        "the focus-loss handler gates movement application on controlled_by_server == 0, so a server MoveTo \
         survives the focus loss; the body moved {:.3} m in 150 frames after the alt-tab \
         ({:?} -> {:?}), goal {:?}, last_move_to_error {err:#04X}",
        moved(&before, &after),
        before.frame.origin,
        after.frame.origin,
        goal.frame.origin
    );
    assert_ne!(
        err, 0x36,
        "0x36 is an explicit command cancellation -- the alt-tab issued Ready"
    );
}

/// **The run lock survives the alt-tab too**: focus loss does not change auto-run. Movement
/// application's first branch issues `WalkForward` (`0x45000005`) if auto-run is set, so an
/// auto-running character that loses the window keeps running.
#[test]
fn unfocusing_the_client_does_not_cancel_the_run_lock() {
    let mut app = setup();
    app.input_manager_mut()
        .unwrap()
        .inject_action(dereth_input::InputEvent {
            action: dereth_client_runtime::actions::movement::action::AUTORUN,
            input_map: dereth_input::InputMapId(4),
            toggle: dereth_input::ToggleType::Hold,
            extent: 1.0,
            start: true,
            repeat_delta: 0,
            repeat_total: 0,
            from_key_down: true,
        });
    frames(&mut app, 20);
    assert!(app.movement_commands().lists.auto_run, "the run lock is on");
    let before = position(&app);

    lose_focus(&mut app, 5_000);
    frames(&mut app, 30);

    assert!(
        app.movement_commands().lists.auto_run,
        "focus loss does not change auto-run; the lock must survive"
    );
    assert!(
        moved(&before, &position(&app)) > 0.5,
        "an auto-running character keeps running through an alt-tab; it moved {:.3} m",
        moved(&before, &position(&app))
    );
}

// -------------------------------------------------------------------------------------------
// Station 2 — the other side of the same conditional.
// -------------------------------------------------------------------------------------------

/// Behaviour: movement.focus.losing-focus-stops-self-powered-walking
/// **A keyboard walk *does* stop, and that is retail.** `autonomy_level` is 2 and
/// `controlled_by_server` is 0 while the player drives himself, so both ownership tests fall
/// through. Keyboard clearing has emptied the three lists in this keyboard-only case, and
/// applying current movement issues `Ready`.
#[test]
fn unfocusing_the_client_while_walking_under_your_own_power_does_stop_you() {
    let mut app = setup();
    let start = position(&app);
    forward(&mut app, true, 1_000);
    frames(&mut app, 60);
    let before = position(&app);
    assert!(
        !app.movement_commands().lists.controlled_by_server,
        "the player owns his own body"
    );
    assert!(
        moved(&start, &before) > 0.5,
        "he is walking before the alt-tab"
    );

    lose_focus(&mut app, 5_000);
    frames(&mut app, 60);
    let a = position(&app);
    frames(&mut app, 60);
    let b = position(&app);

    // "Comes to rest", not "moved less than X": the stop animation carries the body a little way
    // and what matters is that it ends.
    assert!(
        moved(&a, &b) < 0.05,
        "movement application with Ready stops an autonomous walk; 60 frames \
         after the alt-tab it had moved {:.3} m and it moved a further {:.3} m in the 60 after \
         that",
        moved(&before, &a),
        moved(&a, &b)
    );
}
