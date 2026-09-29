//! ACE: Source/ACE.Server/Network/NetworkSession.cs::SendBundle
//! Large messages fragment/reassemble both ways; SendBundle sends tail before middle; only the UI
//! queue keeps order; one bundle per 5 ms.
//! Fixture: locally constructed packets and session state on a virtual clock.

use std::time::Duration;

use dereth_primitives::NetQueue;
use dereth_transport::wire::PacketFlags;
use empyrean_net::{ClockSnapshotExt, Event, GameMessageGroup, OutboundMessage};

use crate::common::{client_addr, connect, harness, parse_all, raw_connected, session_of, TICK};

/// A message of `len` bytes: `opcode`, then a pattern that makes misordered fragments visible.
pub fn message(opcode: u32, len: usize) -> Vec<u8> {
    let mut m = opcode.to_le_bytes().to_vec();
    #[allow(clippy::cast_possible_truncation)]
    m.extend((0..len.saturating_sub(4)).map(|i| (i * 7 + opcode as usize) as u8));
    m
}

/// Server to client: two large messages and small ones, on two queues, all queued before one
/// flush, so their fragments share packets (ACE's tail-first packing). The client transport gets
/// every message whole, in order per queue.
#[test]
fn large_messages_fragment_and_reassemble_server_to_client() {
    let mut h = harness();
    let c = h.add_client(client_addr(10, 50_000), "frag", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let id = session_of(&h, c).expect("session");

    let sent: Vec<(GameMessageGroup, Vec<u8>)> = vec![
        (GameMessageGroup::UIQueue, message(0xF001, 3000)),
        (GameMessageGroup::UIQueue, message(0xF002, 40)),
        (GameMessageGroup::SmartboxQueue, message(0xF003, 1500)),
        (GameMessageGroup::UIQueue, message(0xF004, 900)),
        (GameMessageGroup::SmartboxQueue, message(0xF005, 12)),
        (GameMessageGroup::SmartboxQueue, message(0xF006, 448 * 3)),
    ];
    for (group, data) in &sent {
        h.net.server.send(
            id,
            OutboundMessage {
                group: *group,
                data: data.clone(),
            },
        );
    }

    let mut got: Vec<(NetQueue, Vec<u8>)> = Vec::new();
    h.run_until(Duration::from_secs(5), TICK, |h| {
        while let Some(m) = h.clients[c].poll() {
            let mut data = m.opcode.to_le_bytes().to_vec();
            data.extend_from_slice(&m.body);
            got.push((m.queue, data));
        }
        got.len() >= sent.len()
    });

    for (group, queue) in [
        (GameMessageGroup::UIQueue, NetQueue::UiQueue),
        (GameMessageGroup::SmartboxQueue, NetQueue::WorldObjects),
    ] {
        let want: Vec<&Vec<u8>> = sent
            .iter()
            .filter(|(g, _)| *g == group)
            .map(|(_, d)| d)
            .collect();
        let have: Vec<&Vec<u8>> = got
            .iter()
            .filter(|(q, _)| *q == queue)
            .map(|(_, d)| d)
            .collect();
        assert_eq!(have, want, "{group:?}: every message once, whole, in order");
    }
    assert_eq!(h.clients[c].rejected, 0);
}

/// Client to server: large blobs from the client transport, interleaved with small ones, reach
/// the world whole and in fragment-sequence order.
#[test]
fn large_messages_fragment_and_reassemble_client_to_server() {
    let mut h = harness();
    let c = h.add_client(client_addr(11, 50_000), "frag2", "pw");
    assert!(connect(&mut h, c, Duration::from_secs(5)));
    let id = session_of(&h, c).expect("session");
    h.events.clear();

    let sent = [
        message(0xF7B1, 2500),
        message(0xF7B1, 8),
        message(0xF7B1, 1200),
        message(0xF7B1, 64),
        message(0xF7B1, 448),
    ];
    for m in &sent {
        h.clients[c].send(NetQueue::Weenie, m);
    }
    let done = h.run_until(Duration::from_secs(5), TICK, |h| {
        h.events
            .iter()
            .filter(|e| matches!(e, Event::Message { .. }))
            .count()
            >= sent.len()
    });
    assert!(done, "all messages arrived");
    let got: Vec<&Vec<u8>> = h
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Message {
                session, message, ..
            } if *session == id => Some(&message.data),
            _ => None,
        })
        .collect();
    assert_eq!(got, sent.iter().collect::<Vec<_>>());
}

/// ACE's `SendBundle` on one 900-byte message: the first packet carries fragment 0 alone (a
/// "large" message fills it); with 452 bytes left the message is no longer "large", and the next
/// packet carries the **tail** (fragment 2, 4 bytes) because a pending tail goes before the next
/// piece; fragment 1 (448 bytes) no longer fits beside it and follows alone.
#[test]
fn send_bundle_sends_the_tail_before_the_middle() {
    let (mut net, id, _, now) = raw_connected();
    net.send(
        id,
        OutboundMessage {
            group: GameMessageGroup::UIQueue,
            data: message(0xF7B0, 900),
        },
    );
    let out: Vec<_> = net.poll(now).collect();
    let packets = parse_all(&out);
    let frags: Vec<Vec<(u16, u16, usize)>> = packets
        .iter()
        .map(|p| {
            p.fragments
                .iter()
                .map(|f| (f.header.blob_num, f.header.num_frags, f.payload.len()))
                .collect()
        })
        .collect();
    assert_eq!(
        frags,
        vec![vec![(0, 3, 448)], vec![(2, 3, 4)], vec![(1, 3, 448)]]
    );
    for p in &packets {
        assert!(p
            .header
            .header
            .contains(PacketFlags::ENCRYPTED_CHECKSUM | PacketFlags::BLOB_FRAGMENTS));
        let f = &p.fragments[0];
        assert_eq!(f.header.blob_id_high, 0x8000_0000, "MessageFragment Id");
        assert_eq!(f.header.queue_id, GameMessageGroup::UIQueue as u16);
    }
    let seqs: Vec<u32> = packets.iter().map(|p| p.header.seq_id).collect();
    assert_eq!(seqs, vec![3, 4, 5], "the TimeSync took sequence 2");
}

/// `UIQueue` stops at the first message that does not fit ("UIQueue messages must go out in
/// order"); other queues let a later small message jump into the gap.
#[test]
fn only_the_ui_queue_keeps_message_order_inside_a_packet() {
    for (group, first_packet) in [
        (GameMessageGroup::UIQueue, vec![0u32]),
        (GameMessageGroup::SmartboxQueue, vec![0, 2]),
    ] {
        let (mut net, id, _, now) = raw_connected();
        for len in [400, 100, 20] {
            net.send(
                id,
                OutboundMessage {
                    group,
                    data: message(0xF7B0, len),
                },
            );
        }
        let out: Vec<_> = net.poll(now).collect();
        let packets = parse_all(&out);
        let seqs: Vec<u32> = packets[0]
            .fragments
            .iter()
            .map(|f| f.header.blob_id_low)
            .collect();
        assert_eq!(seqs, first_packet, "{group:?}");
    }
}

/// ACE sends at most one bundle per 5 ms per session (`minimumTimeBetweenBundles`): two groups
/// with messages go out on two polls 5 ms apart.
#[test]
fn one_bundle_per_five_milliseconds() {
    let (mut net, id, _, mut now) = raw_connected();
    net.send(
        id,
        OutboundMessage {
            group: GameMessageGroup::UIQueue,
            data: message(0xF7B0, 16),
        },
    );
    net.send(
        id,
        OutboundMessage {
            group: GameMessageGroup::SmartboxQueue,
            data: message(0xF7B0, 16),
        },
    );
    assert_eq!(net.poll(now).count(), 1);
    now = now.advanced(Duration::from_millis(4));
    assert_eq!(net.poll(now).count(), 0, "4 ms later: still waiting");
    now = now.advanced(Duration::from_millis(1));
    assert_eq!(net.poll(now).count(), 1, "5 ms later: the second bundle");
}
