use super::*;
use dereth_transport::indicator::CompletedBlob;

fn net() -> Net {
    let mut n = Net::new(NetConfig::default());
    n.add_connection(0x0B, 0x0B, 1, 0xDEAD_BEEF, 0x1234_5678, None);
    n
}

/// Behaviour: none (precomputed datagrams preserve log-off bytes and state).
/// The goodbye asked for ahead of time is the one the log-off sends, byte for byte, and
/// asking for it sends nothing and logs nothing off.
#[test]
fn the_goodbye_ahead_of_time_is_the_one_the_log_off_sends() {
    let mut n = Net::new(NetConfig::default());
    n.add_connection(0x0B, 0x0B, 1, 1, 2, None);
    n.add_connection(0x0C, 0x0C, 1, 3, 4, None);
    let ahead = n.goodbye();
    assert_eq!(ahead.len(), 2, "one per connection");
    assert!(n.take_outgoing().is_empty(), "asking sent nothing");
    assert!(!n.log_off_sent());
    n.log_off_server();
    assert_eq!(n.take_outgoing(), ahead);
}

/// The seam mapping is total and round-trips for every queue the wire can name.
#[test]
fn every_queue_maps_to_the_seam_and_back() {
    for id in 1..12u16 {
        let q = Queue::from_wire(id).expect("1..=11 are queues");
        assert_eq!(from_seam_queue(to_seam_queue(q)), Some(q), "queue {id}");
    }
    // Ids outside the range are drops, not errors, in both directions.
    assert_eq!(from_seam_queue(NetQueue::Other(0)), None);
    assert_eq!(from_seam_queue(NetQueue::Other(12)), None);
}

/// The teardown points the current server back at the login server.
#[test]
fn the_teardown_points_the_current_server_back_at_the_login_server() {
    let mut n = Net::new(NetConfig::default());
    n.add_connection(0x0B, 0x0B, 1, 1, 2, None); // the login server
    n.add_connection(0x0C, 0x0C, 1, 3, 4, None); // the world the referral sent us to
    n.world_recipient = RecipientId(0x0C);
    n.enter_world();

    // Station 1 -- in the world, on the world server.
    assert_eq!(n.logon_recipient(), RecipientId(0x0B));
    assert_eq!(n.world_recipient(), RecipientId(0x0C));
    assert!(n.in_game());

    // Station 2 -- out.
    assert_eq!(n.exit_world_disconnect(), 1, "the world connection went");
    assert_eq!(
        n.world_recipient(),
        RecipientId(0x0B),
        "world_recipient = logon_recipient, the statement before the loop"
    );
    assert_eq!(
        n.connection_state(RecipientId(0x0C)),
        ConnectionState::Disconnected
    );
    assert_eq!(
        n.connection_state(RecipientId(0x0B)),
        ConnectionState::ConnectionRequestAcked,
        "and the login server is kept"
    );
    // **This is what the ordering of those two statements buys**, and it is only visible here:
    // Removing the connection
    // would have re-entered the server log-off and the client would have gone silent.
    assert!(
        !n.log_off_sent(),
        "the teardown never re-enters the server log-off"
    );
    assert!(
        n.pending_out.is_empty(),
        "and puts no Disconnect on the wire"
    );
}

#[test]
fn send_routes_login_queues_to_the_login_recipient() {
    let mut n = Net::new(NetConfig::default());
    n.add_connection(0x0B, 0x0B, 1, 1, 2, None); // the first is the login server
    n.add_connection(0x0C, 0x0C, 1, 3, 4, None);
    n.world_recipient = RecipientId(0x0C);
    assert_eq!(n.logon_recipient(), RecipientId(0x0B));
    assert_eq!(n.world_recipient(), RecipientId(0x0C));

    n.send(NetQueue::Logon, false, &0xF7C8u32.to_le_bytes());
    n.send(NetQueue::ClientCache, false, &0xF7E3u32.to_le_bytes());
    n.send(NetQueue::UiQueue, false, &0xF7B1u32.to_le_bytes());
    n.send(NetQueue::Weenie, false, &0xF7B1u32.to_le_bytes());

    let mut crypto = dereth_transport::CryptoSystem::new(0);
    let login_out = n
        .connections
        .get_mut(&0x0B)
        .expect("login")
        .flow
        .transmit_new_packets(&mut crypto, LocalTime(0.0));
    let world_out = n
        .connections
        .get_mut(&0x0C)
        .expect("world")
        .flow
        .transmit_new_packets(&mut crypto, LocalTime(0.0));

    let queues_in = |out: &[Vec<u8>]| -> Vec<u16> {
        out.iter()
            .flat_map(|dg| {
                ParsedPacket::parse(dg)
                    .expect("parse")
                    .fragments
                    .iter()
                    .map(|f| f.header.queue_id)
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    assert_eq!(queues_in(&login_out), vec![4, 5]);
    assert_eq!(queues_in(&world_out), vec![9, 3]);
}

/// And the ordering type follows the same split, with bit 61 set on the login-bound blobs even
/// though nothing reads it back. Compatibility note #38.
#[test]
fn send_stamps_the_documented_ordering_types() {
    let mut n = net();
    n.send(NetQueue::Logon, false, &0xF7C8u32.to_le_bytes());
    n.send(NetQueue::UiQueue, false, &0xF7B1u32.to_le_bytes());
    let mut crypto = dereth_transport::CryptoSystem::new(0);
    let out = n
        .connections
        .get_mut(&0x0B)
        .expect("conn")
        .flow
        .transmit_new_packets(&mut crypto, LocalTime(0.0));
    let highs: Vec<u32> = out
        .iter()
        .flat_map(|dg| {
            ParsedPacket::parse(dg)
                .expect("parse")
                .fragments
                .iter()
                .map(|f| f.header.blob_id_high & 0xFF00_0000)
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(highs, vec![0x2300_0000, 0x0300_0000]);
}

/// The stamp counts up per blob and the sequence counter runs 1, 2, 3, ... — **starting at 1**.
///
/// Oracle: the six recorded retail sessions, whose first client fragment is
/// always blob id `0x23000001_00000001` and whose second is always `0x23000002_00000002`. That
/// is one more than the unordered stamp's post-increment and the non-ephemeral id getter's
/// post-increment from `make_initial_sequence_id(0)` predict on their own; see
/// [`FIRST_BLOB_COUNTER`] for why, and for
/// what a sequence of 0 does to a server.
#[test]
fn send_advances_the_stamp_and_the_sequence_counter() {
    let mut n = net();
    for _ in 0..3 {
        n.send(NetQueue::UiQueue, false, &0xF7B1u32.to_le_bytes());
    }
    let mut crypto = dereth_transport::CryptoSystem::new(0);
    let out = n
        .connections
        .get_mut(&0x0B)
        .expect("conn")
        .flow
        .transmit_new_packets(&mut crypto, LocalTime(0.0));
    let ids: Vec<(u16, u32)> = out
        .iter()
        .flat_map(|dg| {
            ParsedPacket::parse(dg)
                .expect("parse")
                .fragments
                .iter()
                .map(|f| {
                    let id = NetBlobId(f.header.blob_id());
                    (id.ordering_stamp(), id.low32())
                })
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(ids, vec![(1, 1), (2, 2), (3, 3)]);
}

/// A connected link keeps talking: a cumulative ACK every 2.0 s, with `TimeSync` and
/// `EchoRequest` riding it every 6 intervals, and nothing at all before the first boundary.
///
/// Oracle: recorded session 01, the retail client against this same server. Its
/// client-to-server stream after the handshake is `AckSequence` at 2.023, 4.023, 6.023, ... and
/// `header_ = 0x0B004002` — `AckSequence | TimeSync | EchoRequest | Flow | EncryptedChecksum` —
/// on two of every three of them, because a packet holding only `TimeSync`, `EchoRequest` and
/// `Flow` waits for a packet to ride. (No datagram arrives here, so no `Flow` report is made.)
/// The cadences themselves are the cumulative-ack enqueue and the local-interval bump.
///
/// Before this existed `Net::tick` computed `advance_interval`'s events and dropped them, and
/// enqueued no ACK, so a connected client sent **nothing** once it had said its piece and ACE
/// eventually terminated the session on `Network.TimeoutTick`.
#[test]
fn a_connected_link_acks_every_two_seconds_and_carries_the_time_sync() {
    let mut n = net();
    n.connections.get_mut(&0x0B).expect("conn").state = ConnectionState::Connected;
    n.connections
        .get_mut(&0x0B)
        .expect("conn")
        .receiver
        .window
        .highest_id_received = 7;

    let mut seen: Vec<(f64, u32)> = Vec::new();
    let mut t = 0.0;
    while t < 8.0 {
        n.tick(LocalTime(t), Duration::from_millis(0));
        for (bytes, _) in n.take_outgoing() {
            let p = ParsedPacket::parse(&bytes).expect("parse");
            seen.push((t, p.header.header.0));
            if let Some(b) = p
                .optional
                .get(&dereth_transport::wire::PacketFlags::ACK_SEQUENCE)
            {
                assert_eq!(
                    u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                    7,
                    "the ACK carries highest_id_received"
                );
            }
        }
        t += 0.25;
    }

    // Nothing before the first 2.0 s boundary, then one packet every 2.0 s.
    let times: Vec<f64> = seen.iter().map(|(t, _)| *t).collect();
    assert_eq!(times, vec![2.0, 4.0, 6.0], "one cumulative ACK every 2.0 s");

    // Every one of them is an AckSequence, and the 6-interval boundary (3.0 s, 6.0 s) puts the
    // time-sensitive pair on the next ACK to leave.
    for (_, flags) in &seen {
        assert_ne!(flags & dereth_transport::wire::PacketFlags::ACK_SEQUENCE, 0);
    }
    let with_sync: Vec<f64> = seen
        .iter()
        .filter(|(_, f)| {
            f & dereth_transport::wire::PacketFlags::TIME_SYNC != 0
                && f & dereth_transport::wire::PacketFlags::ECHO_REQUEST != 0
        })
        .map(|(t, _)| *t)
        .collect();
    assert_eq!(
        with_sync,
        vec![4.0, 6.0],
        "TimeSync/EchoRequest ride an ACK, never go alone"
    );
    // Riding a non-disposable section makes the packet sequenced and encrypted, which is what
    // the retail 0x0B004002 packets are and the bare 0x00004000 ACKs are not.
    let sync_flags = seen.iter().find(|(t, _)| *t == 4.0).expect("4.0 s").1;
    assert_ne!(
        sync_flags & dereth_transport::wire::PacketFlags::ENCRYPTED_CHECKSUM,
        0
    );
}

/// The keep alive is cmd nop to port plus one.
#[test]
fn the_keep_alive_is_cmd_nop_to_port_plus_one() {
    let mut n = Net::new(NetConfig::default());
    let addr: SocketAddr = "127.0.0.1:19000".parse().expect("addr");
    n.add_connection(0x0B, 0x0B, 1, 0xDEAD_BEEF, 0x1234_5678, Some(addr));
    n.connections.get_mut(&0x0B).expect("conn").state = ConnectionState::Connected;

    let mut keep_alives: Vec<(Vec<u8>, Option<SocketAddr>)> = Vec::new();
    let mut t = 0.0;
    while t < 112.0 {
        n.tick(LocalTime(t), Duration::from_millis(0));
        for dg in n.take_outgoing() {
            let p = ParsedPacket::parse(&dg.0).expect("parse");
            if p.header
                .header
                .contains(dereth_transport::wire::PacketFlags::CICMD_COMMAND)
            {
                keep_alives.push(dg);
            }
        }
        t += 0.5;
    }

    assert_eq!(keep_alives.len(), 1, "one keep-alive per 220 intervals");
    let (bytes, to) = &keep_alives[0];
    assert_eq!(
        *to,
        Some("127.0.0.1:19001".parse::<SocketAddr>().expect("addr")),
        "the keep-alive goes to the server's port + 1"
    );
    let p = ParsedPacket::parse(bytes).expect("parse");
    assert_eq!(p.header.seq_id, 0, "unsequenced");
    assert_eq!(
        p.header.rec_id, 0,
        "rec_id = 0, not the connection's 0x0B: the optional-header send's no-receiver arm \
                    zeroes it regardless of the assigned NetID"
    );
    assert_eq!(p.header.iteration, 0, "iteration = 0, not the connection's");
    assert_eq!(
        p.header.interval, 0,
        "interval = 0; the arm never writes it in either branch"
    );
    assert!(!p.header.header.is_encrypted(), "the section is disposable");
    assert_eq!(
        p.optional
            .get(&dereth_transport::wire::PacketFlags::CICMD_COMMAND)
            .map(Vec::as_slice),
        Some(&[1, 0, 0, 0, 0, 0, 0, 0][..]),
        "the no-op command = 1, parameter = 0"
    );
}

/// An unregistered queue silently discards; ids 0 and >= 12 never reach a queue at all.
#[test]
fn send_to_an_unmapped_queue_is_a_drop_not_an_error() {
    let mut n = net();
    n.send(NetQueue::Other(0), false, b"junk");
    n.send(NetQueue::Other(12), false, b"junk");
    assert_eq!(n.connections[&0x0B].flow.waiting_len(), 0);
}

/// `poll` returns whole blobs, opcode lifted out, in the frame's queue-drain order.
#[test]
fn poll_returns_whole_blobs_in_drain_order() {
    let mut n = net();
    let blob = |queue_id: u16, opcode: u32| CompletedBlob {
        id: NetBlobId(1),
        queue_id,
        sender: 0x0B,
        payload: {
            let mut v = opcode.to_le_bytes().to_vec();
            v.extend_from_slice(&[0xAA, 0xBB]);
            v
        },
    };
    // Filed in the reverse of the drain order.
    n.queues.add_received_blob(blob(9, 0xF7B0));
    n.queues.add_received_blob(blob(10, 0xF745));
    n.queues.add_received_blob(blob(5, 0xF7E3));
    n.queues.add_received_blob(blob(4, 0xF7DE));

    let got: Vec<(NetQueue, u32, Vec<u8>)> = std::iter::from_fn(|| n.poll())
        .map(|m| (m.queue, m.opcode, m.body))
        .collect();
    assert_eq!(
        got,
        vec![
            (NetQueue::Logon, 0xF7DE, vec![0xAA, 0xBB]),
            (NetQueue::ClientCache, 0xF7E3, vec![0xAA, 0xBB]),
            (NetQueue::WorldObjects, 0xF745, vec![0xAA, 0xBB]),
            (NetQueue::UiQueue, 0xF7B0, vec![0xAA, 0xBB]),
        ]
    );
    assert!(n.poll().is_none());
}

/// Net is a transport.
#[test]
fn net_is_a_transport() {
    let mut n = net();
    let t: &mut dyn Transport = &mut n;
    assert!(t.poll().is_none());
}

/// A round trip through the transport: a server packet in, a whole blob out.
///
/// Oracle: the worked packet's fragment shape from
/// `docs/networking/01-packet-format.md` §6 — a UI-queue blob whose first dword is the
/// opcode `0xF7B0` — rebuilt here so the fragment is addressed to this connection.
#[test]
fn a_fed_packet_becomes_a_polled_message() {
    let mut n = net();
    let msg = {
        let mut v = 0x0000_F7B0u32.to_le_bytes().to_vec();
        v.extend_from_slice(&0x5000_0001u32.to_le_bytes());
        v.extend_from_slice(&2u32.to_le_bytes());
        v
    };
    let frag = dereth_transport::Fragment::new(
        dereth_transport::FragmentHeader {
            blob_id_low: 1,
            blob_id_high: 0x8000_0000,
            num_frags: 1,
            blob_frag_size: 0,
            blob_num: 0,
            queue_id: 9,
        },
        msg.clone(),
    );
    let mut p = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
        seq_id: 2,
        rec_id: 0x0B,
        interval: 0x0100,
        iteration: 1,
        ..Default::default()
    });
    p.add_fragment(frag).expect("frag");
    // The server's first draw; incoming crypto is initialized from seed `0xDEADBEEF`.
    let mut server = dereth_transport::CryptoSystem::new(0xDEAD_BEEF);
    let datagram = p.serialize(Some(server.next())).expect("serialize");

    n.feed(&datagram, None, LocalTime(1.0)).expect("accepted");
    let got = n.poll().expect("one whole blob");
    assert_eq!(got.queue, NetQueue::UiQueue);
    assert_eq!(got.opcode, 0xF7B0);
    assert_eq!(got.body, msg[4..].to_vec());

    // And the handshake completed: the first non-ConnectRequest packet moves to `Connected`.
    assert_eq!(
        n.connection_state(RecipientId(0x0B)),
        ConnectionState::Connected
    );
}

/// A duplicate encrypted packet is rejected and does **not** consume an ISAAC value, so the
/// next real packet still decrypts. `docs/networking/02-reliability-and-flow.md` §2.2, at the
/// `Net` level.
#[test]
fn feeding_a_duplicate_does_not_desynchronise_the_stream() {
    let mut n = net();
    let build = |seq: u32, key: u32, body: u8| {
        let mut p = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: seq,
            rec_id: 0x0B,
            iteration: 1,
            ..Default::default()
        });
        p.add_fragment(dereth_transport::Fragment::new(
            dereth_transport::FragmentHeader {
                blob_id_low: u32::from(body),
                blob_id_high: 0x8000_0000,
                num_frags: 1,
                blob_frag_size: 0,
                blob_num: 0,
                queue_id: 9,
            },
            vec![body; 8],
        ))
        .expect("frag");
        p.serialize(Some(key)).expect("serialize")
    };
    let mut server = dereth_transport::CryptoSystem::new(0xDEAD_BEEF);
    let k1 = server.next();
    let k2 = server.next();

    let first = build(2, k1, 1);
    n.feed(&first, None, LocalTime(1.0))
        .expect("first accepted");
    // The identical datagram again: not newer, never NAKed -> dropped, no draw.
    assert_eq!(
        n.feed(&first, None, LocalTime(1.0)),
        Err(RejectReason::DuplicateSequence)
    );
    // So sequence 3 still decrypts under the *second* stream value.
    n.feed(&build(3, k2, 2), None, LocalTime(1.0))
        .expect("the stream never lost its place");
}

/// TimeSync is the master clock and the caller hard-sets its game clock to it.
#[test]
fn a_timesync_header_is_handed_to_the_caller() {
    let mut n = net();
    let mut p = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
        seq_id: 2,
        rec_id: 0x0B,
        iteration: 1,
        ..Default::default()
    });
    p.add_optional_header(
        dereth_transport::wire::PacketFlags::TIME_SYNC,
        1234.5f64.to_le_bytes().to_vec(),
    )
    .expect("timesync");
    let mut server = dereth_transport::CryptoSystem::new(0xDEAD_BEEF);
    let datagram = p.serialize(Some(server.next())).expect("serialize");

    assert!(n.take_time_sync().is_none());
    n.feed(&datagram, None, LocalTime(1.0)).expect("accepted");
    assert_eq!(n.take_time_sync(), Some(1234.5));
    assert!(n.take_time_sync().is_none(), "taken once");
}

/// The optional-header processor computes an EchoResponse sample as
/// the local clock minus the echoed local time minus the holding time, narrowing only
/// the final result. Every addressed receiver keeps its latest sample, but only the
/// current-world receiver reaches the ping-response step.
/// The snapshot later copies that latest value without adding it to the ping ring a
/// second time.
#[test]
fn echo_response_updates_its_receiver_and_samples_only_the_current_world_once() {
    fn response(rec_id: u16, local_time: f32, holding_time: f32) -> Vec<u8> {
        let mut p = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: 2,
            rec_id,
            interval: 0x0100,
            iteration: 1,
            ..Default::default()
        });
        let mut body = Vec::with_capacity(8);
        body.extend_from_slice(&local_time.to_le_bytes());
        body.extend_from_slice(&holding_time.to_le_bytes());
        p.add_optional_header(dereth_transport::wire::PacketFlags::ECHO_RESPONSE, body)
            .expect("EchoResponse");
        let mut server = dereth_transport::CryptoSystem::new(0xDEAD_BEEF);
        p.serialize(Some(server.next()))
            .expect("encrypted EchoResponse")
    }

    let mut n = Net::new(NetConfig::default());
    n.add_connection(0x0B, 0x0B, 1, 0xDEAD_BEEF, 0x1234_5678, None);
    n.add_connection(0x0C, 0x0C, 1, 0xDEAD_BEEF, 0x1234_5678, None);
    n.world_recipient = RecipientId(0x0C);

    n.feed(&response(0x0B, 12.5, 0.25), None, LocalTime(12.0))
        .expect("non-current receiver accepts EchoResponse");
    assert_eq!(
        n.receiver(RecipientId(0x0B))
            .expect("login receiver")
            .round_trip_latency,
        -0.75,
        "retail does not clamp a stale/negative payload sample"
    );
    assert_eq!(
        n.link_status.round_trip_delays.count(),
        0,
        "a non-current receiver is not a current-world ping sample"
    );

    // 16,777,217 cannot be represented by f32. This must remain +0.75: narrowing `now` before
    // subtracting, instead of at native's final store, would produce -0.25.
    n.feed(
        &response(0x0C, 16_777_216.0, 0.25),
        None,
        LocalTime(16_777_217.0),
    )
    .expect("current-world receiver accepts EchoResponse");
    assert_eq!(
        n.receiver(RecipientId(0x0C))
            .expect("world receiver")
            .round_trip_latency,
        0.75
    );
    assert_eq!(n.link_status.round_trip_delays.count(), 1);
    assert_eq!(n.link_status.round_trip_delays.total(), 0.75);

    n.process_connections(LocalTime(16_777_219.0));
    assert_eq!(n.link_status.snapshot.round_trip_delay, 0.75);
    assert_eq!(
        n.link_status.round_trip_delays.count(),
        1,
        "the heartbeat snapshot must not double-sample the EchoResponse RTT"
    );
    assert_eq!(n.link_status.round_trip_delays.total(), 0.75);
}

/// The default server port is 7304, not 9000, and the client version is "1802".
#[test]
fn the_defaults_are_the_retail_ones() {
    let cfg = NetConfig::default();
    assert_eq!(cfg.port, 7304);
    assert_eq!(cfg.client_version, "1802");
    assert_eq!(cfg.port_mode(), PortMode::Fixed(0));
}
