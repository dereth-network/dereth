//! The receivers of `Movement_VectorUpdate 0xF74E` and `Movement_PositionAndMovementEvent 0xF619`.
//! A vector update is refused unless its stamp is strictly newer, writes the stamp before asking
//! whether the player accepts server position, parks when its object is unknown, and carries both
//! velocity and omega (zero included) to the physics body. A position-and-movement event applies
//! its movement half even when its position half is stale, and its trailing buffer starts where
//! the position pack ends, not at `0xF74C`'s origin.
//! Fixture: every recorded `0xF74E` in seven recordings, replayed through the session's
//! world-object gate into `ObjectStream`; the corpus holds no `0xF619`, so every `0xF619` here is
//! constructed from the layout `dereth-protocol` decodes. No recorded omega is non-zero, so the
//! omega test is constructed too. The last test drives long-solo-play's recorded jump into a body.

#![cfg(windows)]

use dereth_client::world::SceneWrites;
use std::sync::Arc;

use dereth_client::objects::ObjectStream;
use dereth_client_net::client_session::dispatch::world_objects::{dispatch, InstanceTable};
use dereth_client_net::client_session::ordering::ParkedBlobs;
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{
    IncomingMessage, LocalTime, NetBlobId, NetQueue, ObjectId, RecipientId, Vec3,
};
use dereth_protocol::movement::{
    MovementBody, MovementBuffer, MovementPositionAndMovementEvent, MovementVectorUpdate,
    PositionPack,
};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::{ObjDesc, PhysicsDesc, PhysicsTimestamps, PublicWeenieDesc};
use dereth_protocol::{Message, Opcode};

/// The seven recordings the vector-update census reads.
const SCENARIOS: &[&str] = &[
    "first-login-walk-jump",
    "early-inventory-and-casting",
    "short-second-connection",
    "login-account-booted",
    "ddd-interrogation-only",
    "long-solo-play",
    "short-play-with-training",
];

/// Every recorded server-to-client blob of one opcode, as raw bodies (the opcode dword stripped).
fn recorded(op: u32) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for name in SCENARIOS {
        let c = Corpus::load(name)
            .expect("the netblob corpus parses")
            .unwrap_or_else(|| panic!("fixtures/message-corpus/{name}/blobs.jsonl is missing"));
        for b in c
            .blobs
            .iter()
            .filter(|b| b.opcode == op && b.dir == Direction::ServerToClient)
        {
            out.push(b.payload[4..].to_vec());
        }
    }
    out
}

/// The wire, as far as this crate can see it: `dereth_client_net::client_session`'s WorldObjects gate, then the object
/// stream: the whole of what `App::frame` does with a queue-10 blob, minus the transport.
struct Wire {
    table: InstanceTable,
    parked: ParkedBlobs,
}

impl Wire {
    fn new() -> Self {
        Self {
            table: InstanceTable::new(),
            parked: ParkedBlobs::default(),
        }
    }

    /// Send one already-encoded body. Returns whether the gate delivered it to the stream.
    fn send_body(&mut self, stream: &mut ObjectStream, op: Opcode, body: Vec<u8>) -> bool {
        let im = IncomingMessage {
            opcode: op.0,
            queue: NetQueue::WorldObjects,
            sender: RecipientId::default(),
            blob_id: NetBlobId::default(),
            body,
        };
        let d = dispatch(&mut self.table, &mut self.parked, None, &im);
        match d.event {
            Some(e @ SessionEvent::WorldObject { .. }) => {
                stream.apply_event(&e, LocalTime(1.0));
                true
            }
            _ => false,
        }
    }

    fn send<M: Message>(&mut self, stream: &mut ObjectStream, m: &M) -> bool {
        let body = dereth_protocol::write_body(m).expect("the message encodes");
        self.send_body(stream, M::OPCODE, body)
    }

    /// Create one object and record its instance sequence as the original create gate does,
    /// so later messages with that instance pass the instance test.
    fn spawn(&mut self, stream: &mut ObjectStream, id: ObjectId, ts: PhysicsTimestamps) {
        let m = ItemCreateObject(ObjectCreatePayload {
            id,
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::SETUP,
                setup_id: Some(0x0200_0001),
                timestamps: ts,
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        });
        assert!(self.send(stream, &m), "the create must reach the stream");
        self.table.set(id, ts.instance);
    }
}

fn vector_update(body: &[u8]) -> MovementVectorUpdate {
    dereth_protocol::read_body_padded::<MovementVectorUpdate>(body)
        .expect("the recorded blob decodes")
}

// ---------------------------------------------------------------------------------------------
// 1. The census, and the instrument's own control
// ---------------------------------------------------------------------------------------------

/// What the corpus holds, asserted before anything is built on it — including the **zero**, which
/// is the whole reason `0xF619`'s tests below are constructed.
#[test]
fn every_recorded_vector_update_carries_no_spin_and_is_its_objects_first() {
    let vectors = recorded(Opcode::MOVEMENT_VECTOR_UPDATE.0);
    assert!(!vectors.is_empty(), "the corpus carries no 0xF74E at all");
    assert!(
        recorded(Opcode::MOVEMENT_POSITION_AND_MOVEMENT_EVENT.0).is_empty(),
        "0xF619 has no producer in ACE and none in the corpus"
    );

    // The control that keeps the reader honest: the `0xF74C`/`0xF748` neighbours are read by the
    // same filter, so a filter that had silently matched nothing would show up here.
    assert!(
        !recorded(Opcode::MOVEMENT_POSITION_EVENT.0).is_empty()
            && !recorded(Opcode::MOVEMENT_SET_OBJECT_MOVEMENT.0).is_empty(),
        "the reader finds no 0xF748 or 0xF74C, so it is reading nothing"
    );

    // Measure what the recorded messages carry: it determines which vector component has a
    // discriminating recorded oracle below.
    let decoded: Vec<_> = vectors.iter().map(|b| vector_update(b)).collect();
    assert!(
        decoded.iter().any(|m| Vec3::from(m.velocity) != Vec3::ZERO),
        "no recorded vector update carries a non-zero velocity, so the velocity has no oracle"
    );
    assert_eq!(
        decoded
            .iter()
            .filter(|m| Vec3::from(m.omega) != Vec3::ZERO)
            .count(),
        0,
        "and NONE carries a non-zero omega -- `set_omega` has no recorded oracle at all"
    );
    assert!(
        decoded.iter().all(|m| m.timestamps.event == 1),
        "every recorded VECTOR_TS is 1, i.e. the object's first vector update"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. `0xF74E`, against every recorded instance
//
// The handler: look up the object (missing: park); compare the message's instance with the
// object's (newer: park; older: refuse); refuse without any write unless the vector stamp is
// strictly newer; write the stamp; for the player, stop there unless server position is accepted;
// otherwise set velocity and then omega, both with activation, omega even when it is zero.
// ---------------------------------------------------------------------------------------------

/// **The rejecting test.** Every recorded blob, replayed byte-for-byte through the session gate
/// into a real `ObjectStream`, each about an object created with the instance sequence the message
/// names. None of them may land in `unhandled`.
#[test]
fn every_recorded_vector_update_reaches_the_receiver() {
    let bodies = recorded(Opcode::MOVEMENT_VECTOR_UPDATE.0);
    assert!(!bodies.is_empty(), "the corpus carries no 0xF74E at all");

    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let mut expected: Vec<(ObjectId, Vec3, Vec3)> = Vec::new();
    for body in &bodies {
        let m = vector_update(body);
        wire.spawn(
            &mut stream,
            m.id,
            PhysicsTimestamps {
                instance: m.timestamps.instance,
                ..PhysicsTimestamps::default()
            },
        );
        assert!(
            wire.send_body(&mut stream, Opcode::MOVEMENT_VECTOR_UPDATE, body.clone()),
            "the instance gate must deliver a vector update whose sequence matches the object's"
        );
        expected.push((m.id, m.velocity.into(), m.omega.into()));
    }

    assert_eq!(
        stream.stats.vector_updates,
        bodies.len() as u64,
        "every recorded instance was accepted"
    );
    assert_eq!(stream.stats.vector_stale, 0);
    assert_eq!(stream.stats.vector_unknown, 0);
    assert_eq!(
        stream.stats.vector_player_autonomous, 0,
        "none of the recorded vector updates is about *this* player"
    );
    assert_eq!(
        stream.stats.unhandled, 0,
        "and none of them was counted as unhandled"
    );

    // Every one of them is queued for the physics object, with the numbers off the wire.
    for (id, velocity, omega) in expected {
        assert_eq!(
            stream.take_vector_update(id),
            Some((velocity, omega)),
            "{id:?} must have the recorded pair waiting"
        );
        assert_eq!(
            stream.take_vector_update(id),
            None,
            "and it is drained, not sticky"
        );
    }

    // Every object's vector timestamp advanced to the message's.
    for body in &bodies {
        let m = vector_update(body);
        assert_eq!(
            stream.presence(m.id).expect("still there").vector_ts,
            m.timestamps.event
        );
    }
}

/// Behaviour: movement.vector-update.a-stale-update-is-refused
/// The original vector-update handler requires a strictly newer stamp. **Equality is stale**,
/// and a refusal writes nothing — not the stamp, not the pair.
#[test]
fn a_vector_update_that_is_not_strictly_newer_is_refused_outright() {
    let id = ObjectId(0x8000_0A42);
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    wire.spawn(
        &mut stream,
        id,
        PhysicsTimestamps {
            instance: 0,
            vector: 7,
            ..PhysicsTimestamps::default()
        },
    );
    assert_eq!(
        stream.presence(id).expect("spawned").vector_ts,
        7,
        "the create seeds VECTOR_TS from the descriptor"
    );
    // The create itself parks a pair: applying a physics description always sets velocity with
    // activation, and the description leaves velocity zero when its flag is clear, so every
    // non-static object is activated from birth. The pair is drained here so that the question
    // below is "did the vector update queue anything" rather than "is anything queued".
    assert_eq!(
        stream.take_vector_update(id),
        Some((Vec3::ZERO, Vec3::ZERO)),
        "the create's own set_velocity((0,0,0), TRUE)"
    );

    let msg = |event: u16| MovementVectorUpdate {
        id,
        velocity: Vec3::new(1.0, 2.0, 3.0).into(),
        omega: Vec3::ZERO.into(),
        timestamps: dereth_protocol::types::physicsdesc::PhysicsEventStamp { instance: 0, event },
    };

    assert!(
        wire.send(&mut stream, &msg(7)),
        "the gate delivers it; the receiver is what refuses"
    );
    assert_eq!(
        stream.stats.vector_stale, 1,
        "an EQUAL stamp is stale: the test is is_newer"
    );
    assert_eq!(stream.stats.vector_updates, 0);
    assert_eq!(
        stream.presence(id).expect("there").vector_ts,
        7,
        "and the stamp did not move"
    );
    assert_eq!(stream.take_vector_update(id), None);

    assert!(wire.send(&mut stream, &msg(6)));
    assert_eq!(stream.stats.vector_stale, 2, "and so is an older one");

    assert!(wire.send(&mut stream, &msg(8)));
    assert_eq!(stream.stats.vector_updates, 1, "one past it is not");
    assert_eq!(stream.presence(id).expect("there").vector_ts, 8);
    assert_eq!(
        stream.take_vector_update(id),
        Some((Vec3::new(1.0, 2.0, 3.0), Vec3::ZERO))
    );
}

/// The original handler asks only the player whether server position is accepted. Its
/// autonomy predicate is `autonomy_level != 2`, with constructor value 2, so the **default**
/// answer is false.
///
/// Both directions, and the ordering: the stamp is written *before* the player test,
/// so a message the autonomy arm throws away still advances `VECTOR_TS`. A build that put the
/// player test first would pass every other assertion here and fail that one.
#[test]
fn a_vector_update_about_the_player_obeys_use_position_from_server() {
    let id = ObjectId(0x5000_000A);
    let msg = |event: u16| MovementVectorUpdate {
        id,
        velocity: Vec3::new(0.0, 0.0, 3.5).into(),
        omega: Vec3::ZERO.into(),
        timestamps: dereth_protocol::types::physicsdesc::PhysicsEventStamp { instance: 0, event },
    };

    // Default autonomy refuses server position, so the velocity/omega pair is not queued.
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    wire.spawn(&mut stream, id, PhysicsTimestamps::default());
    // The create's own zero velocity with activation enabled, drained so the assertion
    // below still asks about the *vector update*. See the note in
    // `a_vector_update_that_is_not_strictly_newer_is_refused_outright`. Note the spawn happens
    // *before* `world.player` is set, so the autonomy arm does not apply to it — which is also
    // true of the original client: the create makes the body, and dispatch learns the player ID
    // from `0xF746`.
    assert_eq!(
        stream.take_vector_update(id),
        Some((Vec3::ZERO, Vec3::ZERO))
    );
    stream.world.player = Some(id);
    assert!(wire.send(&mut stream, &msg(1)));
    assert_eq!(stream.stats.vector_player_autonomous, 1);
    assert_eq!(stream.stats.vector_updates, 0);
    assert_eq!(
        stream.take_vector_update(id),
        None,
        "nothing is queued for the body"
    );
    assert_eq!(
        stream.presence(id).expect("there").vector_ts,
        1,
        "the vector stamp advances before the player-autonomy test"
    );

    // The very next message is therefore measured against the advanced stamp, which is the whole
    // reason that ordering is worth a line.
    stream.note_use_position_from_server(true);
    assert!(wire.send(&mut stream, &msg(1)));
    assert_eq!(stream.stats.vector_stale, 1, "1 is no longer newer than 1");
    assert!(wire.send(&mut stream, &msg(2)));
    assert_eq!(stream.stats.vector_updates, 1);
    assert_eq!(
        stream.take_vector_update(id),
        Some((Vec3::new(0.0, 0.0, 3.5), Vec3::ZERO))
    );

    // **The control**: the same message about an object that is *not* the player is never asked
    // the question at all: the original handler bypasses the autonomy predicate for other objects.
    let other = ObjectId(0x8000_0A42);
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    wire.spawn(&mut stream, other, PhysicsTimestamps::default());
    assert!(wire.send(
        &mut stream,
        &MovementVectorUpdate {
            id: other,
            ..msg(1)
        }
    ));
    assert_eq!(stream.stats.vector_player_autonomous, 0);
    assert_eq!(stream.stats.vector_updates, 1);
}

/// **The limb with no oracle, said in the test's own name.** Every recorded omega is zero, so
/// nothing in the corpus distinguishes applying angular velocity from ignoring it. This is
/// the constructed stand-in: a non-zero omega must be carried. The original handler sets omega
/// immediately after velocity with no intervening test, so zero omega beside non-zero velocity
/// must also be carried to clear an object's existing spin.
#[test]
fn the_omega_limb_has_no_recorded_oracle_and_is_asserted_by_construction() {
    let id = ObjectId(0x8000_0A42);
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    wire.spawn(&mut stream, id, PhysicsTimestamps::default());

    let spin = Vec3::new(0.0, 0.0, 1.5);
    assert!(wire.send(
        &mut stream,
        &MovementVectorUpdate {
            id,
            velocity: Vec3::ZERO.into(),
            omega: spin.into(),
            timestamps: dereth_protocol::types::physicsdesc::PhysicsEventStamp {
                instance: 0,
                event: 1,
            },
        }
    ));
    assert_eq!(stream.take_vector_update(id), Some((Vec3::ZERO, spin)));

    // And the pair that stops it again.
    assert!(wire.send(
        &mut stream,
        &MovementVectorUpdate {
            id,
            velocity: Vec3::new(1.0, 0.0, 0.0).into(),
            omega: Vec3::ZERO.into(),
            timestamps: dereth_protocol::types::physicsdesc::PhysicsEventStamp {
                instance: 0,
                event: 2,
            },
        }
    ));
    assert_eq!(
        stream.take_vector_update(id),
        Some((Vec3::new(1.0, 0.0, 0.0), Vec3::ZERO)),
        "the zero omega is carried too: `set_omega` is not conditional on being non-zero"
    );
}

/// The original handler parks a vector update when object lookup fails. The `None` arm of
/// `dereth_client_net::client_session` dispatch reproduces this: an update for an object not yet created is
/// **parked, not dropped**, as script effects are.
#[test]
fn a_vector_update_for_an_unknown_object_is_parked_rather_than_delivered() {
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    let id = ObjectId(0x8000_0A46);
    assert!(
        !wire.send(
            &mut stream,
            &MovementVectorUpdate {
                id,
                velocity: Vec3::new(1.0, 0.0, 0.0).into(),
                omega: Vec3::ZERO.into(),
                timestamps: dereth_protocol::types::physicsdesc::PhysicsEventStamp {
                    instance: 0,
                    event: 1,
                },
            }
        ),
        "the session's gate parks it; nothing reaches the receiver"
    );
    assert_eq!(stream.stats.vector_updates, 0);
    assert_eq!(
        stream.stats.vector_unknown, 0,
        "it never got as far as the receiver"
    );
    assert_eq!(stream.stats.unhandled, 0);
}

// ---------------------------------------------------------------------------------------------
// 3. `0xF619`: **every case here is constructed; the corpus has none**
//
// The handler unpacks the position event through an in/out cursor (unknown object: park; old
// instance: refuse without applying movement), then applies the object movement from the advanced
// cursor, and relinquishes local control to the server when that succeeds. Position unpacking
// returns success unconditionally after applying the position, so a stale position stamp refuses
// the position while the movement still runs.
// ---------------------------------------------------------------------------------------------

/// A `PositionPack` with no optional limbs, so its encoding is a fixed number of dwords and the
/// movement buffer that follows starts at a blob offset that is `0 (mod 4)`.
fn position_pack(position_ts: u16, instance_ts: u16) -> PositionPack {
    use dereth_protocol::movement::position_flags as f;
    PositionPack {
        flags: f::ORIENTATION_HAS_NO_X | f::ORIENTATION_HAS_NO_Y | f::ORIENTATION_HAS_NO_Z,
        origin: dereth_protocol::types::Origin {
            objcell_id: 0x0001_0001,
            origin: Vec3::new(10.0, 20.0, 3.0).into(),
        },
        orientation: dereth_primitives::Quat::new(1.0, 0.0, 0.0, 0.0).into(),
        velocity: None,
        placement_id: None,
        instance_timestamp: instance_ts,
        position_timestamp: position_ts,
        teleport_timestamp: 0,
        force_position_timestamp: 0,
    }
}

/// The trailing movement buffer starts at the cursor reread after position unpacking in the
/// original `0xF619` arm: `4 (opcode) + 4 (id) + |PositionPack|`, therefore a multiple
/// of four — **not** `0xF74C`'s `MovementBuffer::BLOB_ORIGIN` of 10.
fn movement_tail(movement_ts: u16) -> Vec<u8> {
    let b = MovementBuffer {
        movement_timestamp: movement_ts,
        server_control_timestamp: 0,
        autonomous: false,
        // `movement_type = INVALID` selects the original movement unpacker's default arm, the only
        // one that carries an `InterpretedMotionState` -- which is why the encoder insists on the
        // pair. Most recorded `0xF74C` buffers have this shape.
        body: MovementBody {
            interpreted: Some(dereth_protocol::movement::InterpretedMotionState::default()),
            ..MovementBody::default()
        },
    };
    let mut w = dereth_protocol::Writer::with_origin(0);
    b.write(&mut w).expect("the buffer encodes");
    w.into_inner()
}

/// **The rejecting test for `0xF619` — constructed, because the corpus has zero instances.**
///
/// One message must apply both its position and its trailing movement buffer.
#[test]
fn a_constructed_position_and_movement_event_applies_both_halves() {
    let id = ObjectId(0x8000_0A42);
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    wire.spawn(&mut stream, id, PhysicsTimestamps::default());
    assert!(
        stream.presence(id).expect("spawned").position.is_none(),
        "nothing placed it yet"
    );

    let m = MovementPositionAndMovementEvent {
        id,
        position: position_pack(4, 0),
        movement: movement_tail(9),
    };
    assert!(wire.send(&mut stream, &m), "the instance gate delivers it");

    assert_eq!(stream.stats.position_and_movement_events, 1);
    assert_eq!(stream.stats.position_and_movement_undecodable, 0);
    assert_eq!(
        stream.stats.unhandled, 0,
        "and it is not counted as unhandled"
    );

    let p = stream.presence(id).expect("still there");
    assert_eq!(
        p.position_ts, 4,
        "the position half ran -- the position timestamp"
    );
    assert_eq!(
        p.position
            .expect("the position half placed it")
            .frame
            .origin,
        Vec3::new(10.0, 20.0, 3.0)
    );
    assert_eq!(
        p.movement_ts, 9,
        "and the movement half ran -- the movement timestamp"
    );
    assert_eq!(stream.stats.movement_updates, 1);
    assert!(
        stream.take_movement(id).is_some(),
        "the buffer is queued for the renderer"
    );
}

/// Behaviour: movement.position-and-movement.a-stale-position-still-runs-the-movement-half
/// Original position-event unpacking returns success **unconditionally** after its void
/// position-application call. A position refused by the `POSITION_TS` gate therefore still
/// lets the trailing movement run.
///
/// A build that gated the movement half on the position half passes the test above and fails this
/// one — which is the whole reason it is a separate test.
#[test]
fn a_stale_position_still_lets_the_movement_half_run() {
    let id = ObjectId(0x8000_0A42);
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    wire.spawn(
        &mut stream,
        id,
        PhysicsTimestamps {
            position: 20,
            ..PhysicsTimestamps::default()
        },
    );

    // `position_timestamp = 5` is far behind the object's 20, so `received_position` refuses it.
    let m = MovementPositionAndMovementEvent {
        id,
        position: position_pack(5, 0),
        movement: movement_tail(3),
    };
    assert!(wire.send(&mut stream, &m));

    assert_eq!(
        stream.stats.stale_positions, 1,
        "the position half refused it"
    );
    assert_eq!(
        stream.presence(id).expect("there").position_ts,
        20,
        "and did not move the stamp"
    );
    assert_eq!(
        stream.stats.movement_updates, 1,
        "the movement half ran anyway"
    );
    assert_eq!(stream.presence(id).expect("there").movement_ts, 3);
    assert_eq!(stream.stats.position_and_movement_events, 1);
}

/// The control for the alignment claim. `MovementBuffer::BLOB_ORIGIN` is 10 — `0xF74C`'s, where
/// the buffer follows `[opcode][id][instance]` — and reading a `0xF619`'s tail at that origin pads
/// by the wrong amount. If a future change reaches for `decoded_movement()` here, this reddens.
#[test]
fn the_trailing_buffer_is_not_aligned_the_way_a_set_object_movement_is() {
    let bytes = movement_tail(9);
    assert!(
        MovementBuffer::read(&mut dereth_protocol::Reader::with_origin(&bytes, 0)).is_ok(),
        "at the 0xF619 arm's own origin it decodes"
    );
    let at_f74c = dereth_protocol::Reader::with_origin(&bytes, MovementBuffer::BLOB_ORIGIN);
    let mut at_f74c = at_f74c;
    let decoded = MovementBuffer::read(&mut at_f74c);
    assert!(
        decoded.is_err() || at_f74c.expect_exhausted().is_err(),
        "and at 0xF74C's origin of 10 it does not consume exactly -- the two are different pads"
    );
}

/// A malformed trailing movement buffer causes the current receiver to refuse the whole
/// message and **count** the failure. With no recorded `0xF619` to confirm the inferred layout,
/// future traffic that disagrees with it must produce a visible counter rather than silence.
#[test]
fn a_position_and_movement_event_whose_tail_is_not_a_buffer_is_counted() {
    let id = ObjectId(0x8000_0A42);
    let mut stream = ObjectStream::new();
    let mut wire = Wire::new();
    wire.spawn(&mut stream, id, PhysicsTimestamps::default());

    let m = MovementPositionAndMovementEvent {
        id,
        position: position_pack(4, 0),
        movement: vec![0xFF; 3],
    };
    assert!(wire.send(&mut stream, &m), "the gate still delivers it");
    assert_eq!(stream.stats.position_and_movement_undecodable, 1);
    assert_eq!(stream.stats.position_and_movement_events, 0);
    assert_eq!(
        stream.stats.unhandled, 0,
        "counted as its own failure, not as 'no receiver'"
    );
    assert_eq!(
        stream.presence(id).expect("there").position_ts,
        0,
        "malformed movement tails are counted without applying either half"
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The consumer: the pair reaches a real physics body
// ---------------------------------------------------------------------------------------------

/// A receiver that parks a value nothing drains is the same
/// defect as no receiver at all, so this drives the corpus's own recorded jump into a real
/// `dereth_physics` body through `WorldScene::sync_objects` — the one production caller.
///
/// The object, the create that builds it and the vector update are all `long-solo-play`'s
/// `0x5000000A`: one of the few recorded vector updates that carry a non-zero velocity.
#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
#[test]
fn the_recorded_vector_update_reaches_the_physics_body() {
    use dereth_client::world::{load_region, SceneConfig, WorldScene};
    use dereth_render::device::{DeviceConfig, Gpu};

    let id = ObjectId(0x5000_000A);
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let corpus = Corpus::load("long-solo-play")
        .expect("parses")
        .expect("long-solo-play is required");
    let mut gpu = Gpu::new(
        None,
        &DeviceConfig {
            width: 320,
            height: 240,
            ..DeviceConfig::default()
        },
    )
    .expect("a software GPU device");

    // The recorded blob this test is about, and everything about the same object that precedes it.
    let vector = corpus
        .blobs
        .iter()
        .find(|b| {
            b.opcode == Opcode::MOVEMENT_VECTOR_UPDATE.0
                && b.payload.get(4..8) == Some(id.0.to_le_bytes().as_slice())
        })
        .expect("long-solo-play carries 0x5000000A's vector update");
    let velocity: Vec3 = vector_update(&vector.payload[4..]).velocity.into();
    assert_ne!(
        velocity,
        Vec3::ZERO,
        "this is the recorded jump, not one of the twenty stops"
    );

    let mut stream = ObjectStream::new();
    for row in corpus.blobs.iter().filter(|r| {
        r.dir == Direction::ServerToClient
            && r.t_rel_micros < vector.t_rel_micros
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
        .expect("the recorded player");
    assert_eq!(
        stream.player(),
        None,
        "this body takes the remote-object path"
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
            ..SceneConfig::default()
        },
    )
    .expect("the recorded block loads");
    let region = load_region(&store).expect("the region");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("a physics owner");
    {
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
        .expect("the create reaches the scene");
    if let Some(c) = scene.character.as_mut() {
        stream.sync_physics(&store, &mut c.world);
    }
    let mut now = std::time::Duration::from_micros(vector.t_rel_micros).as_secs_f64();
    for _ in 0..3 {
        now += 1.0 / 30.0;
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(now),
            1.0 / 30.0,
        );
    }

    let handle = scene
        .character
        .as_ref()
        .and_then(|c| c.world.by_object_id(id))
        .expect("the recorded object has a physics body");
    let before = scene
        .character
        .as_ref()
        .unwrap()
        .world
        .get(handle)
        .unwrap()
        .velocity();

    // The recorded blob, byte for byte, straight into the stream the App feeds.
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: Opcode::MOVEMENT_VECTOR_UPDATE,
            body: vector.payload[4..].to_vec(),
        },
        LocalTime(now),
    );
    assert_eq!(
        stream.stats.vector_updates, 1,
        "the receiver accepted the recorded blob"
    );

    let applied = scene.draw.stats.vector_updates_applied;
    scene
        .sync_objects(&store, &mut gpu, &mut stream)
        .expect("the drain");
    assert_eq!(
        scene.draw.stats.vector_updates_applied,
        applied + 1,
        "sync_objects is the drain -- a queued pair nothing drains is no receiver at all"
    );
    assert_eq!(scene.draw.stats.vector_updates_without_body, 0);
    assert_eq!(
        stream.take_vector_update(id),
        None,
        "and the queue is emptied, not grown"
    );

    let body = scene.character.as_ref().unwrap().world.get(handle).unwrap();
    assert_ne!(
        before, velocity,
        "the fixture would prove nothing if it were already set"
    );
    assert_eq!(
        body.velocity_vector, velocity,
        "the physics-object velocity setter received the number off the wire"
    );
    assert_eq!(
        body.omega_vector,
        Vec3::ZERO,
        "the angular-velocity setter received the recorded zero"
    );
}
