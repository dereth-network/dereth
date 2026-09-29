//! Divergence: V270, V283
//! Every section kind verifies on the server; client id 300 accepted; EchoResponse beside
//! ACK/fragment taken; NetError disconnect ends the session.
//! Fixture: locally constructed packets and session state on a virtual clock.

// V270, V283.

use dereth_transport::conn::{build_login_request, ConnectionAuthenticator};
use dereth_transport::session::SequenceWindow;
use dereth_transport::wire::optional::{flags as mflags, OPTIONAL_HEADERS};
use dereth_transport::wire::{OutPacket, PacketFlags, ProtoHeader};
use dereth_transport::{CryptoSystem, Fragment, FragmentHeader};
use empyrean_net::client_packet::ClientPacket;
use empyrean_net::{Event, NetworkStatistics, PortKind, SessionId, SessionTerminationReason};

use crate::fragments::message;
use crate::keys::by_hand;

/// A body for `mask` as the retail client lays it out, with non-zero bytes wherever the section
/// has any, so its hash is not zero.
fn body(mask: u32) -> Vec<u8> {
    match mask {
        PacketFlags::EMPTY_HEADER1 | PacketFlags::DISCONNECT => Vec::new(),
        PacketFlags::REQUEST_RETRANSMIT | PacketFlags::REJECT_RETRANSMIT => {
            [2u32, 7, 9].iter().flat_map(|v| v.to_le_bytes()).collect()
        }
        PacketFlags::LOGIN_REQUEST => {
            let mut auth = ConnectionAuthenticator::account_password("sections", "pw");
            auth.extra_data = vec![2, b'p', b'w'];
            build_login_request(&auth)
        }
        PacketFlags::FLOW => vec![0x34, 0x12, 0, 0, 0x05, 0x00],
        _ => {
            let n = match mask {
                PacketFlags::LOGON_SERVER_ADDR => 16,
                PacketFlags::REFERRAL | PacketFlags::CONNECT_REQUEST => 32,
                PacketFlags::ACK_SEQUENCE | PacketFlags::ECHO_REQUEST => 4,
                _ => 8,
            };
            #[allow(clippy::cast_possible_truncation)]
            (0..n).map(|i| 0x11u8.wrapping_mul(i as u8 + 1)).collect()
        }
    }
}

fn exclusive(mask: u32) -> bool {
    OPTIONAL_HEADERS
        .iter()
        .any(|s| s.mask == mask && s.flags & mflags::EXCLUSIVE != 0)
}

/// A client packet the shared writer builds with `mask` (alone when the section is exclusive,
/// otherwise with an ACK and, if `fragment`, a message fragment), under `key` when it needs one.
fn packet(rec_id: u16, seq: u32, mask: u32, fragment: bool, key: impl FnOnce() -> u32) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: seq,
        rec_id,
        iteration: 1,
        ..ProtoHeader::default()
    });
    p.add_optional_header(mask, body(mask)).expect("section");
    if !exclusive(mask) {
        if mask != PacketFlags::ACK_SEQUENCE {
            p.add_optional_header(PacketFlags::ACK_SEQUENCE, 1u32.to_le_bytes().to_vec())
                .expect("ack");
        }
        if fragment {
            p.add_fragment(Fragment::new(
                FragmentHeader {
                    blob_id_low: 1,
                    blob_id_high: 0x8000_0000,
                    num_frags: 1,
                    blob_num: 0,
                    queue_id: 5,
                    blob_frag_size: 0,
                },
                message(0xF7B1, 16),
            ))
            .expect("fragment");
        }
    }
    let key = if p.needs_encryption() {
        Some(key())
    } else {
        None
    };
    p.serialize(key).expect("serialize")
}

/// The sections the retail client sends to a server: everything but the server's own
/// `ServerSwitch`, `LogonServerAddr`, `Referral` and `ConnectRequest`.
const CLIENT_SENT: [u32; 15] = [
    PacketFlags::EMPTY_HEADER1,
    PacketFlags::REQUEST_RETRANSMIT,
    PacketFlags::REJECT_RETRANSMIT,
    PacketFlags::ACK_SEQUENCE,
    PacketFlags::DISCONNECT,
    PacketFlags::LOGIN_REQUEST,
    PacketFlags::WORLD_LOGIN_REQUEST,
    PacketFlags::CONNECT_RESPONSE,
    PacketFlags::NET_ERROR,
    PacketFlags::NET_ERROR_DISCONNECT,
    PacketFlags::CICMD_COMMAND,
    PacketFlags::TIME_SYNC,
    PacketFlags::ECHO_REQUEST,
    PacketFlags::ECHO_RESPONSE,
    PacketFlags::FLOW,
];

/// Every section the shared writer can build verifies on the server: the server's payload hash
/// is the one the writer put into the checksum, section by section, plaintext or encrypted.
#[test]
fn every_section_kind_the_shared_writer_builds_verifies_on_the_server() {
    assert_eq!(OPTIONAL_HEADERS.len(), 19);
    for spec in &OPTIONAL_HEADERS {
        for fragment in [false, true] {
            let mut stream = CryptoSystem::new(0x5EC7_1015);
            let bytes = packet(11, 1, spec.mask, fragment, || stream.next());
            let p = ClientPacket::unpack(&bytes).unwrap_or_else(|| panic!("{} unpacks", spec.name));
            let mut window = SequenceWindow::new(0x5EC7_1015);
            let mut stats = NetworkStatistics::new();
            assert!(
                p.verify_crc(&mut window, &mut stats),
                "{} (fragment: {fragment}) verifies",
                spec.name
            );
            assert_eq!(stats.c2s_crc_errors_aggregate(), 0, "{}", spec.name);
        }
    }
}

/// A client packet carrying client id 300 (a session slot past 256, as retail's servers assigned
/// up to 397) is read and verified by the server (V283); the client's own parser still rejects a
/// header id of 256 or more on what it receives.
#[test]
fn a_client_id_of_300_is_accepted_by_the_server() {
    let mut stream = CryptoSystem::new(0x5EC7_1015);
    let bytes = packet(300, 1, PacketFlags::ACK_SEQUENCE, true, || stream.next());
    let p = ClientPacket::unpack(&bytes).expect("the server's parser accepts id 300");
    assert_eq!(p.header.rec_id, 300);
    let mut window = SequenceWindow::new(0x5EC7_1015);
    let mut stats = NetworkStatistics::new();
    assert!(p.verify_crc(&mut window, &mut stats), "and it verifies");
    assert!(matches!(
        dereth_transport::wire::ParsedPacket::parse(&bytes),
        Err(dereth_transport::wire::WireError::RecIdOutOfRange(300))
    ));
}

fn crc_errors(net: &empyrean_net::ServerNet) -> i64 {
    net.statistics().c2s_crc_errors_aggregate
}

fn message_count(net: &mut empyrean_net::ServerNet) -> usize {
    net.events()
        .filter(|e| matches!(e, Event::Message { .. }))
        .count()
}

fn termination(
    net: &empyrean_net::ServerNet,
    session: SessionId,
) -> Option<SessionTerminationReason> {
    net.session(session)
        .and_then(|s| s.core.pending_termination.as_ref().map(|p| p.reason))
}

/// The defect: an `EchoResponse` beside an ACK and a message fragment. ACE neither consumed nor
/// hashed the `EchoResponse`, so the checksum failed and the fragment was lost with it.
#[test]
fn an_echo_response_beside_an_ack_and_a_fragment_is_taken() {
    let (mut net, session, from, mut stream, now) = by_hand();
    let bytes = packet(
        session.client_id,
        2,
        PacketFlags::ECHO_RESPONSE,
        true,
        || stream.next(),
    );
    net.on_datagram(PortKind::C2S, from, &bytes, now);
    assert_eq!(crc_errors(&net), 0, "the checksum verifies");
    assert_eq!(
        message_count(&mut net),
        1,
        "the fragment's message is delivered"
    );
    assert_eq!(
        net.session(session)
            .expect("live")
            .network
            .last_received_packet_sequence(),
        2
    );
}

/// A client's `NetErrorDisconnect` verifies, so ACE's handling of it runs: the session ends with
/// `ClientSentNetworkErrorDisconnect` rather than waiting out its timeout.
#[test]
fn a_net_error_disconnect_ends_the_session() {
    let (mut net, session, from, mut stream, now) = by_hand();
    let bytes = packet(
        session.client_id,
        2,
        PacketFlags::NET_ERROR_DISCONNECT,
        false,
        || stream.next(),
    );
    net.on_datagram(PortKind::C2S, from, &bytes, now);
    assert_eq!(crc_errors(&net), 0);
    assert_eq!(
        termination(&net, session),
        Some(SessionTerminationReason::ClientSentNetworkErrorDisconnect)
    );
}

/// Every section the retail client sends, on an established session: none fails the checksum.
/// The sections the server acts on are acted on as ACE does (the fragment beside a shareable
/// section is delivered; `Disconnect` and `NetErrorDisconnect` end the session); the others are
/// read, hashed and ignored.
#[test]
fn every_client_sent_section_verifies_on_a_session() {
    for mask in CLIENT_SENT {
        let (mut net, session, from, mut stream, now) = by_hand();
        let fragment = !exclusive(mask);
        let bytes = packet(session.client_id, 2, mask, fragment, || stream.next());
        net.on_datagram(PortKind::C2S, from, &bytes, now);
        assert_eq!(crc_errors(&net), 0, "{mask:#x}: the checksum verifies");
        let delivered = message_count(&mut net);
        assert_eq!(delivered, usize::from(fragment), "{mask:#x}");
        let ended = termination(&net, session);
        match mask {
            PacketFlags::DISCONNECT => {
                assert_eq!(
                    ended,
                    Some(SessionTerminationReason::PacketHeaderDisconnect)
                )
            }
            PacketFlags::NET_ERROR_DISCONNECT => {
                assert_eq!(
                    ended,
                    Some(SessionTerminationReason::ClientSentNetworkErrorDisconnect)
                )
            }
            _ => assert_eq!(ended, None, "{mask:#x}"),
        }
    }
}
