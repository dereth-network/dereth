//! Using a door: the server turns the player to the door (a non-sticky `TurnToObject` in
//! `Movement_SetObjectMovement 0xF74C`), the door's `Item_SetState 0xF74B` makes its collision
//! body ethereal, and the player can then walk through it under his own power. An accepted door
//! turn first ends any earlier stick, and a same-batch replacement turn cancels once and keeps
//! the new target. Fixture: early-inventory-and-casting's training-academy door sequence, the
//! academy's environment cells from the retail dats, and a real `App` on a headless device.
//! No connection, window, settings or file writes.

use crate::common::sim_app::{frames, position};

use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_primitives::ObjectId;
use dereth_protocol::objects::{ItemCreateObject, ItemSetState};
use dereth_protocol::{Message, Opcode};

/// The recorded door sequence the App tests below replay, read straight off the recording: the
/// door's create, the non-sticky turn to it, its opening state, and a successful use-done.
#[test]
fn recorded_academy_station_is_a_successful_nonsticky_door_turn_and_open() {
    let rows = Corpus::load("early-inventory-and-casting")
        .expect("corpus decodes")
        .expect("early-inventory-and-casting")
        .blobs;
    let row = |idx| {
        rows.iter()
            .find(|r| r.idx == idx)
            .expect("recorded station")
    };
    let create = ItemCreateObject::read(&mut dereth_protocol::Reader::new(&row(52).payload[4..]))
        .unwrap()
        .0;
    assert_eq!(create.id, ObjectId(0x77f0_3033));
    assert_eq!(create.wdesc.name, "Door");
    assert_eq!(create.physicsdesc.setup_id, Some(0x0200_05da));
    assert_eq!(create.physicsdesc.mtable_id, Some(0x0900_0086));
    assert_eq!(create.physicsdesc.state, 0x0001_0018);
    assert_eq!(create.physicsdesc.position.unwrap().objcell_id, 0x7f03_01b4);
    let movement = dereth_protocol::movement::MovementSetObjectMovement::read(
        &mut dereth_protocol::Reader::new(&row(147).payload[4..]),
    )
    .unwrap();
    assert_eq!(movement.id, ObjectId(0x5000_0003));
    let buffer = movement.decoded_movement().unwrap();
    assert!(!buffer.autonomous);
    assert_eq!(buffer.body.current_style, 61);
    let Some(dereth_protocol::movement::MoveToArm::TurnToObject { target, params, .. }) =
        buffer.body.decode_move_to().unwrap()
    else {
        panic!("recorded Door turn");
    };
    assert_eq!(target, create.id);
    let dereth_protocol::movement::MovementParameters::TurnTo { bitfield, .. } = params else {
        panic!("recorded TurnTo parameters");
    };
    assert_eq!(bitfield & dereth_animation::motion::flags::STICKY, 0);
    let opened =
        ItemSetState::read(&mut dereth_protocol::Reader::new(&row(149).payload[4..])).unwrap();
    assert_eq!(opened.id, create.id);
    assert_eq!(opened.state, 0x0001_001c);
    let done = row(150);
    assert_eq!(done.opcode, 0xf7b0);
    assert_eq!(
        u32::from_le_bytes(done.payload[12..16].try_into().unwrap()),
        Opcode::ITEM_USE_DONE.0
    );
    let done = dereth_protocol::objects::ItemUseDone::read(&mut dereth_protocol::Reader::new(
        &done.payload[16..],
    ))
    .unwrap();
    assert_eq!(done.failure_type, 0);
}

#[test]
fn recorded_academy_cell_collision_supports_grounded_walk_without_scene_objects() {
    use dereth_client::character::Character;
    use dereth_physics::LandSource;
    use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
    use std::sync::Arc;
    let rows = Corpus::load("early-inventory-and-casting")
        .unwrap()
        .unwrap()
        .blobs;
    let r = rows.iter().find(|r| r.idx == 143).unwrap();
    let mut a = dereth_protocol::actions::unpack_action(&r.payload).unwrap();
    let recorded = dereth_protocol::movement::MovementMoveToState::read(&mut a.body)
        .unwrap()
        .0;
    assert!(recorded.contact);
    let p = recorded.position;
    let origin = Position::new(
        CellId(p.objcell_id),
        Frame::new(
            Vec3::new(p.frame.origin.x, p.frame.origin.y, p.frame.origin.z),
            Quat::new(
                p.frame.orientation.w,
                p.frame.orientation.x,
                p.frame.orientation.y,
                p.frame.orientation.z,
            ),
        ),
    );
    let store = Arc::new(dereth_dat::testing::open_store().unwrap());
    let region = dereth_client::world::load_region(&store).unwrap();
    let mut c = Character::new(&store, &region, 0x7f03, (96.0, 96.0)).unwrap();
    c.land().load_block_cells(origin.cell.landblock());
    let cell = c.land().env_cell(origin.cell).unwrap();
    assert!(cell.portals.iter().any(|p| p.other_cell_id == 0x7f03_01b4));
    c.teleport(origin);
    c.stop_completely_from_action();
    for frame in 1..=90 {
        c.update(LocalTime(f64::from(frame) / 30.0));
        if frame >= 10 {
            assert!(c.on_ground());
        }
    }
    assert!(
        c.on_ground(),
        "recorded contact=true station must stand before USE is tested"
    );
    c.input.forward = true;
    let mut visited = std::collections::BTreeSet::new();
    for frame in 91..=270 {
        c.update(LocalTime(f64::from(frame) / 30.0));
        assert!(c.on_ground());
        visited.insert(c.position().cell);
    }
    for id in [0x7f03_01ad, 0x7f03_01b4, 0x7f03_01b1] {
        assert!(
            visited.contains(&CellId(id)),
            "the bare room must permit the doorway transit"
        );
    }
    assert!(dereth_animation::motion::moveto::distance(&origin, &c.position()) > 10.0);
}

#[test]
fn academy_cell_extents_probe_the_default_streaming_window_margin() {
    use dereth_physics::V3;
    let store = dereth_dat::testing::open_store().unwrap();
    let mut loader = dereth_client::env_cells::EnvCellLoader::new();
    for block in [0x7f03, 0x8602, 0x8c04] {
        let cells = loader.load_block(&store, block);
        let mut lo = dereth_primitives::Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
        let mut hi =
            dereth_primitives::Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut count = 0;
        for cell in &cells {
            let rot = dereth_physics::math::l2g(cell.cell.frame.rotation);
            for vertex in &cell.structure.vertex_array.vertices {
                let p = cell
                    .cell
                    .frame
                    .origin
                    .add(dereth_physics::math::localtoglobalvec(rot, vertex.position));
                lo.x = lo.x.min(p.x);
                lo.y = lo.y.min(p.y);
                lo.z = lo.z.min(p.z);
                hi.x = hi.x.max(p.x);
                hi.y = hi.y.max(p.y);
                hi.z = hi.z.max(p.z);
                count += 1;
            }
        }
        eprintln!(
            "academy extent block={block:#x} cells={} vertices={count} lo={lo:?} hi={hi:?}",
            cells.len()
        );
        assert_eq!(cells.len(), 568);
        assert_eq!(count, 7396);
        for (min, max) in [(lo.x, hi.x), (lo.y, hi.y)] {
            assert!(
                min - 0.5 >= -3.0 * 192.0 && max + 0.5 < 4.0 * 192.0,
                "academy geometry plus a 0.5 m camera margin stays inside the default radius-3 window"
            );
        }
    }
    assert_eq!(
        loader.stats.missing + loader.stats.undecodable + loader.stats.no_environment,
        0
    );
}
mod actual_app {
    use super::*;
    use dereth_client::{app::App, config::Config, world::SceneConfig};
    use dereth_client_net::client_session::{testing::CorpusBlob, SessionEvent};
    use dereth_primitives::{CellId, Frame, LocalTime, Position, Quat, Vec3};
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;

    fn row(rows: &[CorpusBlob], index: usize) -> &CorpusBlob {
        rows.iter()
            .find(|r| r.idx == index)
            .expect("recorded station")
    }

    fn feed(app: &mut App, row: &CorpusBlob) {
        assert_eq!(row.dir, Direction::ServerToClient);
        let event = match row.opcode {
            0xf746 => SessionEvent::PlayerCreated(ObjectId(u32::from_le_bytes(
                row.payload[4..8].try_into().unwrap(),
            ))),
            0xf7b0 => SessionEvent::UiEvent {
                opcode: Opcode(u32::from_le_bytes(row.payload[12..16].try_into().unwrap())),
                blob: row.payload[12..].to_vec(),
            },
            _ => SessionEvent::WorldObject {
                opcode: Opcode(row.opcode),
                body: row.payload[4..].to_vec(),
            },
        };
        app.probe_mut()
            .objects_mut()
            .apply_event(&event, LocalTime(row.t_rel_micros as f64 / 1_000_000.0));
        app.apply_hud_events(std::slice::from_ref(&event));
        app.probe_mut()
            .apply_interaction_events(std::slice::from_ref(&event));
    }

    fn input(app: &mut App, action: dereth_input::ActionId, start: bool) {
        app.input_manager_mut()
            .unwrap()
            .inject_action(dereth_input::InputEvent {
                action,
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

    fn snapshot(app: &App, label: &str, door: ObjectId) {
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        let d = c.driver();
        let o = c.world.get(c.handle).unwrap();
        let h = app.objects().physics.handle(door).unwrap();
        let door = c.world.get(h).unwrap();
        eprintln!(
            "{label}: pos={:?} velocity={:?} ground={} transient={:?} player_state={:#x} \
            raw={:?} interp={:?} pending={:?} moveto={:?} server={} door_state={:#x} target_updates={}",
            c.position(),
            c.velocity(),
            c.on_ground(),
            o.transient_state,
            o.state.0,
            d.movement.interp.raw_state,
            d.movement.interp.interpreted_state,
            d.movement.interp.pending_motions,
            d.movement.moveto.movement_type,
            app.probe().movement_commands().lists.controlled_by_server,
            door.state.0,
            c.stats.target_updates
        );
    }

    /// Behaviour: movement.door.no-way-of-opening-a-door-leaves-a-body-that-turns-but-cannot-walk
    /// The recorded academy sequence and use response, with the forward key pressed by this test.
    /// No player position arrives from the server after the use, so all motion is local.
    #[test]
    fn academy_door_use_opens_the_remote_collision_body_and_manual_motion_releases() {
        academy_journey(false, false);
    }

    /// Behaviour: movement.door.an-accepted-door-turn-unsticks-the-previous-target
    /// A constructed earlier combat stick (not in the recording) precedes the door. The accepted
    /// door movement cancels and unsticks before it starts its new `TurnToObject`, so the old
    /// sticky correction does not survive the new command.
    #[test]
    fn accepted_door_movement_unsticks_a_prior_combat_target_before_the_new_turn() {
        academy_journey(true, false);
    }

    /// Two accepted turns in one batch (the second with constructed, newer stamps). The earlier
    /// turn is cancelled once, and its target clean-up does not erase the replacement's.
    #[test]
    fn same_batch_door_turn_replacement_keeps_the_new_subscription_and_cancels_once() {
        academy_journey(true, true);
    }

    fn academy_journey(prior_stick: bool, repeated_turn: bool) {
        let rows = Corpus::load("early-inventory-and-casting")
            .unwrap()
            .unwrap()
            .blobs;
        let door = ObjectId(0x77f0_3033);
        let mut a = dereth_protocol::actions::unpack_action(&row(&rows, 143).payload).unwrap();
        let recorded = dereth_protocol::movement::MovementMoveToState::read(&mut a.body)
            .unwrap()
            .0;
        let p = &recorded.position;
        let origin = Position::new(
            CellId(p.objcell_id),
            Frame::new(
                Vec3::new(p.frame.origin.x, p.frame.origin.y, p.frame.origin.z),
                Quat::new(
                    p.frame.orientation.w,
                    p.frame.orientation.x,
                    p.frame.orientation.y,
                    p.frame.orientation.z,
                ),
            ),
        );
        let mut app = crate::common::sim_app::new(Config {
            headless: true,
            sound: false,
            ui: true,
            width: 640,
            height: 480,
            dat_dir: dereth_dat::testing::dat_dir(),
            ..Default::default()
        })
        .expect("required DAT and headless App");
        app.start_shell().unwrap();
        app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
        frames(&mut app, 4);
        app.load_static_scene(SceneConfig {
            // The recorded interior origin has negative y. Keep its 7F03 cell block resident
            // while the outside-coordinate viewer window centers on 7F02.
            landblock: 0x7f03,
            character: true,
            land_radius: 1,
            scenery_radius: 0,
            cell_statics: true,
            mesh_collision: true,
            particles: false,
            ..Default::default()
        })
        .expect("real academy cell and collision scene");
        for r in rows
            .iter()
            .filter(|r| r.idx < 146 && r.dir == Direction::ServerToClient)
        {
            feed(&mut app, r);
        }
        frames(&mut app, 1);
        // Use the last recorded pre-USE player placement, after the descriptor rebuild.
        let c = app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap();
        c.land().load_block_cells(origin.cell.landblock());
        c.teleport(origin);
        c.stop_completely_from_action();
        frames(&mut app, 60);
        snapshot(&app, "pre-use", door);
        assert!(app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .on_ground());

        if prior_stick {
            let old = rows
                .iter()
                .filter(|r| r.idx < 146 && r.opcode == Opcode::ITEM_CREATE_OBJECT.0)
                .filter_map(|r| {
                    ItemCreateObject::read(&mut dereth_protocol::Reader::new(&r.payload[4..])).ok()
                })
                .find(|m| {
                    m.0.wdesc.obj_type & dereth_client_model::weenie::item_type::CREATURE != 0
                        && m.0.id != app.objects().player().unwrap()
                        && m.0.physicsdesc.position.is_some()
                })
                .expect("recorded existing creature");
            let c = app
                .probe_mut()
                .world_state_mut()
                .unwrap()
                .character
                .as_mut()
                .unwrap();
            c.apply_movement_style(dereth_animation::MotionCommand::SWORD_COMBAT);
            frames(&mut app, 60);
            let h = app
                .objects()
                .physics
                .handle(old.0.id)
                .expect("real creature body");
            let c = app
                .probe_mut()
                .world_state_mut()
                .unwrap()
                .character
                .as_mut()
                .unwrap();
            let target = c.world.get(h).unwrap();
            let (radius, height) = (target.radius(), target.height());
            // The production stick entry point, used to construct an earlier combat stick.
            c.stick_to_object(old.0.id, radius, height);
            assert_eq!(c.sticky_target(), Some(old.0.id));
            eprintln!("constructed prior combat stick target={:#x}", old.0.id.0);
        }

        app.probe_mut().objects_mut().world.set_selected_object(
            Some(door),
            false,
            &mut dereth_client_model::RecordingSink::default(),
        );
        frames(&mut app, 1);
        let button = {
            let shell = app.ui().unwrap();
            let screen: &dyn std::any::Any = shell.flow.current().unwrap();
            let screen = screen.downcast_ref::<GamePlayScreen>().unwrap();
            shell
                .ui
                .get_child_recursive(
                    screen.root().unwrap(),
                    dereth_ui_screens::toolbar::target_mode::USE_BUTTON,
                )
                .unwrap()
        };
        app.ui_mut().unwrap().ui.broadcast_element_message(
            button,
            dereth_ui::msg::element::id::BUTTON_CLICKED,
            0,
            0,
        );
        frames(&mut app, 1);
        assert!(
            app.interaction().last_sent.iter().any(|r| matches!(r,
            dereth_client_model::Request::UseEvent(m) if m.object == door)),
            "actual DAT USE requests this door"
        );
        if prior_stick {
            let c = app.world_state().unwrap().character.as_ref().unwrap();
            assert!(
                c.driver().movement.sticky.initialized,
                "prior stick really received a target"
            );
            assert!(
                c.sticky_target().is_some(),
                "the one-second lifetime has not elapsed"
            );
        }
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        let (failed_before, performed_before, updates_before) = (
            c.stats.move_tos_failed,
            c.stats.move_tos_performed,
            c.stats.target_updates,
        );
        for index in 147..=150 {
            feed(&mut app, row(&rows, index));
        }
        if repeated_turn {
            let mut message = dereth_protocol::movement::MovementSetObjectMovement::read(
                &mut dereth_protocol::Reader::new(&row(&rows, 147).payload[4..]),
            )
            .unwrap();
            let mut buffer = message.decoded_movement().unwrap();
            buffer.movement_timestamp = buffer.movement_timestamp.wrapping_add(1);
            buffer.server_control_timestamp = buffer.server_control_timestamp.wrapping_add(1);
            message.movement =
                dereth_protocol::movement::MovementSetObjectMovement::encode_movement(&buffer)
                    .unwrap();
            let event = SessionEvent::WorldObject {
                opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
                body: dereth_protocol::write_body(&message).unwrap(),
            };
            let accepted = app.objects().stats.movement_updates;
            app.probe_mut()
                .objects_mut()
                .apply_event(&event, LocalTime(46.4));
            assert_eq!(app.objects().stats.movement_updates, accepted + 1);
            // An identical duplicate cannot install/cancel another turn.
            app.probe_mut()
                .objects_mut()
                .apply_event(&event, LocalTime(46.5));
            assert_eq!(app.objects().stats.movement_updates, accepted + 1);
        }
        frames(&mut app, 1);
        if prior_stick {
            assert_eq!(
                app.world_state()
                    .unwrap()
                    .character
                    .as_ref()
                    .unwrap()
                    .sticky_target(),
                None,
                "accepted Door F74C must unstick before applying its new TurnToObject"
            );
        }
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert_eq!(
            c.stats.move_tos_performed,
            performed_before + 1 + u64::from(repeated_turn)
        );
        assert_eq!(
            c.stats.move_tos_failed,
            failed_before + u64::from(repeated_turn)
        );
        frames(&mut app, 89);
        snapshot(&app, "opened", door);
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert!(
            !c.is_moving_to(),
            "new target subscription must survive and finish the turn"
        );
        assert!(
            !c.driver().movement.motions_pending(),
            "turn/style animation completion drains"
        );
        assert!(
            c.stats.target_updates > updates_before,
            "the new Door target was delivered"
        );
        assert_eq!(
            c.stats.move_tos_failed,
            failed_before + u64::from(repeated_turn),
            "no late cancel/report"
        );
        let h = app.objects().physics.handle(door).unwrap();
        assert!(
            c.world.get(h).unwrap().state.is_ethereal(),
            "recorded remote door truly opens"
        );

        let before = position(&app);
        input(
            &mut app,
            dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
            true,
        );
        let mut entered_door_cell = false;
        for _ in 0..90 {
            frames(&mut app, 1);
            entered_door_cell |= position(&app).cell == CellId(0x7f03_01b4);
            assert!(app
                .world_state()
                .unwrap()
                .character
                .as_ref()
                .unwrap()
                .on_ground());
        }
        assert!(
            entered_door_cell,
            "the physical body traverses the actual Door cell"
        );
        assert_eq!(position(&app).cell, CellId(0x7f03_01b1));
        snapshot(&app, "forward", door);
        assert!(
            dereth_animation::motion::moveto::distance(&before, &position(&app)) > 1.0,
            "fresh physical input must translate after opening, not merely turn"
        );
        input(
            &mut app,
            dereth_client_runtime::actions::movement::action::MOVE_FORWARD,
            false,
        );
        frames(&mut app, 60);
        let stopped = position(&app);
        frames(&mut app, 60);
        assert!(dereth_animation::motion::moveto::distance(&stopped, &position(&app)) < 0.05);
        input(
            &mut app,
            dereth_client_runtime::actions::movement::action::MOVE_BACKWARD,
            true,
        );
        frames(&mut app, 30);
        assert!(
            dereth_animation::motion::moveto::distance(&stopped, &position(&app)) > 0.3,
            "next command works"
        );
        input(
            &mut app,
            dereth_client_runtime::actions::movement::action::MOVE_BACKWARD,
            false,
        );
        frames(&mut app, 30);
        if repeated_turn {
            ignored_buffers_do_not_run_the_prefix(&mut app, &rows, door);
        }
        app.shutdown();
    }

    /// Constructed refusals after the recorded sequence: an autonomous echo, a stale or duplicate
    /// movement stamp, and an older server-control stamp are all refused and do not cancel or
    /// unstick; only the fresh fifth update does.
    fn ignored_buffers_do_not_run_the_prefix(app: &mut App, rows: &[CorpusBlob], door: ObjectId) {
        let h = app.objects().physics.handle(door).unwrap();
        let c = app
            .probe_mut()
            .world_state_mut()
            .unwrap()
            .character
            .as_mut()
            .unwrap();
        let target = c.world.get(h).unwrap();
        let (radius, height) = (target.radius(), target.height());
        c.stick_to_object(door, radius, height);
        frames(app, 1);
        let performed = app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .stats
            .move_tos_performed;
        let failed = app
            .world_state()
            .unwrap()
            .character
            .as_ref()
            .unwrap()
            .stats
            .move_tos_failed;
        let accepted = app.objects().stats.movement_updates;
        let rejected = (
            app.objects().stats.movement_own_echo,
            app.objects().stats.movement_stale,
            app.objects().stats.movement_old_control,
        );
        let mut message = dereth_protocol::movement::MovementSetObjectMovement::read(
            &mut dereth_protocol::Reader::new(&row(rows, 147).payload[4..]),
        )
        .unwrap();
        for station in 0..5 {
            let p = app.objects().presence(message.id).unwrap();
            let mut buffer = message.decoded_movement().unwrap();
            buffer.movement_timestamp = p.movement_ts.wrapping_add(1);
            buffer.server_control_timestamp = p.server_control_ts.wrapping_add(1);
            buffer.autonomous = station == 0;
            match station {
                1 => buffer.movement_timestamp = p.movement_ts,
                2 => buffer.movement_timestamp = p.movement_ts.wrapping_sub(1),
                3 => buffer.server_control_timestamp = p.server_control_ts.wrapping_sub(1),
                _ => {}
            }
            message.movement =
                dereth_protocol::movement::MovementSetObjectMovement::encode_movement(&buffer)
                    .unwrap();
            app.probe_mut().objects_mut().apply_event(
                &SessionEvent::WorldObject {
                    opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
                    body: dereth_protocol::write_body(&message).unwrap(),
                },
                LocalTime(60.0 + f64::from(station)),
            );
            frames(app, 1);
            let c = app.world_state().unwrap().character.as_ref().unwrap();
            assert_eq!(
                c.sticky_target(),
                if station == 4 { None } else { Some(door) }
            );
            assert_eq!(
                c.stats.move_tos_performed,
                performed + u64::from(station == 4)
            );
            assert_eq!(c.stats.move_tos_failed, failed);
            assert_eq!(
                app.objects().stats.movement_updates,
                accepted + u64::from(station == 4)
            );
        }
        assert_eq!(app.objects().stats.movement_own_echo, rejected.0 + 1);
        assert_eq!(app.objects().stats.movement_stale, rejected.1 + 2);
        assert_eq!(app.objects().stats.movement_old_control, rejected.2 + 1);
        frames(app, 90);
        let c = app.world_state().unwrap().character.as_ref().unwrap();
        assert!(!c.is_moving_to());
        assert_eq!(
            c.stats.move_tos_failed, failed,
            "no delayed cancellation after the accepted successor"
        );
    }
}
