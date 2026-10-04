//! Session queue admission: receive does not admit future messages against an old owner; a UI
//! callback can delete its owner before the next stamp; physical release is a snapshot and re-parks
//! once; the network timeout clock is independent of the UI clock; a surviving weenie accepts UI
//! without releasing physics.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_client_net::client_session::{testing::MockTransport, Session, SessionEvent};
use dereth_primitives::{LocalTime, NetQueue, ObjectId};
use dereth_protocol::{
    events::pack_event,
    objects::{ItemServerSaysRemove, ItemSetState},
    write_blob,
};

/// Behaviour: objects.ordered-replies.a-message-for-a-replaced-owner-waits-for-the-new-owner
#[test]
fn receive_does_not_admit_future_messages_against_the_old_owner() {
    let id = ObjectId(42);
    let mut session = Session::new(MockTransport::new());
    session.begin_object_arrival(id, 1);
    let blob = write_blob(&ItemSetState {
        id,
        state: 0x800,
        timestamps: dereth_protocol::types::PhysicsEventStamp {
            instance: 2,
            event: 9,
        },
    })
    .unwrap();
    session
        .transport
        .deliver_blob(NetQueue::WorldObjects, &blob);
    session.receive(LocalTime(9_000.0));
    assert!(session.drain_events().next().is_none());
    // Host deletion/replacement occurs before this next WorldObjects queue entry is admitted.
    session.object_deleted(id);
    session.begin_object_arrival(id, 2);
    assert!(session.process_next_world_object(LocalTime(0.5)));
    let events: Vec<_> = session.drain_events().collect();
    assert!(matches!(events.as_slice(),[SessionEvent::WorldObject {body,..}] if body==&blob[4..]));
    assert!(!session.process_next_world_object(LocalTime(0.5)));
    assert!(
        session.take_object_message(id).is_empty(),
        "no pre-admission parked copy"
    );
}

#[test]
fn a_ui_callback_can_delete_its_owner_before_the_next_ready_stamp_is_advanced() {
    let id = ObjectId(43);
    let mut session = Session::new(MockTransport::new());
    session.begin_object_arrival(id, 1);
    for stamp in [2, 1, 1] {
        session.transport.deliver_blob(
            NetQueue::UiQueue,
            &pack_event(
                id,
                stamp,
                &ItemServerSaysRemove {
                    object: ObjectId(100 + stamp),
                },
            )
            .unwrap(),
        );
    }
    session.receive(LocalTime(8_000.0));
    assert!(session.process_next_ui(LocalTime(0.2))); // gap2 is parked
    assert!(session.drain_events().next().is_none());
    assert!(session.process_next_ui(LocalTime(0.2))); // stamp1, not the predecoded2 tail
    let events: Vec<_> = session.drain_events().collect();
    assert!(matches!(events.as_slice(),[SessionEvent::UiEvent {blob,..}]
        if blob==&write_blob(&ItemServerSaysRemove {object:ObjectId(101)}).unwrap()));
    // Actual object-deletion API must cancel the continuation along with the SequenceGate owner.
    session.object_deleted(id);
    session.begin_object_arrival(id, 2);
    assert!(session.process_next_ui(LocalTime(0.2)));
    let events: Vec<_> = session.drain_events().collect();
    assert!(matches!(events.as_slice(),[SessionEvent::UiEvent {blob,..}]
        if blob==&write_blob(&ItemServerSaysRemove {object:ObjectId(101)}).unwrap()));
    assert!(!session.process_next_ui(LocalTime(0.2)));
    assert!(
        !session.process_next_object_ui(id, LocalTime(0.2)),
        "old stamp2 was not replayed"
    );
}

#[test]
fn physical_release_is_a_snapshot_and_future_bytes_repark_once_without_spin() {
    let id = ObjectId(44);
    let mut session = Session::new(MockTransport::new());
    let blob = write_blob(&ItemSetState {
        id,
        state: 0x800,
        timestamps: dereth_protocol::types::PhysicsEventStamp {
            instance: 2,
            event: 7,
        },
    })
    .unwrap();
    session
        .transport
        .deliver_blob(NetQueue::WorldObjects, &blob);
    session.receive(LocalTime(7_000.0));
    assert!(session.process_next_world_object(LocalTime(0.1)));
    assert!(session.drain_events().next().is_none());
    session.begin_object_arrival(id, 1);
    let batch = session.take_object_message(id);
    assert_eq!(batch, vec![blob.clone()]);
    session.process_object_message(&batch[0], LocalTime(0.2));
    assert!(session.drain_events().next().is_none());
    let batch = session.take_object_message(id);
    assert_eq!(
        batch,
        vec![blob.clone()],
        "one unchanged physical parked entry, no duplicate"
    );
    session.begin_object_arrival(id, 2); // host promotion, deliberately not an old-owner deletion
    session.process_object_message(&batch[0], LocalTime(0.3));
    let events: Vec<_> = session.drain_events().collect();
    assert!(matches!(events.as_slice(),[SessionEvent::WorldObject {body,..}] if body==&blob[4..]));
    assert!(session.take_object_message(id).is_empty());
}

#[test]
fn raw_network_timeout_domain_is_independent_of_ui_callback_clock() {
    let mut session = Session::new(MockTransport::new());
    session.network_use_time(LocalTime(1_000.0));
    session.enter_world(ObjectId(45), "private-phase-account");
    session.drain_events().for_each(drop);
    session
        .transport
        .deliver_blob(NetQueue::UiQueue, &0xF7DF_u32.to_le_bytes());
    session.receive(LocalTime(1_000.0));
    assert!(session.process_next_ui(LocalTime(0.01)));
    session.drain_events().for_each(drop);
    assert_eq!(
        session.state(),
        dereth_client_net::client_session::SessionState::EnteringWorld
    );
    session.network_use_time(LocalTime(1_109.0));
    assert_eq!(
        session.state(),
        dereth_client_net::client_session::SessionState::EnteringWorld
    );
    session.network_use_time(LocalTime(1_111.0));
    assert!(matches!(
        session.state(),
        dereth_client_net::client_session::SessionState::Disconnected(_)
    ));
}

#[test]
fn a_surviving_weenie_accepts_ui_without_publishing_or_releasing_physics() {
    let id = ObjectId(48);
    let mut session = Session::new(MockTransport::new());
    let physical = write_blob(&ItemSetState {
        id,
        state: 0x4000,
        timestamps: dereth_protocol::types::PhysicsEventStamp {
            instance: 1,
            event: 1,
        },
    })
    .unwrap();
    session
        .transport
        .deliver_blob(NetQueue::WorldObjects, &physical);
    session.receive(LocalTime(10.0));
    assert!(session.process_next_world_object(LocalTime(1.0)));
    assert!(session.drain_events().next().is_none());
    // Explicit callback from successful CreateWeenie after failed physical InitNullObject.
    session.begin_weenie_arrival(id);
    assert!(!session.instances_mut().knows(id));
    assert!(
        session.drain_events().next().is_none(),
        "Weenie publication is not the object net-blob processing"
    );
    session.transport.deliver_blob(
        NetQueue::UiQueue,
        &pack_event(
            id,
            1,
            &ItemServerSaysRemove {
                object: ObjectId(480),
            },
        )
        .unwrap(),
    );
    session.receive(LocalTime(11.0));
    assert!(session.process_next_ui(LocalTime(1.1)));
    let events: Vec<_> = session.drain_events().collect();
    assert!(matches!(events.as_slice(),[SessionEvent::UiEvent {blob,..}]
        if blob==&write_blob(&ItemServerSaysRemove {object:ObjectId(480)}).unwrap()));
    assert!(!session.instances_mut().knows(id));
    assert_eq!(
        session.take_object_message(id),
        vec![physical],
        "no physical callback was admitted"
    );
}

/// Behaviour: none (borrowed UI bodies distinguish event kinds without copying or validating payloads).
#[test]
fn ui_bodies_borrow_payloads_and_keep_short_messages_distinct_from_other_events() {
    use dereth_protocol::Opcode;
    assert_eq!(SessionEvent::WorldReset.ui_body(), None);
    assert_eq!(
        SessionEvent::WorldObject {
            opcode: Opcode::ITEM_SET_STATE,
            body: vec![1, 2, 3, 4, 5],
        }
        .ui_body(),
        None
    );
    for length in 0..=4 {
        let e = SessionEvent::UiEvent {
            opcode: Opcode::ITEM_USE_DONE,
            blob: vec![0xff; length],
        };
        assert_eq!(e.ui_body(), Some((Opcode::ITEM_USE_DONE, &[][..])));
    }
    let blob = vec![0xff, 0xff, 0xff, 0xff, 7, 8, 9];
    let payload = blob[4..].as_ptr();
    let e = SessionEvent::UiEvent {
        opcode: Opcode::ITEM_USE_DONE,
        blob,
    };
    let (opcode, body) = e.ui_body().expect("UI body");
    assert_eq!(
        opcode,
        Opcode::ITEM_USE_DONE,
        "the event opcode remains authoritative"
    );
    assert_eq!(body, [7, 8, 9]);
    assert_eq!(
        body.as_ptr(),
        payload,
        "the body borrows the original storage"
    );
}
