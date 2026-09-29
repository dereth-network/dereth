//! A server approach (`Movement_SetObjectMovement 0xF74C` move-to) handed over to the player:
//! the approach runs through real `App` frames, a fresh movement key takes control back from the
//! server and cancels the old move-to in the same frame, and releasing the key leaves the body
//! stopped. An accepted interpreted replacement also ends the approach before installing its own
//! motion. Fixture: real terrain and animation from the retail dats, with the player's identity
//! and assets from early-inventory-and-casting's recorded create, placed on the test terrain;
//! the movement messages and input events are constructed. No socket, window or file writes.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_animation::MotionCommand;
use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::world::{SceneConfig, DEFAULT_LANDBLOCK};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, ObjectId, Position};
use dereth_protocol::movement::{
    MoveToArm, MovementBody, MovementBuffer, MovementParameters, MovementSetObjectMovement,
};
use dereth_protocol::objects::ItemCreateObject;
use dereth_protocol::{Message, Opcode};

fn frames(app: &mut App, count: usize) {
    for _ in 0..count {
        assert!(app.frame());
    }
}

fn position(app: &App) -> Position {
    app.world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .position()
}

fn setup() -> App {
    let mut app = App::new(Config {
        headless: true,
        width: 320,
        height: 240,
        sound: false,
        ui: false,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("required real DAT App and headless device");
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
    app.objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(id), LocalTime(1.0));
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&create).expect("constructed terrain placement"),
        },
        LocalTime(1.0),
    );
    unhide_the_player(&mut app, &corpus, id);
    frames(&mut app, 90);
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(c.on_ground(), "real terrain must support the local body");
    assert!(
        !c.world
            .get(c.handle)
            .expect("the local collision body")
            .state()
            .is_hidden(),
        "premise: the login create arrives with HIDDEN_PS and a hidden body never advances its \
         animation offset, so the recorded unhide has to have reached it"
    );
    assert!(!c.is_moving_to() && !c.driver().movement.motions_pending());
    app
}

/// early-inventory-and-casting's recorded `0xF745` for the player is the *login-tunnel* create: its
/// physics-state word is `0x00404410` — `HIDDEN_PS | GRAVITY_PS | IGNORE_COLLISIONS_PS |
/// EDGE_SLIDE_PS` — and retail unhides the body 6.7 s later with the `0xF74B Item_SetState` at
/// `t_rel = 24.562`, `state = 0x00400408`. The create's state word reaches the player's
/// physical body. While its hidden flag is set, the position update skips the part array's
/// animation offset outright, so a
/// station that replays the create and not the unhide drives a body that can never move.
/// The full byte-level note is in `movement/run_speed.rs`.
fn unhide_the_player(app: &mut App, corpus: &Corpus, id: ObjectId) {
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
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_SET_STATE,
            body: row.payload[4..].to_vec(),
        },
        LocalTime(1.0),
    );
}

fn approach(app: &mut App, metres: f32) -> Position {
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
    let accepted = app.objects().stats.movement_updates;
    app.objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
            body: dereth_protocol::write_body(&message).unwrap(),
        },
        LocalTime(3.0),
    );
    assert_eq!(app.objects().stats.movement_updates, accepted + 1);
    frames(app, 1);
    assert!(app.movement_commands().lists.controlled_by_server);
    assert!(app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .is_moving_to());
    goal
}

fn forward(app: &mut App, start: bool) {
    app.input_manager_mut()
        .unwrap()
        .inject_action(dereth_input::InputEvent {
            action: dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
            input_map: dereth_input::InputMapId(4),
            toggle: dereth_input::ToggleType::Hold,
            extent: 1.0,
            start,
            repeat_delta: 0,
            repeat_total: 0,
            from_key_down: start,
        });
    frames(app, 1);
}

fn manual_move_and_stop(app: &mut App) {
    let stopped = position(app);
    let retakes = app.movement_commands().control_retakes_from_commands;
    forward(app, true);
    assert!(!app.movement_commands().lists.controlled_by_server);
    assert_eq!(
        app.movement_commands().control_retakes_from_commands,
        retakes + 1
    );
    assert!(
        !app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .is_moving_to(),
        "the same input frame must cancel the old approach, not merely release its ownership"
    );
    frames(app, 90);
    assert!(
        dereth_animation::motion::moveto::distance(&stopped, &position(app)) > 1.0,
        "fresh input must move the SAME local collision body"
    );
    forward(app, false);
    frames(app, 60);
    let stopped = position(app);
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(!c.is_moving_to());
    assert_eq!(
        c.driver().movement.interp.raw_state.forward_command,
        MotionCommand::READY
    );
    frames(app, 60);
    assert!(
        dereth_animation::motion::moveto::distance(&stopped, &position(app)) < 0.05,
        "release must remain stopped, not resume the server approach"
    );
}

/// Behaviour: movement.approach.once-it-is-over-the-player-can-walk-the-body-himself-again
#[test]
fn app_approach_arrives_then_fresh_input_moves_and_releases_the_same_body() {
    let mut app = setup();
    let start = position(&app);
    let goal = approach(&mut app, 3.0);
    frames(&mut app, 360);
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(!c.is_moving_to() && !c.driver().movement.motions_pending());
    assert_eq!(c.stats.move_tos_failed, 0, "arrival is not cancellation");
    assert!(dereth_animation::motion::moveto::distance(&start, &c.position()) > 1.0);
    let clearance = dereth_animation::motion::moveto::cylinder_distance(
        c.radius(),
        c.height(),
        &c.position(),
        0.0,
        0.0,
        &goal,
    );
    assert!(
        clearance <= 0.6,
        "arrival must reach requested clearance: {clearance}"
    );
    assert!(
        app.movement_commands().lists.controlled_by_server,
        "retail deliberately retains ownership until a real held/fresh command exists"
    );
    manual_move_and_stop(&mut app);
    app.shutdown();
}

/// Behaviour: movement.approach.a-key-press-ends-an-approach-on-the-spot
#[test]
fn app_fresh_input_interrupts_an_unfinished_server_approach_and_stays_stopped_on_release() {
    let mut app = setup();
    approach(&mut app, 30.0);
    frames(&mut app, 15);
    assert!(app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .is_moving_to());
    manual_move_and_stop(&mut app);
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert_eq!(
        c.stats.last_move_to_error, 0x36,
        "explicit command cancellation"
    );
    app.shutdown();
}

/// Behaviour: movement.approach.a-replacement-ends-the-old-approach-before-the-new-one-is-installed
/// A constructed, accepted `0xF74C` interpreted-motion update (movement type 0) travels the
/// App's own path: object stream, then scene, then the local body's driver. READY and a moving
/// replacement both end the old approach before installing their state; a stale timestamp or
/// an autonomous echo of the player's own movement is refused and leaves the approach running.
#[test]
fn app_accepted_interpreted_replacement_cancels_before_installing_the_new_motion() {
    let mut app = setup();
    for command in [MotionCommand::READY, MotionCommand::WALK_FORWARD] {
        approach(&mut app, 30.0);
        frames(&mut app, 15);
        let player = app.objects().player().unwrap();
        let p = app.objects().presence(player).unwrap();
        let instance = p.instance;
        let movement_stamp = p.movement_ts;
        let server_stamp = p.server_control_ts;
        let failures = app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .stats
            .move_tos_failed;
        for control in 0..4 {
            let message = MovementSetObjectMovement {
                id: player,
                instance_sequence: instance,
                movement: MovementSetObjectMovement::encode_movement(&MovementBuffer {
                    movement_timestamp: if control == 0 {
                        movement_stamp.wrapping_sub(1)
                    } else {
                        movement_stamp.wrapping_add(control)
                    },
                    server_control_timestamp: if control == 0 {
                        server_stamp
                    } else {
                        server_stamp.wrapping_add(control)
                    },
                    autonomous: control == 1,
                    body: MovementBody {
                        current_style: MotionCommand::NON_COMBAT.to_index().unwrap(),
                        interpreted: Some(dereth_protocol::movement::InterpretedMotionState {
                            current_style: Some(MotionCommand::NON_COMBAT.to_index().unwrap()),
                            forward_command: Some(command.to_index().unwrap()),
                            forward_speed: Some(0.7),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                })
                .unwrap(),
            };
            let accepted = app.objects().stats.movement_updates;
            app.objects_mut().apply_event(
                &SessionEvent::WorldObject {
                    opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
                    body: dereth_protocol::write_body(&message).unwrap(),
                },
                LocalTime(4.0),
            );
            assert_eq!(
                app.objects().stats.movement_updates,
                accepted + u64::from(control >= 2)
            );
            frames(&mut app, 1);
            let c = app.world_state().unwrap().character.as_ref().unwrap();
            if control < 2 {
                assert!(
                    c.is_moving_to(),
                    "stale/autonomous controls preserve the approach"
                );
                assert_eq!(c.stats.move_tos_failed, failures);
            } else {
                assert!(
                    !c.is_moving_to(),
                    "accepted case 0 must cancel before replacement"
                );
                assert_eq!(
                    c.stats.move_tos_failed,
                    failures + 1,
                    "repeat replacement is idempotent"
                );
                assert_eq!(c.stats.last_move_to_error, 0x36);
                assert_eq!(
                    c.driver().movement.interp.interpreted_state.forward_command,
                    command
                );
                assert_eq!(
                    c.driver().movement.interp.interpreted_state.forward_speed,
                    0.7
                );
            }
        }
        if command == MotionCommand::READY {
            frames(&mut app, 60);
            let stopped = position(&app);
            frames(&mut app, 60);
            assert!(dereth_animation::motion::moveto::distance(&stopped, &position(&app)) < 0.05);
        } else {
            let start = position(&app);
            frames(&mut app, 90);
            assert!(
                dereth_animation::motion::moveto::distance(&start, &position(&app)) > 0.5,
                "cancellation must not stop the newly installed real animation/physics motion"
            );
        }
        manual_move_and_stop(&mut app);
    }
    app.shutdown();
}
