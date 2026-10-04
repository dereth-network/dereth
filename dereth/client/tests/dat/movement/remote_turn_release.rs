//! An arrow-key release clears a remote player's turn state, so quiet frames after the stop do not
//! keep drawing the remote turning. Fixture: early-inventory-and-casting's recorded player create,
//! copied as a positioned remote three metres away, on real `DEFAULT_LANDBLOCK` terrain in a
//! headless App. One socket-free journey covers the three client-side halves:
//!
//! 1. `Pump::key_message_for(ArrowLeft)` drives the shipped map on key-down and key-up, and the
//!    replay transport captures the actual `0xF61C Movement_MoveToState` bodies the App emits;
//! 2. ACE's `MovementData(Creature, MoveToState)` conversion is reproduced only at the server
//!    boundary, turning those captured raw states into serialized `0xF74C` type-0 bodies;
//! 3. the same App receives those bodies for a real, positioned remote player and advances quiet
//!    frames across the native 30 Hz physics gate.
//!
//! No datagram leaves this process. ACE is evidence for the server hand-off, not for client
//! behavior: `GameActionMoveToState.Handle` broadcasts every accepted state, and its conversion
//! maps a left raw turn to canonical `TurnRight` with negative speed while an absent turn remains
//! absent. The remote application and quiet ticks are dereth-client's production path.

use crate::common::sim_app::frames;

use dereth_animation::motion::HoldKey;
use dereth_animation::MotionCommand;
use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::pump::Pump;
use dereth_client::world::{SceneConfig, DEFAULT_LANDBLOCK};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, ObjectId, Position};
use dereth_protocol::actions::unpack_action;
use dereth_protocol::movement::{
    movement_type, InterpretedMotionState, MovementBody, MovementBuffer, MovementMoveToState,
    MovementSetObjectMovement, RawMotionState,
};
use dereth_protocol::objects::{physics_state, ItemCreateObject, ItemSetState};
use dereth_protocol::{Message, Opcode};
use dereth_ui::framework::mode;
use winit::keyboard::KeyCode;

const REMOTE: ObjectId = ObjectId(0x5000_0F98);

fn body_position(app: &App) -> Position {
    app.world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position()
}

fn heading(p: Position) -> f32 {
    dereth_physics::math::get_heading(&p.frame)
}

fn turned(a: f32, b: f32) -> f32 {
    let mut d = b - a;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d
}

/// A gameplay App with a recorded player identity plus a second copy of that player's real setup,
/// positioned three metres away and therefore inside the object manager's activity radius.
fn setup() -> App {
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        width: 320,
        height: 240,
        sound: false,
        ui: true,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("required real DAT App and headless device");
    app.start_shell().expect("InputShell and shipped maps");
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
    app.queue_ui_mode(mode::GAME_PLAY);
    frames(&mut app, 60);

    let corpus = Corpus::load("early-inventory-and-casting")
        .expect("corpus decodes")
        .expect("early-inventory-and-casting");
    let player_row = corpus
        .blobs
        .iter()
        .find(|r| r.dir == Direction::ServerToClient && r.opcode == Opcode::LOGIN_CREATE_PLAYER.0)
        .expect("recorded player identity");
    let local = ObjectId(u32::from_le_bytes(
        player_row.payload[4..8].try_into().unwrap(),
    ));
    let row = corpus
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient
                && r.opcode == Opcode::ITEM_CREATE_OBJECT.0
                && u32::from_le_bytes(r.payload[4..8].try_into().unwrap()) == local.0
        })
        .expect("recorded player assets");
    let mut create = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
        .expect("recorded F745");
    assert_ne!(
        create.0.physicsdesc.state & physics_state::HIDDEN_PS,
        0,
        "the recorded login create has not yet reached its authoritative unhide"
    );
    let mut unhide = corpus
        .blobs
        .iter()
        .filter(|r| r.dir == Direction::ServerToClient && r.opcode == ItemSetState::OPCODE.0)
        .filter_map(|r| dereth_protocol::read_body_padded::<ItemSetState>(&r.payload[4..]).ok())
        .find(|m| m.id == local && m.state & physics_state::HIDDEN_PS == 0)
        .expect("early-inventory-and-casting's later authoritative login unhide");
    let here = body_position(&app);
    create.0.physicsdesc.position = Some(position_wire(here));
    create.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::POSITION;
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(local), LocalTime(1.0));
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("local create encodes"),
        },
        LocalTime(1.0),
    );

    let mut remote = create;
    remote.0.id = REMOTE;
    let mut there = here;
    there.frame.origin.x += 3.0;
    remote.0.physicsdesc.position = Some(position_wire(there));
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&remote).expect("remote create encodes"),
        },
        LocalTime(1.0),
    );
    unhide.id = REMOTE;
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_SET_STATE,
            body: dereth_protocol::write_body(&unhide).expect("recorded F74B unhide encodes"),
        },
        LocalTime(1.0),
    );
    frames(&mut app, 90);

    let scene = app.world_state().unwrap();
    assert!(
        scene.character.as_ref().unwrap().on_ground(),
        "real terrain supports the sender"
    );
    assert_eq!(scene.character.as_ref().unwrap().object_id(), local);
    let handle = app
        .objects()
        .physics
        .handle(REMOTE)
        .expect("remote player has a physics body");
    let remote_body = scene
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .expect("live remote body");
    assert!(remote_body.transient_state.in_contact() && remote_body.transient_state.on_walkable());
    app
}

fn position_wire(p: Position) -> dereth_protocol::types::PositionWire {
    dereth_protocol::types::PositionWire {
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
    }
}

/// The real Windows-message half available to safe tests. `winit::KeyEvent` itself has a private
/// platform field, so this begins at `Pump::key_message_for`, immediately after winit supplied the
/// physical key, and then follows the production InputShell/App path.
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
            time_ms: 980_000,
        }
    }

    fn arrow_left(&mut self, app: &mut App, down: bool) {
        self.time_ms += 10;
        let message = self
            .pump
            .key_message_for(KeyCode::ArrowLeft, down, self.time_ms)
            .expect("winit maps ArrowLeft to VK_LEFT and its extended scan code");
        self.pump.dispatch(message);
        app.input_manager_mut()
            .expect("input shell")
            .on_message(message);
    }
}

/// Captured outgoing F61C bodies from the App's actual socket-free replay transport.
struct Wire {
    reassembly: dereth_transport::indicator::Indicator,
    states: Vec<MovementMoveToState>,
}

impl Wire {
    fn attach(app: &mut App) -> Self {
        let mut net = dereth_client::net::ClientNetwork::new(
            "127.0.0.1:19000",
            7304,
            "remote-turn-station",
            "unused",
            0,
        )
        .unwrap();
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().unwrap()),
        );
        app.attach_replay_network(net)
            .expect("phase-owned socket-free endpoint");
        Self {
            reassembly: dereth_transport::indicator::Indicator::default(),
            states: Vec::new(),
        }
    }

    fn frames(&mut self, app: &mut App, count: usize) {
        for _ in 0..count {
            assert!(app.frame());
            self.observe(app);
        }
    }

    fn observe(&mut self, app: &mut App) {
        #[allow(clippy::cast_precision_loss)]
        let now = LocalTime(app.frames_drawn() as f64 * dereth_client::app::HEADLESS_STEP);
        for (bytes, _) in app
            .replay_network_mut()
            .expect("replay endpoint")
            .take_outgoing()
        {
            let packet = dereth_transport::ParsedPacket::parse(&bytes).expect("real output packet");
            for blob in
                self.reassembly
                    .check_in_packet(&packet.fragments, packet.header.rec_id, now)
            {
                if blob.payload.get(..4) != Some(0xF7B1_u32.to_le_bytes().as_slice()) {
                    continue;
                }
                let Ok(mut action) = unpack_action(&blob.payload) else {
                    continue;
                };
                if action.sub_type == MovementMoveToState::OPCODE {
                    self.states.push(
                        MovementMoveToState::read(&mut action.body).expect("actual F61C body"),
                    );
                }
            }
        }
    }
}

/// ACE `MovementData(Creature, MoveToState)`'s load-bearing turn conversion. The rest of the
/// fields are copied only enough to keep the same state complete; this test emits no actions,
/// forward movement, or sidestep.
fn ace_f74c(raw: &RawMotionState, id: ObjectId, instance: u16, movement_ts: u16) -> Vec<u8> {
    let index = |cmd: u32| {
        MotionCommand(cmd)
            .to_index()
            .unwrap_or_else(|| panic!("wire command {cmd:#010x}"))
    };
    let current_style = raw.current_style.map(index);
    let turn = raw.turn_command.map(|command| {
        let hold = raw.current_holdkey.unwrap_or(HoldKey::Invalid as u32);
        let mut speed = if hold == HoldKey::Run as u32 {
            1.5
        } else {
            1.0
        };
        if raw.turn_speed.is_some_and(|s| s != 0.0 && s <= 1.5) {
            speed = raw.turn_speed.unwrap();
        }
        if command == MotionCommand::TURN_LEFT.0 {
            speed *= -1.0;
        }
        (
            MotionCommand::TURN_RIGHT
                .to_index()
                .expect("TurnRight is shipped"),
            speed,
        )
    });
    let interpreted = InterpretedMotionState {
        current_style,
        turn_command: turn.map(|x| x.0),
        turn_speed: turn.map(|x| x.1),
        ..Default::default()
    };
    let buffer = MovementBuffer {
        movement_timestamp: movement_ts,
        server_control_timestamp: 0,
        autonomous: true,
        body: MovementBody {
            movement_type: movement_type::INVALID,
            current_style: current_style.unwrap_or(0),
            interpreted: Some(interpreted),
            ..Default::default()
        },
    };
    let message = MovementSetObjectMovement {
        id,
        instance_sequence: instance,
        movement: MovementSetObjectMovement::encode_movement(&buffer).expect("F74C movement"),
    };
    dereth_protocol::write_body(&message).expect("F74C body")
}

fn remote_heading(app: &App) -> f32 {
    heading(
        app.world_state()
            .unwrap()
            .server_object_position(REMOTE)
            .expect("remote position"),
    )
}

fn remote_physics(app: &App) -> (bool, bool) {
    let handle = app
        .objects()
        .physics
        .handle(REMOTE)
        .expect("remote physics handle");
    let body = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .expect("remote live body");
    (body.cell.is_some(), body.transient_state.is_active())
}

fn remote_body_sample(app: &App) -> (Position, f64, bool, bool, bool, bool) {
    let handle = app
        .objects()
        .physics
        .handle(REMOTE)
        .expect("remote physics handle");
    let body = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .expect("remote live body");
    (
        body.position,
        body.update_time(),
        body.transient_state.is_active(),
        body.motion.is_some(),
        body.state.is_hidden(),
        body.state.is_frozen(),
    )
}

/// Behaviour: movement.remote.an-arrow-key-release-clears-remote-turn-state
#[test]
fn arrow_key_release_bytes_clear_remote_turn_state_during_quiet_physics_ticks() {
    let mut app = setup();
    let mut wire = Wire::attach(&mut app);
    let mut hand = Hand::new();
    wire.frames(&mut app, 4);
    wire.states.clear(); // initial reporter state is real, but it predates this key.

    hand.arrow_left(&mut app, true);
    wire.frames(&mut app, 8);
    let pressed = wire
        .states
        .iter()
        .find(|m| m.0.raw_motion_state.turn_command == Some(MotionCommand::TURN_LEFT.0))
        .cloned()
        .expect("ArrowLeft down emits an F61C carrying TurnLeft");

    let after_press = wire.states.len();
    hand.arrow_left(&mut app, false);
    wire.frames(&mut app, 8);
    let released = wire.states[after_press..]
        .iter()
        .find(|m| m.0.raw_motion_state.turn_command.is_none())
        .cloned()
        .expect("ArrowLeft up emits a later F61C with no turn command");
    assert!(
        !app.probe().char_input().turn_left,
        "the same release stopped the local projection"
    );

    let presence = app.objects().presence(REMOTE).expect("remote presence");
    let instance = presence.instance;
    let press_ts = presence.movement_ts.wrapping_add(1);
    let stop_ts = press_ts.wrapping_add(1);
    for (raw, ts) in [
        (&pressed.0.raw_motion_state, press_ts),
        (&released.0.raw_motion_state, stop_ts),
    ] {
        app.probe_mut().objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
                body: ace_f74c(raw, REMOTE, instance, ts),
            },
            LocalTime(2.0),
        );
        // Keep the turning state alive long enough to cross multiple 30 Hz ticks before applying
        // the stop. The stop leg gets the same duration below as a quiet-frame stability window.
        if ts == press_ts {
            let h0 = remote_heading(&app);
            let mut saw_active = false;
            let mut saw_sequence_omega = false;
            let mut saw_animation_tick = false;
            let mut previous_frame = app
                .world_state()
                .unwrap()
                .server_object_sequence(REMOTE)
                .unwrap()
                .frame_number();
            for _ in 0..90 {
                frames(&mut app, 1);
                let (in_cell, active) = remote_physics(&app);
                assert!(
                    in_cell,
                    "the remote player left its physical cell during the premise"
                );
                saw_active |= active;
                // Interpreted turn root motion is returned as `SharedMotion::advance`'s frame
                // offset. `PhysicsObj::omega` is the independent F74E vector-update field.
                let sequence = app
                    .world_state()
                    .unwrap()
                    .server_object_sequence(REMOTE)
                    .unwrap();
                saw_sequence_omega |= sequence.omega != dereth_primitives::Vec3::ZERO;
                saw_animation_tick |= (sequence.frame_number() - previous_frame).abs() > 1e-5;
                previous_frame = sequence.frame_number();
            }
            let h1 = remote_heading(&app);
            assert_eq!(
                app.world_state().unwrap().server_object_turn(REMOTE),
                Some(MotionCommand::TURN_RIGHT),
                "ACE canonicalizes left to TurnRight with negative speed"
            );
            assert!(
                saw_active,
                "the positioned remote body never crossed an active physics tick"
            );
            assert!(
                saw_sequence_omega,
                "the real turn never reached the remote animation sequence"
            );
            assert!(
                saw_animation_tick,
                "the remote turn animation never advanced"
            );
            eprintln!("held turn:animation/omega active; published heading {h0} -> {h1}");
        }
    }

    frames(&mut app, 2); // admit/apply the stop and cross its first physics tick.
    assert_eq!(
        app.world_state().unwrap().server_object_turn(REMOTE),
        Some(MotionCommand::NONE),
        "the empty-turn F74C must clear the remote interpreted turn state"
    );
    let sequence = app
        .world_state()
        .unwrap()
        .server_object_sequence(REMOTE)
        .unwrap();
    assert_eq!(
        sequence.omega,
        dereth_primitives::Vec3::ZERO,
        "the stopped turn has no root omega"
    );
    let settled = remote_heading(&app);
    frames(&mut app, 30);
    let quiet = remote_heading(&app);
    assert_eq!(
        app.world_state().unwrap().server_object_turn(REMOTE),
        Some(MotionCommand::NONE),
        "quiet ticks must not restore the released turn command"
    );
    assert_eq!(
        app.world_state()
            .unwrap()
            .server_object_sequence(REMOTE)
            .unwrap()
            .omega,
        dereth_primitives::Vec3::ZERO,
        "quiet ticks must not restore the old turn root omega"
    );
    assert!(
        turned(settled, quiet).abs() < 0.01,
        "quiet 30 Hz physics ticks must not retain the old turn: {settled} -> {quiet}"
    );
}

#[test]
fn remote_turn_sequence_rotation_reaches_the_attached_body_and_published_scene_frame() {
    let mut app = setup();
    let presence = app.objects().presence(REMOTE).expect("remote presence");
    let body = ace_f74c(
        &RawMotionState {
            current_holdkey: Some(HoldKey::Run as u32),
            current_style: Some(MotionCommand::NON_COMBAT.0),
            turn_command: Some(MotionCommand::TURN_LEFT.0),
            turn_speed: Some(1.0),
            ..Default::default()
        },
        REMOTE,
        presence.instance,
        presence.movement_ts.wrapping_add(1),
    );
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
            body,
        },
        LocalTime(2.0),
    );
    frames(&mut app, 2); // admit the F74C and attach the shared driver to the live body.

    let mut saw_rotation_offset = false;
    for _ in 0..90 {
        let mut sequence = app
            .world_state()
            .unwrap()
            .server_object_sequence(REMOTE)
            .expect("real sequence");
        let (body_before, time_before, active_before, motion_before, hidden_before, frozen_before) =
            remote_body_sample(&app);
        frames(&mut app, 1);
        let (body_after, time_after, active_after, motion_after, hidden_after, frozen_after) =
            remote_body_sample(&app);
        let scene = app.world_state().unwrap();
        let published = scene
            .server_object_position(REMOTE)
            .expect("published remote position");
        assert_eq!(
            published, body_after,
            "finish_object_physics publishes the actual body frame"
        );

        let quantum = time_after - time_before;
        if quantum <= 0.0 {
            continue;
        }
        let mut offset = dereth_primitives::Frame::default();
        sequence.update(quantum, Some(&mut offset), &mut Vec::new());
        let expected = heading(Position::new(
            body_before.cell,
            dereth_animation::frame::combine(&body_before.frame, &offset),
        ));
        let requested = turned(heading(body_before), expected);
        if requested.abs() <= 0.01 {
            continue;
        }
        if !active_before {
            // `set_active(true, now)` restarts an inactive body's clock, so that frame deliberately
            // performs no internal position update. A manual sequence projection is not its oracle.
            continue;
        }
        saw_rotation_offset = true;
        let achieved = turned(heading(body_before), heading(body_after));
        assert!(
            active_after,
            "the in-range body went inactive across the measured turn tick"
        );
        assert!(
            motion_before && motion_after,
            "the shared motion adapter was absent on the tick"
        );
        assert!(
            !hidden_before && !hidden_after,
            "a hidden body does not advance its motion source"
        );
        assert!(
            !frozen_before && !frozen_after,
            "a frozen body does not enter the internal position update"
        );
        assert!(
            (achieved - requested).abs() < 0.01,
            "SharedMotion::advance requested {requested} degrees but the attached body achieved \
             {achieved} (sequence omega {:?}, quantum {quantum})",
            sequence.omega,
        );
    }
    assert!(
        saw_rotation_offset,
        "the real remote turn sequence produced no rotation offset"
    );
}
