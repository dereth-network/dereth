//! Logging out to character select plays a six-second departure in the world. *Yes* on the
//! confirmation requests character log-off: it prints *"Logging off...\n"*, sends `0xF653` (eight
//! bytes on the logon channel), disables movement, and arms a local fade deadline of current time +
//! 3.0 s (+ 20 s more for a player killer), queuing **no** mode. The shard's logout emote (`0xF74C`,
//! action index `0x11E`, `LogOut`) plays on the body, the deadline starts the world fade into portal
//! space, and only the server's `0xF658` about six seconds later queues character management
//! `0x1000000A`. The recorded shard sends no `0xF755` and no `HIDDEN_PS` on logout; when both are
//! supplied the body receives the dematerialise's fourteen emitters inside the window. *No* leaves
//! the player in the world.
//! Fixture: the five recorded logout sessions, the retail dats, and a headless `App` in the world
//! with a socket-free replay link, driven through the shipped logout buttons and the input pump.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;
use dereth_scene::world_scene::SceneReads;

use dereth_animation::MotionCommand;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::actions::unpack_action;
use dereth_protocol::movement::{
    InterpretedMotionState, MotionAction, MovementBody, MovementBuffer, MovementMoveToState,
    MovementSetObjectMovement,
};
use dereth_protocol::objects::{
    EffectsPlayScriptType, ItemCreateObject, ItemSetState, ObjectCreatePayload,
};
use dereth_protocol::types::{PhysicsDesc, PhysicsEventStamp, PublicWeenieDesc};
use dereth_protocol::{Message, Opcode};
use dereth_ui::framework::mode;
use dereth_ui_screens::screens::gameplay::{logout, GamePlayScreen};
use dereth_ui_screens::screens::teleport::TeleportAnimState;
use {
    dereth_client::app::App, dereth_client_runtime::config::Config,
    dereth_client_runtime::net::ClientNetwork,
};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::EmitterOwner};

/// The five recorded sessions that reach the world and then leave it. (`login-account-booted` is a bad
/// password and `ddd-interrogation-only` never enters.)
const LOGOUT_SESSIONS: [&str; 5] = [
    "first-login-walk-jump",
    "early-inventory-and-casting",
    "short-second-connection",
    "long-solo-play",
    "short-play-with-training",
];

/// Command index 286 — what every recorded logout `0xF74C` carries, what ACE calls
/// `MotionCommand.LogOut = 0x1000011E`, and the final retail client's `LogOut`.
const LOG_OUT_INDEX: u16 = 0x011E;
/// The 2013 client's own `LogOut`, three entries below it. See the oracle test.
const RETAIL_2013_LOG_OUT_INDEX: u16 = 0x011B;

/// The physics state each recorded logout `0xF74B` carries: `REPORT_COLLISIONS | GRAVITY |
/// EDGE_SLIDE`, and **`HIDDEN_PS` clear**.
const LOGOUT_STATE: u32 = 0x0040_0408;
/// Setting hidden ORs the physics state with `0x4000`.
const HIDDEN_PS: u32 = 0x0000_4000;
/// The state word a recorded *teleport* hide carries, for the shard-readiness test.
const TELEPORT_HIDE_STATE: u32 = 0x0040_4410;
/// Physics script type 116, the hide script.
const PS_HIDE: u32 = 116;

/// The physics-script table ID on every recorded player create.
const PLAYER_TABLE: u32 = 0x3400_0004;
/// The Aluvian male body.
const PLAYER_SETUP: u32 = 0x0200_0001;
const PLAYER: ObjectId = ObjectId(0x5000_0F60);

/// A headless frame of `App` is `dereth_physics::globals::MIN_QUANTUM` exactly.
const FPS: f64 = 30.0;

/// A log-off request sets the local fade deadline to current time + 3.0 seconds.
const LOG_OFF_DELAY: f64 = dereth_client_runtime::teleport::LOG_OFF_DELAY_SECONDS;

/// `ACE.Server/Network/Session.cs:152` — `logOffRequestTime.AddSeconds(6)`. The window is a
/// **server constant**, not the animation's length.
const DEPARTURE_SECONDS: f64 = 6.0;

// ---------------------------------------------------------------------------------------------
// The oracles: the recording, asserted before anything is built on it
// ---------------------------------------------------------------------------------------------

/// The client's `0xF653` in a recorded session, and everything the server said about the player
/// between it and the server's answer.
struct Window {
    /// Seconds between the client's `0xF653` and the server's.
    length: f64,
    /// The command indices every `0xF74C` about the player carried, in order.
    motions: Vec<u16>,
    /// The state words every `0xF74B` about the player carried, in order.
    states: Vec<u32>,
    /// How many `0xF755` the server sent about **anything** inside the window.
    play_scripts: usize,
    /// Whether a `0xF658` arrives at or within one second after the server's answer.
    character_set_follows: bool,
}

fn logout_window(name: &str) -> Option<Window> {
    let c = Corpus::shared(name);
    let request = c.blobs.iter().find(|r| {
        r.dir == Direction::ClientToServer && r.opcode == Opcode::LOGIN_EXECUTE_LOG_OFF.0
    })?;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: microseconds of a two-minute recording; f64 is exact well past this.
    let t0 = request.t_rel_micros as f64 / 1e6;
    let player = ObjectId(u32::from_le_bytes(request.payload[4..8].try_into().ok()?));

    let answer = c.blobs.iter().find(|r| {
        r.dir == Direction::ServerToClient
            && r.opcode == Opcode::LOGIN_EXECUTE_LOG_OFF.0
            && r.t_rel_micros > request.t_rel_micros
    })?;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: as above.
    let t1 = answer.t_rel_micros as f64 / 1e6;

    let mut w = Window {
        length: t1 - t0,
        motions: Vec::new(),
        states: Vec::new(),
        play_scripts: 0,
        character_set_follows: false,
    };
    for row in c.blobs.iter().filter(|r| {
        r.dir == Direction::ServerToClient
            && r.t_rel_micros >= request.t_rel_micros
            && r.t_rel_micros <= answer.t_rel_micros
    }) {
        match Opcode(row.opcode) {
            Opcode::MOVEMENT_SET_OBJECT_MOVEMENT => {
                let Ok(m) = dereth_protocol::read_body_padded::<MovementSetObjectMovement>(
                    &row.payload[4..],
                ) else {
                    continue;
                };
                if m.id != player {
                    continue;
                }
                let Ok(b) = m.decoded_movement() else {
                    continue;
                };
                if let Some(i) = b.body.interpreted {
                    w.motions.extend(i.actions.iter().map(|a| a.command_index));
                }
            }
            Opcode::ITEM_SET_STATE => {
                if let Ok(m) = dereth_protocol::read_body_padded::<ItemSetState>(&row.payload[4..])
                {
                    if m.id == player {
                        w.states.push(m.state);
                    }
                }
            }
            Opcode::EFFECTS_PLAY_SCRIPT_TYPE => w.play_scripts += 1,
            _ => {}
        }
    }
    w.character_set_follows = c.blobs.iter().any(|r| {
        r.dir == Direction::ServerToClient
            && r.opcode == Opcode::LOGIN_LOGIN_CHARACTER_SET.0
            && r.t_rel_micros >= answer.t_rel_micros
            && r.t_rel_micros < answer.t_rel_micros + 1_000_000
    });
    Some(w)
}

/// **What the shard actually sends when a character leaves — measured, not assumed.**
///
/// Five recordings, one shape: six seconds, one `0xF74C` carrying one action, one `0xF74B` with
/// `HIDDEN_PS` clear, **no `0xF755` at all**, and the `0xF658` in the same instant as the answer.
#[test]
fn the_recorded_logout_window_is_six_seconds_of_one_emote_and_no_dematerialise() {
    let mut seen = 0usize;
    for name in LOGOUT_SESSIONS {
        let Some(w) = logout_window(name) else {
            continue;
        };
        seen += 1;
        assert!(
            (5.9..6.1).contains(&w.length),
            "{name}: the window is {:.4} s — ACE's `logOffRequestTime.AddSeconds(6)`",
            w.length
        );
        assert_eq!(
            w.motions,
            vec![LOG_OUT_INDEX],
            "{name}: exactly one motion about the player, and it is the logout emote"
        );
        assert!(
            !w.states.is_empty(),
            "{name}: the state re-broadcast is there"
        );
        for s in &w.states {
            assert_eq!(*s, LOGOUT_STATE, "{name}: the recorded logout state word");
            assert_eq!(
                s & HIDDEN_PS,
                0,
                "{name}: HIDDEN_PS is CLEAR — nothing is hidden"
            );
        }
        assert_eq!(
            w.play_scripts, 0,
            "{name}: no 0xF755 in the window — this shard asks for no dematerialise, which is \
             `Player.LogOut_Final`'s own shape and not a decode gap"
        );
        assert!(
            w.character_set_follows,
            "{name}: the 0xF658 arrives with the answer"
        );
    }
    assert_eq!(
        seen,
        LOGOUT_SESSIONS.len(),
        "all five recorded logouts must be readable"
    );
}

/// **The clap's wire index is `0x11E`, and the final retail table names it `LogOut`.**
///
/// The recordings were made by the 2013 client, whose command table had three fewer rows below
/// this one: it called `0x11E` `DoubleSlashHigh` and numbered its own `LogOut` `0x11B`. The server
/// sends `0x11E`, because the server's numbering is the final client's. The client uses the final
/// table, so the name and the wire agree, and the 2013 numbering survives only as the 2013 map,
/// which is what reads a 2013 index.
#[test]
fn the_logout_emote_is_wire_index_0x11e_which_the_final_table_names_log_out() {
    let on_the_wire = MotionCommand::from_index(LOG_OUT_INDEX).expect("index 286 is in the table");
    assert_eq!(
        on_the_wire.0, 0x1000_011E,
        "the id the shard broadcasts — ACE's MotionCommand.LogOut and the recorded logout motion"
    );
    assert_eq!(
        on_the_wire.to_index(),
        Some(LOG_OUT_INDEX),
        "and it round-trips, so a client that plays it plays the shard's own animation"
    );
    assert_eq!(
        on_the_wire,
        MotionCommand::LOG_OUT,
        "and it is the table's LogOut"
    );
    assert_eq!(on_the_wire.name(), Some("LogOut"));

    // The 2013 fact, kept explicit: that client's `LogOut` index maps onto this one.
    assert_eq!(
        dereth_animation::command::retail_2013_index_to_current(RETAIL_2013_LOG_OUT_INDEX),
        Some(LOG_OUT_INDEX),
        "the 2013 client's LogOut (0x11B) is the final table's 0x11E"
    );
}

// ---------------------------------------------------------------------------------------------
// The harness — the real `App::frame`, with a link
// ---------------------------------------------------------------------------------------------

/// Network connection is disabled. The helper below attaches a socket-free replay endpoint,
/// making the application link present so its logon-event loop can run. No server is contacted.
fn config() -> Config {
    Config {
        headless: true,
        sound: false,
        world: false,
        character: false,
        ui: true,
        connect: false,
        account: "f60".into(),
        host: "127.0.0.1".into(),
        port: 9000,
        preferences_file: std::env::temp_dir().join("dere-f60-not-created/prefs.ini"),
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// An application standing in the world on the gameplay screen, with an animatable body.
fn app_in_game() -> Option<App> {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats must be at {} (set DERETH_TEST_DAT_DIR)",
        client_dir().display()
    );
    let mut app =
        App::new(config()).unwrap_or_else(|e| panic!("a headless App on the software device: {e}"));
    app.start_shell().expect("the UI shell comes up");
    app.load_static_scene(SceneConfig {
        character: true,
        cell_statics: false,
        mesh_collision: false,
        land_radius: 1,
        scenery_radius: 0,
        particles: true,
        ..SceneConfig::default()
    })
    .expect("the static scene loads");
    app.queue_ui_mode(mode::GAME_PLAY);
    for _ in 0..40 {
        if current_mode(&app) == Some(mode::GAME_PLAY) {
            let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "f60", "unused", 0)
                .expect("a socket-free replay net");
            net.session.transport.add_connection(
                0xB,
                0,
                1,
                0xDEAD_BEEF,
                0x1234_5678,
                Some("127.0.0.1:19000".parse().expect("addr")),
            );
            // Keep the exact character id `Session::log_off` must put behind literal `0xF653`.
            net.session.enter_world(PLAYER, "f60");
            app.attach_replay_network(net)
                .expect("the replay endpoint attaches");
            log_in(&mut app);
            // Discard bring-up traffic. Each test starts with an empty, inspectable wire window.
            let _ = app
                .replay_network_mut()
                .expect("the replay endpoint")
                .take_outgoing();
            return Some(app);
        }
        if current_mode(&app) == Some(mode::INTRO) {
            let root = app
                .ui()
                .and_then(|u| u.flow.current())
                .and_then(|s| s.roots().first().copied());
            if let (Some(root), Some(shell)) = (root, app.ui_mut()) {
                shell.ui.broadcast_element_message(
                    root,
                    dereth_ui_screens::screens::intro::MSG_SKIP,
                    0,
                    0,
                );
            }
        }
        app.frame();
    }
    panic!(
        "the flow did not reach the gameplay screen: {:?}",
        current_mode(&app)
    );
}

/// Complete payloads the socket-free App has put on its real packet-controller output.
fn outgoing_payloads(app: &mut App) -> Vec<Vec<u8>> {
    let mut payloads = Vec::new();
    for (raw, _) in app
        .replay_network_mut()
        .expect("the replay endpoint")
        .take_outgoing()
    {
        let packet =
            dereth_transport::wire::ParsedPacket::parse(&raw).expect("the client's own datagram");
        payloads.extend(packet.fragments.into_iter().map(|f| f.payload));
    }
    payloads
}

fn current_mode(app: &App) -> Option<dereth_ui::UiMode> {
    app.ui().and_then(|u| u.flow.current_mode())
}

fn send<M: Message>(app: &mut App, m: &M) {
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: M::OPCODE,
            body: dereth_protocol::write_body(m).expect("encodes"),
        },
        LocalTime(0.0),
    );
}

/// Log the synthetic player in with the `phstable_id` every recorded player create carries, which
/// is what gives the body a script table and a motion table to play out of.
fn log_in(app: &mut App) {
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(PLAYER), LocalTime(0.0));
    let (block, origin) = {
        let s = app.world_scene().expect("a scene");
        (
            s.viewer_block().expect("a resident block"),
            s.character.as_ref().expect("a body").render_frame().origin,
        )
    };
    send(
        app,
        &ItemCreateObject(ObjectCreatePayload {
            id: PLAYER,
            objdesc: dereth_protocol::types::ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                    | dereth_protocol::types::physicsdesc::flags::SETUP
                    | dereth_protocol::types::physicsdesc::flags::PETABLE,
                setup_id: Some(PLAYER_SETUP),
                phstable_id: Some(PLAYER_TABLE),
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: (u32::try_from(block.0).expect("x") << 24)
                        | (u32::try_from(block.1).expect("y") << 16)
                        | 1,
                    frame: dereth_protocol::types::Frame {
                        origin: origin.into(),
                        orientation: dereth_primitives::Quat::IDENTITY.into(),
                    },
                }),
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        }),
    );
    for _ in 0..3 {
        app.frame();
    }
}

fn click(app: &mut App, id: dereth_ui::ElementId) {
    let shell = app.ui_mut().expect("shell");
    let h = shell
        .ui
        .get_element(id)
        .unwrap_or_else(|| panic!("{:#X} is in the shipped layout", id.0));
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
}

/// Keyboard messages through the production pump and the shipped input maps.
struct Keys {
    pump: dereth_desktop::pump::Pump,
    time_ms: u32,
}

impl Keys {
    fn new() -> Self {
        let mut pump = dereth_desktop::pump::Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        Self {
            pump,
            time_ms: 970_000,
        }
    }

    fn key(&mut self, app: &mut App, code: winit::keyboard::KeyCode, down: bool) {
        self.time_ms += 10;
        let message = self
            .pump
            .key_message_for(code, down, self.time_ms)
            .expect("winit supplies the retail scan code");
        self.pump.dispatch(message);
        app.input_manager_mut()
            .expect("the input shell")
            .on_message(message);
    }
}

fn screen(app: &mut App) -> &mut GamePlayScreen {
    let shell = app.ui_mut().expect("the shell is up");
    let s = shell.flow.current_mut().expect("a screen");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen")
}

/// Raise the confirmation the way the player does — *Exit to Character Selection* on the
/// Gameplay Options page.
fn raise_the_confirmation(app: &mut App) {
    click(app, logout::EXIT_TO_CHARACTER_SELECTION);
    app.frame();
    app.frame();
    assert!(
        screen(app).logout_dialog().is_some(),
        "Exit to Character Selection raises the confirmation"
    );
}

fn secs(app: &mut App, seconds: f64) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: a frame count in a test.
    let n = (seconds * FPS).round() as u32;
    for _ in 0..n {
        app.frame();
    }
}

/// The emitters a hide script puts on the body.
fn body_emitters(app: &App) -> usize {
    app.world_scene()
        .expect("a scene")
        .emitter_degrade_probe()
        .into_iter()
        .filter(|e| matches!(e.owner, EmitterOwner::Body))
        .count()
}

/// Every motion the body's interpreter is holding — the queued actions and the motion table's
/// pending nodes together, which is what "the body is playing this" means across the frames it
/// takes an action to become a node.
fn body_motions(app: &App) -> Vec<MotionCommand> {
    let c = app
        .world_state()
        .expect("a scene")
        .character
        .as_ref()
        .expect("a body");
    let d = c.driver();
    d.movement
        .interp
        .interpreted_state
        .actions
        .iter()
        .map(|a| a.action)
        .chain(d.motion_table.pending().iter().map(|n| n.motion))
        .collect()
}

/// The shard's two messages about the departing player, in the order the recording carries them:
/// `0xF74B` re-broadcasting the ordinary standing state, then `0xF74C` with the logout emote.
fn server_plays_the_departure(app: &mut App) {
    send(
        app,
        &ItemSetState {
            id: PLAYER,
            state: LOGOUT_STATE,
            timestamps: PhysicsEventStamp {
                instance: 0,
                event: 1,
            },
        },
    );
    let p = app
        .probe_mut()
        .objects_mut()
        .presence(PLAYER)
        .expect("the player's presence");
    let (instance, movement_ts, server_ts) = (p.instance, p.movement_ts, p.server_control_ts);
    // ACE `Player_Location.cs` `Player.SendMotionAsCommands` — `Motion(NonCombat, Ready)` with
    // `AddCommand(LogOut)`, which is exactly what the recorded buffer decodes to.
    let buffer = MovementBuffer {
        movement_timestamp: movement_ts.wrapping_add(1),
        server_control_timestamp: server_ts,
        autonomous: false,
        body: MovementBody {
            current_style: MotionCommand::NON_COMBAT.to_index().expect("style index"),
            interpreted: Some(InterpretedMotionState {
                current_style: MotionCommand::NON_COMBAT.to_index(),
                forward_command: MotionCommand::READY.to_index(),
                actions: vec![MotionAction {
                    command_index: LOG_OUT_INDEX,
                    stamp_and_autonomy: 2,
                    speed: 1.0,
                }],
                ..InterpretedMotionState::default()
            }),
            ..MovementBody::default()
        },
    };
    let m = MovementSetObjectMovement {
        id: PLAYER,
        instance_sequence: instance,
        movement: MovementSetObjectMovement::encode_movement(&buffer).expect("a 0xF74C buffer"),
    };
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
            body: dereth_protocol::write_body(&m).expect("wire body"),
        },
        LocalTime(0.0),
    );
}

/// A one-row `0xF658`, so the shell has a list to stand character select on.
fn a_character_set() -> Box<dereth_protocol::login::LoginCharacterSet> {
    Box::new(dereth_protocol::login::LoginCharacterSet {
        status: 0,
        characters: vec![dereth_protocol::login::CharacterIdentity {
            gid: PLAYER,
            name: "F60".into(),
            seconds_greyed_out: 0,
        }],
        deleted: vec![],
        num_allowed_characters: 5,
        account: "f60".into(),
        use_turbine_chat: 0,
        has_throne_of_destiny: 0,
    })
}

// ---------------------------------------------------------------------------------------------
// The departure — one run, read at six times
// ---------------------------------------------------------------------------------------------

/// Behaviour: logout.departure.the-logout-emote-plays-before-the-screen-changes
///
/// **The departure has an order and a duration, and this asserts both.**
///
/// One run read at six simulated times: the frame after *Yes*, +0.5 s, +1.0 s, +3.1 s, +5.0 s and
/// +6.0 s. A client that snaps to character select fails at the first; one that never leaves fails
/// at the last.
#[test]
fn the_departure_plays_for_six_seconds_before_the_screen_changes() {
    let _gpu = gpu_lock();
    let mut app = app_in_game().expect("an application in the world on the gameplay screen");
    assert_eq!(
        current_mode(&app),
        Some(mode::GAME_PLAY),
        "the player is in the world"
    );

    // --- t + 1 frame: the answer is given -----------------------------------------------------
    raise_the_confirmation(&mut app);
    click(&mut app, logout::BUTTON_YES);
    app.frame();
    assert_eq!(
        current_mode(&app),
        Some(mode::GAME_PLAY),
        "the confirmed non-quit branch queues NO mode: the player stays in the world \
         while the departure plays"
    );
    assert!(
        app.teleport().log_off_pending(),
        "the log-off request armed the pending latch — the client asked to leave"
    );

    // --- t + 0.5 s: the shard's two messages land, as the corpus has them ----------------------
    secs(&mut app, 0.5);
    assert_eq!(
        current_mode(&app),
        Some(mode::GAME_PLAY),
        "still in the world half a second in"
    );
    server_plays_the_departure(&mut app);
    secs(&mut app, 0.5);

    // --- t + 1.0 s: the clap is on the body ---------------------------------------------------
    let motions = body_motions(&app);
    let log_out = MotionCommand::from_index(LOG_OUT_INDEX).expect("the shard's LogOut id");
    assert!(
        motions.contains(&log_out),
        "the clap over the head — the body took the shard's LogOut at t + 1.0 s, not {motions:?}"
    );
    assert_eq!(
        current_mode(&app),
        Some(mode::GAME_PLAY),
        "and it is happening in the world"
    );

    // --- t + 3.1 s: `logOffRequestTime` passes and the world folds into portal space -----------
    secs(&mut app, LOG_OFF_DELAY - 1.0 + 0.1);
    assert_ne!(
        app.teleport().anim.state,
        TeleportAnimState::Off,
        "the log-off deadline (current time + {LOG_OFF_DELAY}) has passed, so the world-screen time update began world fade-out"
    );
    assert!(
        !app.teleport().log_off_pending(),
        "consuming the log-off request cleared the latch, so the fade begins exactly once"
    );
    assert_eq!(
        current_mode(&app),
        Some(mode::GAME_PLAY),
        "the fade plays on the gameplay screen"
    );

    // --- t + 5.0 s: portal space, and still not character select --------------------------------
    secs(&mut app, 1.9);
    assert!(
        app.teleport().world_hidden(),
        "`world-screen hiding` — the world is behind the portal tunnel at t + 5 s"
    );
    assert!(
        app.teleport().tunnels_played >= 1,
        "the portal tunnel was entered"
    );
    assert_eq!(
        current_mode(&app),
        Some(mode::GAME_PLAY),
        "five seconds in and the client has still not queued the screen itself — only the \
         server's 0xF658 may do that"
    );

    // --- t + 6.0 s: the server answers, and only now does the screen change ---------------------
    secs(&mut app, DEPARTURE_SECONDS - 5.0);
    app.process_logon_event_queue(vec![
        SessionEvent::LoggedOff,
        SessionEvent::CharacterSet(a_character_set()),
    ]);
    for _ in 0..4 {
        app.frame();
    }
    assert_eq!(
        current_mode(&app),
        Some(mode::CHARACTER_MANAGEMENT),
        "the character-set notice reaches screen flow and queues character management (0x1000000A)"
    );
}

/// Behaviour: logout.departure.a-player-killer-stays-until-the-fade-deadline
///
/// **A player killer's client-side departure waits twenty extra seconds.**
///
/// Log-off sends `0xF653` before querying the local game object's player-killer predicate, which
/// adds exactly `20.0` seconds when true. The server's PK-battle delay remains authoritative and
/// independent; this local deadline only decides when the world-screen update starts collapsing
/// the projection. An NPK's three-second deadline would have `WorldFadeOut` started at the first
/// assertion.
#[test]
fn a_player_killer_stays_in_the_world_until_the_native_twenty_three_second_fade_deadline() {
    let _gpu = gpu_lock();
    let mut app = app_in_game().expect("the real D3D12 test device");
    update_player_killer_status(&mut app, 1, 0x40);
    assert!(
        app.objects()
            .world
            .weenie(PLAYER)
            .expect("the local player")
            .is_player_killer(),
        "PKLite also answers the native predicate"
    );
    update_player_killer_status(&mut app, 0, 0x20);
    assert!(
        app.objects()
            .world
            .weenie(PLAYER)
            .expect("the local player")
            .is_player_killer(),
        "a stale Int134 cannot replace the accepted PKLite state"
    );
    update_player_killer_status(&mut app, 2, 0x20);
    assert!(
        !app.objects()
            .world
            .weenie(PLAYER)
            .expect("the local player")
            .is_player_killer(),
        "impenetrable is a distinct bit, not a guessed nonzero PK status"
    );
    update_player_killer_status(&mut app, 3, 4);
    assert!(
        app.objects()
            .world
            .weenie(PLAYER)
            .expect("the local player")
            .is_player_killer(),
        "the production player-killer predicate used by log-off is true"
    );

    raise_the_confirmation(&mut app);
    click(&mut app, logout::BUTTON_YES);
    app.frame();
    assert!(
        app.teleport().log_off_pending(),
        "the physical answer arms the local request deadline immediately"
    );
    assert!(
        outgoing_payloads(&mut app)
            .iter()
            .all(|p| p.get(..4) != Some(&0xF653_u32.to_le_bytes())),
        "the input callback is after this frame's packet-processing stage"
    );
    app.frame();
    let logoffs: Vec<_> = outgoing_payloads(&mut app)
        .into_iter()
        .filter(|p| p.get(..4) == Some(&0xF653_u32.to_le_bytes()))
        .collect();
    assert_eq!(
        logoffs,
        vec![vec![0x53, 0xF6, 0x00, 0x00, 0x60, 0x0F, 0x00, 0x50]],
        "one literal [0xF653, player] leaves immediately, not at the local fade deadline"
    );

    secs(&mut app, LOG_OFF_DELAY + 0.1);
    assert_eq!(
        app.teleport().anim.state,
        TeleportAnimState::Off,
        "PK log-off adds twenty seconds before any projection collapse"
    );
    assert!(
        app.teleport().log_off_pending(),
        "the delayed fade has not consumed the latch"
    );

    secs(&mut app, 19.7);
    assert_eq!(
        app.teleport().anim.state,
        TeleportAnimState::Off,
        "the world remains ordinary just before the 23-second deadline"
    );
    assert!(app.teleport().log_off_pending());

    secs(&mut app, 0.3);
    assert_eq!(
        app.teleport().anim.state,
        TeleportAnimState::WorldFadeOut,
        "only after 3 + 20 seconds does the native log-off fade begin"
    );
    assert!(
        !app.teleport().log_off_pending(),
        "starting log-off consumed the request once"
    );
}

/// **Requesting log-off disables movement after sending its one request.**
///
/// The key enters through the real pump and input-manager path. The confirmation's accepted *Yes*
/// is the exact edge that calls `App::log_off_character`: the held `W`, its command-list row and
/// the projected forward motion are cleared there, and one stopped `0xF61C` is emitted.
#[test]
fn an_accepted_logout_stops_held_input_and_reports_that_stop_once() {
    let _gpu = gpu_lock();
    let mut app = app_in_game().expect("the real D3D12 test device");
    let mut keys = Keys::new();

    keys.key(&mut app, winit::keyboard::KeyCode::KeyW, true);
    app.frame();
    assert!(
        app.probe().char_input().forward,
        "the physical W press reached the character"
    );
    assert!(
        !app.probe().movement_commands().lists.substate.is_empty(),
        "and it is genuinely held in the command interpreter's substate list"
    );
    let _ = outgoing_payloads(&mut app);

    raise_the_confirmation(&mut app);
    let before_yes = outgoing_payloads(&mut app);
    assert!(
        before_yes.iter().any(|p| {
            let Ok(mut action) = unpack_action(p) else {
                return false;
            };
            if action.sub_type != MovementMoveToState::OPCODE {
                return false;
            }
            MovementMoveToState::read(&mut action.body).is_ok_and(|m| {
                m.0.raw_motion_state.forward_command == Some(MotionCommand::WALK_FORWARD.0)
            })
        }),
        "the pre-Yes wire window contains the held-W F61C, and is drained before the stop oracle"
    );
    click(&mut app, logout::BUTTON_YES);
    app.frame();
    assert!(
        !app.probe().char_input().forward
            && app.probe().movement_commands().lists.substate.is_empty(),
        "disabling movement clears the held list and its projection on the accepted edge"
    );

    // Packet processing precedes the UI in App::frame, so both outputs queued by log-off are
    // serialized on the following frame.
    app.frame();
    let payloads = outgoing_payloads(&mut app);
    let logoffs: Vec<_> = payloads
        .iter()
        .filter(|p| p.get(..4) == Some(0xF653_u32.to_le_bytes().as_slice()))
        .cloned()
        .collect();
    assert_eq!(
        logoffs,
        vec![vec![0x53, 0xF6, 0x00, 0x00, 0x60, 0x0F, 0x00, 0x50]],
        "one literal [F653, player] leaves before the departure"
    );
    let stopped: Vec<_> = payloads
        .iter()
        .filter_map(|p| {
            if p.get(..4) != Some(0xF7B1_u32.to_le_bytes().as_slice()) {
                return None;
            }
            let mut action = unpack_action(p).ok()?;
            (action.sub_type == MovementMoveToState::OPCODE)
                .then(|| MovementMoveToState::read(&mut action.body).expect("the stop decodes"))
        })
        .collect();
    assert_eq!(
        stopped.len(),
        1,
        "disabling movement produces one stopped F61C"
    );
    assert_eq!(
        stopped[0].0.raw_motion_state.forward_command, None,
        "the movement event reports the cleared forward slot"
    );

    // A newly pressed movement key is consumed but cannot repopulate the disabled interpreter.
    keys.key(&mut app, winit::keyboard::KeyCode::KeyA, true);
    app.frame();
    assert!(
        !app.probe().char_input().turn_left
            && app.probe().movement_commands().lists.turn.is_empty(),
        "movement is disabled until the world controller accepts another player create"
    );

    // The capture's authoritative freeze and clear are presentation state, not an input-enable
    // edge. Neither may resurrect the W which disabling movement removed.
    for (event, state) in [(2, 0x0140_0408), (3, LOGOUT_STATE)] {
        send(
            &mut app,
            &ItemSetState {
                id: PLAYER,
                state,
                timestamps: PhysicsEventStamp { instance: 0, event },
            },
        );
        app.frame();
        assert!(!app.probe().char_input().forward && !app.probe().char_input().turn_left);
    }

    // Accepted player creation is the matching input-enable edge. Drive that decoded production
    // event because this test already consumed its one logout wire; the session's accepted-F746
    // decode is covered independently.
    app.process_logon_event_queue(vec![SessionEvent::PlayerCreated(PLAYER)]);
    assert!(
        app.probe().movement_commands().is_enabled(),
        "the accepted player-create event re-enables"
    );
    keys.key(&mut app, winit::keyboard::KeyCode::KeyD, true);
    app.frame();
    assert!(
        app.probe().char_input().turn_right,
        "a fresh movement key works after movement is re-enabled"
    );
}

fn update_player_killer_status(app: &mut App, sequence: u8, value: i32) {
    use dereth_client_model::qualities::{QualityUpdate, UpdateOutcome};
    use dereth_client_model::{Qualities, StatKey, StatType, StatValue};

    let world = &mut app.probe_mut().objects_mut().world;
    let player = world.weenie_mut(PLAYER).expect("the local player");
    player.qualities.get_or_insert_with(Qualities::default);
    let outcome = world.apply_player_quality_update(&QualityUpdate {
        subject: None,
        sequence,
        key: StatKey::new(StatType::Int, 134),
        value: StatValue::Int(value),
    });
    if sequence == 0 {
        assert_eq!(
            outcome,
            Some(UpdateOutcome::Stale),
            "the older timestamp is rejected"
        );
    } else {
        assert_eq!(
            outcome,
            Some(UpdateOutcome::Applied),
            "the current Int134 is accepted"
        );
    }
}

/// **The control.** *No* on the same confirmation must leave the player where he was: no log-off
/// armed, no fade, no screen change throughout the eight seconds measured here.
///
/// This rejects a departure begun despite refusing confirmation; the positive test separately
/// rejects never changing mode.
#[test]
fn answering_no_leaves_the_player_in_the_world_for_ever() {
    let _gpu = gpu_lock();
    let mut app = app_in_game().expect("an application in the world on the gameplay screen");
    raise_the_confirmation(&mut app);
    click(&mut app, logout::BUTTON_NO);
    app.frame();

    secs(&mut app, DEPARTURE_SECONDS + 2.0);
    assert!(
        !app.teleport().log_off_pending(),
        "nothing was asked of the player controller"
    );
    assert_eq!(app.teleport().anim.state, TeleportAnimState::Off, "no fade");
    assert_eq!(body_emitters(&app), 0, "no dematerialise");
    assert_eq!(
        current_mode(&app),
        Some(mode::GAME_PLAY),
        "eight seconds after *No* the player is still in the world"
    );
}

/// **The dematerialise is received in full when a shard asks for it.**
///
/// The recorded shard never sends one (ACE's `Player.LogOut_Final` has no `PlayScript`). The
/// identical run with the teleport's own hide script + `HIDDEN_PS` pair dropped into the window
/// produces fourteen emitters on the body, inside the departure, while the screen has not changed.
///
/// Absent-then-present in one run: the extra fourteen emitters are **not** there before the hide
/// and **are** there during the window. This test does not measure their eventual disappearance.
#[test]
fn a_shard_that_does_ask_for_the_dematerialise_is_already_fully_received() {
    let _gpu = gpu_lock();
    let mut app = app_in_game().expect("an application in the world on the gameplay screen");
    assert_eq!(body_emitters(&app), 0, "the body starts with no emitters");

    raise_the_confirmation(&mut app);
    click(&mut app, logout::BUTTON_YES);
    app.frame();
    secs(&mut app, 0.5);
    server_plays_the_departure(&mut app);
    secs(&mut app, 0.25);
    // The baseline is taken **after** the emote, so what is measured below is the hide's own
    // fourteen and not whatever the departure motion itself is emitting.
    let before = body_emitters(&app);

    // The two messages this shard does not send, exactly as a teleport sends them.
    send(
        &mut app,
        &EffectsPlayScriptType {
            id: PLAYER,
            script_type: i32::try_from(PS_HIDE).expect("116"),
            intensity: 1.0,
        },
    );
    send(
        &mut app,
        &ItemSetState {
            id: PLAYER,
            state: TELEPORT_HIDE_STATE,
            timestamps: PhysicsEventStamp {
                instance: 0,
                event: 2,
            },
        },
    );
    secs(&mut app, 0.5);

    assert_eq!(
        body_emitters(&app),
        before + 14,
        "the hide's fourteen create-particle hooks are on the body — the client receives the \
         dematerialise in full the moment a shard asks for it (it had {before} before the hide)"
    );
    assert_eq!(
        current_mode(&app),
        Some(mode::GAME_PLAY),
        "and it plays inside the departure window, which is the whole point of the window"
    );
}
