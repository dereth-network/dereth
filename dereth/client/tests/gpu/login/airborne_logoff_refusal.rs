//! Exit to Character Selection in mid-air is refused out loud and cancelled, not deferred. Out of
//! contact the gameplay time step sends the literal `Cannot log off while in mid-air.` as notice type
//! `0x1A` (the overhead strip's type: no timestamp, no log file, not chat-log output), clears the
//! end-session request and the one-shot ending-session guard, and requests nothing; landing arms
//! nothing, and a second press on the ground runs the normal departure. Exit Game is tested before
//! the contact check, so it works in mid-air.
//! Fixture: a headless software device, retail terrain, the player's `0xF745` create, `0xF74B`
//! unhide and `0x0013` description from `early-inventory-and-casting`, a real space-bar jump; no
//! network: `Teleport::log_off_pending` stands for the `0xF653` request.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::pump::Pump;
use dereth_client::world::{SceneConfig, DEFAULT_LANDBLOCK};
use dereth_client_net::client_session::{
    testing::{Corpus, Direction},
    SessionEvent,
};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::objects::{physics_state, ItemSetState};
use dereth_protocol::{Message, Opcode};
use dereth_ui_screens::screens::gameplay::{logout, GamePlayScreen};

fn frames(app: &mut App, count: usize) {
    for _ in 0..count {
        assert!(app.frame());
    }
}

/// A physical key, as Windows delivers it.
fn key(app: &mut App, code: winit::keyboard::KeyCode, down: bool, time: u32) {
    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let message = pump.key_message_for(code, down, time).unwrap();
    app.input_manager_mut().unwrap().on_message(message);
}

fn space(app: &mut App, down: bool, time: u32) {
    key(app, winit::keyboard::KeyCode::Space, down, time);
}

fn body(app: &App) -> &dereth_client::character::Character {
    app.world_state().unwrap().character.as_ref().unwrap()
}

/// Injects global message 1 with the action at the input manager's own queue, so the test checks
/// what the gameplay time step does with the flags input handling produces, independently of the
/// scan code that raised the action. [`the_action_is_bound_to_a_control_in_the_shipped_keymap`]
/// covers the binding.
fn inject_action(app: &mut App, action: u32) {
    let e = dereth_input::InputEvent {
        action: dereth_input::ActionId(action),
        input_map: dereth_client::ui::UI_INPUT_MAP,
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
}

/// `CharacterOptionsLogout` — *Exit to Character Selection*.
const LOGOUT_TO_SELECT: u32 = 0x1000_0026;

/// The text of every live bubble element in the floating-message box's list, in list order.
fn bubbles(app: &mut App) -> Vec<String> {
    let shell = app.ui().expect("shell");
    let Some(root) = shell.flow.current().map(|s| s.roots()[0]) else {
        return Vec::new();
    };
    let Some(list) = shell
        .ui
        .get_child_recursive(root, dereth_ui_screens::hud::speech_bubbles::LIST_BOX)
    else {
        return Vec::new();
    };
    let shell = app.ui_mut().expect("shell");
    shell
        .ui
        .children(list)
        .into_iter()
        .filter_map(|h| {
            shell
                .ui
                .text_element_mut(h)
                .map(|t| t.glyphs.inq_text(false))
        })
        .collect()
}

fn screen(app: &mut App) -> &mut GamePlayScreen {
    let shell = app.ui_mut().expect("shell");
    let s = shell.flow.current_mut().expect("the gameplay screen is up");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen is up")
}

/// A real body on real terrain with the gameplay screen up: a physics body a jump can actually
/// take off the ground.
fn app_with_a_body() -> App {
    let mut app = App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 640,
        height: 480,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("required DATs and headless device");
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

    // The corpus login edge: the player's own `0xF746` / `0xF745` out of a recorded session,
    // re-seated on this terrain, which is what gives the body a real weenie and a real physics
    // descriptor rather than a hand-written one.
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
    assert_ne!(
        create.0.physicsdesc.state & physics_state::HIDDEN_PS,
        0,
        "the recorded login create precedes its authoritative unhide"
    );
    let unhide = corpus
        .blobs
        .iter()
        .filter(|r| r.dir == Direction::ServerToClient && r.opcode == ItemSetState::OPCODE.0)
        .filter_map(|r| dereth_protocol::read_body_padded::<ItemSetState>(&r.payload[4..]).ok())
        .find(|m| m.id == id && m.state & physics_state::HIDDEN_PS == 0)
        .expect("early-inventory-and-casting's later authoritative login unhide");
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
    app.objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).unwrap(),
        },
        LocalTime(1.0),
    );
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemSetState::OPCODE,
            body: dereth_protocol::write_body(&unhide).expect("recorded F74B unhide encodes"),
        },
        LocalTime(1.0),
    );
    frames(&mut app, 60);
    let character = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(!character
        .world
        .get(character.handle)
        .unwrap()
        .state
        .is_hidden());

    // The `0x0013 Login_PlayerDescription` out of the same session. Without it the body
    // has no attributes and the jump action never starts a charge, so the jump
    // instrument would silently report "on the ground" for the wrong reason.
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
    app.objects_mut().apply_event(&event, LocalTime(1.0));
    app.apply_hud_events(std::slice::from_ref(&event));
    app.apply_interaction_events(std::slice::from_ref(&event));
    frames(&mut app, 10);

    assert!(
        body(&app).on_ground(),
        "the body has settled before anything is asked of it"
    );
    assert!(body(&app).in_contact());
    app
}

/// A real charged jump off the space bar. Returns once the body has actually left contact, which
/// is the state the timed screen update gate reads.
fn jump(app: &mut App, at: u32) {
    space(app, true, at);
    frames(app, 10);
    assert!(
        app.objects().world.combat.jump_pending,
        "the charge is running"
    );
    space(app, false, at + 300);
    frames(app, 2);
    assert!(
        !body(app).in_contact(),
        "the release must actually take the body off the ground — this is the instrument"
    );
}

/// The action is on a control in the shipped `DefaultMap`, so a player can raise it at all.
#[test]
fn the_action_is_bound_to_a_control_in_the_shipped_keymap() {
    let _gpu = gpu_lock();
    let mut app = app_with_a_body();
    let m = app.input_manager_mut().expect("an input manager");
    let bound = m
        .manager
        .keymap
        .sections
        .iter()
        .flat_map(|s| s.bindings().iter())
        .filter(|(_, a)| a.0 == LOGOUT_TO_SELECT)
        .count();
    assert!(
        bound > 0,
        "the shipped DefaultMap binds a control to {LOGOUT_TO_SELECT:#010X}"
    );
}

/// Behaviour: login.logoff.a-log-off-in-mid-air-is-refused-out-loud
///
/// Press log off in mid-jump: the exact sentence reaches the strip on type `0x1A`, no log-off
/// request is armed — and, because the refusal path also clears that request, **landing arms
/// nothing either**. The press is gone; the player has to make it again.
#[test]
fn a_log_off_pressed_in_mid_air_is_refused_out_loud_and_landing_does_not_revive_it() {
    let _gpu = gpu_lock();
    let mut app = app_with_a_body();

    assert!(
        !app.teleport().log_off_pending(),
        "nothing asked of the player system yet"
    );
    let before_bubbles = bubbles(&mut app).len();
    let before_spew = app.hud().stats.spew_lines;
    let before_notices = app.interaction().stats.panel_notice_strings;

    jump(&mut app, 800_000);

    inject_action(&mut app, LOGOUT_TO_SELECT);
    assert!(
        app.frame(),
        "the frame that raises global message 1 and advances timed panel work"
    );

    // The notice is raised in the frame of the press. It reaches the strip on the frame after:
    // `Scroll::pending` is drained by the HUD fan-out before the interaction drain in `App::frame`,
    // so a line queued by a screen's timed update is fanned out next frame. That
    // one-frame seam belongs to every panel-raised `0x1A` line in this build (salvage's, the
    // journal's), and it is asserted here.
    assert_eq!(
        app.interaction().stats.panel_notice_strings,
        before_notices + 1,
        "one UI display-string notice, in the frame the time step refused"
    );
    assert_eq!(screen(&mut app).logoff_refusals, 1);
    assert!(app.frame(), "the fan-out frame");

    // 1. The player was told, in the words retail uses, on the type retail uses.
    let after = bubbles(&mut app);
    assert_eq!(
        after.len(),
        before_bubbles + 1,
        "exactly one bubble joined the spew panel's list: {after:?}"
    );
    assert!(
        after.iter().any(|t| t == logout::AIRBORNE_REFUSAL),
        "the strip carries the line verbatim: {after:?}"
    );
    assert_eq!(
        app.hud().stats.spew_lines,
        before_spew + 1,
        "…and it got there through the type-0x1A fan-out, which is what the spew panel accepts"
    );

    // 2. Nothing was asked of the player system, and nothing is standing by to be asked.
    assert!(
        !app.teleport().log_off_pending(),
        "no player-system character logoff, so no 0xF653 was built"
    );
    assert!(
        !screen(&mut app).do_end_session,
        "the refusal discarded the end-session request"
    );
    assert!(
        !screen(&mut app).ending_session,
        "the refusal unlatched the one-shot ending-session guard"
    );

    // 3. Still airborne, several frames on: no second line. `do_end_session` is clear, so
    //    the gameplay time step returns at its request gate and never reaches contact again.
    while !body(&app).in_contact() {
        assert!(app.frame());
        assert_eq!(
            screen(&mut app).logoff_refusals,
            1,
            "one press buys exactly one refusal"
        );
    }

    // 4. Landed. Nothing re-arms: `do_end_session` was cleared by the refusal, so contact
    //    returning changes nothing.
    frames(&mut app, 30);
    assert!(body(&app).on_ground(), "the body is back on the ground");
    assert!(
        !app.teleport().log_off_pending(),
        "landing must NOT log the player off — the refusal falls through to clear the end-session \
         request, so the press was cancelled rather than deferred"
    );
    assert_eq!(
        bubbles(&mut app).len(),
        before_bubbles + 1,
        "and no second line on landing"
    );
    assert_eq!(screen(&mut app).logoff_refusals, 1);

    // 5. The second press, on the ground, is heard because the refusal cleared the one-shot
    //    ending-session guard.
    inject_action(&mut app, LOGOUT_TO_SELECT);
    assert!(app.frame());
    assert!(
        app.teleport().log_off_pending(),
        "the ground-contact path requests character logoff with the false flag and arms the transport request"
    );
    assert_eq!(
        screen(&mut app).logoff_refusals,
        1,
        "and it was not refused"
    );
    assert_eq!(
        bubbles(&mut app).len(),
        before_bubbles + 1,
        "and said nothing"
    );
}

/// The discriminating negative: the *same* press, from the *same* body, with the body on the
/// ground, runs the ordinary departure — no line, and the pending log-off request armed in the
/// frame after the key. Without this a gate wired permanently shut would read as a pass.
#[test]
fn on_the_ground_the_same_press_runs_the_normal_departure_and_says_nothing() {
    let _gpu = gpu_lock();
    let mut app = app_with_a_body();

    let before_bubbles = bubbles(&mut app).len();
    assert!(!app.teleport().log_off_pending());

    inject_action(&mut app, LOGOUT_TO_SELECT);
    assert!(app.frame());

    assert_eq!(
        screen(&mut app).logoff_refusals,
        0,
        "in contact: the refusal branch is not taken"
    );
    assert_eq!(
        bubbles(&mut app).len(),
        before_bubbles,
        "nothing is said on the normal path"
    );
    assert!(
        app.teleport().log_off_pending(),
        "the departure sequence is running: the grounded path armed its pending character-logoff request"
    );
    // The non-quit arm queues no mode, so the player is still standing in the world.
    assert_eq!(
        app.ui().expect("a shell").flow.current_mode(),
        Some(dereth_ui::framework::mode::GAME_PLAY),
        "no mode is queued on this arm — character select arrives with the shard's 0xF658"
    );
}
