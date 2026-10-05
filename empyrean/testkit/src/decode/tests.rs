//! Behaviour: none (message-decoder and wrapper adapter contracts).

use super::*;
use dereth_primitives::NetQueue;

fn incoming(opcode: u32, body: &[u8]) -> IncomingMessage {
    IncomingMessage {
        opcode,
        body: body.to_vec(),
        queue: NetQueue::UiQueue,
        sender: Default::default(),
        blob_id: Default::default(),
    }
}

#[test]
fn alternate_bodies_keep_their_candidate_order_and_round_trip_gate() {
    let cases: &[(u32, &[&str])] = &[
        (
            0x0147,
            &[
                "CommunicationChannelBroadcast",
                "CommunicationChannelBroadcastRecv",
            ],
        ),
        (
            0x0149,
            &[
                "CommunicationChannelIndexRecv",
                "CommunicationChannelIndexRequest",
            ],
        ),
        (
            0x0148,
            &[
                "CommunicationChannelListRecv",
                "CommunicationChannelListRequest",
            ],
        ),
        (0xF655, &["CharacterDeleteAck", "CharacterDeleteRequest"]),
        (0xF653, &["LoginExecuteLogOff", "LoginExecuteLogOffRequest"]),
        (0x00A3, &["FellowshipQuitNotice", "FellowshipQuitRequest"]),
        (0xF7DE, &["CommunicationTurbineChat", "SendToRoomById"]),
    ];
    assert_eq!(table().values().map(Vec::len).sum::<usize>(), 200);
    for (opcode, expected) in cases {
        let names: Vec<_> = table()[opcode].iter().map(|(name, _)| *name).collect();
        assert_eq!(&names, expected);
    }
    assert_eq!(
        decode(&incoming(0xF653, &[])).result,
        Ok("LoginExecuteLogOff")
    );
    assert_eq!(
        decode(&incoming(0xF653, &[1, 2, 3, 4])).result,
        Ok("LoginExecuteLogOffRequest")
    );
    assert_eq!(
        decode(&incoming(0xF7DE, &[1, 2, 3])).result,
        Ok("CommunicationTurbineChat")
    );
    // The string reader accepts a single NUL as empty, but its writer emits length zero.
    let error = decode(&incoming(0x0004, &[1, 0, 0, 0])).result.unwrap_err();
    assert!(
        error.contains("re-encodes differently from byte 0"),
        "{error}"
    );
}

#[test]
fn padding_and_known_trailing_bytes_remain_distinct() {
    let body = [2, 0, b'h', b'i'];
    assert_eq!(
        decode(&incoming(0x0004, &body)).result,
        Ok("CommunicationPopUpString")
    );
    let mut padded = body.to_vec();
    padded.extend_from_slice(&[0, 0, 0]);
    assert_eq!(
        decode(&incoming(0x0004, &padded)).result,
        Ok("CommunicationPopUpString")
    );
    padded.push(0);
    assert!(decode(&incoming(0x0004, &padded))
        .result
        .unwrap_err()
        .contains("4 bytes left over"));

    let mut fellowship =
        proto::write_body(&proto::social::FellowshipFullUpdate::default()).unwrap();
    fellowship.extend_from_slice(&[1, 2, 3, 4]);
    let message = incoming(0x02BE, &fellowship);
    assert!(decode(&message)
        .result
        .unwrap_err()
        .contains("4 bytes left over"));
    assert_reencodes(&message);
}

#[test]
#[cfg(all(feature = "captures", feature = "soak"))]
fn wrapper_consumers_keep_short_bare_action_and_matching_event_boundaries() {
    use crate::capture_replay::wire::{action_type, event_type};
    use crate::soak::bot::kind;
    use dereth_primitives::ObjectId;
    use proto::comms::CommunicationPopUpString;
    let value = CommunicationPopUpString {
        message: "hi".into(),
    };
    let event = proto::events::pack_event(ObjectId(7), 2, &value).unwrap();
    assert_eq!(event_type(&event), Some(0x0004));
    assert_eq!(action_type(&event), None);
    for end in 0..16 {
        assert_eq!(event_type(&event[..end]), None);
    }
    let ordered = incoming(proto::OrderedEventHeader::MAGIC, &event[4..]);
    assert_eq!(kind(&ordered), 0x0004);
    assert_eq!(decode(&ordered).result, Ok("CommunicationPopUpString"));
    let bare = incoming(0x0004, &[2, 0, b'h', b'i']);
    let short = incoming(proto::OrderedEventHeader::MAGIC, &[0; 11]);
    assert_eq!(kind(&short), proto::OrderedEventHeader::MAGIC);
    assert_eq!(kind(&bare), 0x0004);
    let action = [0xB1, 0xF7, 0, 0, 2, 0, 0, 0, 4, 0, 0, 0];
    assert_eq!(action_type(&action), Some(4));
    assert_eq!(event_type(&action), None);
    for end in 0..12 {
        assert_eq!(action_type(&action[..end]), None);
    }
    let action_message = incoming(proto::OrderedActionHeader::MAGIC, &action[4..]);
    assert_eq!(kind(&action_message), proto::OrderedActionHeader::MAGIC);
    let wrong = incoming(
        proto::OrderedEventHeader::MAGIC,
        &[0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0, 0, 0],
    );
    assert_eq!(
        all_of::<CommunicationPopUpString>(&[short, action_message, wrong, bare, ordered]),
        vec![value.clone(), value]
    );
}
