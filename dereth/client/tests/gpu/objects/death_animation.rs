//! A killed creature's death animation starts exactly **once**, across the creature and the
//! corpse that replaces it.
//!
//! * Every recorded death is a single
//!   `0xF74C` carrying `current_style = 0x3D` (NonCombat) and `forward_command = 17`
//!   (`MotionCommand::DEAD`, `0x40000011`). There is **no** second motion and no `0x02BD`-family
//!   death event for the dying object. ACE broadcasts exactly one motion update and then, after
//!   the animation's length, creates the corpse and destroys the creature.
//! * The corpse inherits the dead creature's motion state, so its `0xF745` create carries a
//!   12-byte movement buffer whose forward command is `Dead` (long-solo-play idx 862,
//!   t = 101.101 s, right after the creature's `0xF747` delete). The corpse wears the creature's
//!   own setup and motion table, so if it played that motion from the top the player would watch
//!   the same 64-frame fall-over a second time.
//!
//! Retail runs world-entry finalization for every created object with `parent_id == 0` and a
//! position, **after** object creation has installed the description and unpacked its movement
//! buffer. The finalization first removes linked animations, then tells the movement manager that
//! the object entered the world. Linked-animation removal drops every **non-cyclic** sequence
//! node, so an object that enters the world already in a motion stands in that motion's *cycle*
//! and never plays its way into it. For the golem's `Dead` the DAT has
//! `(anim 0x030007AA, frames 0..63 @ 30 fps)` as the link and `(anim 0x030007AA, frames 63..63
//! @ 0 fps)` as the cycle: "falls over again" versus "is already lying there". Here the
//! finalization runs from `WorldScene::sync_objects`, the first point after creation at which the
//! descriptor's movement buffer has been unpacked.
//!
//! Fixture: long-solo-play's sparring golem and its corpse, replayed from the reassembled corpus
//! into a live scene. No datagram leaves this process.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::objects::ObjectStream;
use dereth_client::world::SceneWrites;
use dereth_client::world::{SceneConfig, WorldScene};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_dat::RetailDatStore;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::Opcode;
use dereth_render::device::Gpu;

use super::common::{retail_store, test_gpu};

const PLAYER: ObjectId = ObjectId(0x5000_000a);
/// long-solo-play's Sparring Golem.
const MOVER: ObjectId = ObjectId(0x8000_09d2);
/// The corpse the server creates when it dies, long-solo-play idx 862.
const CORPSE: ObjectId = ObjectId(0x8000_0a3c);
/// The death `0xF74C`, long-solo-play idx 831.
const DEATH_MICROS: u64 = 98_963_836;
/// The corpse's `0xF745`, long-solo-play idx 862.
const CORPSE_MICROS: u64 = 101_101_460;
/// `MotionCommand::DEAD` as the wire carries it: an index into the 412-entry `command_ids`
/// table, which ACE produces by truncating `MotionCommand.Dead = 0x40000011` to a `ushort`.
const DEAD_INDEX: u16 = 17;

/// One sampled frame of one object's motion sequence.
#[derive(Debug, Clone, PartialEq)]
struct Pose {
    /// `(anim_id, low_frame, high_frame, framerate)` for every node, in sequence order.
    nodes: Vec<(u32, i32, i32, f32)>,
    frame_number: i32,
}

impl Pose {
    /// A **link** node is a non-cyclic one. Linked-animation removal drops exactly the nodes before
    /// `first_cyclic`; for the `Dead` motion, the link is the 64-frame fall and the cycle is the
    /// one-frame hold at its end. "The fall is on screen" is therefore "a node with
    /// `low_frame != high_frame` is present".
    fn is_playing_the_fall(&self) -> bool {
        self.nodes.iter().any(|n| n.1 != n.2)
    }
}

fn read_rows() -> Vec<dereth_client_net::client_session::testing::CorpusBlob> {
    Corpus::load("long-solo-play")
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
                && r.payload.len() >= 8
                && [PLAYER, MOVER, CORPSE].contains(&ObjectId(u32::from_le_bytes(
                    r.payload[4..8].try_into().unwrap(),
                )))
        })
        .collect()
}

fn id_of(r: &dereth_client_net::client_session::testing::CorpusBlob) -> ObjectId {
    ObjectId(u32::from_le_bytes(r.payload[4..8].try_into().unwrap()))
}

struct Station {
    store: std::sync::Arc<RetailDatStore>,
    gpu: Gpu,
    scene: WorldScene,
    stream: ObjectStream,
    rows: Vec<dereth_client_net::client_session::testing::CorpusBlob>,
}

impl Station {
    /// Everything about the golem and the player up to (but not including) the death, replayed
    /// into a live scene with the observer parked beside the corridor the golem walks.
    fn open() -> Self {
        let store = retail_store();
        let mut gpu = test_gpu(320, 240);
        let rows = read_rows();
        let mut stream = ObjectStream::new();
        for row in rows.iter().filter(|r| r.t_rel_micros < DEATH_MICROS) {
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
        let region = dereth_client::world::load_region(&store).expect("region");
        scene
            .attach_character(&store, &region, &mut gpu)
            .expect("physics owner");
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
        Self {
            store,
            gpu,
            scene,
            stream,
            rows,
        }
    }

    fn feed(&mut self, row: &dereth_client_net::client_session::testing::CorpusBlob, at: f64) {
        self.stream.apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode(row.opcode),
                body: row.payload[4..].to_vec(),
            },
            LocalTime(at),
        );
    }

    fn sync(&mut self) {
        self.scene
            .sync_objects(&self.store, &mut self.gpu, &mut self.stream)
            .expect("production sync");
        self.stream.sync_physics(
            &self.store,
            &mut self.scene.character.as_mut().unwrap().world,
        );
    }

    fn step(&mut self, now: f64) {
        self.scene.update(
            Default::default(),
            Default::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
    }

    fn pose(&self, id: ObjectId) -> Option<Pose> {
        let seq = self.scene.server_object_sequence(id)?;
        let frame_number = self.scene.server_object_pose(id)?.3;
        Some(Pose {
            nodes: seq
                .nodes()
                .iter()
                .map(|n| (n.anim_id.0, n.low_frame, n.high_frame, n.framerate))
                .collect(),
            frame_number,
        })
    }
}

/// Count the **starts** of the fall: a rising edge of "a link animation is in the sequence".
/// A restart shows up as a second rising edge; a replay on a fresh object shows up as one of
/// its own. Summing the edges over the creature and the corpse it becomes is exactly the
/// number of times the player watches the thing fall over.
fn fall_starts(samples: &[Option<Pose>]) -> usize {
    let mut starts = 0;
    let mut playing = false;
    for s in samples {
        let now = s.as_ref().is_some_and(Pose::is_playing_the_fall);
        if now && !playing {
            starts += 1;
        }
        playing = now;
    }
    starts
}

/// **The census.** The shape of the traffic: one motion per death, and nothing else about that
/// object.
///
/// Runs against the reassembled corpus rather than against a hand-built payload, so it is the
/// recorded server talking.
#[test]
fn every_recorded_death_is_one_movement_message_and_no_death_event() {
    let mut deaths = 0usize;
    let mut sessions = std::collections::BTreeSet::new();
    for corpus in Corpus::shared_all() {
        let name = corpus.name.as_str();
        // Every `0xF74C` that names `MotionCommand::DEAD` as its interpreted forward command.
        let mut per_object: std::collections::BTreeMap<ObjectId, Vec<u64>> =
            std::collections::BTreeMap::new();
        for r in &corpus.blobs {
            if r.dir != Direction::ServerToClient
                || r.opcode != Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0
                || r.payload.len() < 24
            {
                continue;
            }
            let p = &r.payload[4..];
            let id = ObjectId(u32::from_le_bytes(p[0..4].try_into().unwrap()));
            // object id, instance, movement_ts, server_control_ts, autonomous, align(4).
            let mut o = 12usize;
            if p[o] != 0 {
                continue; // MovementType::Invalid only: the interpreted-state arm.
            }
            o += 4; // movement_type, motion_flags, current_style: u16
            let word = u32::from_le_bytes(p[o..o + 4].try_into().unwrap());
            o += 4;
            let flags = word & 0x7F;
            let read = |o: &mut usize| {
                let v = u16::from_le_bytes(p[*o..*o + 2].try_into().unwrap());
                *o += 2;
                v
            };
            if flags & 0x01 != 0 {
                read(&mut o);
            }
            if flags & 0x02 != 0 && read(&mut o) == DEAD_INDEX {
                per_object.entry(id).or_default().push(r.t_rel_micros);
            }
        }
        for (id, times) in &per_object {
            assert_eq!(
                times.len(),
                1,
                "{name}: object {id:?} was told to die {} times, at {times:?}. If the *server* \
                 ever sends two, this is where that shows up.",
                times.len()
            );
            deaths += 1;
            sessions.insert(name);
        }
    }
    assert!(
        deaths > 0,
        "the corpus holds recorded creature deaths ({sessions:?}); none were found, so the decode \
         above has moved"
    );
}

/// The corpse's create really does carry the dead creature's motion. Without this the two tests
/// below could pass by being pointed at nothing.
#[test]
fn the_corpse_create_carries_the_dead_motion_in_its_physics_desc() {
    let rows = read_rows();
    let create = rows
        .iter()
        .find(|r| id_of(r) == CORPSE && r.opcode == Opcode::ITEM_CREATE_OBJECT.0)
        .expect("long-solo-play records the corpse's create");
    assert_eq!(create.t_rel_micros, CORPSE_MICROS);
    let mut stream = ObjectStream::new();
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode(create.opcode),
            body: create.payload[4..].to_vec(),
        },
        LocalTime(0.0),
    );
    let buf = stream
        .presence(CORPSE)
        .and_then(|p| p.pending_movement.clone())
        .expect("the corpse's PhysicsDesc carries a movement buffer");
    let state = buf
        .body
        .interpreted
        .as_ref()
        .expect("an interpreted-state buffer");
    assert_eq!(
        state.forward_command,
        Some(DEAD_INDEX),
        "the corpse is created already dead: ACE copies the creature's current motion state, \
         which its death set to `Motion(NonCombat, Dead)`"
    );
}

/// Behaviour: objects.death.a-killed-creature-falls-over-once-across-itself-and-its-corpse
///
/// **The rejecting test.** One kill, one fall.
#[test]
fn a_killed_creature_falls_over_exactly_once_across_itself_and_its_corpse() {
    let mut st = Station::open();
    let t0 = 98.0;
    // A second of the creature alive, so the death is an edge rather than the first thing the
    // sequence ever saw.
    for f in 0..30u32 {
        st.step(t0 + f64::from(f) / 30.0);
    }
    assert!(
        !st.pose(MOVER)
            .expect("the golem is in the scene")
            .is_playing_the_fall()
            || st
                .pose(MOVER)
                .expect("pose")
                .nodes
                .iter()
                .all(|n| n.0 != 0x0300_07AA),
        "the golem must not already be playing its death animation before it is told to die"
    );

    let death = st
        .rows
        .iter()
        .find(|r| id_of(r) == MOVER && r.t_rel_micros == DEATH_MICROS)
        .expect("the recorded death")
        .clone();
    assert_eq!(death.opcode, Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0);
    st.feed(&death, t0 + 1.0);
    st.sync();

    // The creature's own fall, then the corpse's create at the recorded 2.14 s offset, then
    // three more seconds of frames. The corpse's create is the only other message admitted, so
    // anything the corpse does is its `PhysicsDesc`'s doing and nothing else's.
    let corpse_create = st
        .rows
        .iter()
        .find(|r| id_of(r) == CORPSE && r.opcode == Opcode::ITEM_CREATE_OBJECT.0)
        .expect("the recorded corpse")
        .clone();
    let corpse_at = t0 + 1.0 + (CORPSE_MICROS - DEATH_MICROS) as f64 / 1e6;

    let mut creature: Vec<Option<Pose>> = Vec::new();
    let mut corpse: Vec<Option<Pose>> = Vec::new();
    let mut fed = false;
    for f in 0..=150u32 {
        let now = t0 + 1.0 + f64::from(f) / 30.0;
        if !fed && now >= corpse_at {
            fed = true;
            st.feed(&corpse_create, now);
            st.sync();
        }
        st.step(now);
        creature.push(st.pose(MOVER));
        corpse.push(st.pose(CORPSE));
    }
    assert!(
        fed,
        "the corpse's create has to land inside the sampled window"
    );

    // Non-vacuity: the creature really did fall, all 64 frames of it.
    let played = creature
        .iter()
        .flatten()
        .filter(|p| p.is_playing_the_fall())
        .count();
    assert!(
        played >= 60,
        "the golem must spend at least 60 sampled frames falling over, not {played}: the \
         measurement below means nothing if it never played the animation at all"
    );
    assert!(
        creature.iter().flatten().any(|p| p
            .nodes
            .iter()
            .any(|n| n.0 == 0x0300_07AA && (n.1, n.2) == (0, 63))),
        "and the animation it plays is the golem's `Dead` link, anim 0x030007AA frames 0..63"
    );

    // The corpse is there and wearing the same animation.
    assert!(
        corpse.iter().flatten().count() >= 60,
        "the corpse must be in the scene for the tail of the window"
    );
    assert!(
        corpse
            .iter()
            .flatten()
            .all(|p| p.nodes.iter().all(|n| n.0 == 0x0300_07AA)),
        "the corpse wears the creature's own setup and motion table, so its sequence holds the \
         same animation: {:?}",
        corpse
            .iter()
            .flatten()
            .map(|p| p.nodes.clone())
            .collect::<Vec<_>>()
    );

    // The whole claim, in one number.
    let starts = fall_starts(&creature) + fall_starts(&corpse);
    assert_eq!(
        starts,
        1,
        "the fall-over animation started {starts} times for one kill. The creature's own \
         starts: {}. The corpse's: {}. World-entry finalization removes linked animations, \
         which keeps a corpse created already dead from replaying the fall into the pose it \
         already has.",
        fall_starts(&creature),
        fall_starts(&corpse)
    );

    // Stated the way the player sees it: the corpse is lying down on its very first frame.
    let first = corpse
        .iter()
        .flatten()
        .next()
        .expect("a first corpse frame");
    assert_eq!(
        (first.nodes.as_slice(), first.frame_number),
        (&[(0x0300_07AAu32, 63i32, 63i32, 0.0f32)][..], 63),
        "the corpse's first drawn frame must be the end of the death animation, not its start"
    );
}
