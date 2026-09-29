//! ACE: Source/ACE.Server/Network/NetworkSession.cs::NetworkSession
//! Corrupted encrypted/plain packets are dropped and counted; CryptoSystem search follows ACE and
//! gives up past max effort; unknown-section packets verify; server packets equal the shared core
//! byte for byte; referral/server-switch bodies are ACE's.
//! Fixture: locally constructed packets and session state on a virtual clock.

use std::time::Duration;

use dereth_primitives::NetQueue;
use dereth_transport::wire::ParsedPacket;
use empyrean_common::cryptography::crypto_system::CryptoSystem;
use empyrean_net::{ClockSnapshotExt, Event, PortKind};

use crate::common::{client_addr, connect, harness, login_request_datagram, server, TICK};
use crate::fragments::message;

/// A client packet corrupted in flight fails its encrypted checksum: it is dropped and counted,
/// and nothing reaches the world. The client resends it when the server asks for the gap, its key
/// was parked again when the damaged copy failed (V267), and the message then arrives exactly
/// once.
#[test]
fn a_corrupted_encrypted_packet_is_dropped_counted_and_recovered() {
    let mut h = harness();
    let c = h.add_client(client_addr(20, 50_000), "crc", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    h.events.clear();
    let errors_before = h.net.server.statistics().c2s_crc_errors_aggregate;

    let first = message(0xF7B1, 100);
    h.clients[c].send(NetQueue::Weenie, &first);
    let now = h.now_seconds();
    h.clients[c].tick(now);
    let addr = h.clients[c].addr;
    let mut corrupted = 0;
    for (to, mut bytes) in h.clients[c].take_outgoing() {
        let p = ParsedPacket::parse(&bytes).expect("client packet");
        if !p.fragments.is_empty() {
            let last = bytes.len() - 1;
            bytes[last] ^= 0x5A;
            corrupted += 1;
        }
        h.net.client_send(addr, to, bytes);
    }
    assert_eq!(corrupted, 1);
    h.net.pump();
    assert_eq!(
        h.net.server.statistics().c2s_crc_errors_aggregate,
        errors_before + 1,
        "dropped and counted"
    );
    assert!(
        h.net.server.events().next().is_none(),
        "nothing reached the world"
    );

    // Two more messages: the gap below them makes the server ask for the lost sequence.
    let second = message(0xF7B1, 20);
    let third = message(0xF7B1, 30);
    h.clients[c].send(NetQueue::Weenie, &second);
    h.run_until(Duration::from_millis(200), TICK, |_| false);
    h.clients[c].send(NetQueue::Weenie, &third);
    let done = h.run_until(Duration::from_secs(10), TICK, |h| {
        h.events
            .iter()
            .filter(|e| matches!(e, Event::Message { .. }))
            .count()
            >= 3
    });
    assert!(done, "the corrupted message was recovered");
    let got: Vec<&Vec<u8>> = h
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Message { message, .. } => Some(&message.data),
            _ => None,
        })
        .collect();
    assert_eq!(
        got,
        vec![&first, &second, &third],
        "each exactly once, in order"
    );
    assert_eq!(
        h.net.server.statistics().c2s_crc_errors_aggregate,
        errors_before + 1
    );
}

/// A plaintext packet with a bad checksum (here a LoginRequest) is dropped and counted too.
#[test]
fn a_corrupted_plaintext_packet_is_dropped_and_counted() {
    let mut net = server(Default::default());
    let mut bytes = login_request_datagram("crc2", "pw", "1802");
    bytes[8] ^= 1; // the checksum field
    net.on_datagram(
        PortKind::C2S,
        client_addr(21, 40_000),
        &bytes,
        empyrean_net::ClockSnapshot::at_seconds(0.0),
    );
    assert_eq!(net.statistics().c2s_crc_errors_aggregate, 1);
    assert_eq!(
        net.statistics().c2s_packets_aggregate,
        1,
        "it reached ProcessPacket"
    );
    assert!(net.events().next().is_none(), "no LoginRequest event");
}

/// ACE's `CryptoSystem` (kept as the port of ACE's type; since V267 the server checks keys with
/// the shared retail window, see `keys`): the first key is drawn at construction; `Search` looks at the current
/// key, then the parked set, then up to 256 keys ahead, parking what it passes; `ConsumeKey`
/// advances past the current key or unparks.
#[test]
fn crypto_system_search_follows_ace() {
    let seed = 0x1234_5678;
    // The shared client stream for the same seed: the keys the client encrypts with.
    let mut client = dereth_transport::CryptoSystem::new(seed);
    let keys: Vec<u32> = (0..300).map(|_| client.next()).collect();

    let mut cs = CryptoSystem::new(seed);
    assert_eq!(
        cs.current_key, keys[0],
        "the same stream as the client's outgoing keys"
    );

    // In order.
    assert!(cs.search(keys[0]));
    cs.consume_key(keys[0]);
    assert_eq!(cs.current_key, keys[1]);

    // Packet 2 lost: key 2 found 1 ahead, key 1 parked.
    assert!(cs.search(keys[2]));
    cs.consume_key(keys[2]);
    let xors = |cs: &CryptoSystem| cs.xors.clone().expect("not released");
    assert!(xors(&cs).contains(&keys[1]) && xors(&cs).len() == 1);
    assert_eq!(cs.current_key, keys[3]);

    // The retransmission of packet 1 comes out of the parked set.
    assert!(cs.search(keys[1]));
    cs.consume_key(keys[1]);
    assert!(xors(&cs).is_empty());

    // A duplicate of packet 1: its key is gone; the search walks 256 keys ahead, parks them all,
    // and fails.
    assert!(!cs.search(keys[1]));
    assert_eq!(xors(&cs).len(), 256);
    // The next packets' keys are then found among the parked ones.
    assert!(cs.search(keys[3]));
    cs.consume_key(keys[3]);
    assert_eq!(xors(&cs).len(), 255);
    assert!(cs.search(keys[4]));
}

/// With nothing parked, a key more than 256 draws ahead is not found.
#[test]
fn crypto_system_gives_up_past_the_maximum_effort_level() {
    let seed = 99;
    let mut client = dereth_transport::CryptoSystem::new(seed);
    let keys: Vec<u32> = (0..300).map(|_| client.next()).collect();
    assert!(
        CryptoSystem::new(seed).search(keys[256]),
        "256 ahead is the last one looked at"
    );
    assert!(!CryptoSystem::new(seed).search(keys[257]));
}

/// Not ACE's (V270/V283, a fix): every optional section is hashed, so a client packet carrying a
/// non-empty section ACE did not read (here `NetErrorDisconnect`, 8 bytes) verifies under the
/// right key, and the session is terminated as the section asks. ACE hashed only the sections it
/// reads, so this packet failed its checksum, was dropped and counted, and the session lived on.
#[test]
fn a_packet_with_a_section_ace_does_not_read_verifies() {
    use dereth_transport::wire::{OutPacket, PacketFlags, ProtoHeader};
    use empyrean_net::{AccountSelect, ClockSnapshot, SessionTerminationReason};

    let mut net = server(Default::default());
    let from = client_addr(22, 40_000);
    let now = ClockSnapshot::at_seconds(1.0);
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("neterr", "pw", "1802"),
        now,
    );
    let Some(Event::LoginRequest { session, .. }) = net.events().next() else {
        panic!()
    };
    assert_eq!(
        net.account_select_callback(session, now),
        AccountSelect::Continue
    );
    net.accept_login(session, 1, "neterr".into(), 1);
    let out: Vec<_> = net.poll(now).collect();
    let (_, cr, _) = crate::common::find_connect_request(&out).expect("ConnectRequest");
    net.on_datagram(
        PortKind::S2C,
        from,
        &crate::common::connect_response_datagram(cr.cookie, session.client_id),
        now,
    );
    let _ = net.poll(now).count();

    // The client's first encrypted packet, keyed exactly as the client keys it.
    let mut keys = dereth_transport::CryptoSystem::new(cr.seeds().client_to_server);
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 2,
        rec_id: session.client_id,
        iteration: 1,
        ..ProtoHeader::default()
    });
    p.add_optional_header(PacketFlags::NET_ERROR_DISCONNECT, vec![0; 8])
        .expect("section");
    let bytes = p.serialize(Some(keys.next())).expect("encrypted");
    let errors = net.statistics().c2s_crc_errors_aggregate;
    net.on_datagram(PortKind::C2S, from, &bytes, now);
    assert_eq!(net.statistics().c2s_crc_errors_aggregate, errors);
    assert_eq!(
        net.session(session)
            .and_then(|s| s.core.pending_termination.as_ref().map(|p| p.reason)),
        Some(SessionTerminationReason::ClientSentNetworkErrorDisconnect)
    );
}

/// ACE's `ServerPacket` hashes its optional-header bytes as one block; the shared `OutPacket` sums
/// one hash per section. The two agree whenever every section but the last is a whole number of
/// dwords, which holds for every section the server writes (AckSequence 4, TimeSync 8,
/// EchoResponse 8, RejectRetransmit 4 + 4n, ConnectRequest 32, Referral 32, ServerSwitch 8). This
/// pins that the ACE-shaped writer and the shared core produce the same datagram, byte for byte,
/// plaintext and encrypted, with and without fragments.
#[test]
fn server_packets_match_the_shared_core_byte_for_byte() {
    use dereth_transport::wire::{Fragment, FragmentHeader, OutPacket, PacketFlags, ProtoHeader};
    use empyrean_net::server_packet::{ServerPacket, ServerPacketFragment};

    let sections: [(u32, Vec<u8>); 3] = [
        (
            PacketFlags::ACK_SEQUENCE,
            0x0102_0304u32.to_le_bytes().to_vec(),
        ),
        (PacketFlags::TIME_SYNC, 12_345.678f64.to_le_bytes().to_vec()),
        (
            PacketFlags::ECHO_RESPONSE,
            [1.5f32.to_le_bytes(), 0.25f32.to_le_bytes()].concat(),
        ),
    ];
    let frag_header = FragmentHeader {
        blob_id_low: 7,
        blob_id_high: 0x8000_0000,
        num_frags: 1,
        blob_num: 0,
        queue_id: 9,
        ..FragmentHeader::default()
    };
    let payloads: [Vec<u8>; 2] = [(0..37u8).collect(), (0..3u8).collect()];
    let mut keys = dereth_transport::CryptoSystem::new(0x1234_5678);
    let mut compared = 0;
    for encrypted in [false, true] {
        for n_sections in 0..=3 {
            for n_frags in 0..=2 {
                if n_sections == 0 && n_frags == 0 {
                    continue;
                }
                let key = encrypted.then(|| keys.next());
                let header = ProtoHeader {
                    seq_id: 5,
                    rec_id: 11,
                    interval: 0x0100,
                    iteration: 1,
                    ..ProtoHeader::default()
                };

                let mut ace = ServerPacket::new();
                ace.header = header;
                for (mask, body) in &sections[..n_sections] {
                    ace.add_flags(*mask);
                    ace.initialize_data_writer().extend_from_slice(body);
                }
                for p in &payloads[..n_frags] {
                    ace.add_flags(PacketFlags::BLOB_FRAGMENTS);
                    let mut f = ServerPacketFragment::new(p.clone());
                    f.header = frag_header;
                    ace.fragments.push(f);
                }
                if let Some(k) = key {
                    ace.add_flags(PacketFlags::ENCRYPTED_CHECKSUM);
                    assert!(ace.set_issac_xor(k));
                }

                let mut core = OutPacket::new(header);
                for (mask, body) in &sections[..n_sections] {
                    core.add_optional_header(*mask, body.clone())
                        .expect("section");
                }
                for p in &payloads[..n_frags] {
                    core.add_fragment(Fragment::new(frag_header, p.clone()))
                        .expect("fragment");
                }
                if core.needs_encryption() != encrypted {
                    // The shared core applies the client's encrypted-iff rule; compare only where
                    // it agrees with the flags given here.
                    continue;
                }
                assert_eq!(
                    ace.create_ready_to_send_packet(),
                    core.serialize(key).expect("serialise"),
                    "encrypted {encrypted}, {n_sections} sections, {n_frags} fragments"
                );
                compared += 1;
            }
        }
    }
    assert!(compared >= 8, "only {compared} combinations compared");
}

/// `PacketOutboundReferral` and `PacketOutboundServerSwitch` write the shared layouts exactly.
#[test]
fn referral_and_server_switch_bodies_are_aces() {
    use empyrean_net::packets::packet_outbound_referral::packet_outbound_referral;
    use empyrean_net::packets::packet_outbound_server_switch::packet_outbound_server_switch;

    let mut r = packet_outbound_referral(
        0x1122_3344_5566_7788,
        &["8", "8", "8", "8"],
        [1, 2, 3, 4],
        0x2328,
        false,
        [9; 4],
    );
    let expected: [u8; 32] = [
        0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, // key
        0x02, 0x00, // AF_INET
        0x23, 0x28, // port, big-endian
        1, 2, 3, 4, // host
        0, 0, 0, 0, 0, 0, 0, 0, // sin_zero
        0x18, 0x00, // server id
        0, 0, 0, 0, 0, 0, // padding
    ];
    assert_eq!(r.initialize_data_writer().as_slice(), expected.as_slice());
    let mut s = packet_outbound_server_switch();
    assert_eq!(
        s.initialize_data_writer().as_slice(),
        [0x18, 0, 0, 0, 0, 0, 0, 0].as_slice()
    );
}
