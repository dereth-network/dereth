//! Completing an accepted player teleport: the old approach and stick are cancelled once (a
//! duplicate does not cancel again), the run lock is turned off with its notice while a held key
//! survives, and each accepted teleport sends one `Movement_MoveToState 0xF61C` tail from its
//! destination with its own stamps. Same-batch creates, targets and movements keep their wire
//! order around the teleport. Fixture: early-inventory-and-casting's
//! accepted teleport edge, unchanged, on real terrain and bodies from the retail dats; the
//! motion before the teleport (autorun, approach, held key) is constructed.

use std::sync::Arc;

use dereth_animation::motion::{MoveToRequest, MovementParameters};
use dereth_animation::MotionCommand;
use dereth_client::app::{
    apply_player_teleport, body_motion, complete_player_teleport, player_timestamps,
};
use dereth_client::character::{Character, CharacterInput, MovementCommands};
use dereth_client::objects::ObjectStream;
use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction, MockTransport};
use dereth_client_net::client_session::{PositionReporter, Session, SessionEvent};
use dereth_primitives::{LocalTime, ObjectId, Position, Vec3};
use dereth_protocol::{Message, Opcode};

const TARGET: ObjectId = ObjectId(0x8000_0997);

fn event(row: &CorpusBlob) -> SessionEvent {
    SessionEvent::WorldObject {
        opcode: Opcode(row.opcode),
        body: row.payload[4..].to_vec(),
    }
}

/// The same ObjectStream accepted-edge producer as App, fed the recording's decoded blobs.
fn pending_teleport() -> (ObjectStream, Position, CorpusBlob) {
    let (mut objects, origin, edge) = before_teleport();
    objects.apply_event(
        &event(&edge),
        LocalTime(edge.t_rel_micros as f64 / 1_000_000.0),
    );
    (objects, origin, edge)
}

fn before_teleport() -> (ObjectStream, Position, CorpusBlob) {
    let corpus = Corpus::load("early-inventory-and-casting")
        .expect("corpus decodes")
        .expect("early-inventory-and-casting required");
    let created = corpus
        .blobs
        .iter()
        .find(|r| r.dir == Direction::ServerToClient && r.opcode == Opcode::LOGIN_CREATE_PLAYER.0)
        .expect("recorded player identity");
    let player = ObjectId(u32::from_le_bytes(
        created.payload[4..8].try_into().unwrap(),
    ));
    let mut objects = ObjectStream::new();
    objects.apply_event(&SessionEvent::PlayerCreated(player), LocalTime(0.0));
    for row in corpus.blobs.iter().filter(|r| {
        r.dir == Direction::ServerToClient
            && ([
                Opcode::ITEM_CREATE_OBJECT.0,
                Opcode::MOVEMENT_POSITION_EVENT.0,
                Opcode::EFFECTS_PLAYER_TELEPORT.0,
            ]
            .contains(&r.opcode)
                // The player's own state edges, in wire order, because the create above is the
                // *login-tunnel* create: early-inventory-and-casting's `0xF745` for the player
                // carries `state = 0x00404410` — `HIDDEN_PS | GRAVITY_PS | IGNORE_COLLISIONS_PS |
                // EDGE_SLIDE_PS` — and retail unhides him 6.7 s later with the `0xF74B` at idx 95,
                // `t_rel 24.562`, `state = 0x00400408`. Object creation applies that state word
                // to the player's own physical body with no exemption. While the hidden flag
                // is set, its position update skips the part array's whole animation offset,
                // so a body left wearing that word can neither walk nor turn. Without the
                // `0xF74B`, every station below would stand on a player still hidden 50 s after
                // leaving the login tunnel. The full byte-level note is in `movement/run_speed.rs`.
                //
                // **Only the player's.** early-inventory-and-casting carries two `0xF74B` before
                // this station's return point (idx 324): the player's unhide at idx 95 and the
                // academy door's `0x0001001C` at idx 149. The door's belongs to the door station
                // (`movement::door_turn_and_open`), so it stays out. The teleport's own hide is
                // idx 325 — *after* the position event this loop returns on — so it is out of
                // reach either way and this station hands out an unhidden, pre-teleport body.
                || (r.opcode == Opcode::ITEM_SET_STATE.0
                    && u32::from_le_bytes(r.payload[4..8].try_into().expect("an id")) == player.0))
    }) {
        let before = objects.presence(player).and_then(|p| p.position);
        if row.opcode == Opcode::MOVEMENT_POSITION_EVENT.0
            && u32::from_le_bytes(row.payload[4..8].try_into().unwrap()) == player.0
        {
            let wire = dereth_protocol::movement::MovementPositionEvent::read(
                &mut dereth_protocol::Reader::new(&row.payload[4..]),
            )
            .expect("recorded player position");
            if wire.position.teleport_timestamp == 1 {
                assert_eq!(before.unwrap().cell.0, 0x7F03_01B0);
                assert_eq!(
                    objects
                        .physics_state(player)
                        .map(|w| w & dereth_physics::PhysicsState::HIDDEN_PS),
                    Some(0),
                    "premise: early-inventory-and-casting's player create is the login-tunnel create \
                     (state 0x00404410, HIDDEN_PS set) and the recorded unhide is idx 95 at \
                     t_rel 24.562; the body this station hands out stands 50 s after that, so it \
                     must not still be wearing the tunnel word"
                );
                return (objects, before.unwrap(), row.clone());
            }
        }
        objects.apply_event(
            &event(row),
            LocalTime(row.t_rel_micros as f64 / 1_000_000.0),
        );
        assert!(
            objects.take_player_teleport().is_none(),
            "selected first teleport, not an old latch"
        );
    }
    panic!("early-inventory-and-casting accepted teleport absent");
}

fn body_at(pos: Position, id: ObjectId) -> Character {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let region = dereth_client::world::load_region(&store).expect("region");
    let block = pos.cell.landblock();
    let mut c = Character::new(
        &store,
        &region,
        (u16::from(block.x()) << 8) | u16::from(block.y()),
        (96.0, 96.0),
    )
    .expect("real DAT body");
    c.adopt_server_id(Some(id));
    c.land().load_block_cells(block);
    c.teleport(pos);
    for i in 0..=60 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the origin room must really support the body"
    );
    c
}

fn approach(c: &mut Character) {
    let mut target = c.position();
    target.frame.origin.x += 8.0;
    c.perform_move_to(
        &MoveToRequest::MoveToObject {
            object_id: TARGET,
            top_level_id: TARGET,
            radius: 0.0,
            height: 0.0,
        },
        &MovementParameters::default(),
        Some(1.0),
    );
    c.update_target(target, Vec3::ZERO, true);
    assert!(c.is_moving_to());
    assert!(c.driver().movement.moveto.initialized);
}

#[test]
fn accepted_teleport_cancels_old_approach_and_sticky_without_repeating_on_duplicate() {
    let (mut objects, origin, edge) = pending_teleport();
    let mut c = body_at(origin, objects.player().unwrap());
    approach(&mut c);
    c.stick_to_object(TARGET, 0.0, 0.0);
    assert_eq!(c.sticky_target(), Some(TARGET));
    assert!(c.wanted_target().is_some());
    let failed_before = c.stats.move_tos_failed;
    let destination = apply_player_teleport(&mut objects, &mut c).expect("accepted teleport");
    assert_eq!(destination.cell.0, 0xDA55_001D);
    assert_eq!(c.position(), destination);
    assert!(
        !c.is_moving_to(),
        "teleport_hook cancels the old approach before it can run in the destination"
    );
    assert_eq!(c.stats.move_tos_failed, failed_before + 1);
    assert_eq!(c.stats.last_move_to_error, 0x3c);
    assert_eq!(c.sticky_target(), None);
    assert!(c.wanted_target().is_none());
    assert_eq!(
        c.driver().movement.interp.raw_state.forward_command,
        MotionCommand::READY
    );

    // A duplicate position/teleport cannot cancel a newly started post-teleport approach.
    approach(&mut c);
    let failed_after = c.stats.move_tos_failed;
    objects.apply_event(&event(&edge), LocalTime(75.0));
    assert!(apply_player_teleport(&mut objects, &mut c).is_none());
    assert!(c.is_moving_to());
    assert_eq!(c.stats.move_tos_failed, failed_after);
}

fn command(mc: &mut MovementCommands, input: &mut CharacterInput, cmd: MotionCommand, start: bool) {
    assert!(mc.on_action(
        dereth_client_runtime::actions::movement::MovementAction::SetMotion(
            dereth_client_runtime::actions::movement::CmdStruct {
                command: cmd.0,
                extent: None,
                start: Some(start)
            },
        ),
        input
    ));
}

fn tick(c: &mut Character, mc: &mut MovementCommands, input: &mut CharacterInput, now: f64) {
    if mc.take_control_retake_pending()
        || mc.use_time(
            c.driver().movement.interp.motions_pending(),
            c.is_moving_to(),
            input,
        )
    {
        c.take_control_from_server();
    }
    c.input = *input;
    c.update(LocalTime(now));
}

fn xy_distance(a: Position, b: Position) -> f32 {
    let a = a.frame.origin;
    let b = b.frame.origin;
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

/// Behaviour: notice.autorun.turning-the-run-lock-on-or-off-says-so-in-the-message-window
#[test]
fn autorun_is_off_in_the_explicit_destination_payload_then_fresh_manual_move_and_stop_work() {
    let (mut objects, origin, edge) = pending_teleport();
    let player = objects.player().unwrap();
    let mut c = body_at(origin, player);
    let mut mc = MovementCommands::default();
    let mut input = CharacterInput::default();
    command(
        &mut mc,
        &mut input,
        MotionCommand(dereth_client_runtime::actions::movement::command::AUTO_RUN),
        true,
    );
    assert!(mc.lists.auto_run && input.forward);
    let before_run = c.position();
    for i in 61..=90 {
        tick(&mut c, &mut mc, &mut input, f64::from(i) / 30.0);
    }
    assert_eq!(
        c.driver().movement.interp.raw_state.forward_command,
        MotionCommand::WALK_FORWARD
    );
    assert!(
        xy_distance(before_run, c.position()) > 0.05,
        "real DAT autorun moved the body before teleport"
    );
    assert_eq!(mc.take_notices(), vec!["AutoRun ON"]);

    let stamps = player_timestamps(objects.presence(player));
    let mut reporter = PositionReporter::new(0.0);
    let mut session = Session::new(MockTransport::new());
    let destination = complete_player_teleport(&mut objects, &mut c, &mut mc, &mut input, |c| {
        assert!(
            !c.input.auto_run && !c.input.forward,
            "send follows actual input reapplication"
        );
        assert_eq!(
            c.driver().movement.interp.raw_state.forward_command,
            MotionCommand::READY
        );
        reporter
            .send_movement_event(74.356, &body_motion(c, stamps), &mut session)
            .expect("emitted");
    })
    .expect("accepted teleport");
    assert!(!mc.lists.auto_run && !input.auto_run && !input.forward);
    assert_eq!(mc.take_notices(), vec!["AutoRun OFF"]);
    assert_eq!(
        session.transport.sent.len(),
        1,
        "explicit teleport-completion tail, not a later poll"
    );
    let action =
        dereth_protocol::actions::unpack_action(&session.transport.sent[0].payload).unwrap();
    assert_eq!(action.sub_type, Opcode::MOVEMENT_MOVE_TO_STATE);
    let packet =
        dereth_protocol::movement::MovementMoveToState::read(&mut action.body.clone()).unwrap();
    assert_eq!(packet.0.position.objcell_id, destination.cell.0);
    assert_eq!(packet.0.timestamps.teleport, 1);
    assert_eq!(
        packet.0.raw_motion_state.forward_command, None,
        "Ready is omitted on the wire"
    );

    for i in 1..=60 {
        tick(&mut c, &mut mc, &mut input, 74.356 + f64::from(i) / 30.0);
    }
    let stopped = c.position();
    assert!(c.on_ground());
    for i in 61..=90 {
        tick(&mut c, &mut mc, &mut input, 74.356 + f64::from(i) / 30.0);
    }
    assert!(
        xy_distance(stopped, c.position()) < 0.001,
        "autorun cannot resume at the destination"
    );
    command(&mut mc, &mut input, MotionCommand::WALK_FORWARD, true);
    for i in 91..=150 {
        tick(&mut c, &mut mc, &mut input, 74.356 + f64::from(i) / 30.0);
    }
    assert!(
        xy_distance(stopped, c.position()) > 1.0,
        "fresh manual input physically moves after portal"
    );
    command(&mut mc, &mut input, MotionCommand::WALK_FORWARD, false);
    for i in 151..=210 {
        tick(&mut c, &mut mc, &mut input, 74.356 + f64::from(i) / 30.0);
    }
    let released = c.position();
    for i in 211..=240 {
        tick(&mut c, &mut mc, &mut input, 74.356 + f64::from(i) / 30.0);
    }
    assert!(
        xy_distance(released, c.position()) < 0.001,
        "manual release stops without an old approach restarting"
    );

    // A normal newer POSITION_TS with the same TELEPORT_TS does not run the completion tail.
    let mut position = dereth_protocol::movement::MovementPositionEvent::read(
        &mut dereth_protocol::Reader::new(&edge.payload[4..]),
    )
    .unwrap();
    position.position.position_timestamp = position.position.position_timestamp.wrapping_add(1);
    let mut writer = dereth_protocol::Writer::new();
    position.write(&mut writer).unwrap();
    objects.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_POSITION_EVENT,
            body: writer.into_inner(),
        },
        LocalTime(83.0),
    );
    assert!(
        complete_player_teleport(&mut objects, &mut c, &mut mc, &mut input, |_| panic!(
            "non-teleport position must not emit teleport-completion movement"
        ))
        .is_none()
    );

    position.position.position_timestamp = position.position.position_timestamp.wrapping_add(1);
    position.position.teleport_timestamp = 0;
    let mut writer = dereth_protocol::Writer::new();
    position.write(&mut writer).unwrap();
    objects.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_POSITION_EVENT,
            body: writer.into_inner(),
        },
        LocalTime(84.0),
    );
    assert!(
        complete_player_teleport(&mut objects, &mut c, &mut mc, &mut input, |_| panic!(
            "stale TELEPORT_TS must not emit teleport-completion movement"
        ))
        .is_none()
    );
}

#[test]
fn teleport_tail_sends_even_with_autorun_already_off_and_preserves_a_held_key_when_turning_it_off()
{
    for held in [false, true] {
        let (mut objects, origin, _) = pending_teleport();
        let player = objects.player().unwrap();
        let mut c = body_at(origin, player);
        let mut mc = MovementCommands::default();
        let mut input = CharacterInput::default();
        if held {
            command(&mut mc, &mut input, MotionCommand::WALK_FORWARD, true);
            command(
                &mut mc,
                &mut input,
                MotionCommand(dereth_client_runtime::actions::movement::command::AUTO_RUN),
                true,
            );
            for i in 61..=90 {
                tick(&mut c, &mut mc, &mut input, f64::from(i) / 30.0);
            }
            assert_eq!(mc.lists.substate.len(), 1);
            assert!(input.forward && input.auto_run);
            mc.take_notices();
        }
        let stamps = player_timestamps(objects.presence(player));
        let mut reporter = PositionReporter::new(0.0);
        let mut session = Session::new(MockTransport::new());
        let before = body_motion(&c, stamps);
        reporter
            .send_movement_event(73.0, &before, &mut session)
            .expect("prime unchanged raw state");
        session.transport.sent.clear();
        complete_player_teleport(&mut objects, &mut c, &mut mc, &mut input, |c| {
            let now = body_motion(c, stamps);
            assert!(
                !reporter.movement_state_changed(&now),
                "raw command need not change at teleport"
            );
            reporter
                .send_movement_event(74.356, &now, &mut session)
                .expect("unconditional tail");
        })
        .expect("accepted teleport");
        assert_eq!(session.transport.sent.len(), 1);
        assert!(!input.auto_run && !mc.lists.auto_run);
        assert_eq!(input.forward, held);
        assert_eq!(mc.lists.substate.len(), usize::from(held));
        assert_eq!(
            c.driver().movement.interp.raw_state.forward_command,
            if held {
                MotionCommand::WALK_FORWARD
            } else {
                MotionCommand::READY
            }
        );
        assert_eq!(
            mc.take_notices(),
            if held { vec!["AutoRun OFF"] } else { vec![] }
        );
    }
}

/// Behaviour: movement.teleport.an-accepted-teleport-cancels-the-approach-sticky-and-autorun
/// Behaviour: movement.teleport.every-drawn-frame-applies-a-teleport-before-it-reports-a-position
#[test]
fn actual_app_teleport_step_clears_autorun_and_the_old_move_to() {
    use dereth_client::config::Config;
    use dereth_client::world::SceneConfig;
    let (objects, origin, edge) = before_teleport();
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        frames: None,
        sound: false,
        ui: false,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("required headless simulated App and retail DATs");
    app.start_shell().expect("input shell");
    let block = origin.cell.landblock();
    app.load_static_scene(SceneConfig {
        landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("actual App world");
    {
        let c = app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap();
        c.land().load_block_cells(block);
        c.teleport(origin);
    }
    // Finish the recorded earlier creates (including their embedded motion) before the
    // constructed autorun/approach. Replaying those earlier commands only after arming runlock
    // would test control loss clearing autorun, not this accepted teleport edge.
    *app.probe_mut().objects_mut() = objects;
    assert!(app.frame());
    assert_eq!(app.probe().player_teleports_applied(), 0);
    app.input_manager_mut()
        .expect("input exists")
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
    assert!(app.frame());
    assert!(
        app.probe().movement_commands().lists.auto_run && app.probe().char_input().forward,
        "the real App input route must own an active run lock"
    );
    {
        let c = app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap();
        approach(c);
        c.stick_to_object(TARGET, 0.0, 0.0);
    }
    // Constructed pre-teleport activity, unchanged recorded accepted position edge.
    app.probe_mut()
        .objects_mut()
        .apply_event(&event(&edge), LocalTime(74.356));
    assert!(app.frame());
    assert_eq!(app.probe().player_teleports_applied(), 1);
    assert!(!app.probe().movement_commands().lists.auto_run && !app.probe().char_input().forward);
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert_eq!(
        c.position().cell.landblock(),
        dereth_primitives::CellId(0xDA55_001D).landblock()
    );
    assert!(!c.is_moving_to());
    assert_eq!(c.sticky_target(), None);
    assert!(c.wanted_target().is_none());
    assert_eq!(c.stats.last_move_to_error, 0x3c);
    assert_eq!(
        c.driver().movement.interp.raw_state.forward_command,
        MotionCommand::READY
    );
    assert_eq!(
        app.probe().position_reporter_stats().movement_events,
        0,
        "no live session is fabricated"
    );
    app.shutdown();
}

/// Synthetic command ordering around an unchanged captured teleport. Object-event dispatch
/// is synchronous: movement accepted before the teleport is canceled by teleport completion;
/// a command accepted afterward is not.
#[test]
fn actual_app_preserves_accepted_movement_and_teleport_order_in_one_batch() {
    use dereth_client::{config::Config, world::SceneConfig};
    use dereth_protocol::movement::{
        movement_type, MoveToArm, MovementBody, MovementBuffer, MovementPositionEvent,
        MovementSetObjectMovement,
    };
    for movement_after in [false, true] {
        let (objects, origin, edge) = before_teleport();
        let mut app = crate::common::sim_app::new(Config {
            headless: true,
            frames: None,
            sound: false,
            ui: false,
            dat_dir: dereth_dat::testing::dat_dir(),
            ..Default::default()
        })
        .expect("required headless simulated App and retail DATs");
        let block = origin.cell.landblock();
        app.load_static_scene(SceneConfig {
            landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
            character: true,
            land_radius: 1,
            scenery_radius: 0,
            cell_statics: false,
            mesh_collision: false,
            particles: false,
            ..Default::default()
        })
        .expect("actual App world");
        app.probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap()
            .teleport(origin);
        *app.probe_mut().objects_mut() = objects;
        assert!(
            app.frame(),
            "create/initial motion precede the constructed batch"
        );
        assert_eq!(app.probe().player_teleports_applied(), 0);

        let player = app.objects().player().unwrap();
        let p = app.objects().presence(player).unwrap();
        let position =
            MovementPositionEvent::read(&mut dereth_protocol::Reader::new(&edge.payload[4..]))
                .unwrap();
        let mut target = position.position.origin;
        target.origin.x += 8.0;
        let params = MovementParameters::default();
        let mut buffer = MovementBuffer {
            movement_timestamp: p.movement_ts.wrapping_add(1),
            server_control_timestamp: p.server_control_ts.wrapping_add(1),
            autonomous: false,
            body: MovementBody {
                movement_type: movement_type::MOVE_TO_POSITION,
                current_style: MotionCommand::HAND_COMBAT.to_index().unwrap(),
                unhandled: MovementBody::encode_move_to(&MoveToArm::MoveToPosition {
                    origin: target,
                    params: dereth_protocol::movement::MovementParameters::MoveTo {
                        bitfield: params.flags,
                        distance_to_object: params.distance_to_object,
                        min_distance: params.min_distance,
                        fail_distance: params.fail_distance,
                        speed: params.speed,
                        walk_run_threshold: params.walk_run_threshold,
                        desired_heading: params.desired_heading,
                    },
                    run_rate: 1.0,
                }),
                ..Default::default()
            },
        };
        let mut message = MovementSetObjectMovement {
            id: player,
            instance_sequence: p.instance,
            movement: MovementSetObjectMovement::encode_movement(&buffer).unwrap(),
        };
        let movement = SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
            body: dereth_protocol::write_body(&message).unwrap(),
        };
        buffer.body.current_style = MotionCommand::SWORD_COMBAT.to_index().unwrap();
        message.movement = MovementSetObjectMovement::encode_movement(&buffer).unwrap();
        let rejected_movement = SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
            body: dereth_protocol::write_body(&message).unwrap(),
        };
        let failed_before = app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .stats
            .move_tos_failed;
        let performed_before = app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .stats
            .move_tos_performed;
        let losses_before = app.probe().control_transfer_counts().1;
        let ordered = if movement_after {
            [event(&edge), movement]
        } else {
            [movement, event(&edge)]
        };
        for e in ordered {
            app.probe_mut()
                .objects_mut()
                .apply_event(&e, LocalTime(74.356));
        }
        // Both rejects occur after the valid command. They must not erase or re-order it.
        app.probe_mut()
            .objects_mut()
            .apply_event(&event(&edge), LocalTime(74.357));
        app.probe_mut()
            .objects_mut()
            .apply_event(&rejected_movement, LocalTime(74.358));
        let mut stale_teleport = position;
        stale_teleport.position.position_timestamp =
            stale_teleport.position.position_timestamp.wrapping_add(1);
        stale_teleport.position.teleport_timestamp =
            stale_teleport.position.teleport_timestamp.wrapping_sub(1);
        app.probe_mut().objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::MOVEMENT_POSITION_EVENT,
                body: dereth_protocol::write_body(&stale_teleport).unwrap(),
            },
            LocalTime(74.359),
        );
        assert!(app.frame());
        assert_eq!(
            app.probe().player_teleports_applied(),
            1,
            "only the accepted edge completes"
        );
        assert_eq!(
            app.probe().control_transfer_counts().1,
            losses_before + 1,
            "the accepted loss is counted once at dispatch"
        );
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert_eq!(
            c.stats.move_tos_performed,
            performed_before + 1,
            "the old command is applied, not discarded"
        );
        assert_eq!(
            c.is_moving_to(),
            movement_after,
            "movement_after={movement_after}: accepted packet order owns cancellation"
        );
        assert_eq!(
            c.stats.move_tos_failed,
            failed_before + u64::from(!movement_after)
        );
        if !movement_after {
            assert_eq!(c.stats.last_move_to_error, 0x3c);
        }
        assert_eq!(
            c.driver().movement.interp.interpreted_state.current_style,
            MotionCommand::HAND_COMBAT,
            "the earlier command's style survives; the rejected style never applies"
        );
        assert!(
            app.objects()
                .presence(player)
                .unwrap()
                .pending_movement
                .is_none(),
            "no second consumer may replay the command"
        );
        assert!(
            app.frame(),
            "a following frame cannot redispatch accepted edges"
        );
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert_eq!(c.stats.move_tos_performed, performed_before + 1);
        assert_eq!(app.probe().player_teleports_applied(), 1);
        app.shutdown();
    }
}

/// Recorded movement shape with explicitly constructed local identity/control stamps. These are
/// stimuli, not a claim that the recording performed these three commands around two portals.
fn stamped_movement(objects: &ObjectStream) -> SessionEvent {
    use dereth_protocol::movement::MovementSetObjectMovement;
    let corpus = Corpus::load("long-solo-play")
        .expect("corpus decodes")
        .expect("long-solo-play required");
    let row = corpus
        .blobs
        .iter()
        .find(|r| {
            r.dir == Direction::ServerToClient && r.opcode == Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
        })
        .expect("recorded movement shape");
    let mut message =
        MovementSetObjectMovement::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
            .unwrap();
    let id = objects.player().unwrap();
    let p = objects.presence(id).unwrap();
    let mut buffer = message.decoded_movement().unwrap();
    message.id = id;
    message.instance_sequence = p.instance;
    buffer.autonomous = false;
    buffer.movement_timestamp = p.movement_ts.wrapping_add(1);
    buffer.server_control_timestamp = p.server_control_ts.wrapping_add(1);
    message.movement = MovementSetObjectMovement::encode_movement(&buffer).unwrap();
    SessionEvent::WorldObject {
        opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
        body: dereth_protocol::write_body(&message).unwrap(),
    }
}
fn lifecycle_app(origin: Position) -> dereth_client::app::App {
    use dereth_client::{config::Config, world::SceneConfig};
    let mut app = crate::common::sim_app::new(Config {
        headless: true,
        frames: None,
        sound: false,
        ui: false,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Default::default()
    })
    .expect("required headless simulated App and retail DATs");
    let block = origin.cell.landblock();
    app.load_static_scene(SceneConfig {
        landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
        character: true,
        land_radius: 1,
        scenery_radius: 0,
        cell_statics: false,
        mesh_collision: false,
        particles: false,
        ..Default::default()
    })
    .expect("actual App world");
    let c = app
        .probe_mut()
        .world_state_mut()
        .unwrap()
        .character
        .as_mut()
        .unwrap();
    c.land().load_block_cells(block);
    c.teleport(origin);
    for i in 0..=8 {
        c.update(LocalTime(f64::from(i) / 30.0));
    }
    assert!(
        c.on_ground(),
        "the real body is grounded before the synthetic batch"
    );
    app
}

/// Constructed lifecycle stations use a real captured setup/table recipe but new identities and
/// stamps. They are not claims that these two commands occurred in the recording.
fn lifecycle_movement(
    objects: &ObjectStream,
    origin: Position,
    target: Option<ObjectId>,
) -> SessionEvent {
    use dereth_protocol::movement::{
        movement_type, MoveToArm, MovementBody, MovementBuffer, MovementSetObjectMovement,
    };
    let player = objects.player().unwrap();
    let p = objects.presence(player).unwrap();
    let mut origin: dereth_protocol::types::Origin = dereth_protocol::types::Origin {
        objcell_id: origin.cell.0,
        origin: dereth_protocol::types::Vec3 {
            x: origin.frame.origin.x,
            y: origin.frame.origin.y,
            z: origin.frame.origin.z,
        },
    };
    origin.origin.x += 8.0;
    let pms = MovementParameters::default();
    let params = dereth_protocol::movement::MovementParameters::MoveTo {
        bitfield: pms.flags,
        distance_to_object: pms.distance_to_object,
        min_distance: pms.min_distance,
        fail_distance: pms.fail_distance,
        speed: pms.speed,
        walk_run_threshold: pms.walk_run_threshold,
        desired_heading: pms.desired_heading,
    };
    let arm = match target {
        Some(target) => MoveToArm::MoveToObject {
            target,
            origin,
            params,
            run_rate: 1.0,
        },
        None => MoveToArm::MoveToPosition {
            origin,
            params,
            run_rate: 1.0,
        },
    };
    let buffer = MovementBuffer {
        movement_timestamp: p.movement_ts.wrapping_add(1),
        server_control_timestamp: p.server_control_ts.wrapping_add(1),
        autonomous: false,
        body: MovementBody {
            movement_type: if target.is_some() {
                movement_type::MOVE_TO_OBJECT
            } else {
                movement_type::MOVE_TO_POSITION
            },
            current_style: MotionCommand::NON_COMBAT.to_index().unwrap(),
            unhandled: MovementBody::encode_move_to(&arm),
            ..Default::default()
        },
    };
    SessionEvent::WorldObject {
        opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
        body: dereth_protocol::write_body(&MovementSetObjectMovement {
            id: player,
            instance_sequence: p.instance,
            movement: MovementSetObjectMovement::encode_movement(&buffer).unwrap(),
        })
        .unwrap(),
    }
}
#[test]
fn actual_app_prepares_a_same_batch_player_create_before_its_movement() {
    use dereth_client::character::{ALUVIAN_MALE_MOTION_TABLE, ALUVIAN_MALE_SETUP};
    use dereth_protocol::objects::ItemCreateObject;
    let (_, origin, _) = before_teleport();
    let mut app = lifecycle_app(origin);
    let player = ObjectId(0x7000_0001);
    let mut recipe = [
        "first-login-walk-jump",
        "early-inventory-and-casting",
        "short-second-connection",
        "login-account-booted",
        "ddd-interrogation-only",
        "long-solo-play",
    ]
    .into_iter()
    .flat_map(|name| Corpus::load(name).unwrap().unwrap().blobs)
    .filter(|r| r.dir == Direction::ServerToClient && r.opcode == Opcode::ITEM_CREATE_OBJECT.0)
    .map(|r| ItemCreateObject::read(&mut dereth_protocol::Reader::new(&r.payload[4..])).unwrap())
    .find(|c| {
        c.0.physicsdesc
            .setup_id
            .is_some_and(|s| s != ALUVIAN_MALE_SETUP.0)
            && c.0
                .physicsdesc
                .mtable_id
                .is_some_and(|m| m != 0 && m != ALUVIAN_MALE_MOTION_TABLE.0)
    })
    .expect("captured recipe changes both setup and movement-manager table");
    recipe.0.id = player;
    recipe.0.physicsdesc.movement = None;
    recipe.0.physicsdesc.bitfield &= !dereth_protocol::types::physicsdesc::flags::MOVEMENT;
    let want_setup = dereth_primitives::DataId(recipe.0.physicsdesc.setup_id.unwrap());
    let want_table = dereth_primitives::DataId(recipe.0.physicsdesc.mtable_id.unwrap());
    let parts_before: Vec<_> = app
        .world_state()
        .unwrap()
        .character
        .as_ref()
        .unwrap()
        .driver()
        .part_array
        .parts
        .iter()
        .map(|p| p.gfxobj_id)
        .collect();
    app.probe_mut()
        .objects_mut()
        .apply_event(&SessionEvent::PlayerCreated(player), LocalTime(0.0));
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&recipe).unwrap(),
        },
        LocalTime(0.0),
    );
    let movement = lifecycle_movement(app.objects(), origin, None);
    app.probe_mut()
        .objects_mut()
        .apply_event(&movement, LocalTime(0.0));
    assert!(app.frame());
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert_eq!(c.setup_id(), want_setup);
    assert_eq!(c.motion_table_id(), want_table);
    assert_eq!(c.stats.setup_changes, 1);
    assert_ne!(
        c.driver()
            .part_array
            .parts
            .iter()
            .map(|p| p.gfxobj_id)
            .collect::<Vec<_>>(),
        parts_before,
        "actual local part array rebuilt"
    );
    assert_eq!(c.stats.move_tos_performed, 1);
    // The later command reaches the create's *new* movement manager and is answered by it.
    // Stopping drains its own completed motions, so the move-to is not left parked behind the
    // READY queued by entering the default state: the plan runs at once, and this recipe's table
    // `0x09000202` (substate `Off`) has no turn cycle, so trying to turn returns
    // `BAD_MOVEMENT_COMMAND (0x43)` and cancels the move-to with that error, the new table's
    // own verdict delivered inside the same frame.
    assert!(
        !c.is_moving_to()
            && c.stats.move_tos_failed == 1
            && c.stats.last_move_to_error == dereth_animation::table::manager::BAD_MOVEMENT_COMMAND,
        "later command must reach the create's new movement manager and be answered by its \
         table: moving {} failed {} err 0x{:X} table {:?} substate {:?}",
        c.is_moving_to(),
        c.stats.move_tos_failed,
        c.stats.last_move_to_error,
        c.motion_table_id(),
        c.driver().motion_table.state.substate
    );
    assert!(app
        .objects()
        .presence(player)
        .unwrap()
        .pending_movement
        .is_none());
    app.shutdown();
}
#[test]
fn actual_app_prepares_a_same_batch_new_target_and_its_bounds_before_move_to_object() {
    use dereth_protocol::objects::ItemCreateObject;
    let (objects, origin, _) = before_teleport();
    let mut app = lifecycle_app(origin);
    *app.probe_mut().objects_mut() = objects;
    assert!(app.frame());
    let target = ObjectId(0x7000_0002);
    assert!(app.objects().presence(target).is_none());
    let corpus = Corpus::load("early-inventory-and-casting")
        .unwrap()
        .unwrap();
    let mut recipe = corpus
        .blobs
        .iter()
        .filter(|r| r.dir == Direction::ServerToClient && r.opcode == Opcode::ITEM_CREATE_OBJECT.0)
        .map(|r| {
            ItemCreateObject::read(&mut dereth_protocol::Reader::new(&r.payload[4..])).unwrap()
        })
        .find(|c| Some(c.0.id) == app.objects().player())
        .unwrap();
    recipe.0.id = target;
    recipe.0.physicsdesc.movement = None;
    recipe.0.physicsdesc.bitfield &= !dereth_protocol::types::physicsdesc::flags::MOVEMENT;
    let pos = recipe
        .0
        .physicsdesc
        .position
        .as_mut()
        .expect("recorded player world position");
    pos.objcell_id = origin.cell.0;
    pos.frame.origin = dereth_protocol::types::Vec3 {
        x: origin.frame.origin.x + 8.0,
        y: origin.frame.origin.y,
        z: origin.frame.origin.z,
    };
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&recipe).unwrap(),
        },
        LocalTime(0.0),
    );
    let movement = lifecycle_movement(app.objects(), origin, Some(target));
    app.probe_mut()
        .objects_mut()
        .apply_event(&movement, LocalTime(0.0));
    assert!(app.frame());
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    let driver = c.driver();
    let move_to = &driver.movement.moveto;
    assert_eq!(
        move_to.movement_type,
        dereth_animation::table::MovementType::MoveToObject,
        "a target created earlier in the batch is not the missing-target position fallback"
    );
    assert_eq!(move_to.sought_object_id, target);
    let handle = app
        .objects()
        .physics
        .handle(target)
        .expect("actual target physics body");
    let body = c.world.get(handle).unwrap();
    assert!(
        body.radius() > 0.0 && body.height() > 0.0,
        "real DAT target has nonzero dimensions"
    );
    assert_eq!(move_to.sought_object_radius, body.radius());
    assert_eq!(move_to.sought_object_height, body.height());
    drop(driver);
    assert!(app
        .objects()
        .presence(app.objects().player().unwrap())
        .unwrap()
        .pending_movement
        .is_none());
    // A constructed same-batch successor, not a recorded sequence: the old object approach is
    // cancelled by an interpreted-motion replacement (with a changed style in the second pass),
    // immediately followed by another accepted object approach. Clearing the old target must
    // finish before installing the new one; otherwise only the first target update survives the frame.
    for changed_style in [false, true] {
        use dereth_protocol::movement::{MovementBody, MovementBuffer, MovementSetObjectMovement};
        let new_target = ObjectId(target.0 + 1 + u32::from(changed_style));
        recipe.0.id = new_target;
        recipe
            .0
            .physicsdesc
            .position
            .as_mut()
            .unwrap()
            .frame
            .origin
            .x = origin.frame.origin.x + 20.0;
        app.probe_mut().objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::ITEM_CREATE_OBJECT,
                body: dereth_protocol::write_body(&recipe).unwrap(),
            },
            LocalTime(1.0),
        );
        let player = app.objects().player().unwrap();
        let p = app.objects().presence(player).unwrap();
        let style = if changed_style {
            MotionCommand::HAND_COMBAT
        } else {
            MotionCommand::NON_COMBAT
        };
        let replacement = MovementSetObjectMovement {
            id: player,
            instance_sequence: p.instance,
            movement: MovementSetObjectMovement::encode_movement(&MovementBuffer {
                movement_timestamp: p.movement_ts.wrapping_add(1),
                server_control_timestamp: p.server_control_ts.wrapping_add(1),
                autonomous: false,
                body: MovementBody {
                    current_style: style.to_index().unwrap(),
                    interpreted: Some(dereth_protocol::movement::InterpretedMotionState {
                        current_style: Some(style.to_index().unwrap()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            })
            .unwrap(),
        };
        let updates = app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .stats
            .target_updates;
        let failed = app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .stats
            .move_tos_failed;
        let accepted = app.objects().stats.movement_updates;
        app.probe_mut().objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
                body: dereth_protocol::write_body(&replacement).unwrap(),
            },
            LocalTime(1.0),
        );
        let next = lifecycle_movement(app.objects(), origin, Some(new_target));
        app.probe_mut()
            .objects_mut()
            .apply_event(&next, LocalTime(1.0));
        assert_eq!(app.objects().stats.movement_updates, accepted + 2);
        assert!(app.frame());
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert_eq!(c.driver().movement.moveto.sought_object_id, new_target);
        assert!(c.is_moving_to());
        assert_eq!(
            c.stats.move_tos_failed,
            failed + 1,
            "only the OLD approach canceled"
        );
        for _ in 0..45 {
            assert!(app.frame());
        }
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert!(c.is_moving_to());
        // A stationary target sends **no** second update: the target observer
        // re-sends only when the led position has drifted more than the radius from
        // what it last sent. What must survive the later frames is the subscription and the
        // approach, and the one update sent when it was registered.
        assert!(
            c.stats.target_updates >= updates + 1,
            "the new target subscription must be fed its first update"
        );
        assert_eq!(
            c.wanted_target().map(|t| t.id),
            Some(new_target),
            "the new target subscription must survive later frames"
        );
    }
    app.shutdown();
}

/// The descriptor's own movement buffer is another producer of the same runtime call. The
/// constructed approach must precede the unchanged recorded teleport, not restart after it.
#[test]
fn actual_app_cancels_create_embedded_approach_before_later_teleport() {
    use dereth_protocol::{movement::MovementSetObjectMovement, objects::ItemCreateObject};
    let (objects, origin, edge) = before_teleport();
    let mut app = lifecycle_app(origin);
    *app.probe_mut().objects_mut() = objects;
    assert!(app.frame());
    let player = app.objects().player().unwrap();
    let corpus = Corpus::load("early-inventory-and-casting")
        .unwrap()
        .unwrap();
    let mut recipe = corpus
        .blobs
        .iter()
        .filter(|r| r.dir == Direction::ServerToClient && r.opcode == Opcode::ITEM_CREATE_OBJECT.0)
        .map(|r| {
            ItemCreateObject::read(&mut dereth_protocol::Reader::new(&r.payload[4..])).unwrap()
        })
        .find(|c| c.0.id == player)
        .unwrap();
    let SessionEvent::WorldObject { body, .. } = lifecycle_movement(app.objects(), origin, None)
    else {
        unreachable!()
    };
    let buffer = MovementSetObjectMovement::read(&mut dereth_protocol::Reader::new(&body))
        .unwrap()
        .decoded_movement()
        .unwrap();
    let mut writer = dereth_protocol::Writer::new();
    buffer.body.write(&mut writer).unwrap();
    recipe.0.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::MOVEMENT;
    recipe.0.physicsdesc.bitfield &= !dereth_protocol::types::physicsdesc::flags::ANIMFRAME;
    recipe.0.physicsdesc.movement = Some((writer.into_inner(), 0));
    recipe.0.physicsdesc.timestamps.movement = buffer.movement_timestamp;
    recipe.0.physicsdesc.timestamps.server_controlled_move = buffer.server_control_timestamp;
    // The recipe is the recorded *login* create reused as a carrier for an embedded
    // movement buffer; at `t_rel 74.35` the wire sends no create for the player at all, so its
    // `state` is an artefact of the template. Left alone it re-delivers the login-tunnel word
    // (state mask `0x00404410`, `HIDDEN_PS` set) to a body the recording unhid 50 s earlier,
    // and a hidden body takes no animation offset. `ObjectStream`'s create path writes the
    // presence's state word **ungated**, unlike the state-update path, whose STATE_TS
    // comparison would have refused this stale stamp — so the carrier has
    // to say what the player's word actually is at this instant rather than what it was at login.
    recipe.0.physicsdesc.state = app
        .objects()
        .physics_state(player)
        .expect("the recorded player's current state word");
    let stats = app.world_state().unwrap().character.as_ref().unwrap().stats;
    app.probe_mut().objects_mut().apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::ITEM_CREATE_OBJECT,
            body: dereth_protocol::write_body(&recipe).unwrap(),
        },
        LocalTime(74.35),
    );
    app.probe_mut()
        .objects_mut()
        .apply_event(&event(&edge), LocalTime(74.356));
    assert!(app.frame());
    let c = app.world_state().unwrap().character.as_ref().unwrap();
    assert!(
        !c.world
            .get(c.handle)
            .expect("the local collision body")
            .state()
            .is_hidden(),
        "premise: the constructed carrier must not put the login-tunnel word back on \
         the body -- a hidden body takes no animation offset, so the approach below would be \
         measured against something that cannot move"
    );
    assert_eq!(c.stats.move_tos_performed, stats.move_tos_performed + 1);
    assert_eq!(c.stats.move_tos_failed, stats.move_tos_failed + 1);
    assert_eq!(c.stats.last_move_to_error, 0x3c);
    assert!(!c.is_moving_to());
    assert_eq!(app.probe().player_teleports_applied(), 1);
    assert!(app
        .objects()
        .presence(player)
        .unwrap()
        .pending_movement
        .is_none());
    assert!(app.frame());
    assert_eq!(
        app.world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .stats
            .move_tos_performed,
        stats.move_tos_performed + 1,
        "embedded command cannot dispatch twice"
    );
    app.shutdown();
}

#[test]
fn accepted_journal_keeps_each_teleports_position_and_echo_stamps_until_exactly_once_drain() {
    use dereth_client::objects::PlayerMotionDispatch;
    use dereth_protocol::movement::MovementPositionEvent;
    let (mut objects, origin, edge) = before_teleport();
    let _ = objects.take_player_motion_dispatches(); // initial recorded create is before this station
    let player = objects.player().unwrap();
    let initial_control = objects.presence(player).unwrap().server_control_ts;
    let first = stamped_movement(&objects);
    objects.apply_event(&first, LocalTime(74.35));
    objects.apply_event(&event(&edge), LocalTime(74.356));
    let second = stamped_movement(&objects);
    objects.apply_event(&second, LocalTime(74.36));
    let mut position =
        MovementPositionEvent::read(&mut dereth_protocol::Reader::new(&edge.payload[4..])).unwrap();
    position.position.position_timestamp = position.position.position_timestamp.wrapping_add(1);
    position.position.teleport_timestamp = position.position.teleport_timestamp.wrapping_add(1);
    position.position.origin.origin.x += 4.0;
    let second_teleport = SessionEvent::WorldObject {
        opcode: Opcode::MOVEMENT_POSITION_EVENT,
        body: dereth_protocol::write_body(&position).unwrap(),
    };
    objects.apply_event(&second_teleport, LocalTime(74.37));
    let third = stamped_movement(&objects);
    objects.apply_event(&third, LocalTime(74.38));
    objects.apply_event(&second_teleport, LocalTime(74.39)); // duplicate position/teleport
    objects.apply_event(&first, LocalTime(74.40)); // stale movement/control
    let final_stamps = player_timestamps(objects.presence(player));
    assert_eq!(final_stamps.server_control, initial_control.wrapping_add(3));
    let dispatch = objects.take_player_motion_dispatches();
    assert_eq!(
        dispatch.len(),
        5,
        "three accepted commands and two accepted teleports, no rejected edges"
    );
    assert!(matches!(&dispatch[0], PlayerMotionDispatch::Movement(_)));
    assert!(matches!(&dispatch[2], PlayerMotionDispatch::Movement(_)));
    assert!(matches!(&dispatch[4], PlayerMotionDispatch::Movement(_)));
    let mut c = body_at(origin, player);
    let mut commands = MovementCommands::default();
    let mut input = CharacterInput::default();
    let mut reporter = PositionReporter::new(0.0);
    let mut session = Session::new(MockTransport::new());
    let mut teleports = 0u16;
    for e in dispatch {
        let PlayerMotionDispatch::Teleport {
            position,
            timestamps,
        } = e
        else {
            continue;
        };
        teleports += 1;
        assert_eq!(timestamps.teleport, teleports);
        assert_eq!(
            timestamps.server_control,
            initial_control.wrapping_add(teleports)
        );
        assert_ne!(
            timestamps.server_control, final_stamps.server_control,
            "not the batch's final timestamp snapshot"
        );
        // The production accepted-edge completion/sender seam, isolated from movement here to
        // inspect its two distinct position/stamp echoes. The App test above proves interleaving.
        dereth_client::app::complete_player_teleport_at(
            position,
            &mut c,
            &mut commands,
            &mut input,
            |c| {
                reporter
                    .send_movement_event(74.4, &body_motion(c, timestamps), &mut session)
                    .expect("explicit send");
            },
        );
        let action = dereth_protocol::actions::unpack_action(
            &session.transport.sent.last().unwrap().payload,
        )
        .unwrap();
        let outgoing =
            dereth_protocol::movement::MovementMoveToState::read(&mut action.body.clone()).unwrap();
        assert_eq!(outgoing.0.timestamps, timestamps);
        assert_eq!(outgoing.0.position.objcell_id, position.cell.0);
        assert_eq!(outgoing.0.position.frame.origin.x, position.frame.origin.x);
    }
    assert_eq!(teleports, 2);
    assert_eq!(
        session.transport.sent.len(),
        2,
        "one explicit tail per accepted teleport"
    );
    assert!(objects.take_player_motion_dispatches().is_empty());
    assert!(
        objects.take_movement(player).is_none(),
        "snapshot cannot redispatch journal movement"
    );
    assert!(objects.take_player_teleport().is_none());

    // Supported isolated-component snapshot adapters share consumption with the App journal.
    // They intentionally do not promise cross-kind ordering; full App uses the journal.
    objects.apply_event(&stamped_movement(&objects), LocalTime(74.5));
    assert!(objects.take_movement(player).is_some());
    assert!(objects.take_player_motion_dispatches().is_empty());
    position.position.position_timestamp = position.position.position_timestamp.wrapping_add(1);
    position.position.teleport_timestamp = position.position.teleport_timestamp.wrapping_add(1);
    objects.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_POSITION_EVENT,
            body: dereth_protocol::write_body(&position).unwrap(),
        },
        LocalTime(74.6),
    );
    assert!(objects.take_player_teleport().is_some());
    assert!(objects.take_player_motion_dispatches().is_empty());
}
