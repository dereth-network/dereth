//! A remote creature walks by the dats' root motion, not by server positions: after one recorded
//! move-to-object with no later position updates it turns, walks, arrives, sticks and stops, and
//! the position the scene publishes is the collision result on the floor. A bodyless viewer keeps
//! the creature's animation running without translating it, and a static/unstatic state edge
//! hands the animation between its owners without advancing it twice.
//! Fixture: long-solo-play's Sparring Golem (0x800009D2) and player, recorded blobs up to the
//! `0xF74C` at t=91.500548, then constructed `0xF74C` and `0xF74B` messages, on a headless
//! software device.
#![cfg(gpu)]

use dereth_animation::motion::moveto::distance;
use dereth_animation::MotionCommand;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{Frame, LocalTime, ObjectId};
use dereth_protocol::{Message, Opcode};
use dereth_render::device::{DeviceConfig, Gpu};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

const PLAYER: ObjectId = ObjectId(0x5000_000a);
const MOVER: ObjectId = ObjectId(0x8000_09d2);
const START: f64 = 91.5;

/// Behaviour: movement.remote.a-bodyless-viewer-animates-without-translating
#[test]
fn bodyless_viewer_retains_its_animation_ladder_without_claiming_physical_translation() {
    let store = std::sync::Arc::new(
        dereth_dat::testing::open_store().expect("DERETH_TEST_DAT_DIR required"),
    );
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: 320,
            height: 240,
            ..Default::default()
        },
    )
    .expect("required headless WARP");
    let corpus = Corpus::load("long-solo-play")
        .expect("locked corpus decodes")
        .expect("long-solo-play required");
    let mut stream = ObjectStream::new();
    for row in corpus.blobs.iter().filter(|r| {
        r.dir == Direction::ServerToClient
            && [
                Opcode::ITEM_CREATE_OBJECT.0,
                Opcode::MOVEMENT_POSITION_EVENT.0,
                Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0,
            ]
            .contains(&r.opcode)
            && [PLAYER, MOVER].contains(&ObjectId(u32::from_le_bytes(
                r.payload[4..8].try_into().unwrap(),
            )))
            && (r.opcode != Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
                || u32::from_le_bytes(r.payload[4..8].try_into().unwrap()) == MOVER.0)
            && r.t_rel_micros < 91_500_548
    }) {
        stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode(row.opcode),
                body: row.payload[4..].to_vec(),
            },
            LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
        );
    }
    let start = stream
        .presence(MOVER)
        .and_then(|p| p.position)
        .expect("recorded mover");
    let block = start.cell.landblock();
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
    .expect("bodyless scene");
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("bodyless production sync");
    assert!(
        scene.character.is_none(),
        "no PhysicsWorld owner in this fallback"
    );
    assert!(stream.physics.handle(MOVER).is_none());
    scene.update(
        Default::default(),
        Default::default(),
        LocalTime(START - 0.1),
        1.0 / 30.0,
    );
    let approach = corpus
        .blobs
        .iter()
        .find(|r| {
            r.t_rel_micros == 91_500_548
                && r.opcode == Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
                && u32::from_le_bytes(r.payload[4..8].try_into().unwrap()) == MOVER.0
        })
        .expect("the recorded approach");
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode(approach.opcode),
            body: approach.payload[4..].to_vec(),
        },
        LocalTime(START),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("bodyless approach after create");
    let mut clock_changes = 0;
    let mut turning = false;
    for frame in 0..=150 {
        let old = scene.server_object_sequence(MOVER).unwrap();
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(START + f64::from(frame) / 30.0),
            1.0 / 30.0,
        );
        let got = scene.server_object_sequence(MOVER).unwrap();
        assert!(
            !got.nodes().is_empty(),
            "real DAT animation remains present"
        );
        clock_changes += usize::from((old.frame_number() - got.frame_number()).abs() > 1e-5);
        let command = scene.server_object_move_to_state(MOVER).unwrap().0;
        turning |= [MotionCommand::TURN_LEFT, MotionCommand::TURN_RIGHT].contains(&command);
        assert_eq!(
            scene.server_object_position(MOVER).unwrap().frame.origin,
            start.frame.origin,
            "the explicitly bodyless path still does not integrate root translation"
        );
    }
    assert!(
        turning,
        "queued real-DAT link completes and permits a turn command"
    );
    assert!(
        clock_changes > 0,
        "skipping both owners would leave a frozen pose"
    );
    scene
        .draw(&mut gpu)
        .expect("bodyless render still consumes the shared driver");
}

/// Behaviour: movement.remote.a-remote-approach-moves-by-dat-root-motion-and-publishes-its-position
#[test]
fn remote_approach_uses_real_dat_root_motion_and_publishes_achieved_position() {
    let store = std::sync::Arc::new(
        dereth_dat::testing::open_store().expect("DERETH_TEST_DAT_DIR required"),
    );
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: 320,
            height: 240,
            ..Default::default()
        },
    )
    .expect("required headless WARP");
    let rows = Corpus::load("long-solo-play")
        .expect("locked corpus decodes")
        .expect("long-solo-play required")
        .blobs;
    let rows: Vec<_> = rows
        .into_iter()
        .filter(|r| {
            r.dir == Direction::ServerToClient
                && [
                    Opcode::ITEM_CREATE_OBJECT.0,
                    Opcode::MOVEMENT_POSITION_EVENT.0,
                    Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0,
                ]
                .contains(&r.opcode)
                && [PLAYER, MOVER].contains(&ObjectId(u32::from_le_bytes(
                    r.payload[4..8].try_into().unwrap(),
                )))
                && (r.opcode != Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
                    || u32::from_le_bytes(r.payload[4..8].try_into().unwrap()) == MOVER.0)
                && r.t_rel_micros <= 91_500_548
        })
        .collect();
    let mut stream = ObjectStream::new();
    for row in rows.iter().filter(|r| r.t_rel_micros < 91_500_548) {
        stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode(row.opcode),
                body: row.payload[4..].to_vec(),
            },
            LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
        );
    }
    let start = stream
        .presence(MOVER)
        .and_then(|p| p.position)
        .expect("recorded mover");
    let block = start.cell.landblock();
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
    .expect("scene");
    let region = dereth_world_data::landblock::load_region(&store).expect("region");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("physics owner");
    // This unbound observer is not the recorded player. It stands nearby so the 96 m object
    // activity gate includes the remote bodies, but outside their approach corridor.
    let mut observer = start;
    observer.frame.origin.x += 3.0;
    scene
        .character
        .as_ref()
        .unwrap()
        .land()
        .load_block_cells(block);
    scene.character.as_mut().unwrap().teleport(observer);
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("render-side production sync");
    stream.sync_physics(&store, &mut scene.character.as_mut().unwrap().world);
    let handle = stream.physics.handle(MOVER).expect("remote physics body");
    scene.update(
        Default::default(),
        Default::default(),
        LocalTime(START - 0.1),
        1.0 / 30.0,
    );
    let initial_velocity = scene
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .unwrap()
        .velocity_vector;
    assert_eq!(
        (initial_velocity.x, initial_velocity.y),
        (0.0, 0.0),
        "later horizontal inertia must come from the motion callback, not capture setup"
    );
    let row = rows.last().unwrap();
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode(row.opcode),
            body: row.payload[4..].to_vec(),
        },
        LocalTime(START),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("approach arrives after create");
    let mut moved = 0.0_f32;
    let mut walk_samples = 0;
    let mut clock_samples = 0;
    let mut floor_samples = 0;
    let mut velocity_samples = 0;
    let mut sticky_seen = false;
    for frame in 0..=240 {
        let now = LocalTime(START + f64::from(frame) / 30.0);
        // Deliberately no ObjectStream input or position sync during observation.
        let before = scene.character.as_ref().unwrap().world.get(handle).unwrap();
        let old_time = before.update_time;
        let old_position = before.position;
        let was_ground = before.transient_state.on_walkable();
        let scale = before.scale;
        let mut inertial = dereth_physics::obj::PhysicsObj::new(
            MOVER,
            std::sync::Arc::clone(&before.geometry),
            old_time,
            true,
        );
        inertial.state = before.state;
        inertial.transient_state = before.transient_state;
        inertial.velocity_vector = before.velocity_vector;
        inertial.acceleration_vector = before.acceleration_vector;
        inertial.omega_vector = before.omega_vector;
        inertial.contact_plane = before.contact_plane;
        inertial.friction = before.friction;
        let mut expected = scene.server_object_sequence(MOVER).unwrap();
        let was_walk =
            scene.server_object_move_to_state(MOVER).unwrap().0 == MotionCommand::WALK_FORWARD;
        scene.update(Default::default(), Default::default(), now, 1.0 / 30.0);
        let body = scene.character.as_ref().unwrap().world.get(handle).unwrap();
        let actual = scene
            .server_object_position(MOVER)
            .expect("published body position");
        assert_eq!(
            actual, body.position,
            "render/target position must be collision result"
        );
        assert_eq!(
            scene.server_object_reported_position(MOVER),
            Some(start),
            "no incoming position updates"
        );
        moved = moved.max(distance(&start, &actual));
        let q = body.update_time - old_time;
        sticky_seen |= scene.server_object_sticky(MOVER) == Some(PLAYER);
        let mut offset = Frame::default();
        if (0.0..0.2).contains(&q) {
            expected.update(q, Some(&mut offset), &mut Vec::new());
            let got = scene.server_object_sequence(MOVER).unwrap();
            if was_ground == body.transient_state.on_walkable()
                && expected
                    .nodes()
                    .iter()
                    .map(|n| n.anim_id)
                    .collect::<Vec<_>>()
                    == got.nodes().iter().map(|n| n.anim_id).collect::<Vec<_>>()
                && expected.curr() == got.curr()
                && expected.first_cyclic() == got.first_cyclic()
            {
                assert!(
                    (got.frame_number() - expected.frame_number()).abs() < 1e-5,
                    "animation advanced more than once at {}: got {}, expected {}",
                    now.0,
                    got.frame_number(),
                    expected.frame_number()
                );
                clock_samples += 1;
            }
            let origin_scale = if was_ground { scale } else { 0.0 };
            offset.origin.x *= origin_scale;
            offset.origin.y *= origin_scale;
            offset.origin.z *= origin_scale;
            let mut desired = dereth_animation::frame::combine(&old_position.frame, &offset);
            inertial.update_physics_internal(q, &mut desired);
            // A creature in walkable contact carries no velocity: contact zeroes acceleration,
            // and the inertial integration of `desired` (the old frame combined with the scaled
            // root motion) moves nothing when the squared velocity is zero. Two things follow, and
            // both can fail:
            //
            // * the published origin is the dat floor. This room's floor plane is z = 0, so the
            //   resting origin is the low path sphere's bottom after the body's own scale,
            //   `(0.55 - 0.54) * 0.9 = 0.009`. Without the scale it is 0.010, ten times the 1e-4
            //   tolerance away.
            // * the shadow never sinks through that floor. The step-down height is the setup's
            //   times the part array's vertical scale; without that factor the 0.9-scale golem
            //   loses contact on alternate sub-steps and penetrates the floor on some frames.
            if scene.server_object_sticky(MOVER).is_none() && body.transient_state.on_walkable() {
                // This DAT room's floor plane is z=0. The low setup sphere's bottom, after
                // the body's scale, therefore determines the achieved contact origin.
                let low = body.geometry.path_spheres()[0];
                let floor_origin = (low.radius - low.center.z) * scale;
                assert!(
                    (actual.frame.origin.z - floor_origin).abs() < 1e-4,
                    "contact must publish the DAT floor at {}: got {}, floor {}",
                    now.0,
                    actual.frame.origin.z,
                    floor_origin
                );
                assert!(
                    desired.origin.z >= actual.frame.origin.z - 1e-4,
                    "a body in walkable contact must not integrate through the floor at {}: \
                     desired {}, published {}",
                    now.0,
                    desired.origin.z,
                    actual.frame.origin.z
                );
                floor_samples += 1;
            }
            // A grounded creature's `velocity_vector` stays at rest and its translation comes
            // entirely from the animation's root motion, so velocity is not the evidence of
            // movement here (only leaving the ground writes it; see
            // `movement::remote_walk_animation`, and `movement::jump_charge_release` for a jump).
            // `cached_velocity` is: the object update writes it as the achieved offset over the
            // quantum, so a non-zero horizontal value is the collision-tested proof that root
            // motion moved the body.
            if body.cached_velocity.x.abs() + body.cached_velocity.y.abs() > 0.1 {
                velocity_samples += 1;
            }
        }
        if was_walk
            && was_ground
            && q > 0.0
            && q < 0.2
            && scene.server_object_move_to_state(MOVER).unwrap().0 == MotionCommand::WALK_FORWARD
            && scene.server_object_sticky(MOVER).is_none()
        {
            walk_samples += 1;
        }
    }
    assert!(walk_samples > 0, "real DAT forward cycle must run");
    assert!(
        clock_samples > 0,
        "exactly-once animation clock samples required"
    );
    assert!(
        floor_samples > 0,
        "the grounded, non-sticky floor invariant must be exercised"
    );
    assert!(
        velocity_samples > 0,
        "the cached achieved offset must show real horizontal \
         translation"
    );
    assert!(
        moved > 1.0,
        "remote body must move without new server positions"
    );
    assert!(
        sticky_seen,
        "autonomous arrival, not a later packet, must initiate sticky handoff"
    );
    assert_eq!(scene.draw.stats.remote_move_tos_failed, 0);
    assert!(!scene.server_object_move_to(MOVER).unwrap().0);
    assert_eq!(scene.server_object_motions_pending(MOVER), Some(0));
    assert_eq!(scene.server_object_sticky(MOVER), None);
    assert_eq!(scene.server_object_target(MOVER), None);
    assert_eq!(
        scene.server_object_motion(MOVER).unwrap().1,
        MotionCommand::READY
    );
    assert_eq!(
        scene.server_object_sequence(MOVER).unwrap().velocity,
        dereth_primitives::Vec3::ZERO
    );
    let stopped = scene.server_object_position(MOVER).unwrap();
    for frame in 241..=270 {
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(START + f64::from(frame) / 30.0),
            1.0 / 30.0,
        );
        assert!(
            distance(&stopped, &scene.server_object_position(MOVER).unwrap()) < 1e-4,
            "finished movement must remain stopped with no new commands"
        );
    }
    // A constructed second command returns to the recorded starting origin. This is explicitly
    // not a recorded server journey: it extends the lifecycle without supplying any new 0xF748.
    use dereth_protocol::movement::{
        movement_type, MoveToArm, MovementBody, MovementSetObjectMovement,
    };
    let mut msg =
        MovementSetObjectMovement::read(&mut dereth_protocol::Reader::new(&row.payload[4..]))
            .unwrap();
    let mut buf = msg.decoded_movement().unwrap();
    let MoveToArm::MoveToObject {
        mut origin,
        params,
        run_rate,
        ..
    } = buf.body.decode_move_to().unwrap().unwrap()
    else {
        panic!("the selected capture command must be MoveToObject");
    };
    origin.objcell_id = start.cell.0;
    origin.origin = dereth_protocol::types::Vec3 {
        x: start.frame.origin.x,
        y: start.frame.origin.y,
        z: start.frame.origin.z,
    };
    buf.movement_timestamp = buf.movement_timestamp.wrapping_add(1);
    buf.body.movement_type = movement_type::MOVE_TO_POSITION;
    buf.body.unhandled = MovementBody::encode_move_to(&MoveToArm::MoveToPosition {
        origin,
        params,
        run_rate,
    });
    msg.movement = MovementSetObjectMovement::encode_movement(&buf).unwrap();
    let mut writer = dereth_protocol::Writer::new();
    msg.write(&mut writer).unwrap();
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
            body: writer.into_inner(),
        },
        LocalTime(START + 9.0),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("second production movement dispatch");
    assert!(
        scene.server_object_move_to(MOVER).unwrap().0,
        "a new destination restarts movement"
    );
    let mut restarted_forward = false;
    for frame in 271..=750 {
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(START + f64::from(frame) / 30.0),
            1.0 / 30.0,
        );
        restarted_forward |=
            scene.server_object_move_to_state(MOVER).unwrap().0 == MotionCommand::WALK_FORWARD;
        assert_eq!(scene.server_object_reported_position(MOVER), Some(start));
        assert_eq!(
            scene.server_object_position(MOVER).unwrap(),
            scene
                .character
                .as_ref()
                .unwrap()
                .world
                .get(handle)
                .unwrap()
                .position
        );
    }
    assert!(restarted_forward);
    assert!(!scene.server_object_move_to(MOVER).unwrap().0);
    assert_eq!(scene.server_object_motions_pending(MOVER), Some(0));
    assert_eq!(scene.draw.stats.remote_move_tos_failed, 0);
    assert!(
        distance(&stopped, &scene.server_object_position(MOVER).unwrap()) > 1.0,
        "second approach must translate the body, not only issue a command"
    );
    let dereth_protocol::movement::MovementParameters::MoveTo {
        distance_to_object, ..
    } = params
    else {
        unreachable!()
    };
    let body = scene.character.as_ref().unwrap().world.get(handle).unwrap();
    let clearance = dereth_animation::motion::moveto::cylinder_distance(
        body.radius(),
        body.height(),
        &body.position,
        0.0,
        0.0,
        &start,
    );
    assert!(
        clearance <= distance_to_object,
        "second arrival clearance {clearance} exceeds {distance_to_object}"
    );

    // Constructed 0xF74B lifecycle edges, not recorded server state changes. Applying the state
    // changes the word in place and does NOT clear ACTIVE when STATIC is set;
    // the activation routine only refuses a new activation. Both animation owners therefore
    // remain reachable unless the scene explicitly releases its old physics attachment.
    let original_state = stream.physics_state(MOVER).expect("recorded physics state");
    assert_eq!(
        original_state & dereth_physics::obj::PhysicsState::STATIC_PS,
        0
    );
    let mut now = body.update_time;
    assert!(body.motion.is_some());
    assert!(
        body.transient_state.is_active(),
        "exercise the already-active STATIC edge"
    );
    let mut phase_changes = 0;
    for make_static in [true, false] {
        let was_active = scene
            .character
            .as_ref()
            .unwrap()
            .world
            .get(handle)
            .unwrap()
            .transient_state
            .is_active();
        let presence = stream.presence(MOVER).unwrap();
        let event = presence.state_ts.wrapping_add(1);
        let state = if make_static {
            original_state | dereth_physics::obj::PhysicsState::STATIC_PS
        } else {
            original_state
        };
        let message = dereth_protocol::objects::ItemSetState {
            id: MOVER,
            state,
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: presence.instance,
                event,
            },
        };
        stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::ITEM_SET_STATE,
                body: dereth_protocol::write_body(&message).unwrap(),
            },
            LocalTime(now),
        );
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("0xF74B scene state");
        stream.sync_physics(&store, &mut scene.character.as_mut().unwrap().world);
        assert_eq!(
            stream.presence(MOVER).unwrap().state_ts,
            event,
            "state gate accepted"
        );
        assert_eq!(
            stream.physics.handle(MOVER),
            Some(handle),
            "state does not respawn a body"
        );
        let body = scene.character.as_ref().unwrap().world.get(handle).unwrap();
        assert_eq!(body.state.0, state);
        assert_eq!(
            body.transient_state.is_active(),
            was_active,
            "set_state must preserve ACTIVE, including an already inactive body"
        );
        let fallback_position = scene.server_object_position(MOVER).unwrap();
        for _ in 0..4 {
            let mut expected = scene.server_object_sequence(MOVER).unwrap();
            let old_phase = expected.frame_number();
            let next = now + 0.1;
            // The nearby-player activation gate resets an inactive body's clock to now
            // during activation. Reattaching does not bypass that zero-quantum frame.
            let active = scene
                .character
                .as_ref()
                .unwrap()
                .world
                .get(handle)
                .unwrap()
                .transient_state
                .is_active();
            let quantum = if !make_static && !active {
                0.0
            } else {
                next - now
            };
            expected.update(quantum, None, &mut Vec::new());
            scene.update(Default::default(), Default::default(), LocalTime(next), 0.1);
            let got = scene.server_object_sequence(MOVER).unwrap();
            assert_eq!(
                got.nodes().iter().map(|n| n.anim_id).collect::<Vec<_>>(),
                expected
                    .nodes()
                    .iter()
                    .map(|n| n.anim_id)
                    .collect::<Vec<_>>(),
                "attachment changes must retain the existing real-DAT sequence"
            );
            assert_eq!(got.curr(), expected.curr());
            assert_eq!(got.first_cyclic(), expected.first_cyclic());
            assert!(
                (got.frame_number() - expected.frame_number()).abs() < 1e-5,
                "STATIC={make_static}: exactly one phase advance, got {}, expected {}",
                got.frame_number(),
                expected.frame_number()
            );
            phase_changes += usize::from((got.frame_number() - old_phase).abs() > 1e-5);
            let body = scene.character.as_ref().unwrap().world.get(handle).unwrap();
            assert_eq!(
                body.motion.is_none(),
                make_static,
                "only one owner after the edge"
            );
            if make_static {
                assert_eq!(
                    scene.server_object_position(MOVER).unwrap(),
                    fallback_position,
                    "the existing static fallback remains animation-only"
                );
            } else {
                assert_eq!(scene.server_object_position(MOVER).unwrap(), body.position);
            }
            now = next;
        }
    }
    assert!(
        phase_changes >= 6,
        "both fallback and reattached clocks must actually advance"
    );

    // A fresh incoming command after reattachment must again move through collision and
    // publish the achieved position. Equality of two stationary positions alone is not proof.
    origin.origin = dereth_protocol::types::Vec3 {
        x: stopped.frame.origin.x,
        y: stopped.frame.origin.y,
        z: stopped.frame.origin.z,
    };
    origin.objcell_id = stopped.cell.0;
    buf.movement_timestamp = buf.movement_timestamp.wrapping_add(1);
    buf.body.unhandled = MovementBody::encode_move_to(&MoveToArm::MoveToPosition {
        origin,
        params,
        run_rate,
    });
    msg.movement = MovementSetObjectMovement::encode_movement(&buf).unwrap();
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_SET_OBJECT_MOVEMENT,
            body: dereth_protocol::write_body(&msg).unwrap(),
        },
        LocalTime(now),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("reattached approach command");
    let reattached_start = scene.server_object_position(MOVER).unwrap();
    let mut reattached_moved = 0.0_f32;
    for _ in 0..240 {
        now += 1.0 / 30.0;
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
        let actual = scene.server_object_position(MOVER).unwrap();
        let body = scene.character.as_ref().unwrap().world.get(handle).unwrap();
        assert!(body.motion.is_some());
        assert_eq!(
            actual, body.position,
            "reattached body remains the position owner"
        );
        assert_eq!(scene.server_object_reported_position(MOVER), Some(start));
        reattached_moved = reattached_moved.max(distance(&reattached_start, &actual));
    }
    // The bound is a fraction of this constructed leg's length, not a retail number: the leg is
    // the trip back to `stopped`, where the first leg's arrival left the body, about 1.8 m of
    // which the move-to's own arrival stand-off takes up some. The claim is that the reattached
    // body physically translates through collision. The first leg's arrival distance is pinned
    // exactly (`radius_sum + 0.3`) by `movement::remote_approach_replacement`, and its stand-off
    // clearance by the `clearance` assertion above.
    assert!(
        reattached_moved > 0.5,
        "reattachment must restore physical root motion"
    );
    assert!(!scene.server_object_move_to(MOVER).unwrap().0);
    assert_eq!(scene.server_object_motions_pending(MOVER), Some(0));
    assert_eq!(
        scene.server_object_motion(MOVER).unwrap().1,
        MotionCommand::READY
    );
    scene
        .draw(&mut gpu)
        .expect("shared part borrows survive actual render submission");
}
