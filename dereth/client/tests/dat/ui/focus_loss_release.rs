//! Losing window focus releases every held control: with Jump held the charge is released and
//! lateral movement still works, and a held mouse-look button is released so the camera stops
//! following the cursor. Fixture: a headless `App` on the default landblock with an animated body,
//! the player created and unhidden from the early-inventory-and-casting recording, and window
//! events fed through `Pump` to the input manager as `App::run_event_loop` does.
//!
//! # What focus loss does
//!
//! The input manager takes one `timeGetTime()` stamp and then does **three** things, in order:
//!
//! 1. **A synthetic zero-extent release per held control.** While the active-control map is not
//!    empty it unlinks one entry, clears that key's meta-mode bit, finds the `ActionState` for the
//!    recorded action, builds an `InputEvent` with extent 0.0 at the one stamp, and fires it down
//!    the ordinary action-event path. It synthesises an event; it does not reach into the
//!    consumers' state.
//! 2. **Keyboard-focus cleanup**, once the active-control hash is empty. The command interpreter
//!    clears keyboard commands from all three command lists and invokes its two further cleanup
//!    hooks.
//! 3. **The mouse-look release.** When mouse look is active and a device exists, it fires a button
//!    release for control 1, subcontrol 0, on the virtual device with activation `0x80` (`ANALOG`)
//!    and the same timestamp. The hold that put the client into mouse look therefore cannot survive
//!    the focus loss.
//!
//! # Where the client does it
//!
//! `Pump::map_window_event` turns `Focused(false)` into `WM_ACTIVATEAPP`/`WM_ACTIVATE`/
//! `WM_KILLFOCUS`, and `dereth_input`'s `WM_KILLFOCUS` arm calls
//! `InputShell::release_pressed_keys`, which walks its `active_controls` and calls
//! `deactivate_action_key`: a real `extent: 0.0, start: false` event on the ordinary queue.
//! `App::apply_input_actions` routes it to the movement command handler's jump release arm. The
//! same arm clears the input manager's `in_mouse_look`; `App::mouse_look` is a second, app-level mouse-look latch, and focus loss releases it too.
//!
//! `winit::event::WindowEvent::MouseInput` and `CursorMoved` both carry a `DeviceId` that cannot
//! be constructed without `unsafe`, which this crate forbids, so the mouse half of
//! `note_flycam_input` is reached through `App::mouse_look_button` and `App::cursor_moved`, the
//! same shape `App::flycam_key` has for the keyboard half.

use crate::common::sim_app::{body, frames, key, movement_key, unhide_player as unhide_the_player};

use dereth_client::{
    app::App,
    config::Config,
    pump::Pump,
    world::{SceneConfig, DEFAULT_LANDBLOCK},
};
use dereth_client_net::client_session::{testing::Corpus, SessionEvent};
use dereth_primitives::num::math;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::{Message, Opcode};

fn setup() -> App {
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 640,
        height: 480,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("required DATs and simulated world");
    app.start_shell().unwrap();
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    frames(&mut app, 4);
    app.load_static_scene(SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("real terrain and animated body");
    frames(&mut app, 60);
    let corpus = Corpus::shared("early-inventory-and-casting");
    let player = corpus.blobs.iter().find(|b| b.opcode == 0xf746).unwrap();
    let id = ObjectId(u32::from_le_bytes(player.payload[4..8].try_into().unwrap()));
    let row = corpus
        .blobs
        .iter()
        .find(|b| b.opcode == 0xf745 && b.payload[4..8] == id.0.to_le_bytes())
        .unwrap();
    let mut create = dereth_protocol::objects::ItemCreateObject::read(
        &mut dereth_protocol::Reader::new(&row.payload[4..]),
    )
    .unwrap();
    let p = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position();
    create.0.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
        objcell_id: p.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: p.frame.origin.x,
                y: p.frame.origin.y,
                z: p.frame.origin.z,
            },
            orientation: dereth_protocol::types::Quat {
                w: p.frame.rotation.w,
                x: p.frame.rotation.x,
                y: p.frame.rotation.y,
                z: p.frame.rotation.z,
            },
        },
    });
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).unwrap(),
        },
        LocalTime(1.0),
    );
    unhide_the_player(&mut app, &corpus, id);
    frames(&mut app, 60);
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(c.on_ground());
    assert!(
        !c.world
            .get(c.handle)
            .expect("the local collision body")
            .state()
            .is_hidden(),
        "premise: the login create arrives with HIDDEN_PS and a hidden body never \
         advances its animation offset, so the recorded unhide has to have reached it"
    );
    assert!(!app.objects().world.combat.jump_pending);
    app
}

fn space(app: &mut App, down: bool, time: u32) {
    key(app, winit::keyboard::KeyCode::Space, down, time);
}

fn player_description(app: &mut App, name: &str) {
    let corpus = Corpus::shared(name);
    let row = corpus
        .blobs
        .iter()
        .find(|b| b.opcode == 0xf7b0 && b.payload[12..16] == 0x13_u32.to_le_bytes())
        .expect("0013 player description");
    let desc = dereth_protocol::read_body::<dereth_protocol::login::LoginPlayerDescription>(
        &row.payload[16..],
    )
    .unwrap();
    let event = SessionEvent::PlayerDescription(Box::new(desc));
    app.probe_mut()
        .objects_mut()
        .apply_event(&event, LocalTime(1.0));
    app.apply_hud_events(std::slice::from_ref(&event));
    app.probe_mut()
        .apply_interaction_events(std::slice::from_ref(&event));
}

fn viewer_offset(app: &App) -> dereth_primitives::Vec3 {
    body(app).camera.manager.viewer_offset
}

/// Exactly the pair `App::run_event_loop` performs for one `WindowEvent`: the direct arm first,
/// then every `Win32Message` the pump maps it to, fed to the input manager.
fn window_event(app: &mut App, event: &dereth_client::platform::window::HostEvent, time: u32) {
    app.note_flycam_input(event);
    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    for m in pump.map_window_event(event, time) {
        app.input_manager_mut().unwrap().on_message(m);
    }
}

fn lose_focus(app: &mut App, time: u32) {
    window_event(
        app,
        &dereth_client::platform::window::HostEvent::Focused(false),
        time,
    );
}

// -------------------------------------------------------------------------------------------
// 1. The per-control releases, and the movement state they must leave behind.
// -------------------------------------------------------------------------------------------

/// **A focus loss with Jump held must release the jump, not strand the charge.**
///
/// `standing_longjump` gates the three lateral movement commands and no turn, so a client that
/// kept it across an alt-tab would be able to turn and unable to translate, with no way out.
#[test]
fn focus_loss_with_jump_held_releases_the_charge_and_leaves_lateral_movement_working() {
    let mut app = setup();
    player_description(&mut app, "early-inventory-and-casting");

    space(&mut app, true, 700_000);
    frames(&mut app, 4);
    // The setup was alive enough to fail: the charge really is up and really is a standing one.
    assert!(
        app.objects().world.combat.jump_pending,
        "the charge started"
    );
    assert!(
        body(&app).driver().movement.interp.standing_longjump,
        "and it is the standing charge that gates lateral movement"
    );
    let before = app.probe().jump_counts();

    lose_focus(&mut app, 700_100);
    frames(&mut app, 4);

    assert!(
        !app.objects().world.combat.jump_pending,
        "the synthesized zero-extent release must reach the combat controller's jump action"
    );
    assert!(
        !body(&app).driver().movement.interp.standing_longjump,
        "the charge is gone, so the lateral gate must be down"
    );
    assert_eq!(
        app.probe().jump_counts().0,
        before.0 + 1,
        "the release increments the jump counter"
    );

    // And the gate really is down: the body translates again, through the real command owner.
    frames(&mut app, 70);
    assert!(body(&app).on_ground(), "the jump finished before this step");
    let start = body(&app).position().frame.origin;
    movement_key(
        &mut app,
        dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
        true,
        700_400,
    );
    frames(&mut app, 40);
    let moved = body(&app).position().frame.origin;
    assert!(
        math::hypotf(moved.x - start.x, moved.y - start.y) > 0.25,
        "lateral movement is still locked after the focus loss: {start:?} -> {moved:?}, \
         longjump={} pending={}",
        body(&app).driver().movement.interp.standing_longjump,
        app.objects().world.combat.jump_pending,
    );
    movement_key(
        &mut app,
        dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
        false,
        700_600,
    );
    frames(&mut app, 10);
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 2. The mouse-look release.
// -------------------------------------------------------------------------------------------

/// Behaviour: ui.focus.losing-focus-releases-every-held-control
///
/// **A focus loss while the mouse-look button is held must release it.**
///
/// The oracle for "mouse-look is live" is the camera's `viewer_offset`. Mouse look has a
/// five-sample per-axis warm-up (`dereth_client_runtime::actions::camera::MOUSE_LOOK_WARMUP`), so
/// each look feeds twelve samples (the first only seeds `last_cursor`, and eleven deltas clear the
/// warm-up with room to spare) with one frame between them so the camera's own tick advances.
///
/// The idle control is asserted first: with nothing held, `viewer_offset` is bit-stable across
/// frames, so "unchanged" below is a real observation and not a camera that never moves.
#[test]
fn focus_loss_releases_the_held_mouse_look_button() {
    let mut app = setup();
    frames(&mut app, 30);

    // Control: the camera is stationary when nothing is driving it.
    let idle = viewer_offset(&app);
    frames(&mut app, 20);
    assert_eq!(
        viewer_offset(&app),
        idle,
        "the camera drifts on its own; the oracle is unusable"
    );

    // Baseline: the button held and the cursor moving turns the camera. Without this the
    // assertion below would pass on a build where mouse-look never worked at all.
    app.mouse_look_button(true);
    look(&mut app, 100.0);
    let held = viewer_offset(&app);
    assert_ne!(
        held, idle,
        "the setup is dead: a held mouse-look button did not move the camera"
    );

    // The button is still physically down when the window loses focus.
    lose_focus(&mut app, 800_000);
    frames(&mut app, 2);

    let after_focus_loss = viewer_offset(&app);
    look(&mut app, 900.0);
    assert_eq!(
        viewer_offset(&app),
        after_focus_loss,
        "the synthesized mouse-look release did not run: the camera still follows the cursor \
         with no button held"
    );

    // A fresh press still works, so the release cleared a latch rather than breaking one.
    app.mouse_look_button(true);
    let regained = viewer_offset(&app);
    look(&mut app, 1700.0);
    assert_ne!(
        viewer_offset(&app),
        regained,
        "a fresh hold after the focus loss must still look"
    );
    app.shutdown();
}

/// Twelve cursor samples marching along x from `from`, one frame apart.
fn look(app: &mut App, from: f64) {
    for i in 0..12 {
        app.cursor_moved(from + f64::from(i) * 60.0, 100.0);
        frames(app, 1);
    }
}
