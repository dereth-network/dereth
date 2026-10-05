//! A remote creature's recorded approach to the player, its replacement, the sticky correction
//! that follows arrival, clean-up, and a second approach starting again. The replacement unsticks
//! the creature and leaves no stale target, and each approach turns, walks and faces its target.
//! Fixture: long-solo-play's recorded blobs for the player and its Sparring Golem (0x800009D2)
//! from t=90 s to t=100 s, unchanged, through `ObjectStream`, `WorldScene` and
//! `ObjectStream::sync_physics` on a headless software device; a local body supplies the physics
//! world and stands off the golem's path. The recording holds server positions, not per-frame
//! poses, so headings are compared with the recorded server's own snaps, never assumed monotonic.

#![cfg(gpu)]

use dereth_animation::motion::moveto::{distance, position_heading};
use dereth_animation::table::MovementType;
use dereth_animation::MotionCommand;
use dereth_client_net::client_session::testing::{Corpus, CorpusBlob, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{LocalTime, ObjectId, Position};
use dereth_protocol::movement::{
    MoveToArm, MovementBody, MovementParameters, MovementSetObjectMovement,
};
use dereth_protocol::{Message, Opcode};
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::world_scene::SceneWrites;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

const PLAYER: ObjectId = ObjectId(0x5000_000a);
const MOVER: ObjectId = ObjectId(0x8000_09d2); // long-solo-play's Sparring Golem
const START: f64 = 90.0;
const WIRE_END: f64 = 100.0;

fn time(row: &CorpusBlob) -> f64 {
    std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()
}

fn subject(row: &CorpusBlob) -> ObjectId {
    ObjectId(u32::from_le_bytes(
        row.payload[4..8].try_into().expect("an object id"),
    ))
}

fn movement(row: &CorpusBlob) -> MovementBody {
    let mut r = dereth_protocol::Reader::new(&row.payload[4..]);
    MovementSetObjectMovement::read(&mut r)
        .expect("recorded 0xF74C")
        .decoded_movement()
        .expect("recorded movement buffer")
        .body
}

fn rows() -> Vec<CorpusBlob> {
    Corpus::load("long-solo-play")
        .expect("decode locked corpus")
        .expect("required long-solo-play")
        .blobs
        .into_iter()
        .filter(|row| {
            row.dir == Direction::ServerToClient
                && [
                    Opcode::ITEM_CREATE_OBJECT.0,
                    Opcode::MOVEMENT_POSITION_EVENT.0,
                    Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0,
                ]
                .contains(&row.opcode)
                && [PLAYER, MOVER].contains(&subject(row))
                && (row.opcode != Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0 || subject(row) == MOVER)
                && time(row) <= WIRE_END
        })
        .collect()
}

fn apply(stream: &mut ObjectStream, row: &CorpusBlob, now: LocalTime) {
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode(row.opcode),
            body: row.payload[4..].to_vec(),
        },
        now,
    );
}

#[derive(Debug)]
struct Sample {
    time: f64,
    moving: bool,
    kind: MovementType,
    top: ObjectId,
    command: MotionCommand,
    auxiliary: MotionCommand,
    nodes: usize,
    initialized: bool,
    motions_pending: usize,
    sticky: Option<ObjectId>,
    tracking: Option<ObjectId>,
    position: Position,
    target_position: Position,
    reported: Position,
    failures: u64,
}

impl Sample {
    fn read(scene: &WorldScene, now: LocalTime) -> Self {
        let (moving, kind, top) = scene.server_object_move_to(MOVER).expect("live mover");
        let (command, auxiliary, nodes, initialized) = scene
            .server_object_move_to_state(MOVER)
            .expect("move-to state");
        Self {
            time: now.0,
            moving,
            kind,
            top,
            command,
            auxiliary,
            nodes,
            initialized,
            motions_pending: scene
                .server_object_motions_pending(MOVER)
                .expect("motion queue"),
            sticky: scene.server_object_sticky(MOVER),
            tracking: scene.server_object_target(MOVER),
            position: scene.server_object_position(MOVER).expect("mover position"),
            target_position: scene
                .server_object_position(PLAYER)
                .expect("target position"),
            reported: scene
                .server_object_reported_position(MOVER)
                .expect("wire position"),
            failures: scene.draw.stats.remote_move_tos_failed,
        }
    }

    fn clean(&self) {
        assert!(!self.moving, "{self:?}");
        assert_eq!(self.kind, MovementType::Invalid, "{self:?}");
        assert_eq!(self.top, ObjectId(0), "{self:?}");
        assert_eq!(
            (self.command, self.auxiliary, self.nodes),
            (MotionCommand::NONE, MotionCommand::NONE, 0),
            "{self:?}"
        );
    }

    fn approaching(&self, command: MotionCommand, nodes: usize) {
        assert!(self.moving && self.initialized, "{self:?}");
        assert_eq!(
            (self.kind, self.top),
            (MovementType::MoveToObject, PLAYER),
            "{self:?}"
        );
        assert_eq!(
            (self.command, self.auxiliary, self.nodes),
            (command, MotionCommand::NONE, nodes),
            "{self:?}"
        );
        assert_eq!(self.tracking, Some(PLAYER), "{self:?}");
        assert_eq!(self.sticky, None, "{self:?}");
    }
}

fn at(samples: &[Sample], t: f64) -> &Sample {
    samples
        .iter()
        .find(|s| s.time >= t)
        .expect("sampled interval")
}

fn heading_error(a: f32, b: f32) -> f32 {
    ((a - b + 180.0).rem_euclid(360.0) - 180.0).abs()
}

/// Behaviour: movement.approach.a-second-one-replaces-the-first-rather-than-queueing
/// Behaviour: movement.approach.the-old-approachs-clean-up-never-outlives-the-next-approachs-subject
#[test]
fn recorded_approach_replacement_unsticks_and_starts_again() {
    let store = std::sync::Arc::new(
        dereth_dat::testing::open_store().expect("retail DATs required: set DERETH_TEST_DAT_DIR"),
    );
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: 320,
            height: 240,
            ..DeviceConfig::default()
        },
    )
    .expect("required headless WARP device");
    let rows = rows();
    let movements: Vec<_> = rows
        .iter()
        .filter(|r| r.opcode == Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0 && time(r) >= START)
        .map(|r| (time(r), movement(r)))
        .collect();
    let approaches: Vec<_> = movements
        .iter()
        .filter_map(
            |(t, body)| match body.decode_move_to().expect("movement arm") {
                Some(MoveToArm::MoveToObject { target, params, .. }) => {
                    assert_eq!(target, PLAYER);
                    let MovementParameters::MoveTo {
                        distance_to_object, ..
                    } = params
                    else {
                        panic!("recorded approach parameters");
                    };
                    Some((*t, distance_to_object))
                }
                _ => None,
            },
        )
        .collect();
    assert_eq!(
        approaches.len(),
        2,
        "the selected recording contains two approaches"
    );
    let sticks: Vec<_> = movements
        .iter()
        .filter(|(_, b)| b.sticky_object == Some(PLAYER))
        .collect();
    assert_eq!(sticks.len(), 2, "two recorded sticky handoffs");
    let (first, second) = (approaches[0].0, approaches[1].0);
    let (first_wire_stick, second_wire_stick) = (sticks[0].0, sticks[1].0);

    let mut stream = ObjectStream::new();
    // No player-created event is synthesised: both recorded objects stay server-positioned. The
    // local body supplies the physics world `App::sync_objects` uses, but is not the target.
    let mut next = 0;
    while next < rows.len() && time(&rows[next]) < START {
        apply(&mut stream, &rows[next], LocalTime(time(&rows[next])));
        next += 1;
    }
    let player = stream
        .presence(PLAYER)
        .and_then(|p| p.position)
        .expect("recorded player position");
    let block = player.cell.landblock();
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
            ..SceneConfig::default()
        },
    )
    .expect("scene loads");
    let region = dereth_world_data::landblock::load_region(&store).expect("retail region");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("physics world and body");
    // Keep the observer within the 96 m object activity radius, outside the two-object path.
    // A 3x3 window keeps the dungeon's negative-y room resident when the viewer recenters.
    let mut observer = stream.presence(MOVER).and_then(|p| p.position).unwrap();
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
        .expect("recorded objects sync");
    stream.sync_physics(&store, &mut scene.character.as_mut().expect("body").world);
    let body = scene.character.as_ref().expect("body");
    let radii = [MOVER, PLAYER].map(|id| {
        let handle = stream
            .physics
            .handle(id)
            .expect("captured object has a physics body");
        body.world.get(handle).expect("live physics body").radius()
    });
    assert!(
        radii.iter().all(|r| *r > 0.0),
        "zero dimensions would invalidate arrival: {radii:?}"
    );
    let radius_sum = radii[0] + radii[1];

    let mut samples = Vec::new();
    let mut armed_in_handler: Vec<(f64, bool)> = Vec::new();
    // Four seconds without new messages after WIRE_END let the recorded final style link finish.
    for frame in 0..=420 {
        let now = LocalTime(START + f64::from(frame) / 30.0);
        while next < rows.len() && time(&rows[next]) <= now.0 {
            apply(&mut stream, &rows[next], now);
            next += 1;
        }
        // `App::sync_objects` order, not only the render-side sync.
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("objects sync each frame");
        // Registering a target sends an unconditional first update inside
        // the 0xF74C move-to-object handler, before any physics pass. The approach is
        // therefore initialized when `sync_objects` returns; it does not wait for the
        // next object update. Sample here, between the handler and the physics step.
        armed_in_handler.push((
            now.0,
            scene
                .server_object_move_to_state(MOVER)
                .is_some_and(|s| s.3),
        ));
        stream.sync_physics(&store, &mut scene.character.as_mut().expect("body").world);
        scene.update(Default::default(), Default::default(), now, 1.0 / 30.0);
        samples.push(Sample::read(&scene, now));
    }
    assert_eq!(
        next,
        rows.len(),
        "all selected, unchanged blobs reached production"
    );
    assert_eq!(
        scene.draw.stats.remote_move_tos_performed,
        u64::try_from(approaches.len()).unwrap()
    );

    let (armed_t, armed) = *armed_in_handler
        .iter()
        .find(|(t, _)| *t >= first)
        .expect("the arm's own frame");
    assert!(
        armed,
        "the target subscription's first update runs inside the 0xF74C handler, so the approach is initialised when sync_objects returns (arm at t={first:.3}, sampled t={armed_t:.3})"
    );
    assert!(
        armed_in_handler
            .iter()
            .filter(|(t, _)| *t < first)
            .all(|(_, a)| !a),
        "and nothing was approaching before the arm arrived"
    );
    let idle = at(&samples, first - 0.1);
    idle.clean();
    assert_eq!(
        (idle.tracking, idle.sticky, idle.motions_pending),
        (None, None, 0)
    );
    let turn = at(&samples, first);
    turn.approaching(MotionCommand::TURN_LEFT, 3);
    assert_eq!(turn.failures, 0);
    let walk = samples
        .iter()
        .find(|s| {
            s.time > first && s.time < first_wire_stick && s.command == MotionCommand::WALK_FORWARD
        })
        .expect("first approach leaves its turn and starts its forward node");
    walk.approaching(MotionCommand::WALK_FORWARD, 2);
    assert!(walk.motions_pending > 0, "DAT transition queued: {walk:?}");
    assert!(
        heading_error(
            dereth_animation::frame::get_heading(&turn.position.frame),
            dereth_animation::frame::get_heading(&walk.position.frame)
        ) > 1.0
    );
    // The first target update goes out when the subscription is registered, so the turn snaps
    // and the walk begins at t=91.700, where the last server word for the golem is still its
    // pre-turn heading (271.34); the recorded server's own snap onto 234.38 arrives by 91.767.
    // The oracle: the heading the node machine snapped to is the heading the recorded server
    // snapped to, read once its word has caught up.
    let snapped = dereth_animation::frame::get_heading(&walk.position.frame);
    let confirmed = samples
        .iter()
        .filter(|s| s.time >= walk.time && s.time <= walk.time + 0.5)
        .map(|s| dereth_animation::frame::get_heading(&s.reported.frame))
        .find(|reported| heading_error(snapped, *reported) < 0.01);
    assert!(
        confirmed.is_some(),
        "recorded heading reached the object: our snap {snapped:.4} must match the server's own \
         word within half a second of the walk starting at t={:.3}; reported then {:.4} ({walk:?})",
        walk.time,
        dereth_animation::frame::get_heading(&walk.reported.frame)
    );
    assert!(
        heading_error(
            dereth_animation::frame::get_heading(&walk.position.frame),
            position_heading(&walk.position, &walk.target_position)
        ) < 0.01,
        "the recorded turn faces this target; not a monotonic-heading rule"
    );
    // This replay includes server corrections. Ground-edge relinks can keep the final node
    // waiting until the recorded replacement arrives; packet timing is not a retail rendered
    // pose oracle. Autonomous arrival and completion without corrections are covered separately.
    let before_first = samples
        .iter()
        .rev()
        .find(|s| s.time < first_wire_stick)
        .unwrap();
    let first_replacement = at(&samples, first_wire_stick);
    first_replacement.clean();
    assert_eq!(first_replacement.sticky, Some(PLAYER));
    assert_eq!(
        first_replacement.failures,
        before_first.failures + u64::from(before_first.moving)
    );
    // The body walks to the stand-off rather than being dragged there. Sticky tracking registers
    // a 0.5 m radius and sends another target update only when the led position moves strictly
    // more than that from the last one sent; between this stick and its expiry the recorded
    // player drifts 3.47 mm, so no further update goes out and the sticky offset closes on the
    // position the body already holds.
    //
    // A `0xF748` for a remote body inside 96 m queues an interpolation node rather than applying
    // the endpoint at once, and a correction within the node's 0.05 completion radius of the body
    // completes without writing any offset. The recorded correction here is that case
    // (long-solo-play's last `0xF748` for the golem before this sample, t=93.8583, lands 0.7 mm
    // from the body), so the claim is that the correction landed inside the completion radius
    // and the body did not move for it. `WIRE_AGREEMENT` (1 mm) is fifty times tighter than that
    // radius and goes red on any regression of the approach; the assertion before it goes red if
    // the stick stops arriving on the stand-off.
    const NODE_COMPLETION_RADIUS: f32 = 0.05;
    const WIRE_AGREEMENT: f32 = 1e-3;
    let corrected = samples
        .iter()
        .find(|s| {
            s.time > first_wire_stick
                && s.time < second
                && s.sticky == Some(PLAYER)
                && (distance(&s.position, &s.target_position) - radius_sum - 0.3).abs() < 0.0001
        })
        .expect("collision-tested sticky correction reaches the retail DAT-radius stand-off");
    let before_wire = samples
        .iter()
        .rev()
        .find(|s| s.time < corrected.time && distance(&s.reported, &corrected.reported) > 0.0)
        .expect("a sample from before this correction reached the wire");
    let arriving = distance(&before_wire.position, &corrected.reported);
    assert!(
        arriving < NODE_COMPLETION_RADIUS,
        "the recorded correction must land inside the interpolation completion radius, \
         or the body walks toward it and this is no longer the case the bound \
         below describes: {arriving:.6} m from the body at t={:.3} ({before_wire:?})",
        before_wire.time
    );
    assert!(
        distance(&corrected.position, &corrected.reported) < WIRE_AGREEMENT,
        "the approach lands on the stand-off itself, so the stick has nothing left to close: \
         {:.6} m from the wire ({corrected:?})",
        distance(&corrected.position, &corrected.reported)
    );
    assert!(
        heading_error(
            dereth_animation::frame::get_heading(&corrected.position.frame),
            position_heading(&corrected.position, &corrected.target_position)
        ) < 0.01,
        "sticky faces its target"
    );

    // Starting sticky tracking adds one second to last frame's double time. This checkpoint is safely
    // past that deadline but before the second approach. A real ground edge can unstick earlier;
    // this test proves no stale sticky/target survives, not that this run expired by timer.
    // `movement::remote_root_motion` (no `0xF748`) and the sticky-tracking unit tests cover
    // autonomous clean-up.
    let expired = at(&samples, first_wire_stick + 1.1);
    expired.clean();
    assert_eq!(
        (expired.sticky, expired.tracking),
        (None, None),
        "no sticky or targeting may survive past the deadline"
    );
    let ready = at(&samples, second - 0.1);
    ready.clean();
    assert_eq!((ready.sticky, ready.tracking), (None, None));
    assert!(
        distance(&ready.position, &ready.target_position) > radius_sum + approaches[1].1,
        "captured target moved outside the second request's arrival radius"
    );
    let restarted = at(&samples, second);
    assert!(restarted.moving && restarted.initialized);
    assert_eq!(
        (restarted.kind, restarted.top),
        (MovementType::MoveToObject, PLAYER)
    );
    assert_eq!(restarted.tracking, Some(PLAYER));
    assert_eq!(restarted.sticky, None);
    assert!(
        restarted.motions_pending > 0,
        "second approach owns a fresh animation link"
    );
    assert!(
        samples.iter().any(|s| s.time > restarted.time
            && s.time < second_wire_stick
            && s.command == MotionCommand::WALK_FORWARD
            && s.nodes == 2),
        "the second approach leaves its completion-gated turn and issues forward movement"
    );

    // The second incoming interpreted/sticky attack cancels an approach if it is still running.
    // That cancellation is not mislabeled as successful client arrival by an aggregate count.
    let before = samples
        .iter()
        .rev()
        .find(|s| s.time < second_wire_stick)
        .unwrap();
    let replacement = at(&samples, second_wire_stick);
    replacement.clean();
    assert_eq!(replacement.sticky, Some(PLAYER));
    assert_eq!(
        replacement.failures,
        before.failures + u64::from(before.moving)
    );
    let last = samples.last().unwrap();
    last.clean();
    assert_eq!(
        (last.sticky, last.tracking, last.motions_pending),
        (None, None, 0),
        "nonsticky state and final DAT completion leave no old work"
    );
}
