//! A remote creature walking a flat dat floor never flaps between its walk cycle and its falling
//! animation: its ground-contact bit stays steady, so it raises no ground edges while it walks.
//! Walkability changes are the only source of the hit-ground and leave-ground callbacks, and each
//! reapplies the interpreted movement, which plays `MotionCommand::FALLING` instead of the forward
//! command for a gravity-affected creature off the ground; an alternating contact bit is the flap.
//! The step-down probe compares the step-down height with twice the first sphere's radius, and
//! both the spheres and the step-down height carry the part array's scale.
//! Fixture: long-solo-play's Sparring Golem (scale 0.9) and its recorded approach, with no position
//! updates after it, on a headless software device; every assertion is a count over the window.
#![cfg(gpu)]

use dereth_animation::MotionCommand;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_protocol::Opcode;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::SceneWrites;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

const PLAYER: ObjectId = ObjectId(0x5000_000a);
const MOVER: ObjectId = ObjectId(0x8000_09d2);
const START: f64 = 91.5;
/// The recorded approach command, the last blob this test admits.
const APPROACH_MICROS: u64 = 91_500_548;

/// One sampled frame: the ground-contact predicate and the selected movement command and pose.
#[derive(Debug, Clone, PartialEq)]
struct Sample {
    frame: u32,
    on_ground: bool,
    command: MotionCommand,
    nodes: Vec<DataId>,
}

/// Behaviour: movement.remote.a-recorded-remote-walk-never-reaches-the-falling-animation
#[test]
fn a_recorded_remote_walk_raises_no_ground_edges_and_never_reaches_the_falling_animation() {
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
    let rows: Vec<_> = Corpus::load("long-solo-play")
        .expect("locked corpus decodes")
        .expect("long-solo-play required")
        .blobs
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
                && r.t_rel_micros <= APPROACH_MICROS
        })
        .collect();
    let mut stream = ObjectStream::new();
    for row in rows.iter().filter(|r| r.t_rel_micros < APPROACH_MICROS) {
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
    // The unbound observer is not the recorded player; park it inside the 96 m activity
    // radius but off the golem's corridor, exactly as `movement::remote_root_motion` does.
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
    let scale = scene
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .unwrap()
        .scale;
    assert!(
        (scale - 1.0).abs() > 1e-6,
        "the premise of this test is a **scaled** creature: long-solo-play's golem is {scale}. \
         A scale of 1 cannot separate a scaled step height from an unscaled one."
    );
    scene.update(
        Default::default(),
        Default::default(),
        LocalTime(START - 0.1),
        1.0 / 30.0,
    );
    let approach = rows.last().expect("the recorded approach");
    assert_eq!(approach.t_rel_micros, APPROACH_MICROS);
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode(approach.opcode),
            body: approach.payload[4..].to_vec(),
        },
        LocalTime(START),
    );
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("approach arrives after create");

    let mut samples: Vec<Sample> = Vec::new();
    for frame in 0..=240u32 {
        let now = LocalTime(START + f64::from(frame) / 30.0);
        // Deliberately no ObjectStream input or position sync during observation: every edge below
        // is the client's own physics, not a server correction landing mid-step.
        scene.update(Default::default(), Default::default(), now, 1.0 / 30.0);
        let ts = scene
            .character
            .as_ref()
            .unwrap()
            .world
            .get(handle)
            .unwrap()
            .transient_state;
        samples.push(Sample {
            frame,
            on_ground: ts.in_contact() && ts.on_walkable(),
            command: scene
                .server_object_move_to_state(MOVER)
                .expect("move-to state")
                .0,
            nodes: scene.server_object_pose(MOVER).expect("live pose").0,
        });
    }

    // Non-vacuity first: this run has to be a walk, or "it never fell" means nothing.
    let walking = samples
        .iter()
        .filter(|s| s.command == MotionCommand::WALK_FORWARD)
        .count();
    assert!(
        walking >= 30,
        "the recorded approach must spend at least a second walking, not {walking} frames: {:?}",
        samples
            .iter()
            .map(|s| s.command)
            .collect::<std::collections::BTreeSet<_>>()
    );

    // The landing. Everything before it is the body settling onto the floor from its spawn
    // height, which is a real ground edge and not the defect.
    let landed = samples
        .iter()
        .position(|s| s.on_ground)
        .expect("the body reaches the floor");
    let after = &samples[landed..];

    // (1) The bit itself. Ground callbacks fire only on edges, so transitions of
    // `CONTACT && ON_WALKABLE` count the sampled ground-entry/ground-exit changes.
    let flips: Vec<u32> = after
        .windows(2)
        .filter(|w| w[0].on_ground != w[1].on_ground)
        .map(|w| w[1].frame)
        .collect();
    assert!(
        flips.len() <= 1,
        "a creature walking a flat floor left the ground {} times in {} frames (at {:?}). \
         Each one is a walkability edge and therefore a ground-entry/ground-exit callback, \
         and movement interpretation swaps the forward command for \
         `MotionCommand::FALLING` on every other one.",
        flips.len(),
        after.len(),
        flips
    );

    // (2) The pose that bit chooses. With the bit steady the walk ladder changes only when a DAT
    // link runs out -- a handful of times over eight seconds, not once every other sub-step.
    let pose_changes = after
        .windows(2)
        .filter(|w| w[0].nodes != w[1].nodes)
        .count();
    assert!(
        pose_changes <= 8,
        "the animation ladder changed {pose_changes} times in {} frames; a flapping contact bit \
         re-links it on every edge. Ladders seen: {:?}",
        after.len(),
        after
            .iter()
            .map(|s| s.nodes.clone())
            .collect::<std::collections::BTreeSet<_>>()
    );

    // (3) And the two together, as a player sees it: while the creature holds its walk command
    // it is on the ground on every single frame.
    let airborne_while_walking = after
        .iter()
        .filter(|s| s.command == MotionCommand::WALK_FORWARD && !s.on_ground)
        .count();
    assert_eq!(
        airborne_while_walking, 0,
        "{airborne_while_walking} of {walking} walking frames report no walkable contact"
    );
}
