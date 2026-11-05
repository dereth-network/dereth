//! Behaviour: none (synthetic wire-format and socket-free handshake contracts).

use super::*;
use crate::wire as ac1999;

fn endpoint() -> SocketAddr {
    "127.0.0.1:9000".parse().unwrap()
}
fn ac1999() -> Handshake {
    let metadata = LoginMetadata {
        portal_iteration: 105,
        cell_iteration: 67,
    };
    Handshake::new(endpoint(), "demo", "secret", metadata).unwrap()
}
fn ac1999_grant(version: &str) -> Vec<u8> {
    let mut body = 1.0f64.to_le_bytes().to_vec();
    body.extend_from_slice(&2.0f64.to_le_bytes());
    body.extend_from_slice(&0x1234_5678u32.to_le_bytes());
    body.extend_from_slice(&u32::try_from(version.len()).unwrap().to_le_bytes());
    body.extend_from_slice(version.as_bytes());
    body.extend_from_slice(&[0; 8]);
    ac1999::encode_generic(
        ac1999::Header {
            flags: ac1999::flags::GRANT | ac1999::flags::CHECKSUM,
            recipient: 77,
            ..Default::default()
        },
        &body,
    )
    .unwrap()
}

#[test]
fn the_ac1999_grant_ack_preserves_full_slot_same_endpoint_and_separate_keys() {
    let mut h = ac1999();
    let request = h.request(LocalTime(0.0)).unwrap().unwrap();
    assert_eq!(
        ac1999::Packet::decode(request.bytes())
            .unwrap()
            .header
            .flags,
        2
    );
    assert!(!format!("{request:?} {h:?}").contains("secret"));
    let grant = ac1999_grant(ac1999::CLIENT_VERSION);
    let Reply::Granted { acknowledgment } = h.receive(&grant, endpoint()).unwrap() else {
        panic!("expected grant")
    };
    assert_eq!(acknowledgment.destination, endpoint());
    // Empty ack: seed + header length contribution + flags + full assigned recipient.
    assert_eq!(
        acknowledgment.bytes(),
        &[0, 0, 0, 0, 2, 2, 0, 0, 0x78, 0x56, 0x34, 0x12, 0xef, 0x71, 0x71, 0x15, 0, 0, 0, 0]
    );
    let grant = h.accepted().expect("the grant");
    assert_eq!(grant.server, 77);
    assert_eq!((grant.world_epoch, grant.recipient), (2.0, 0x1234_5678));
    let mut keys = h.take_keys().unwrap();
    assert_eq!(keys.incoming.next(), 0x1826_00f3);
    assert_eq!(keys.incoming.next(), 0x300b_4a8d);
    assert_eq!(keys.outgoing.next(), 0x1826_00f3);
    assert!(matches!(
        h.receive(&ac1999_grant(ac1999::CLIENT_VERSION), endpoint())
            .unwrap(),
        Reply::Duplicate { .. }
    ));
    assert!(h.take_keys().is_none());
    assert_eq!(h.acknowledgment().unwrap().bytes(), acknowledgment.bytes());
    assert!(h.request(LocalTime(10.0)).unwrap().is_none());
}

#[test]
fn wrong_endpoint_version_and_malformed_grants_leave_state_untouched() {
    let grant = ac1999_grant(ac1999::CLIENT_VERSION);
    let mut h = ac1999();
    assert!(matches!(h.receive(&grant, endpoint()), Err(Error::State)));
    h.request(LocalTime(0.0)).unwrap();
    assert!(matches!(
        h.receive(&grant, "127.0.0.1:9001".parse().unwrap()),
        Err(Error::Endpoint)
    ));
    assert!(matches!(
        h.receive(&ac1999_grant("1802"), endpoint()),
        Err(Error::Version)
    ));
    for end in 0..grant.len() {
        assert!(h.receive(&grant[..end], endpoint()).is_err());
    }
    let mut corrupt = grant.clone();
    corrupt[12] ^= 1;
    assert!(h.receive(&corrupt, endpoint()).is_err());
    assert_eq!(h.state(), State::AwaitingGrant);
    assert!(h.accepted().is_none());
    assert!(h.take_keys().is_none());
    h.receive(&grant, endpoint()).unwrap();
    let mut body = ac1999::Packet::decode(&grant).unwrap().body.to_vec();
    body[16] ^= 1;
    let changed = ac1999::encode_generic(
        ac1999::Header {
            flags: ac1999::flags::GRANT | ac1999::flags::CHECKSUM,
            recipient: 77,
            ..Default::default()
        },
        &body,
    )
    .unwrap();
    assert!(matches!(
        h.receive(&changed, endpoint()),
        Err(Error::Response)
    ));
}

#[test]
fn refusal_terminates_authentication_without_more_credential_sends() {
    let mut h = ac1999();
    h.request(LocalTime(0.0)).unwrap();
    let refusal = ac1999::encode_generic(
        ac1999::Header {
            flags: ac1999::flags::NET_ERROR,
            ..Default::default()
        },
        &[1, 0, 0, 0, 8, 0, 0, 0],
    )
    .unwrap();
    assert!(matches!(
        h.receive(&refusal, endpoint()).unwrap(),
        Reply::Refused {
            string_id: 1,
            table_id: 8
        }
    ));
    assert_eq!(h.state(), State::Refused);
    assert!(h.request(LocalTime(2.0)).unwrap().is_none());
    assert!(matches!(
        h.receive(&ac1999_grant(ac1999::CLIENT_VERSION), endpoint()),
        Err(Error::State)
    ));
    assert!(h.acknowledgment().is_none());
}

#[test]
fn retries_use_injected_time_and_timeout_is_terminal() {
    let mut h = ac1999();
    assert!(matches!(h.request(LocalTime(f64::NAN)), Err(Error::Clock)));
    let first = h.request(LocalTime(0.0)).unwrap().unwrap();
    assert!(h.request(LocalTime(1.9)).unwrap().is_none());
    assert!(matches!(h.request(LocalTime(1.8)), Err(Error::Clock)));
    for i in 1..20 {
        assert_eq!(
            h.request(LocalTime(f64::from(i) * 2.0))
                .unwrap()
                .unwrap()
                .bytes(),
            first.bytes()
        );
    }
    assert!(matches!(h.request(LocalTime(37.0)), Err(Error::Clock)));
    assert!(matches!(h.request(LocalTime(40.0)), Err(Error::Timeout)));
    assert_eq!(h.state(), State::TimedOut);
    assert!(h.request(LocalTime(42.0)).unwrap().is_none());
    assert!(h
        .receive(&ac1999_grant(ac1999::CLIENT_VERSION), endpoint())
        .is_err());
}
