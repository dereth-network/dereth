//! ACE: Source/ACE.Server/Network/NetworkStatistics.cs::NetworkStatistics
//! NetworkStatistics getters/summary, packet log ToStrings, bundle/buffer properties as ACE.
//! Fixture: locally constructed packets and session state on a virtual clock.

use dereth_transport::wire::{OutPacket, PacketFlags, ProtoHeader};
use empyrean_net::client_packet::{packet_header_to_string, ClientPacket, ClientPacketFragment};
use empyrean_net::message_buffer::MessageBuffer;
use empyrean_net::network_bundle::NetworkBundle;
use empyrean_net::server_packet::ServerPacket;
use empyrean_net::{GameMessageGroup, NetworkStatistics, OutboundMessage};

/// A plaintext client packet with the given sections.
fn client_packet(seq: u32, sections: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: seq,
        rec_id: 11,
        iteration: 1,
        ..ProtoHeader::default()
    });
    for (mask, body) in sections {
        p.add_optional_header(*mask, body.clone()).expect("section");
    }
    p.serialize(None).expect("plaintext")
}

/// The counters read back through ACE's getters, and the summary's proportions in
/// `FormatChance`'s form (blank when zero).
#[test]
fn statistics_getters_and_summary() {
    let mut stats = NetworkStatistics::new();
    for _ in 0..3 {
        stats.c2s_packets_aggregate_increment();
    }
    assert_eq!(stats.c2s_requests_for_retransmit_aggregate_increment(), 1);
    assert_eq!(stats.s2c_packets_aggregate_increment(), 1);
    assert_eq!(stats.c2s_crc_errors_aggregate_increment(), 1);
    assert_eq!(stats.c2s_crc_errors_aggregate_increment(), 2);
    assert_eq!(
        (
            stats.c2s_packets_aggregate(),
            stats.s2c_packets_aggregate(),
            stats.c2s_requests_for_retransmit_aggregate(),
            stats.s2c_requests_for_retransmit_aggregate(),
            stats.c2s_crc_errors_aggregate()
        ),
        (3, 1, 1, 0, 2)
    );
    assert_eq!(
        stats.summary(),
        "\nnetwork statistics\npackets\nclient=>server: 3\nserver=>client: 1\nrequests for retransmit\nclient=>server: 1 33.3%\nServer=>client: 0 \nCRC errors\nclient=>server: 2 66.6%\n"
    );
}

/// The packet log's lines: `<<<` with the header (flag names in declaration order) and the
/// optional sections ACE reads (flow, then the retransmit request, then the ACK); `>>>` with the
/// final checksum and the ISAAC key when encrypted.
#[test]
fn packet_log_lines_read_as_in_ace() {
    let bytes = client_packet(
        5,
        &[(PacketFlags::ACK_SEQUENCE, 9u32.to_le_bytes().to_vec())],
    );
    let p = ClientPacket::unpack(&bytes).expect("unpacks");
    assert_eq!(p.header_optional.size, 4);
    assert_eq!(
        p.to_string(),
        format!(
            "<<< Seq: 5 Id: 11 Iter: 1 CRC: {} AckSequence AckSeq: 9",
            p.header.checksum
        )
    );

    // Flow first, then the retransmit request (an empty list prints ACE's `DefaultIfEmpty` 0),
    // then the ACK.
    let mut ho = p.header_optional.clone();
    ho.flags |= PacketFlags::FLOW;
    ho.flow_bytes = 1234;
    ho.flow_interval = 7;
    ho.retransmit_data = Some(vec![3, 4]);
    assert_eq!(ho.to_string(), "1234 Interval: 7 requesting 3,4 AckSeq: 9");
    ho.retransmit_data = Some(Vec::new());
    ho.flags = PacketFlags::REQUEST_RETRANSMIT;
    assert_eq!(ho.to_string(), "requesting 0");

    // No section ACE reads: the optional part is empty and the line is trimmed.
    let p = ClientPacket::unpack(&client_packet(6, &[])).expect("unpacks");
    assert_eq!(p.header_optional.to_string(), "");
    assert_eq!(
        p.to_string(),
        format!("<<< Seq: 6 Id: 11 Iter: 1 CRC: {}", p.header.checksum)
    );

    let mut s = ServerPacket::new();
    s.header.seq_id = 2;
    s.header.rec_id = 3;
    s.header.iteration = 4;
    assert_eq!(s.to_string(), ">>> Seq: 2 Id: 3 Iter: 4 CRC: 0");
    s.add_flags(PacketFlags::ENCRYPTED_CHECKSUM);
    assert!(s.set_issac_xor(0x55));
    assert_eq!(s.issac_xor(), 0x55);
    let _ = s.create_ready_to_send_packet();
    // No payload: the final checksum is the header's hash, and the sent one adds `0 ^ xor`.
    let final_checksum = s.header.checksum.wrapping_sub(0x55);
    assert_eq!(
        s.to_string(),
        format!(
            ">>> Seq: 2 Id: 3 Iter: 4 XCRC: {} EncryptedChecksum CRC: {final_checksum} XOR: 85",
            s.header.checksum
        )
    );
    assert_eq!(
        packet_header_to_string(&s.header),
        format!(
            "Seq: 2 Id: 3 Iter: 4 XCRC: {} EncryptedChecksum",
            s.header.checksum
        )
    );
}

/// `NetworkBundle`: any header request makes the bundle need sending, as a message does;
/// `MessageBuffer`: `Count` and `Complete` over its fragments.
#[test]
fn bundle_and_buffer_properties() {
    let mut b = NetworkBundle::default();
    assert!(!b.needs_sending() && !b.has_more_messages());
    assert!((b.client_time() - -1.0).abs() < f32::EPSILON);
    b.set_send_ack(false);
    assert!(
        b.needs_sending() && !b.has_more_messages(),
        "setting a property, even to false, marks it changed"
    );
    assert!(!b.send_ack() && !b.time_sync());
    let mut b = NetworkBundle::default();
    b.set_time_sync(true);
    b.set_client_time(2.5);
    assert!(b.time_sync() && (b.client_time() - 2.5).abs() < f32::EPSILON);
    let mut b = NetworkBundle::default();
    b.enqueue(OutboundMessage {
        group: GameMessageGroup::UIQueue,
        data: vec![1, 2, 3],
    });
    assert!(b.needs_sending() && b.has_more_messages());
    assert_eq!(b.current_size, 3);

    let frag = |index| ClientPacketFragment {
        sequence: 1,
        id: 1,
        count: 2,
        size: 20,
        index,
        queue: 5,
        data: vec![u8::try_from(index).expect("small"); 4],
    };
    let mut m = MessageBuffer::new(1, 2);
    m.add_fragment(frag(1));
    m.add_fragment(frag(1));
    assert_eq!((m.count(), m.complete()), (1, false));
    m.add_fragment(frag(0));
    assert_eq!((m.count(), m.complete()), (2, true));
    m.add_fragment(frag(2));
    assert_eq!(m.count(), 2, "a complete buffer takes nothing more");
}
