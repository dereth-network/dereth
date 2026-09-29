//! ACE: Source/ACE.Server/Network/NetworkSession.cs::NetworkSession
//! Ack/timesync cadences, NAK answered from the resend cache, cache pruned by acks/age and wraps,
//! routing and reply port switch, send error terminates, sequence counting, portal-year ticks,
//! fragment counts, writers, flag descriptions, small handlers, connect seeds, handshake-out-of-
//! turn refused.
//! Fixture: locally constructed packets and session state on a virtual clock.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use dereth_transport::conn::{ConnectRequest, ConnectSeeds};
use dereth_transport::wire::{OutPacket, PacketFlags, ProtoHeader};
use empyrean_net::client_message::ClientMessage;
use empyrean_net::extensions::{
    build_packet_string, calculate_pad_multiple, write_packed_dword,
    write_packed_dword_of_known_type, write_string16l, write_u16_be,
};
use empyrean_net::managers::socket_manager::{get_matched_connection_listener, parse_hosts};
use empyrean_net::message_fragment::MessageFragment;
use empyrean_net::network_session::portal_year_ticks_u16;
use empyrean_net::packet_header_flags_util::unfold_flags;
use empyrean_net::packets::packet_outbound_referral::packet_outbound_referral;
use empyrean_net::session_connection_data::PacketSequence;
use empyrean_net::{
    ClockSnapshot, ClockSnapshotExt, Event, GameMessageGroup, NetworkStatistics, OutboundMessage,
    PortKind, SessionState, SessionTerminationReason,
};

use crate::common::{
    client_addr, connect_response_datagram, login_request_datagram, parse_all, raw_connected,
    server,
};
use crate::fragments::message;

/// A plaintext client packet with the given sections.
fn client_packet(rec_id: u16, seq: u32, sections: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: seq,
        rec_id,
        iteration: 1,
        ..ProtoHeader::default()
    });
    for (mask, body) in sections {
        p.add_optional_header(*mask, body.clone()).expect("section");
    }
    p.serialize(None).expect("plaintext")
}

fn seq_list(ids: &[u32]) -> Vec<u8> {
    let mut b = u32::try_from(ids.len())
        .expect("small")
        .to_le_bytes()
        .to_vec();
    for id in ids {
        b.extend_from_slice(&id.to_le_bytes());
    }
    b
}

/// The ACK is first due 2 s after the session was created (`nextAck`), strictly after; it is an
/// unencrypted `AckSequence`-only packet that repeats the current sequence and reports the last
/// client sequence processed. TimeSync repeats 20 s after the first.
#[test]
fn ack_and_timesync_cadences_are_aces() {
    let (mut net, id, from, _) = raw_connected();
    // The client's own ACK, which keeps the session inside its timeout.
    let keep_alive = client_packet(
        id.client_id,
        1,
        &[(PacketFlags::ACK_SEQUENCE, 2u32.to_le_bytes().to_vec())],
    );
    // The session was created at 100.0 s; the TimeSync went at 100.05 s with sequence 2.
    let at = |s: f64| ClockSnapshot::at_seconds(s);
    assert_eq!(
        net.poll(at(102.0)).count(),
        0,
        "`UtcNow > nextAck` is strict"
    );
    let out: Vec<_> = net.poll(at(102.001)).collect();
    let p = &parse_all(&out)[0];
    assert_eq!(p.header.header.0, PacketFlags::ACK_SEQUENCE);
    assert_eq!(p.header.seq_id, 2, "an ACK does not take a new sequence");
    assert_eq!(
        p.optional[&PacketFlags::ACK_SEQUENCE],
        1u32.to_le_bytes().to_vec()
    );
    assert_eq!(
        net.poll(at(104.001)).count(),
        0,
        "the next ACK is 2 s after this one"
    );
    assert_eq!(net.poll(at(104.002)).count(), 1);

    let mut t: f64 = 104.002;
    let mut timesync_at = None;
    while t < 125.0 {
        t += 0.001;
        if (t * 1000.0).round() % 1000.0 == 0.0 {
            net.on_datagram(PortKind::C2S, from, &keep_alive, at(t));
        }
        let out: Vec<_> = net.poll(at(t)).collect();
        if parse_all(&out)
            .iter()
            .any(|p| p.header.header.contains(PacketFlags::TIME_SYNC))
        {
            timesync_at = Some(t);
            break;
        }
    }
    let timesync_at = timesync_at.expect("a second TimeSync");
    assert!(
        (timesync_at - 120.05).abs() < 0.0015,
        "20 s after the first: {timesync_at}"
    );
    let _ = id;
}

/// A cleartext NAK from the client: cached packets are resent with the `Retransmission` flag,
/// the rest are refused with a `RejectRetransmit`.
#[test]
fn a_client_nak_is_answered_from_the_cache() {
    let (mut net, id, from, now) = raw_connected();
    net.send(
        id,
        OutboundMessage {
            group: GameMessageGroup::UIQueue,
            data: message(0xF7B0, 40),
        },
    );
    let first: Vec<_> = net.poll(now).collect();
    let original = parse_all(&first).remove(0);
    assert_eq!(original.header.seq_id, 3);
    assert_eq!(
        net.session(id).expect("live").network.cached_sequences(),
        vec![2, 3]
    );

    let nak = client_packet(
        id.client_id,
        0,
        &[(PacketFlags::REQUEST_RETRANSMIT, seq_list(&[3, 999]))],
    );
    net.on_datagram(PortKind::C2S, from, &nak, now);
    let out: Vec<_> = net.poll(now).collect();
    let packets = parse_all(&out);
    let resent = packets
        .iter()
        .find(|p| p.header.seq_id == 3)
        .expect("resent");
    assert!(resent.header.header.contains(PacketFlags::RETRANSMISSION));
    assert_eq!(resent.fragments, original.fragments);
    assert_eq!(
        resent.recovered_key(),
        original.recovered_key(),
        "the same ISAAC key"
    );
    let reject = packets
        .iter()
        .find(|p| p.header.header.contains(PacketFlags::REJECT_RETRANSMIT))
        .expect("reject");
    assert_eq!(
        reject.optional[&PacketFlags::REJECT_RETRANSMIT],
        seq_list(&[999])
    );
    assert_eq!(net.statistics().c2s_requests_for_retransmit_aggregate, 1);
}

/// `AckSequence(n)` from the client drops cached packets below `n`; the cache also forgets
/// packets older than 120 s of portal-year time on its 5-second prune.
#[test]
fn the_resend_cache_is_pruned_by_acks_and_by_age() {
    let (mut net, id, from, mut now) = raw_connected();
    for _ in 0..3 {
        net.send(
            id,
            OutboundMessage {
                group: GameMessageGroup::UIQueue,
                data: message(0xF7B0, 40),
            },
        );
        let _ = net.poll(now).count();
        now = now.advanced(Duration::from_millis(5));
    }
    assert_eq!(
        net.session(id).expect("live").network.cached_sequences(),
        vec![2, 3, 4, 5]
    );
    let ack = client_packet(
        id.client_id,
        1,
        &[(PacketFlags::ACK_SEQUENCE, 4u32.to_le_bytes().to_vec())],
    );
    net.on_datagram(PortKind::C2S, from, &ack, now);
    assert_eq!(
        net.session(id).expect("live").network.cached_sequences(),
        vec![4, 5]
    );

    let keep_alive = client_packet(
        id.client_id,
        1,
        &[(PacketFlags::ACK_SEQUENCE, 2u32.to_le_bytes().to_vec())],
    );
    for _ in 0..130 {
        now = now.advanced(Duration::from_secs(1));
        net.on_datagram(PortKind::C2S, from, &keep_alive, now);
        let _ = net.poll(now).count();
    }
    let left = net.session(id).expect("live").network.cached_sequences();
    assert!(
        !left.contains(&4) && !left.contains(&5),
        "aged out: {left:?}"
    );
}

/// V321: the cache's age across the 16-bit wrap of the portal-year time
/// is a full turn: a packet stamped 65,475 is 121 old at 60 and is pruned (ACE's 65535 made it 120
/// and kept it); 120 old is kept either side of the wrap.
#[test]
fn the_resend_cache_age_wraps_by_a_full_turn() {
    use empyrean_net::network_session::cached_packet_is_kept;
    assert!(
        !cached_packet_is_kept(60, 65_475),
        "121 old across the wrap"
    );
    assert!(cached_packet_is_kept(59, 65_475), "120 old across the wrap");
    assert!(cached_packet_is_kept(1_120, 1_000));
    assert!(!cached_packet_is_kept(1_121, 1_000));
    assert!(cached_packet_is_kept(0, 65_535), "1 old across the wrap");
}

/// Routing: session traffic must come from the session's exact endpoint; `P + 1` takes only
/// `ConnectResponse` (and ignores `CICMDCommand`); datagrams over 1024 bytes are dropped.
#[test]
fn packets_are_routed_as_ace_routes_them() {
    let (mut net, id, from, now) = raw_connected();
    let before = net.statistics().c2s_packets_aggregate;
    let ack = client_packet(
        id.client_id,
        1,
        &[(PacketFlags::ACK_SEQUENCE, 2u32.to_le_bytes().to_vec())],
    );

    let other_port = SocketAddr::new(from.ip(), from.port() + 5);
    net.on_datagram(PortKind::C2S, other_port, &ack, now);
    assert_eq!(
        net.statistics().c2s_packets_aggregate,
        before,
        "wrong source port: not processed"
    );

    net.on_datagram(PortKind::C2S, from, &ack, now);
    assert_eq!(net.statistics().c2s_packets_aggregate, before + 1);

    let icmd = client_packet(
        0,
        0,
        &[(PacketFlags::CICMD_COMMAND, vec![1, 0, 0, 0, 0, 0, 0, 0])],
    );
    net.on_datagram(PortKind::S2C, from, &icmd, now);
    net.on_datagram(
        PortKind::S2C,
        client_addr(99, 1),
        &login_request_datagram("x", "pw", "1802"),
        now,
    );
    assert_eq!(net.get_session_count(), 1, "no session from P + 1");

    let mut big = login_request_datagram("y", "pw", "1802");
    big.resize(1025, 0);
    net.on_datagram(PortKind::C2S, client_addr(98, 1), &big, now);
    assert_eq!(net.get_session_count(), 1, "oversized datagram dropped");
    assert!(net.events().next().is_none());
}

/// Before the `ConnectResponse` the server answers from `P` to the login address; after it,
/// from `P + 1` to the address the `ConnectResponse` came from.
#[test]
fn the_reply_port_switches_at_the_connect_response() {
    let mut net = server(Default::default());
    let from = client_addr(60, 40_000);
    let now = ClockSnapshot::at_seconds(1.0);
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("sw", "pw", "1802"),
        now,
    );
    let Some(Event::LoginRequest { session, .. }) = net.events().next() else {
        panic!()
    };
    let _ = net.account_select_callback(session, now);
    net.accept_login(session, 1, "sw".into(), 1);
    let before: Vec<_> = net.poll(now).collect();
    assert!(before
        .iter()
        .all(|o| o.via_port_kind == PortKind::C2S && o.to == from));
    let cookie = net
        .session(session)
        .expect("live")
        .network
        .connection_data
        .connection_cookie;
    let later = ClockSnapshot::at_seconds(1.1);
    let s2c_from = SocketAddr::new(from.ip(), 40_007);
    net.on_datagram(
        PortKind::S2C,
        s2c_from,
        &connect_response_datagram(cookie, session.client_id),
        later,
    );
    let after: Vec<_> = net.poll(later).collect();
    assert!(!after.is_empty());
    assert!(after
        .iter()
        .all(|o| o.via_port_kind == PortKind::S2C && o.to == s2c_from));
}

/// A failed send terminates the session (`SendToSocketException`), keeping the error text.
#[test]
fn a_send_error_terminates_the_session() {
    let (mut net, id, _, now) = raw_connected();
    net.on_send_error(id, "boom", now);
    let s = net.session(id).expect("terminating");
    let p = s.core.pending_termination.as_ref().expect("pending");
    assert_eq!(
        (p.reason, p.extra_reason.as_str()),
        (SessionTerminationReason::SendToSocketException, "boom")
    );
    let _ = net.poll(now).count();
    assert_eq!(
        net.session(id).expect("live").core.state,
        SessionState::TerminationStarted
    );
}

#[test]
fn packet_sequence_starts_at_zero_then_counts() {
    let mut s = PacketSequence::not_client_primed();
    assert_eq!(s.current_value, u32::MAX);
    assert_eq!(s.next_value(), 0);
    assert_eq!(s.next_value(), 1);
    let mut s = PacketSequence::starting_at(1);
    assert_eq!(s.next_value(), 2);
}

#[test]
fn portal_year_ticks_wrap_every_65536_seconds() {
    assert_eq!(portal_year_ticks_u16(1234.9), 1234);
    assert_eq!(portal_year_ticks_u16(65_536.5), 0);
    assert_eq!(portal_year_ticks_u16(70_000.9), 4464);
}

/// `MessageFragment`: the count is the ceiling of `len / 448`; the tail size of a whole multiple of
/// 448 is a full fragment (V271; ACE answered the bare 16-byte header).
#[test]
fn message_fragment_counts_and_tail_sizes() {
    let m = |len| {
        MessageFragment::new(
            OutboundMessage {
                group: GameMessageGroup::UIQueue,
                data: vec![0; len],
            },
            0,
        )
    };
    assert_eq!(
        (m(4).count, m(448).count, m(449).count, m(1000).count),
        (1, 1, 2, 3)
    );
    assert!(m(448).tail_sent && !m(449).tail_sent);
    assert_eq!(m(1000).tail_size(), 16 + 104);
    assert_eq!(
        m(896).tail_size(),
        16 + 448,
        "A whole-multiple length has a full tail"
    );
    assert_eq!(m(1000).next_size(), 464);
}

#[test]
fn extensions_match_aces_writers() {
    assert_eq!(calculate_pad_multiple(5, 4), 3);
    assert_eq!(calculate_pad_multiple(8, 4), 0);
    let mut w = Vec::new();
    write_string16l(&mut w, Some("abc"));
    assert_eq!(w, vec![3, 0, b'a', b'b', b'c', 0, 0, 0]);
    let mut w = Vec::new();
    write_string16l(&mut w, Some("a\u{4E2D}"));
    assert_eq!(w, vec![2, 0, b'a', b'?'], "1252 cannot encode it: '?'");
    let mut w = Vec::new();
    write_string16l(&mut w, None);
    assert_eq!(w, vec![0, 0, 0, 0]);
    let mut w = Vec::new();
    write_packed_dword(&mut w, 0x7FFF);
    write_packed_dword(&mut w, 0x0001_2345);
    assert_eq!(w, vec![0xFF, 0x7F, 0x01, 0x80, 0x45, 0x23]);
    let mut w = Vec::new();
    write_packed_dword_of_known_type(&mut w, 0x5000_0012, 0x5000_0000);
    assert_eq!(w, vec![0x12, 0x00]);
    let mut w = Vec::new();
    write_u16_be(&mut w, 0x1234);
    assert_eq!(w, vec![0x12, 0x34]);
    let dump = build_packet_string(&[0x41, 0x0A], 0, 9999);
    assert!(dump.starts_with("   x    0  1  2"));
    assert!(dump.contains("   0   41 0A") && dump.contains("  |A "));
}

#[test]
fn flags_descriptions_and_statistics_read_as_in_ace() {
    assert_eq!(unfold_flags(0x4002), "EncryptedChecksum | AckSequence");
    assert_eq!(unfold_flags(0), "");
    assert_eq!(
        SessionTerminationReason::NetworkTimeout.get_description(),
        "Network Timeout"
    );
    assert_eq!(
        SessionTerminationReason::DATsNewerThanServer.get_description(),
        "Client has newer DATs than server and cannot be downgraded"
    );
    let stats = NetworkStatistics {
        c2s_packets_aggregate: 1_234_567,
        s2c_packets_aggregate: 12,
        ..NetworkStatistics::default()
    };
    let s = stats.summary();
    assert!(
        s.contains("client=>server: 1,234,567\n") && s.contains("server=>client: 12\n"),
        "{s}"
    );
}

#[test]
fn small_handlers_and_packets() {
    let msg = ClientMessage::new(vec![0xEA, 0xF6, 0, 0, 0x78, 0x56, 0x34, 0x12]).expect("4+ bytes");
    assert_eq!(msg.opcode, 0xF6EA);
    assert_eq!(msg.payload(), &[0x78, 0x56, 0x34, 0x12]);
    assert!(ClientMessage::new(vec![1, 2, 3]).is_none());

    assert_eq!(
        parse_hosts("127.0.0.1,10.0.0.2"),
        vec![
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2))
        ]
    );
    assert_eq!(
        parse_hosts("not an address"),
        vec![IpAddr::V4(Ipv4Addr::UNSPECIFIED)]
    );
    assert_eq!(
        get_matched_connection_listener(PortKind::C2S),
        PortKind::S2C
    );

    let mut local = packet_outbound_referral(
        7,
        &["192", "168", "1", "2"],
        [1, 2, 3, 4],
        9000,
        true,
        [9, 9, 9, 9],
    );
    let data = local.initialize_data_writer().clone();
    assert_eq!(&data[8..10], &2u16.to_le_bytes());
    assert_eq!(&data[10..12], &9000u16.to_be_bytes());
    assert_eq!(
        &data[12..16],
        &[9, 9, 9, 9],
        "a private client is referred to the internal host"
    );
    let mut public = packet_outbound_referral(
        7,
        &["8", "8", "8", "8"],
        [1, 2, 3, 4],
        9000,
        true,
        [9, 9, 9, 9],
    );
    assert_eq!(&public.initialize_data_writer()[12..16], &[1, 2, 3, 4]);
}

/// The seeds go into the `ConnectRequest` by direction and come back out the same.
#[test]
fn connect_seeds_round_trip_by_direction() {
    let seeds = ConnectSeeds {
        server_to_client: 0xAAAA_0001,
        client_to_server: 0xBBBB_0002,
    };
    let cr = ConnectRequest::new(1.5, 42, 3, seeds);
    assert_eq!(
        cr.outgoing_seed, 0xAAAA_0001,
        "the client's `OutgoingSeed` is server to client"
    );
    let back = ConnectRequest::from_bytes(&cr.to_bytes()).expect("32 bytes");
    assert_eq!(back.seeds(), seeds);
}

/// `Session.CheckState`: a `ConnectResponse` outside `AuthConnectResponse`, a `LoginRequest`
/// outside `AuthLoginRequest`, and an ACK/TimeSync/Echo/Flow packet in `AuthLoginRequest` are
/// refused before `NetworkSession` sees them (so they are not even counted).
#[test]
fn check_state_refuses_handshake_packets_out_of_turn() {
    let (mut net, id, from, now) = raw_connected();
    let count = |net: &empyrean_net::ServerNet| net.statistics().c2s_packets_aggregate;
    let before = count(&net);
    let cookie = net
        .session(id)
        .expect("live")
        .network
        .connection_data
        .connection_cookie;
    net.on_datagram(
        PortKind::C2S,
        from,
        &connect_response_datagram(cookie, id.client_id),
        now,
    );
    assert_eq!(
        count(&net),
        before,
        "ConnectResponse on P while AuthConnected"
    );
    net.on_datagram(
        PortKind::C2S,
        from,
        &login_request_datagram("raw", "pw", "1802"),
        now,
    );
    assert_eq!(count(&net), before, "LoginRequest while AuthConnected");
    assert_eq!(
        net.session(id).expect("live").core.state,
        SessionState::AuthConnected
    );

    // A fresh session still at AuthLoginRequest refuses an ACK.
    let other = client_addr(61, 40_000);
    net.on_datagram(
        PortKind::C2S,
        other,
        &login_request_datagram("fresh", "pw", "1802"),
        now,
    );
    let Some(Event::LoginRequest { session: fresh, .. }) = net.events().next() else {
        panic!()
    };
    let before = count(&net);
    let ack = client_packet(
        fresh.client_id,
        0,
        &[(PacketFlags::ACK_SEQUENCE, 0u32.to_le_bytes().to_vec())],
    );
    net.on_datagram(PortKind::C2S, other, &ack, now);
    assert_eq!(count(&net), before, "ACK while AuthLoginRequest");
}
