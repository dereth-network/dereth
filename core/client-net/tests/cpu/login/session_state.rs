//! Contracts for session state.
//! Fixture: shared recorded messages and synthetic state.
//! Behaviour: none (codec, fixture conformance or host-state contracts)

use crate::common::session_fixture::*;

#[test]
fn login_through_enter_world_reaches_playable() {
    let mut s = session();
    assert_eq!(s.state(), SessionState::Connected);

    s.transport
        .deliver_blob(NetQueue::UiQueue, &write_blob(&character_set()).unwrap());
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&LoginWorldInfo {
            connections: 12,
            max_connections: 800,
            world_name: "Frostfell".into(),
        })
        .unwrap(),
    );
    s.tick(t(0.0));
    assert_eq!(s.state(), SessionState::CharacterSelect);
    assert_eq!(s.characters().characters.len(), 1);
    assert_eq!(s.world_name(), Some("Frostfell"));
    assert_eq!(s.account(), "ac01");

    s.enter_world(ObjectId(0x5000_0001), "ac01");
    assert_eq!(s.state(), SessionState::EnteringWorld);
    assert_eq!(s.transport.sent_opcodes(), vec![0xF7C8]);
    assert_eq!(s.transport.sent[0].queue, NetQueue::Logon);
    assert!(
        !s.transport.sent[0].ordered,
        "no action-order header on a Logon-queue message"
    );

    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&LoginEnterGameServerReady).unwrap(),
    );
    s.tick(t(1.0));
    assert_eq!(
        s.transport.sent_opcodes(),
        vec![0xF7C8, 0xF657],
        "in that order"
    );
    assert_eq!(s.transport.sent[1].queue, NetQueue::Logon);
    assert_eq!(s.state(), SessionState::EnteringWorld);

    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&player_description()).unwrap(),
    );
    s.tick(t(2.0));
    assert_eq!(s.state(), SessionState::Playable);

    let events: Vec<_> = s.drain_events().collect();
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::PlayerDescription(_))));
    assert!(events.contains(&SessionEvent::StateChanged(SessionState::Playable)));
}

#[test]
fn no_player_description_within_a_hundred_and_ten_seconds_fires_server_died() {
    let mut s = session();
    s.transport
        .deliver_blob(NetQueue::UiQueue, &write_blob(&character_set()).unwrap());
    s.tick(t(0.0));
    s.enter_world(ObjectId(0x5000_0001), "ac01");
    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&LoginEnterGameServerReady).unwrap(),
    );
    s.tick(t(5.0));
    assert_eq!(s.state(), SessionState::EnteringWorld);

    s.tick(t(114.0));
    assert_eq!(
        s.state(),
        SessionState::EnteringWorld,
        "109 s is not enough"
    );

    s.tick(t(115.0));
    assert_eq!(
        s.state(),
        SessionState::Disconnected(DisconnectReason::ServerDied)
    );
}

#[test]
fn log_off_emits_f653_on_the_logon_queue() {
    let mut s = session();
    s.transport
        .deliver_blob(NetQueue::UiQueue, &write_blob(&character_set()).unwrap());
    s.tick(t(0.0));
    s.enter_world(ObjectId(0x5000_0001), "ac01");
    s.transport.sent.clear();

    s.log_off();
    assert_eq!(s.transport.sent_opcodes(), vec![0xF653]);
    assert_eq!(s.transport.sent[0].queue, NetQueue::Logon);
    assert_eq!(
        s.transport.sent[0].payload,
        write_blob(&dereth_protocol::login::LoginExecuteLogOffRequest {
            character: ObjectId(0x5000_0001)
        })
        .unwrap()
    );
}

#[test]
fn the_enter_world_burst_dispatches_live_rather_than_waiting_for_the_player_description() {
    let mut s = session();
    let ace_id = NetBlobId(0x8000_0000u64 << 32);

    for v in 1..=3i32 {
        #[allow(clippy::cast_sign_loss)] // v is 1..=3
        let blob = pack_event(
            ObjectId(0),
            v as u32,
            &QualitiesPrivateUpdateInt(PrivateUpdate {
                sequence: 1,
                property_id: 25,
                value: v,
            }),
        )
        .unwrap();
        s.transport
            .deliver_blob_with_id(NetQueue::UiQueue, &blob, ace_id);
    }
    s.tick(t(0.0));

    let values: Vec<i32> = s
        .drain_events()
        .filter_map(|e| match e.ui_body() {
            Some((opcode, body)) if opcode == QualitiesPrivateUpdateInt::OPCODE => {
                dereth_protocol::read_body::<QualitiesPrivateUpdateInt>(body)
                    .ok()
                    .map(|m| m.0.value)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        values,
        vec![1, 2, 3],
        "an ACE-stamped burst must dispatch live and in order; holding it is the deadlock"
    );
}

#[test]
fn the_ddd_exchange_completes_and_replies_with_f7ea() {
    use dereth_protocol::admin::{DddInterrogation, TaggedIterationList};

    let mut s = session();
    s.transport.deliver_blob(
        NetQueue::ClientCache,
        &write_blob(&DddInterrogation {
            servers_region: 1,
            name_rule_language: 1,
            product_id: 1,
            supported_languages: vec![0, 1],
        })
        .unwrap(),
    );
    s.tick(t(0.0));
    let events: Vec<_> = s.drain_events().collect();
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::Ddd(dereth_client_net::client_session::DddEvent::Interrogation(
            _
        ))
    )));

    s.answer_ddd_interrogation(
        &dereth_client_net::client_session::dispatch::database::interrogation_response(
            1,
            vec![TaggedIterationList {
                dat_file_type: 0,
                dat_file_id: 1,
                iterations: dereth_protocol::admin::MostlyConsecutiveIntSet::default(),
            }],
            0,
        ),
    );
    assert_eq!(s.transport.sent_opcodes(), vec![0xF7E6]);
    assert_eq!(s.transport.sent[0].queue, NetQueue::ClientCache);

    s.transport.deliver_blob(
        NetQueue::ClientCache,
        &write_blob(&dereth_protocol::admin::DddEndDdd).unwrap(),
    );
    s.tick(t(1.0));
    assert_eq!(
        s.transport.sent_opcodes(),
        vec![0xF7E6, 0xF7EA],
        "the end message goes back as 0xF7EA, not 0xF7EB"
    );
}

#[test]
fn create_player_establishes_the_player_id() {
    let mut s = session();
    s.transport.deliver_blob(
        NetQueue::WorldObjects,
        &write_blob(&dereth_protocol::objects::LoginCreatePlayer {
            player_id: ObjectId(0x5000_0001),
        })
        .unwrap(),
    );
    s.tick(t(0.0));
    assert_eq!(s.player_id(), Some(ObjectId(0x5000_0001)));
    assert!(s
        .drain_events()
        .any(|e| e == SessionEvent::PlayerCreated(ObjectId(0x5000_0001))));
}

#[test]
fn a_character_error_disconnects_with_its_code() {
    let mut s = session();
    s.transport
        .deliver_blob(NetQueue::UiQueue, &write_blob(&character_set()).unwrap());
    s.tick(t(0.0));
    s.enter_world(ObjectId(0x5000_0001), "ac01");

    s.transport.deliver_blob(
        NetQueue::UiQueue,
        &write_blob(&dereth_protocol::login::CharacterError { char_error: 13 }).unwrap(),
    );
    s.tick(t(1.0));
    assert_eq!(
        s.state(),
        SessionState::Disconnected(DisconnectReason::CharacterError(13))
    );
    assert!(s
        .drain_events()
        .any(|e| e == SessionEvent::CharacterError(13)));
}
