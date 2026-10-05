//! When a remote creature's action with a sticky target completes, the owned stick and the target
//! subscription clear in both animation owners, and the creature then walks and stops again.
//! Fixture: long-solo-play's recorded create, position and state for a creature, driven through
//! `ObjectStream` and `WorldScene` on a software device with constructed
//! `Movement_SetObjectMovement 0xF74C` messages; the retail dats.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_scene::world_scene::SceneWrites;
use std::sync::Arc;

use dereth_animation::MotionCommand;
use dereth_client_runtime::landblock::load_region;
use dereth_primitives::{LocalTime, ObjectId};

/// Behaviour: movement.sticky.animation-done-unsticks-before-manual-motion
#[test]
fn remote_action_completion_clears_owned_stick_and_subscription_in_both_animation_owners() {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_client_net::client_session::SessionEvent;
    use dereth_client_runtime::objects::ObjectStream;
    use dereth_protocol::movement::{
        motion_flags, InterpretedMotionState, MotionAction, MovementBody, MovementBuffer,
        MovementSetObjectMovement,
    };
    use dereth_protocol::Opcode;
    use dereth_render::device::Gpu;
    use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

    // Recorded player creation/position/state, rendered as a remote object (no local binding).
    // The wave with a sticky target, and the walk and release after it, are constructed 0xF74C
    // messages, not recorded ones.
    let id = ObjectId(0x5000_000a);
    let store = Arc::new(dereth_dat::testing::open_store().expect("required retail DATs"));
    let corpus = Corpus::load("long-solo-play")
        .expect("locked decode")
        .expect("required long-solo-play");
    let mut gpu = crate::common::test_gpu(320, 240);
    for attached in [false, true] {
        let mut stream = ObjectStream::new();
        for row in corpus.blobs.iter().filter(|r| {
            r.dir == Direction::ServerToClient
                && r.t_rel_micros < 91_500_548
                && [
                    Opcode::ITEM_CREATE_OBJECT.0,
                    Opcode::MOVEMENT_POSITION_EVENT.0,
                    Opcode::ITEM_SET_STATE.0,
                ]
                .contains(&r.opcode)
                && r.payload.get(4..8) == Some(id.0.to_le_bytes().as_slice())
        }) {
            stream.apply_event(
                &SessionEvent::WorldObject {
                    opcode: Opcode(row.opcode),
                    body: row.payload[4..].to_vec(),
                },
                LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
            );
        }
        let initial = stream
            .presence(id)
            .and_then(|p| p.position)
            .expect("recorded player");
        assert_eq!(
            stream.player(),
            None,
            "this body must take the remote ownership path"
        );
        let block = initial.cell.landblock();
        let mut scene = WorldScene::load(
            &store,
            &mut gpu,
            SceneConfig {
                landblock: (u16::from(block.x()) << 8) | u16::from(block.y()),
                character: false,
                land_radius: 1,
                scenery_radius: 0,
                cell_statics: false,
                mesh_collision: false,
                particles: false,
                ..Default::default()
            },
        )
        .expect("remote scene");
        if attached {
            let region = load_region(&store).expect("region");
            scene
                .attach_character(&store, &region, &mut gpu)
                .expect("physics owner");
            let mut observer = initial;
            observer.frame.origin.x += 3.0;
            scene
                .character
                .as_ref()
                .unwrap()
                .land()
                .load_block_cells(block);
            scene.character.as_mut().unwrap().teleport(observer);
        }
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("remote creation");
        if let Some(c) = scene.character.as_mut() {
            stream.sync_physics(&store, &mut c.world);
        }
        let mut now = 92.0;
        for _ in 0..60 {
            now += 1.0 / 30.0;
            scene.update(
                Default::default(),
                Default::default(),
                LocalTime(now),
                1.0 / 30.0,
            );
        }
        assert_eq!(scene.server_object_motions_pending(id), Some(0));
        assert_eq!(scene.server_object_table_motions_pending(id), Some(0));
        assert_eq!(stream.physics.handle(id).is_some(), attached);
        if let Some(c) = scene.character.as_ref() {
            let body = c
                .world
                .by_object_id(id)
                .and_then(|h| c.world.get(h))
                .unwrap();
            assert!(
                !body.state.is_hidden(),
                "the recorded 0xF74B must finish the initially hidden create"
            );
            assert!(
                body.motion.is_some(),
                "PhysicsWorld owns the shared remote driver"
            );
        }

        let send = |scene: &mut WorldScene,
                    stream: &mut ObjectStream,
                    gpu: &mut Gpu,
                    now: f64,
                    body: MovementBody| {
            let p = stream.presence(id).expect("remote presence");
            let buffer = MovementBuffer {
                movement_timestamp: p.movement_ts.wrapping_add(1),
                server_control_timestamp: p.server_control_ts,
                autonomous: false,
                body,
            };
            let message = MovementSetObjectMovement {
                id,
                instance_sequence: p.instance,
                movement: MovementSetObjectMovement::encode_movement(&buffer).expect("wire 0xF74C"),
            };
            stream.apply_event(
                &SessionEvent::WorldObject {
                    opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
                    body: dereth_protocol::write_body(&message).expect("wire body"),
                },
                LocalTime(now),
            );
            assert!(
                stream.presence(id).unwrap().pending_movement.is_some(),
                "accepted wire gate"
            );
            scene
                .sync_objects(&store, gpu, stream)
                .expect("remote movement dispatch");
        };
        let style = MotionCommand::NON_COMBAT.to_index().expect("style index");
        // The mover itself is a resolvable target and avoids adding an unrelated target-position
        // packet/collision journey. Retail has no self-target rejection in this movement arm.
        send(
            &mut scene,
            &mut stream,
            &mut gpu,
            now,
            MovementBody {
                current_style: style,
                motion_flags: motion_flags::STICK_TO_OBJECT,
                sticky_object: Some(id),
                interpreted: Some(InterpretedMotionState {
                    actions: vec![MotionAction {
                        command_index: MotionCommand::WAVE.to_index().unwrap(),
                        stamp_and_autonomy: 1,
                        speed: 16.0,
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        assert_eq!(scene.server_object_sticky(id), Some(id));
        assert_eq!(scene.server_object_target(id), Some(id));
        assert!(scene.server_object_motions_pending(id).unwrap() > 0);
        assert!(scene.server_object_table_motions_pending(id).unwrap() > 0);
        let started = now;
        while scene.server_object_motions_pending(id) != Some(0) && now - started < 0.75 {
            now += 1.0 / 30.0;
            scene.update(
                Default::default(),
                Default::default(),
                LocalTime(now),
                1.0 / 30.0,
            );
        }
        assert!(now - started < 0.75,
            "real action must complete before the 1s sticky expiry: attached={attached}, interp={:?}, table={:?}, sequence={:?}, body={:?}",
            scene.server_object_motions_pending(id), scene.server_object_table_motions_pending(id),
            scene.server_object_sequence(id).map(|s| (s.curr(), s.frame_number())),
            scene.character.as_ref().and_then(|c| c.world.by_object_id(id).and_then(|h| c.world.get(h)))
                .map(|b| (b.update_time, b.transient_state.is_active(), b.position, b.state)));
        assert_eq!(scene.server_object_motions_pending(id), Some(0));
        assert_eq!(scene.server_object_table_motions_pending(id), Some(0));
        assert_eq!(scene.server_object_sticky(id), None, "attached={attached}");
        assert_eq!(scene.server_object_target(id), None, "attached={attached}");
        let before = scene.server_object_position(id).unwrap();
        for command in [MotionCommand::WALK_FORWARD, MotionCommand::READY] {
            send(
                &mut scene,
                &mut stream,
                &mut gpu,
                now,
                MovementBody {
                    current_style: style,
                    interpreted: Some(InterpretedMotionState {
                        forward_command: Some(command.to_index().unwrap()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            for _ in 0..60 {
                now += 1.0 / 30.0;
                scene.update(
                    Default::default(),
                    Default::default(),
                    LocalTime(now),
                    1.0 / 30.0,
                );
            }
            assert_eq!(scene.server_object_motion(id).unwrap().1, command);
        }
        assert_eq!(scene.server_object_motions_pending(id), Some(0));
        assert_eq!(scene.server_object_table_motions_pending(id), Some(0));
        assert_eq!(scene.server_object_target(id), None);
        let stopped = scene.server_object_position(id).unwrap();
        if attached {
            assert!(dereth_animation::motion::moveto::distance(&before, &stopped) > 0.5);
        } else {
            assert_eq!(
                stopped, before,
                "bodyless fallback does not claim physical translation"
            );
        }
        for _ in 0..30 {
            now += 1.0 / 30.0;
            scene.update(
                Default::default(),
                Default::default(),
                LocalTime(now),
                1.0 / 30.0,
            );
        }
        assert!(
            dereth_animation::motion::moveto::distance(
                &stopped,
                &scene.server_object_position(id).unwrap()
            ) < 0.05
        );
    }
}
