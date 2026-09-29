//! Queue 10 — the WorldObjects queue.
//!
//! Sixteen opcodes are handled; anything else returns `NETBLOB_ERROR`, which the queue processor
//! ignores, so the blob is dropped.
//!
//! # The instance-sequence gate runs before anything else
//!
//! Every object handler begins by comparing the message's instance sequence against the object's
//! `update_times[INSTANCE_TS]`, and there are **three** outcomes:
//!
//! | condition | outcome |
//! |---|---|
//! | object unknown, **or** the message's instance sequence is newer | `Queued` — park the blob and replay it when the object arrives |
//! | sequences match | process |
//! | sequences differ the other way (the message is older) | `OldInstance` — drop |
//!
//! Dropping "future" messages instead of parking them **loses objects**, and skipping the gate
//! applies stale appearance data after a teleport or a re-login.
//!
//! For the comparison, see [`dereth_protocol::objects::is_newer`].
//!
//! # The two creates are the exception, and applying the table above to them loses every object
//!
//! `0xF745 Item_CreateObject` and `0xF7DB Item_UpdateObject` are the messages that *make* an object
//! exist, so "the client does not know this object" cannot mean "wait until it does".
//! The create handler has its own five-step gate
//! (`docs/networking/messages/02-world-objects.md` the create-object section), and only its **first** step
//! parks — on an unknown **parent**, never on the object itself:
//!
//! | condition | outcome |
//! |---|---|
//! | `pd.parent_id != 0` and the parent is unknown (step 1) | `Queued` — park on the *parent* |
//! | the object is unknown (step 4) | **create path** — process |
//! | the object exists and this is `0xF7DB` (the recreate flag set, step 3) | create path — process |
//! | the object exists and the message is newer (step 3a) | create path — process |
//! | the object exists and the message is older (step 3b) | `OldInstance` — drop |
//! | the object exists and the sequences are equal (step 3c) | in-place merge — process |
//!
//! Applying the *general* rule to both opcodes would mean no object is ever created against ACE,
//! [`crate::client_session::Session::object_arrived`] could never be reached by its intended caller,
//! and the player's own `PhysicsDesc.timestamps.instance` would be unobservable. So the create path
//! is taken for an unknown object, the player's included.

use crate::client_session::ordering::ParkedBlobs;
use crate::client_session::{DropReason, SessionEvent};
use dereth_primitives::{IncomingMessage, ObjectId};
use dereth_protocol::objects::{is_newer, ObjectDispatchOutcome};
use dereth_protocol::{Opcode, Reader};
use std::collections::HashMap;

/// The subset of per-object update timestamps the ordering gate needs: `INSTANCE_TS`.
///
/// The client keeps the whole nine-slot array on the physics object. Only the instance slot governs
/// *dispatch*, so only that is kept here; the other eight are per-handler state that the object
/// model and the physics layer own.
#[derive(Debug, Clone, Default)]
pub struct InstanceTable {
    by_object: HashMap<ObjectId, u16>,
}

impl InstanceTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The object's instance sequence, or `None` when the client does not know the object.
    #[must_use]
    pub fn get(&self, id: ObjectId) -> Option<u16> {
        self.by_object.get(&id).copied()
    }

    /// Record an object's instance sequence — done when a create or update message is processed.
    pub fn set(&mut self, id: ObjectId, instance: u16) {
        self.by_object.insert(id, instance);
    }

    /// Delete an object from maintenance and timestamp tracking.
    pub fn remove(&mut self, id: ObjectId) {
        self.by_object.remove(&id);
    }

    #[must_use]
    pub fn knows(&self, id: ObjectId) -> bool {
        self.by_object.contains_key(&id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_object.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_object.is_empty()
    }
}

/// The three-way gate, in one place so no handler can skip it.
///
/// `is_newer(object.INSTANCE_TS, message.instance)` means "the message is ahead of the object" — the
/// object has not been re-created locally yet, so the blob is held on it.
#[must_use]
pub fn instance_gate(
    table: &InstanceTable,
    id: ObjectId,
    message_instance: u16,
) -> ObjectDispatchOutcome {
    match table.get(id) {
        None => ObjectDispatchOutcome::Queued,
        Some(known) if is_newer(known, message_instance) => ObjectDispatchOutcome::Queued,
        Some(known) if known != message_instance => ObjectDispatchOutcome::OldInstance,
        Some(_) => ObjectDispatchOutcome::ProcessedOk,
    }
}

/// The sixteen opcodes the client's smart-box event dispatcher handles.
///
/// Twelve are in `dereth_protocol::objects` and four in `dereth_protocol::movement`; the list is the master
/// opcode table's WorldObjects-queue rows.
#[must_use]
pub fn is_world_object_opcode(op: Opcode) -> bool {
    op.info()
        .is_some_and(|i| i.recv_queue == Some(dereth_primitives::NetQueue::WorldObjects))
}

/// What one WorldObjects blob did.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectDispatch {
    pub outcome: ObjectDispatchOutcome,
    pub event: Option<SessionEvent>,
}

/// Dispatch one queue-10 blob.
///
/// A blob shorter than 4 bytes returns `NETBLOB_ERROR`; an unhandled opcode does too. Otherwise the instance gate
/// runs **first**, and only then is the body handed on.
pub fn dispatch(
    table: &mut InstanceTable,
    parked: &mut ParkedBlobs,
    player_id: Option<ObjectId>,
    m: &IncomingMessage,
) -> ObjectDispatch {
    let op = Opcode(m.opcode);
    if !is_world_object_opcode(op) {
        return ObjectDispatch {
            outcome: ObjectDispatchOutcome::Error,
            event: Some(SessionEvent::Dropped {
                queue: m.queue,
                opcode: op,
                reason: DropReason::NoHandler,
            }),
        };
    }

    // `0xF746 Login_CreatePlayer` has no instance sequence and no object gate: it establishes the
    // player id and then replays everything queued before the client knew who it was.
    if op == Opcode::LOGIN_CREATE_PLAYER {
        let Ok(id) = Reader::body(&m.body).u32() else {
            return error_dispatch(m, op);
        };
        // The create-player handler ignores a second one.
        if player_id.is_some() {
            return ObjectDispatch {
                outcome: ObjectDispatchOutcome::Error,
                event: Some(SessionEvent::Dropped {
                    queue: m.queue,
                    opcode: op,
                    reason: DropReason::PlayerAlreadyCreated,
                }),
            };
        }
        return ObjectDispatch {
            outcome: ObjectDispatchOutcome::ProcessedOk,
            event: Some(SessionEvent::PlayerCreated(ObjectId(id))),
        };
    }

    // `0xF751 Effects_PlayerTeleport` addresses the player implicitly and reads only a `u16`.
    if op == Opcode::EFFECTS_PLAYER_TELEPORT {
        return ObjectDispatch {
            outcome: ObjectDispatchOutcome::ProcessedOk,
            event: Some(SessionEvent::WorldObject {
                opcode: op,
                body: m.body.clone(),
            }),
        };
    }

    let Some(id) = leading_object_id(&m.body) else {
        return error_dispatch(m, op);
    };

    // The create opcodes have their own gate, and it
    // never parks on the object being created. See this module's header.
    if op == Opcode::ITEM_CREATE_OBJECT || op == Opcode::ITEM_UPDATE_OBJECT {
        return create_dispatch(table, parked, m, op, id);
    }

    match instance_sequence(op, &m.body) {
        // Messages with no instance sequence of their own — the sound and script effects — still
        // queue on an unknown object but have no sequence check.
        None => {
            if table.knows(id) {
                ObjectDispatch {
                    outcome: ObjectDispatchOutcome::ProcessedOk,
                    event: Some(SessionEvent::WorldObject {
                        opcode: op,
                        body: m.body.clone(),
                    }),
                }
            } else {
                parked.park(id, blob_of(m));
                ObjectDispatch {
                    outcome: ObjectDispatchOutcome::Queued,
                    event: None,
                }
            }
        }
        Some(instance) => match instance_gate(table, id, instance) {
            ObjectDispatchOutcome::Queued => {
                parked.park(id, blob_of(m));
                ObjectDispatch {
                    outcome: ObjectDispatchOutcome::Queued,
                    event: None,
                }
            }
            ObjectDispatchOutcome::OldInstance => ObjectDispatch {
                outcome: ObjectDispatchOutcome::OldInstance,
                event: None,
            },
            _ => ObjectDispatch {
                outcome: ObjectDispatchOutcome::ProcessedOk,
                event: Some(SessionEvent::WorldObject {
                    opcode: op,
                    body: m.body.clone(),
                }),
            },
        },
    }
}

/// The create gate's steps 1, 3 and 4, for `0xF745` and `0xF7DB`.
///
/// The recreate flag is what tells the two apart: the update dispatch passes 1 and the create dispatch
/// passes 0, and step 3 reads it before it reads the sequence.
///
/// The descriptors are unpacked here for the same reason the client unpacks them before the gate:
/// the parent id and the instance sequence both live at the far end of the `PhysicsDesc`, past a
/// variable-length `ObjDesc`. All three descriptors are decoded before checking the gate.
fn create_dispatch(
    table: &InstanceTable,
    parked: &mut ParkedBlobs,
    m: &IncomingMessage,
    op: Opcode,
    id: ObjectId,
) -> ObjectDispatch {
    let mut r = Reader::body(&m.body);
    // The object id again: `leading_object_id` read it without advancing this reader.
    if r.u32().is_err() {
        return error_dispatch(m, op);
    }
    if dereth_protocol::types::ObjDesc::read(&mut r).is_err() {
        return error_dispatch(m, op);
    }
    let Ok(pd) = dereth_protocol::types::PhysicsDesc::read(&mut r) else {
        return error_dispatch(m, op);
    };

    // Step 1. The parent id is 0 unless the `PARENT` bit is set, and a *known*
    // parent falls through — only an unknown one parks, and it parks the blob on the **parent**.
    // The parent id is the `PARENT` bit's field or 0; `dereth_protocol` only populates
    // `parent` when the bit is set, so `None` and `Some(0)` are the same "no parent" here.
    if let Some(parent) = pd.parent.map(|p| p.0) {
        if parent != ObjectId(0) && !table.knows(parent) {
            parked.park(parent, blob_of(m));
            return ObjectDispatch {
                outcome: ObjectDispatchOutcome::Queued,
                event: None,
            };
        }
    }

    let instance = pd.timestamps.instance;
    let outcome = match table.get(id) {
        // Step 4: the object does not exist, so this message creates it.
        None => ObjectDispatchOutcome::ProcessedOk,
        // Step 3 with the recreate flag set: straight to the create path, with no sequence test at all.
        Some(_) if op == Opcode::ITEM_UPDATE_OBJECT => ObjectDispatchOutcome::ProcessedOk,
        // Step 3a: the message is ahead of what we hold, i.e. the server re-created the object.
        // The client falls through to the create path; it does **not** park.
        Some(known) if is_newer(known, instance) => ObjectDispatchOutcome::ProcessedOk,
        // Step 3b.
        Some(known) if known != instance => ObjectDispatchOutcome::OldInstance,
        // Step 3c: the in-place merge.
        Some(_) => ObjectDispatchOutcome::ProcessedOk,
    };
    match outcome {
        ObjectDispatchOutcome::OldInstance => ObjectDispatch {
            outcome: ObjectDispatchOutcome::OldInstance,
            event: None,
        },
        _ => ObjectDispatch {
            outcome: ObjectDispatchOutcome::ProcessedOk,
            event: Some(SessionEvent::WorldObject {
                opcode: op,
                body: m.body.clone(),
            }),
        },
    }
}

fn error_dispatch(m: &IncomingMessage, op: Opcode) -> ObjectDispatch {
    ObjectDispatch {
        outcome: ObjectDispatchOutcome::Error,
        event: Some(SessionEvent::Dropped {
            queue: m.queue,
            opcode: op,
            reason: DropReason::ShortBuffer,
        }),
    }
}

/// Reassemble the blob the way it arrived, so a parked copy replays identically.
fn blob_of(m: &IncomingMessage) -> Vec<u8> {
    let mut v = m.opcode.to_le_bytes().to_vec();
    v.extend_from_slice(&m.body);
    v
}

/// Every WorldObjects message except `0xF746` and `0xF751` opens with the object id.
fn leading_object_id(body: &[u8]) -> Option<ObjectId> {
    Reader::body(body).u32().ok().map(ObjectId)
}

/// Where each message keeps its instance sequence, or `None` when it has none.
///
/// * `0xF745`/`0xF7DB`: at the tail of the `PhysicsDesc`, which means decoding the whole
///   `ObjDesc` and `PhysicsDesc` to reach it. All three descriptors are decoded
///   before the create handler checks the gate.
/// * `0xF747`: a bare `u16` after the id.
/// * `0xF625`, `0xF749`, `0xF74A`, `0xF74B`, `0xF74E`: a `PhysicsEventStamp`, whose **first**
///   `u16` is always the instance sequence.
/// * `0xF74C`: a bare `u16` at offset 8 of the blob.
/// * `0xF748`, `0xF619`: inside the `PositionPack`, after its variable-length quaternion.
/// * `0xF750`, `0xF754`, `0xF755`: none at all.
fn instance_sequence(op: Opcode, body: &[u8]) -> Option<u16> {
    let mut r = Reader::body(body);
    r.u32().ok()?; // the object id
    match op {
        Opcode::ITEM_DELETE_OBJECT | Opcode::MOVEMENT_SET_OBJECT_MOVEMENT => r.u16().ok(),
        Opcode::ITEM_PARENT_EVENT => {
            for _ in 0..3 {
                r.u32().ok()?;
            }
            r.u16().ok()
        }
        Opcode::ITEM_SET_STATE => {
            r.u32().ok()?;
            r.u16().ok()
        }
        Opcode::INVENTORY_PICKUP_EVENT => r.u16().ok(),
        Opcode::MOVEMENT_VECTOR_UPDATE => {
            for _ in 0..6 {
                r.u32().ok()?;
            }
            r.u16().ok()
        }
        Opcode::ITEM_OBJ_DESC_EVENT => {
            dereth_protocol::types::ObjDesc::read(&mut r).ok()?;
            r.u16().ok()
        }
        Opcode::MOVEMENT_POSITION_EVENT | Opcode::MOVEMENT_POSITION_AND_MOVEMENT_EVENT => {
            dereth_protocol::movement::PositionPack::read(&mut r)
                .ok()
                .map(|p| p.instance_timestamp)
        }
        Opcode::ITEM_CREATE_OBJECT | Opcode::ITEM_UPDATE_OBJECT => {
            dereth_protocol::types::ObjDesc::read(&mut r).ok()?;
            dereth_protocol::types::PhysicsDesc::read(&mut r)
                .ok()
                .map(|p| p.timestamps.instance)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::{NetBlobId, NetQueue, RecipientId};
    use dereth_protocol::objects::{ItemDeleteObject, ItemSetState};
    use dereth_protocol::types::PhysicsEventStamp;
    use dereth_protocol::{write_body, Message};

    fn msg(opcode: u32, body: Vec<u8>) -> IncomingMessage {
        IncomingMessage {
            opcode,
            queue: NetQueue::WorldObjects,
            sender: RecipientId(0),
            blob_id: NetBlobId(0),
            body,
        }
    }

    /// The instance-sequence gate produces **all three** outcomes.
    ///
    /// Oracle: `docs/networking/messages/02-world-objects.md` §2 — the three-way test, and
    /// the newer-than test.
    #[test]
    fn the_instance_gate_has_three_outcomes() {
        let mut t = InstanceTable::new();
        let id = ObjectId(0x5000_0001);

        // Unknown object -> Queued.
        assert_eq!(instance_gate(&t, id, 5), ObjectDispatchOutcome::Queued);

        t.set(id, 5);
        // Equal -> process.
        assert_eq!(instance_gate(&t, id, 5), ObjectDispatchOutcome::ProcessedOk);
        // The message is ahead -> Queued, not dropped. Dropping "future" messages loses objects.
        assert_eq!(instance_gate(&t, id, 6), ObjectDispatchOutcome::Queued);
        // The message is behind -> OldInstance.
        assert_eq!(instance_gate(&t, id, 4), ObjectDispatchOutcome::OldInstance);
    }

    /// An unknown object parks the blob rather than dropping it, and the parked copy is the whole
    /// blob so that a replay sees the opcode again.
    #[test]
    fn an_unknown_object_parks_the_whole_blob() {
        let mut t = InstanceTable::new();
        let mut p = ParkedBlobs::new();
        let m = ItemDeleteObject {
            id: ObjectId(7),
            instance_sequence: 3,
        };
        let blob = msg(ItemDeleteObject::OPCODE.0, write_body(&m).unwrap());

        let d = dispatch(&mut t, &mut p, None, &blob);
        assert_eq!(d.outcome, ObjectDispatchOutcome::Queued);
        assert_eq!(p.parked_on(ObjectId(7)), 1);
        let parked = p.release(ObjectId(7));
        assert_eq!(&parked[0][0..4], &ItemDeleteObject::OPCODE.0.to_le_bytes());

        // Once the object is known at the same sequence, the same blob processes.
        t.set(ObjectId(7), 3);
        let d = dispatch(&mut t, &mut p, None, &blob);
        assert_eq!(d.outcome, ObjectDispatchOutcome::ProcessedOk);
    }

    /// A stale instance sequence is dropped outright, with nothing parked.
    #[test]
    fn an_old_instance_is_dropped_not_parked() {
        let mut t = InstanceTable::new();
        let mut p = ParkedBlobs::new();
        t.set(ObjectId(7), 9);
        let m = ItemDeleteObject {
            id: ObjectId(7),
            instance_sequence: 3,
        };
        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(ItemDeleteObject::OPCODE.0, write_body(&m).unwrap()),
        );
        assert_eq!(d.outcome, ObjectDispatchOutcome::OldInstance);
        assert_eq!(p.total(), 0);
        assert!(d.event.is_none());
    }

    /// The `PhysicsEventStamp` messages take their instance sequence from its **first** `u16`.
    #[test]
    fn the_timestamp_pack_messages_read_ts1_as_the_instance() {
        let mut t = InstanceTable::new();
        let mut p = ParkedBlobs::new();
        t.set(ObjectId(7), 4);
        let m = ItemSetState {
            id: ObjectId(7),
            state: 0x400,
            timestamps: PhysicsEventStamp {
                instance: 4,
                event: 99,
            },
        };
        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(ItemSetState::OPCODE.0, write_body(&m).unwrap()),
        );
        assert_eq!(
            d.outcome,
            ObjectDispatchOutcome::ProcessedOk,
            "ts1 is the instance sequence"
        );

        // ts2 is the event sequence and must not be mistaken for it.
        let m = ItemSetState {
            id: ObjectId(7),
            state: 0x400,
            timestamps: PhysicsEventStamp {
                instance: 9,
                event: 4,
            },
        };
        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(ItemSetState::OPCODE.0, write_body(&m).unwrap()),
        );
        assert_eq!(
            d.outcome,
            ObjectDispatchOutcome::Queued,
            "the message is ahead of the object"
        );
    }

    /// The effects messages have no sequence check at all, but still queue on an unknown object.
    #[test]
    fn the_effects_messages_queue_but_do_not_sequence_check() {
        use dereth_protocol::objects::EffectsSoundEvent;
        let mut t = InstanceTable::new();
        let mut p = ParkedBlobs::new();
        let m = EffectsSoundEvent {
            id: ObjectId(7),
            sound_type: 1,
            volume: 1.0,
        };
        let body = write_body(&m).unwrap();

        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(EffectsSoundEvent::OPCODE.0, body.clone()),
        );
        assert_eq!(d.outcome, ObjectDispatchOutcome::Queued);

        t.set(ObjectId(7), 12_345);
        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(EffectsSoundEvent::OPCODE.0, body),
        );
        assert_eq!(
            d.outcome,
            ObjectDispatchOutcome::ProcessedOk,
            "no sequence to check against"
        );
    }

    /// `0xF746` establishes the player id and a second one is ignored.
    #[test]
    fn create_player_sets_the_id_once() {
        let mut t = InstanceTable::new();
        let mut p = ParkedBlobs::new();
        let body = 0x5000_0001u32.to_le_bytes().to_vec();
        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(Opcode::LOGIN_CREATE_PLAYER.0, body.clone()),
        );
        assert_eq!(
            d.event,
            Some(SessionEvent::PlayerCreated(ObjectId(0x5000_0001)))
        );

        let d = dispatch(
            &mut t,
            &mut p,
            Some(ObjectId(0x5000_0001)),
            &msg(Opcode::LOGIN_CREATE_PLAYER.0, body),
        );
        assert_eq!(
            d.outcome,
            ObjectDispatchOutcome::Error,
            "a second CreatePlayer is ignored"
        );
    }

    /// An opcode that is not one of the sixteen returns `Error`, which the client's
    /// blob processing ignores — the blob is simply dropped.
    #[test]
    fn a_non_world_view_opcode_is_an_error() {
        let mut t = InstanceTable::new();
        let mut p = ParkedBlobs::new();
        let d = dispatch(&mut t, &mut p, None, &msg(0x0013, vec![0; 8]));
        assert_eq!(d.outcome, ObjectDispatchOutcome::Error);
        assert!(!is_world_object_opcode(Opcode(0x0013)));
        assert!(is_world_object_opcode(Opcode::ITEM_CREATE_OBJECT));
    }

    // ---------------------------------------------------------------------------------------
    // The create handler, which is the exception to the table above.
    // ---------------------------------------------------------------------------------------

    /// Build a `0xF745`/`0xF7DB` body: the payload, with an optional parent and a given instance.
    fn create_body(id: ObjectId, instance: u16, parent: Option<ObjectId>) -> Vec<u8> {
        use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
        let mut physicsdesc = PhysicsDesc {
            timestamps: dereth_protocol::types::PhysicsTimestamps {
                instance,
                ..dereth_protocol::types::PhysicsTimestamps::default()
            },
            ..PhysicsDesc::default()
        };
        if let Some(p) = parent {
            physicsdesc.bitfield |= flags::PARENT;
            physicsdesc.parent = Some((p, 0));
        }
        let p = dereth_protocol::objects::ObjectCreatePayload {
            id,
            objdesc: ObjDesc::default(),
            physicsdesc,
            wdesc: PublicWeenieDesc::default(),
        };
        write_body(&dereth_protocol::objects::ItemCreateObject(p)).expect("encode")
    }

    /// Step 4: an unknown object takes the *create* path. Parking it
    /// the way every other handler parks means no object is ever created.
    #[test]
    fn an_unknown_object_is_created_rather_than_parked() {
        let mut t = InstanceTable::new();
        let mut p = ParkedBlobs::new();
        let id = ObjectId(0x5000_0001);
        let body = create_body(id, 7, None);

        for op in [Opcode::ITEM_CREATE_OBJECT, Opcode::ITEM_UPDATE_OBJECT] {
            let d = dispatch(&mut t, &mut p, None, &msg(op.0, body.clone()));
            assert_eq!(
                d.outcome,
                ObjectDispatchOutcome::ProcessedOk,
                "{op:?} on an unknown object"
            );
            assert_eq!(
                d.event,
                Some(SessionEvent::WorldObject {
                    opcode: op,
                    body: body.clone()
                }),
                "the create reaches the consumer whole"
            );
            assert_eq!(
                p.total(),
                0,
                "nothing is parked on the object being created"
            );
        }
    }

    /// Step 1: the only thing a create parks on is an unknown **parent**, and a known parent falls
    /// straight through.
    #[test]
    fn a_create_parks_on_an_unknown_parent_and_only_on_that() {
        let mut t = InstanceTable::new();
        let mut p = ParkedBlobs::new();
        let child = ObjectId(0x5000_0002);
        let parent = ObjectId(0x5000_0003);
        let body = create_body(child, 1, Some(parent));

        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(Opcode::ITEM_CREATE_OBJECT.0, body.clone()),
        );
        assert_eq!(d.outcome, ObjectDispatchOutcome::Queued);
        assert_eq!(
            p.parked_on(parent),
            1,
            "parked on the parent, not on the child"
        );
        assert_eq!(p.parked_on(child), 0);

        t.set(parent, 4);
        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(Opcode::ITEM_CREATE_OBJECT.0, body),
        );
        assert_eq!(
            d.outcome,
            ObjectDispatchOutcome::ProcessedOk,
            "a known parent does not park"
        );
    }

    /// Step 3, all three arms, and the `fRecreate` asymmetry between the two opcodes.
    #[test]
    fn a_create_for_a_known_object_follows_step_three() {
        let mut t = InstanceTable::new();
        let mut p = ParkedBlobs::new();
        let id = ObjectId(0x5000_0004);
        t.set(id, 5);

        // 3c: the sequences match -> the in-place merge.
        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(Opcode::ITEM_CREATE_OBJECT.0, create_body(id, 5, None)),
        );
        assert_eq!(d.outcome, ObjectDispatchOutcome::ProcessedOk);
        // 3a: the message is ahead -> the create path, *not* `Queued`.
        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(Opcode::ITEM_CREATE_OBJECT.0, create_body(id, 6, None)),
        );
        assert_eq!(
            d.outcome,
            ObjectDispatchOutcome::ProcessedOk,
            "the server re-created the object"
        );
        // 3b: the message is behind -> dropped.
        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(Opcode::ITEM_CREATE_OBJECT.0, create_body(id, 4, None)),
        );
        assert_eq!(d.outcome, ObjectDispatchOutcome::OldInstance);
        assert!(d.event.is_none());
        // `0xF7DB` passes `fRecreate = 1`, which is read *before* the sequence, so even a stale one
        // rebuilds. ( is a one-line forwarder.)
        let d = dispatch(
            &mut t,
            &mut p,
            None,
            &msg(Opcode::ITEM_UPDATE_OBJECT.0, create_body(id, 4, None)),
        );
        assert_eq!(d.outcome, ObjectDispatchOutcome::ProcessedOk);
        assert_eq!(p.total(), 0, "no arm of step 3 parks");
    }

    /// The sixteen opcodes the object queue dispatches
    /// (`docs/networking/messages/00-dispatch-and-queues.md` §1).
    #[test]
    fn there_are_sixteen_world_view_opcodes() {
        let n = dereth_protocol::OPCODES
            .iter()
            .filter(|i| is_world_object_opcode(i.opcode))
            .count();
        assert_eq!(n, 16);
    }
}
