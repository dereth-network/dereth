//! Real application fixtures shared by device-backed and device-free tests.
//!
//! Behaviour: none (shared fixtures)

use dereth_client_net::client_session::{
    testing::{Corpus, Direction},
    SessionEvent,
};
use dereth_primitives::{LocalTime, ObjectId, Position};
use dereth_protocol::{objects::ItemCreateObject, Message, Opcode};
use dereth_ui::{framework::mode, UiSystem};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use {
    dereth_client::app::App, dereth_client_runtime::app::StartupError,
    dereth_client_runtime::config::Config, dereth_client_runtime::landblock::DEFAULT_LANDBLOCK,
    dereth_client_runtime::scene::SceneConfig, dereth_desktop::pump::Pump,
};

use crate::common::client_dir;

/// Advance the application, failing if any frame stops.
pub fn frames(app: &mut App, count: usize) {
    for _ in 0..count {
        assert!(app.frame());
    }
}

/// The real character position, including its current world cell.
pub fn position(app: &App) -> Position {
    app.world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position()
}

/// The application's real character.
pub fn body(app: &App) -> &dereth_client_runtime::character::Character {
    app.world_state().unwrap().character.as_ref().unwrap()
}

/// Deliver one physical key through the host pump and input manager.
pub fn key(app: &mut App, code: winit::keyboard::KeyCode, down: bool, time: u32) {
    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let message = pump.key_message_for(code, down, time).unwrap();
    app.input_manager_mut().unwrap().on_message(message);
}

/// Resolve an unmodified movement binding and deliver its physical key.
pub fn movement_key(app: &mut App, action: dereth_input::ActionId, down: bool, time: u32) {
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
        dereth_desktop::pump::scan_code_from_key_code(*keycode)
            .is_some_and(|scan| (scan & 0xff) as u32 == (binding.control.offset() & 0x7f) as u32)
    })
    .expect("known physical movement key for this retail DAT");
    key(app, keycode, down, time);
}

/// Apply a recorded player description to objects, HUD, then interaction.
pub fn player_description(app: &mut App, name: &str) {
    let corpus = Corpus::load(name)
        .unwrap()
        .expect("recorded player description");
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

/// Apply the server's recorded unhide after its login-tunnel create.
pub fn unhide_recorded_player(app: &mut App, corpus: &Corpus, id: ObjectId) {
    let row = corpus
        .blobs
        .iter()
        .find(|b| {
            b.dir == Direction::ServerToClient
                && b.opcode == 0xf74b
                && b.payload[4..8] == id.0.to_le_bytes()
                && u32::from_le_bytes(b.payload[8..12].try_into().unwrap())
                    & dereth_physics::PhysicsState::HIDDEN_PS
                    == 0
        })
        .expect("early-inventory-and-casting's recorded 0xF74B unhide for the player");
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_SET_STATE,
            body: row.payload[4..].to_vec(),
        },
        LocalTime(1.0),
    );
}

/// Apply the matching recorded unhide from a corpus of player events.
pub fn unhide_player(app: &mut App, corpus: &Corpus, id: ObjectId) {
    let row = corpus
        .blobs
        .iter()
        .find(|b| {
            b.opcode == 0xf74b
                && b.payload[4..8] == id.0.to_le_bytes()
                && u32::from_le_bytes(b.payload[8..12].try_into().unwrap())
                    & dereth_physics::PhysicsState::HIDDEN_PS
                    == 0
        })
        .expect("early-inventory-and-casting's recorded 0xF74B unhide for the player");
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_SET_STATE,
            body: row.payload[4..].to_vec(),
        },
        LocalTime(1.0),
    );
}

/// Start the UI and configured static scene, then advance into gameplay.
pub fn app_in_gameplay_with(
    frames: u32,
    new: impl FnOnce(Config) -> Result<App, StartupError>,
) -> App {
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
    let mut app = new(cfg).expect("the application comes up");
    app.start_shell().expect("the UI comes up");
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

/// The current gameplay screen and its live UI system.
pub fn gameplay(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is up");
    let any: &mut dyn std::any::Any = &mut **screen;
    (
        ui,
        any.downcast_mut::<GamePlayScreen>()
            .expect("gameplay screen"),
    )
}

/// Build the recorded player on real terrain and finish its recorded unhide.
pub fn app_with_recorded_body_with(new: impl FnOnce(Config) -> Result<App, StartupError>) -> App {
    let mut app = new(Config {
        headless: true,
        width: 320,
        height: 240,
        sound: false,
        ui: false,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("required real DAT application");
    app.start_shell().expect("InputShell");
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
    .expect("real terrain/physics scene");
    frames(&mut app, 60);

    let corpus = Corpus::load("early-inventory-and-casting")
        .expect("corpus decodes")
        .expect("early-inventory-and-casting");
    let player_row = corpus
        .blobs
        .iter()
        .find(|r| r.dir == Direction::ServerToClient && r.opcode == Opcode::LOGIN_CREATE_PLAYER.0)
        .expect("recorded player identity");
    let id = ObjectId(u32::from_le_bytes(
        player_row.payload[4..8].try_into().unwrap(),
    ));
    let row = corpus
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && u32::from_le_bytes(r.payload[4..8].try_into().unwrap()) == id.0
        })
        .expect("recorded player assets");
    let mut create = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .expect("recorded F745");
    let here = position(&app);
    create.0.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
        objcell_id: here.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: dereth_protocol::types::Vec3 {
                x: here.frame.origin.x,
                y: here.frame.origin.y,
                z: here.frame.origin.z,
            },
            orientation: dereth_protocol::types::Quat {
                w: here.frame.rotation.w,
                x: here.frame.rotation.x,
                y: here.frame.rotation.y,
                z: here.frame.rotation.z,
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
            body: dereth_protocol::write_body(&create).expect("constructed terrain placement"),
        },
        LocalTime(1.0),
    );
    unhide_recorded_player(&mut app, &corpus, id);
    frames(&mut app, 90);
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(c.on_ground(), "real terrain must support the local body");
    assert!(
        !c.world
            .get(c.handle)
            .expect("the local collision body")
            .state()
            .is_hidden(),
        "premise: the login create arrives with HIDDEN_PS and a hidden body never \
         advances its animation offset, so the recorded unhide has to have reached it"
    );
    assert!(!c.is_moving_to() && !c.driver().movement.motions_pending());
    app
}
