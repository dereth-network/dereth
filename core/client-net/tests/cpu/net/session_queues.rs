//! Contracts for session queues.
//! Fixture: shared recorded messages and synthetic state.
//! Behaviour: none (codec, fixture conformance or host-state contracts)

use crate::common::session_fixture::*;

#[test]
fn an_ordered_event_for_an_unknown_object_waits_for_it() {
    let mut s = session();
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&player_description()).unwrap(),
    );
    s.tick(t(0.0));
    s.drain_events().count();

    let target = ObjectId(0x5000_00AA);
    let blob = pack_event(
        target,
        1,
        &QualitiesPrivateUpdateInt(PrivateUpdate {
            sequence: 1,
            property_id: 25,
            value: 42,
        }),
    )
    .unwrap();
    s.transport.deliver_blob(NetQueue::UiQueue, &blob);
    s.tick(t(1.0));
    assert!(
        !s.drain_events()
            .any(|e| matches!(e, SessionEvent::UiEvent { .. })),
        "held on the unknown object"
    );

    s.object_arrived(target, 0);
    let events: Vec<_> = s.drain_events().collect();
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::UiEvent { opcode, .. } if *opcode == QualitiesPrivateUpdateInt::OPCODE
        )),
        "replayed once the object exists, got {events:?}"
    );
}

#[test]
fn a_world_view_blob_parked_on_an_unknown_object_replays_through_the_world_view_dispatcher() {
    use dereth_protocol::objects::ItemDeleteObject;

    let mut s = session();
    let target = ObjectId(0x5000_00BB);
    let m = ItemDeleteObject {
        id: target,
        instance_sequence: 3,
    };
    s.transport
        .deliver_blob(NetQueue::WorldObjects, &write_blob(&m).unwrap());
    s.tick(t(0.0));
    let held: Vec<_> = s.drain_events().collect();
    assert!(
        !held
            .iter()
            .any(|e| matches!(e, SessionEvent::WorldObject { .. })),
        "held on the unknown object, got {held:?}"
    );

    s.object_arrived(target, 3);
    let events: Vec<_> = s.drain_events().collect();
    assert!(
        events.iter().any(|e| matches!(
            e,
            SessionEvent::WorldObject { opcode, .. } if *opcode == ItemDeleteObject::OPCODE
        )),
        "replayed as a world controller event, got {events:?}"
    );
    assert!(
        !events.iter().any(|e| matches!(
            e,
            SessionEvent::Dropped {
                queue: NetQueue::UiQueue,
                ..
            }
        )),
        "a physics-list blob must never be offered to the UI switch, got {events:?}"
    );
}

#[test]
fn updates_and_deletes_for_objects_never_created_are_held_then_dropped() {
    use dereth_protocol::objects::{ItemDeleteObject, ItemSetState};
    use dereth_protocol::types::PhysicsEventStamp;

    let mut s = session();
    let world_object_events = |events: &[SessionEvent]| {
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::WorldObject { .. }))
            .count()
    };

    let seen_later = ObjectId(0x8000_0101);
    let update = ItemSetState {
        id: seen_later,
        state: 0x400,
        timestamps: PhysicsEventStamp {
            instance: 1,
            event: 2,
        },
    };
    s.transport
        .deliver_blob(NetQueue::WorldObjects, &write_blob(&update).unwrap());
    let never = ObjectId(0x8000_0102);
    s.transport.deliver_blob(
        NetQueue::WorldObjects,
        &write_blob(&ItemDeleteObject {
            id: never,
            instance_sequence: 1,
        })
        .unwrap(),
    );
    let reborn = ObjectId(0x8000_0103);
    s.transport.deliver_blob(
        NetQueue::WorldObjects,
        &write_blob(&ItemDeleteObject {
            id: reborn,
            instance_sequence: 4,
        })
        .unwrap(),
    );
    s.tick(t(0.0));
    let held: Vec<_> = s.drain_events().collect();
    assert_eq!(
        world_object_events(&held),
        0,
        "all three are held on their ids, got {held:?}"
    );

    s.object_arrived(reborn, 5);
    let events: Vec<_> = s.drain_events().collect();
    assert_eq!(
        world_object_events(&events),
        0,
        "a stale delete does not remove the new instance, got {events:?}"
    );

    s.tick(t(10.0));
    s.drain_events().count();
    s.object_arrived(seen_later, 1);
    let events: Vec<_> = s.drain_events().collect();
    assert!(
        events.iter().any(|e| matches!(e, SessionEvent::WorldObject { opcode, .. } if *opcode == ItemSetState::OPCODE)),
        "the held update is replayed on the create, got {events:?}"
    );

    assert_eq!(s.destroy_expired_parked_blobs(24.9), 0, "not before 25 s");
    assert_eq!(
        s.destroy_expired_parked_blobs(25.1),
        1,
        "the one left, `never`, is dropped at 25 s"
    );
    s.object_arrived(never, 1);
    let events: Vec<_> = s.drain_events().collect();
    assert_eq!(
        world_object_events(&events),
        0,
        "a later create of that id is not deleted by it, got {events:?}"
    );
}

#[test]
fn the_action_counter_is_global_and_rolls_back() {
    let mut s = session();
    assert_eq!(
        s.next_action_stamp(),
        1,
        "a fresh session's first action carries 1"
    );
    let a = s
        .send_action(&dereth_protocol::comms::CommunicationTalk {
            message: "one".into(),
        })
        .unwrap();
    let b = s
        .send_action(&dereth_protocol::items::InventoryDropItem { item: ObjectId(1) })
        .unwrap();
    assert_eq!((a, b), (1, 2), "one counter across two categories");

    s.transport.fail_next_send = true;
    let c = s
        .send_action(&dereth_protocol::comms::CommunicationTalk {
            message: "three".into(),
        })
        .unwrap();
    assert_eq!(c, 3);
    s.report_send_failed();

    let d = s
        .send_action(&dereth_protocol::comms::CommunicationTalk {
            message: "three".into(),
        })
        .unwrap();
    assert_eq!(d, 3);

    for sent in &s.transport.sent {
        assert_eq!(sent.queue, NetQueue::Weenie);
        assert!(sent.ordered);
        assert_eq!(&sent.payload[0..4], &0xF7B1u32.to_le_bytes());
    }
}

#[test]
fn the_queue_map_is_honoured() {
    use dereth_protocol::actions::{outbound_kind, OutboundKind};

    let mut s = session();
    s.send_action(&dereth_protocol::comms::CommunicationTalk {
        message: "hi".into(),
    })
    .unwrap();
    assert_eq!(s.transport.sent[0].queue, NetQueue::Weenie);

    for (op, expect_login) in [
        (Opcode::LOGIN_SEND_ENTER_WORLD, true),
        (Opcode::DDD_REQUEST_DATA_MESSAGE, true),
        (Opcode::OBJECT_SEND_FORCE_OBJDESC, false),
        (Opcode::COMMUNICATION_TALK, false),
    ] {
        let kind = outbound_kind(op).unwrap();
        assert_eq!(kind.goes_to_login_server(), expect_login, "{op:?}");
    }
    assert_eq!(
        outbound_kind(Opcode::COMMUNICATION_TALK),
        Some(OutboundKind::GameAction)
    );
}

#[test]
fn blobs_on_undrained_queues_are_discarded() {
    let mut s = session();
    s.transport.deliver_blob(
        NetQueue::Control,
        &write_blob(&LoginWorldInfo::default()).unwrap(),
    );
    s.transport.deliver_blob(
        NetQueue::Other(11),
        &write_blob(&LoginWorldInfo::default()).unwrap(),
    );
    s.tick(t(0.0));
    let events: Vec<_> = s.drain_events().collect();
    assert_eq!(events.len(), 2);
    assert!(events
        .iter()
        .all(|e| matches!(e, SessionEvent::Dropped { .. })));
    assert_eq!(s.world_name(), None, "nothing was applied");
}

#[test]
fn the_login_queue_only_yields_turbine_chat() {
    let mut s = session();
    s.transport
        .deliver_blob(NetQueue::Logon, &write_blob(&character_set()).unwrap());
    s.transport.deliver_blob(
        NetQueue::Logon,
        &write_blob(&dereth_protocol::comms::CommunicationTurbineChat {
            payload: vec![0xDE, 0xAD],
        })
        .unwrap(),
    );
    s.tick(t(0.0));
    let events: Vec<_> = s.drain_events().collect();
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], SessionEvent::Dropped { .. }));
    assert_eq!(events[1], SessionEvent::TurbineChat(vec![0xDE, 0xAD]));
    assert!(
        s.characters().characters.is_empty(),
        "the character set on queue 4 was discarded, not applied"
    );
}

#[test]
fn a_malformed_body_is_reported_not_fatal() {
    let mut s = session();
    let mut blob = write_blob(&character_set()).unwrap();
    blob.truncate(8);
    s.transport.deliver_blob(NetQueue::UiQueue, &blob);
    s.tick(t(0.0));
    let events: Vec<_> = s.drain_events().collect();
    assert!(matches!(
        events.as_slice(),
        [SessionEvent::Dropped {
            reason: dereth_client_net::client_session::DropReason::Malformed(_),
            ..
        }]
    ));
    assert_eq!(s.state(), SessionState::Connected, "the session survives");
}
