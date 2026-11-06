//! The data connection against a synthetic server built from the same framing. Every clock is a
//! value the test supplies.

use super::*;
use crate::wire::Grant;

const SERVER: u32 = 77;
const RECIPIENT: u32 = 0x1234_5678;

fn endpoint() -> SocketAddr {
    "127.0.0.1:9000".parse().unwrap()
}
fn grant() -> Grant {
    Grant {
        server: SERVER,
        table: 0,
        server_time: 0.0,
        world_epoch: 0.0,
        recipient: RECIPIENT,
        version: "4.35752".to_owned(),
        server_to_client_seed: 0,
        client_to_server_seed: 0,
    }
}
fn connection() -> Connection {
    Connection::new(
        endpoint(),
        &grant(),
        KeyStream::new(0),
        KeyStream::new(0),
        LocalTime(0.0),
    )
    .unwrap()
}
/// The server's key for each of its sequences, drawn in order from sequence two.
fn key(seq: u32) -> u32 {
    let mut keys = KeyStream::new(0);
    let mut k = 0;
    for _ in 2..=seq {
        k = keys.next();
    }
    k
}
fn fragment(id: u32, queue: u16, count: u16, index: u16, bytes: &[u8]) -> Fragment {
    Fragment::new(
        FragmentHeader {
            blob_id_low: id,
            blob_id_high: 0x8000_0000,
            queue_id: queue,
            num_frags: count,
            blob_num: index,
            ..Default::default()
        },
        bytes.to_vec(),
    )
}
fn data_at(seq: u32, time: u8, fragments: &[Fragment]) -> Vec<u8> {
    let body: Vec<u8> = fragments.iter().flat_map(Fragment::to_bytes).collect();
    let hash = fragments
        .iter()
        .fold(0u32, |hash, f| hash.wrapping_add(f.hash()));
    let mut h = Header {
        sequence: seq,
        flags: 0x802,
        recipient: SERVER,
        time,
        payload_len: u16::try_from(body.len()).unwrap(),
        ..Default::default()
    };
    h.checksum = h.checksum_base().wrapping_add(hash ^ key(seq));
    let mut packet = h.encode().to_vec();
    packet.extend(body);
    packet
}
fn data(seq: u32, fragments: &[Fragment]) -> Vec<u8> {
    data_at(seq, 1, fragments)
}
fn message(seq: u32) -> Vec<u8> {
    data(seq, &[fragment(seq, 4, 1, 0, &seq.to_le_bytes())])
}
fn control(seq: u32, flags: u32, body: &[u8]) -> Vec<u8> {
    encode_generic(
        Header {
            sequence: seq,
            flags,
            recipient: SERVER,
            time: 1,
            ..Default::default()
        },
        body,
    )
    .unwrap()
}
fn sequences(list: &[u32]) -> Vec<u8> {
    super::list(list)
}
fn messages(c: &mut Connection) -> Vec<u32> {
    let mut out = Vec::new();
    while let Some(event) = c.poll() {
        if let Event::Message(m) = event {
            out.push(u32::from_le_bytes(m.bytes[..4].try_into().unwrap()));
        }
    }
    out
}
fn sent(c: &mut Connection) -> Vec<Vec<u8>> {
    c.take_outgoing().into_iter().map(|(b, _)| b).collect()
}
fn flags_of(packets: &[Vec<u8>]) -> Vec<u32> {
    packets
        .iter()
        .map(|p| Header::decode(p).unwrap().flags)
        .collect()
}

#[test]
fn the_first_data_sequence_is_two_and_sequence_one_is_dropped() {
    let mut c = connection();
    c.feed(&message(1), endpoint(), LocalTime(0.1)).unwrap();
    c.feed(&message(2), endpoint(), LocalTime(0.1)).unwrap();
    assert_eq!(messages(&mut c), [2]);
}

#[test]
fn the_connection_is_up_on_its_first_ordinary_packet_not_on_a_server_switch() {
    let mut c = connection();
    c.feed(
        &control(0, 0x102, &0.0f64.to_le_bytes()),
        endpoint(),
        LocalTime(0.1),
    )
    .unwrap();
    assert_eq!(c.poll(), Some(Event::ServerSwitch { server_time: 0.0 }));
    assert_eq!(c.poll(), None);
    let out = sent(&mut c);
    let reply = Packet::decode(&out[0]).unwrap();
    assert_eq!(
        (
            reply.header.flags,
            reply.header.recipient,
            reply.header.sequence
        ),
        (0x102, RECIPIENT, 0)
    );
    assert!(reply.body.is_empty() && reply.generic_checksum_ok());
    c.feed(&message(2), endpoint(), LocalTime(0.2)).unwrap();
    assert_eq!(c.poll(), Some(Event::Connected));
    assert!(matches!(c.poll(), Some(Event::Message(_))));
}

#[test]
fn a_hole_filled_behind_more_messages_than_the_event_cap_delivers_them_all() {
    let mut c = connection();
    let behind = u32::try_from(MAX_EVENTS).unwrap() + 20;
    for seq in 3..3 + behind {
        c.feed(&message(seq), endpoint(), LocalTime(0.1)).unwrap();
    }
    assert_eq!(c.poll(), Some(Event::Connected));
    assert_eq!(c.poll(), None);
    // The hole fills; the burst exceeds the cap, the rest wait parked instead of failing.
    c.feed(&message(2), endpoint(), LocalTime(0.2)).unwrap();
    let delivered = messages(&mut c);
    assert_eq!(delivered, (2..3 + behind).collect::<Vec<_>>());
    c.feed(&message(3 + behind), endpoint(), LocalTime(0.3))
        .unwrap();
    assert_eq!(messages(&mut c), [3 + behind]);
}

#[test]
fn out_of_order_packets_are_keyed_by_sequence_and_delivered_in_order() {
    let mut c = connection();
    c.feed(&message(4), endpoint(), LocalTime(0.1)).unwrap();
    c.feed(&message(2), endpoint(), LocalTime(0.1)).unwrap();
    assert_eq!(messages(&mut c), [2]);
    c.feed(&message(4), endpoint(), LocalTime(0.1)).unwrap();
    c.feed(&message(3), endpoint(), LocalTime(0.1)).unwrap();
    c.feed(&message(3), endpoint(), LocalTime(0.1)).unwrap();
    assert_eq!(messages(&mut c), [3, 4]);
}

#[test]
fn missing_packets_are_asked_for_every_quarter_second_with_the_flow_word() {
    let mut c = connection();
    c.feed(&message(4), endpoint(), LocalTime(0.1)).unwrap();
    c.tick(LocalTime(0.2)).unwrap();
    assert!(sent(&mut c).is_empty());
    c.tick(LocalTime(0.3)).unwrap();
    let out = sent(&mut c);
    let p = Packet::decode(&out[0]).unwrap();
    // The first control packet also asks for a clock synchronisation.
    assert_eq!(p.header.flags, 0x1a);
    assert_eq!((p.header.sequence, p.header.recipient), (1, RECIPIENT));
    // The flow word, then the list.
    assert_eq!(&out[0][20..24], &40u32.to_le_bytes());
    assert_eq!(&out[0][24..], &sequences(&[2, 3]));
    assert_eq!(usize::from(p.header.payload_len), out[0].len() - 24);
    c.tick(LocalTime(0.5)).unwrap();
    assert!(sent(&mut c).is_empty());
    c.tick(LocalTime(0.6)).unwrap();
    assert_eq!(flags_of(&sent(&mut c))[0] & !0x10, 0xa);
}

#[test]
fn without_missing_packets_the_heartbeat_reports_the_highest_sequence_every_two_seconds() {
    let mut c = connection();
    c.tick(LocalTime(0.0)).unwrap();
    c.feed(&message(2), endpoint(), LocalTime(0.1)).unwrap();
    c.tick(LocalTime(1.9)).unwrap();
    assert!(sent(&mut c).is_empty());
    c.tick(LocalTime(2.0)).unwrap();
    let out = sent(&mut c);
    // Before any clock synchronisation, the heartbeat asks for one.
    assert_eq!(flags_of(&out), [0x32]);
    assert_eq!(&out[0][24..], &2u32.to_le_bytes());
    // A server switch synchronises the clock; the next ask waits a minute.
    c.feed(
        &control(0, 0x102, &0.0f64.to_le_bytes()),
        endpoint(),
        LocalTime(2.5),
    )
    .unwrap();
    assert_eq!(flags_of(&sent(&mut c)), [0x102]);
    c.tick(LocalTime(4.0)).unwrap();
    assert_eq!(flags_of(&sent(&mut c)), [0x22]);
}

#[test]
fn a_resend_request_is_served_from_the_cache_and_the_rest_are_denied() {
    let mut c = connection();
    for n in 0..=WINDOW {
        c.send(6, &n.to_le_bytes(), LocalTime(0.0)).unwrap();
        sent(&mut c);
    }
    // Sequence two has left the cache; the newest is still in it.
    let newest = WINDOW + 2;
    c.feed(
        &control(0, 0xa, &sequences(&[2, newest])),
        endpoint(),
        LocalTime(1.0),
    )
    .unwrap();
    let out = sent(&mut c);
    let resent = Packet::decode(&out[0]).unwrap();
    assert_eq!(
        (resent.header.flags, resent.header.sequence),
        (0x803, newest)
    );
    let skip = Packet::decode(&out[1]).unwrap();
    assert_eq!((skip.header.flags & !0x10, skip.header.sequence), (0x82, 0));
    assert_eq!(&out[1][24..], &sequences(&[2]));
    let deny = Packet::decode(&out[2]).unwrap();
    assert_eq!(deny.header.flags, 0x2002);
    assert_eq!(deny.body, sequences(&[2]).as_slice());
    assert!(deny.generic_checksum_ok());
    assert_eq!(out.len(), 3);
}

#[test]
fn a_resend_request_is_read_the_way_the_client_reads_it() {
    let mut c = connection();
    c.send(6, b"\x01\0\0\0", LocalTime(0.0)).unwrap();
    sent(&mut c);
    // A list with a leading length word: the count reads as the length, its words as sequences.
    c.feed(
        &control(0, 0xa, &[8, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0]),
        endpoint(),
        LocalTime(1.0),
    )
    .unwrap();
    assert_eq!(flags_of(&sent(&mut c)), [0x803, 0x82 | 0x10]);
    for body in [vec![0, 0, 0, 0], vec![0x73, 0, 0, 0], vec![]] {
        assert_eq!(
            c.feed(&control(0, 0xa, &body), endpoint(), LocalTime(1.0)),
            Err(Error::Control)
        );
    }
}

#[test]
fn a_skip_list_stops_the_wait_for_a_packet_the_server_will_not_resend() {
    let mut c = connection();
    c.feed(&message(3), endpoint(), LocalTime(0.1)).unwrap();
    assert_eq!(messages(&mut c), Vec::<u32>::new());
    c.feed(
        &control(0, 0x82, &sequences(&[2])),
        endpoint(),
        LocalTime(0.2),
    )
    .unwrap();
    assert_eq!(messages(&mut c), [3]);
    c.tick(LocalTime(0.6)).unwrap();
    assert!(flags_of(&sent(&mut c)).iter().all(|f| f & 0x8 == 0));
}

#[test]
fn a_deny_from_ac1999s_server_also_stops_the_wait() {
    let mut c = connection();
    c.feed(&message(3), endpoint(), LocalTime(0.1)).unwrap();
    c.feed(
        &control(0, 0x2002, &sequences(&[2])),
        endpoint(),
        LocalTime(0.2),
    )
    .unwrap();
    assert_eq!(messages(&mut c), [3]);
}

#[test]
fn a_sequenced_control_announces_the_senders_latest_packet() {
    let mut c = connection();
    c.feed(&message(2), endpoint(), LocalTime(0.1)).unwrap();
    c.feed(&control(3, 0x22, &[5, 0, 0, 0]), endpoint(), LocalTime(0.1))
        .unwrap();
    c.tick(LocalTime(0.4)).unwrap();
    let out = sent(&mut c);
    assert_eq!(&out[0][24..], &sequences(&[3]));
    c.feed(&message(3), endpoint(), LocalTime(0.5)).unwrap();
    assert_eq!(messages(&mut c), [2, 3]);
}

#[test]
fn a_newer_time_byte_ends_a_period_and_reports_its_bytes() {
    let mut c = connection();
    let first = data_at(2, 1, &[fragment(2, 4, 1, 0, &[2, 0, 0, 0])]);
    c.feed(&first, endpoint(), LocalTime(0.1)).unwrap();
    assert!(sent(&mut c).is_empty());
    c.feed(
        &data_at(3, 2, &[fragment(3, 4, 1, 0, &[3, 0, 0, 0])]),
        endpoint(),
        LocalTime(0.6),
    )
    .unwrap();
    let out = sent(&mut c);
    let report = Packet::decode(&out[0]).unwrap();
    assert_eq!(
        (
            report.header.flags,
            report.header.time,
            report.header.sequence
        ),
        (0x1002, 1, 0)
    );
    assert_eq!(
        report.body,
        u32::try_from(first.len()).unwrap().to_le_bytes().as_slice()
    );
    assert!(report.generic_checksum_ok());
    // Later packets repeat the newest time byte.
    c.send(6, b"\x01\0\0\0", LocalTime(0.7)).unwrap();
    assert_eq!(Header::decode(&sent(&mut c)[0]).unwrap().time, 2);
}

#[test]
fn data_goes_out_from_sequence_two_with_the_flow_word_and_the_clients_blob_ids() {
    let mut c = connection();
    c.feed(&message(2), endpoint(), LocalTime(0.1)).unwrap();
    c.send(6, &[0xc8, 0xf7, 0, 0], LocalTime(0.2)).unwrap();
    let big = vec![7u8; 500];
    c.send(5, &big, LocalTime(0.2)).unwrap();
    let out = sent(&mut c);
    assert_eq!(out.len(), 3);
    let p = Packet::decode(&out[0]).unwrap();
    assert_eq!((p.header.sequence, p.header.flags), (2, 0x802));
    assert_eq!(&out[0][20..24], &40u32.to_le_bytes());
    let (f, _) = Fragment::from_bytes(&out[0][24..]).unwrap();
    assert_eq!(f.header.blob_id(), 0x8000_0000_0000_0001);
    assert_eq!(
        (f.header.queue_id, f.payload.as_slice()),
        (6, [0xc8, 0xf7, 0, 0].as_slice())
    );
    let sum = f.hash() ^ key(2);
    assert_eq!(
        p.header.checksum,
        p.header.checksum_base().wrapping_add(40).wrapping_add(sum)
    );
    let second = Header::decode(&out[1]).unwrap();
    let third = Header::decode(&out[2]).unwrap();
    assert_eq!((second.sequence, third.sequence), (3, 4));
    let (f, _) = Fragment::from_bytes(&out[2][24..]).unwrap();
    assert_eq!(
        (f.header.blob_id(), f.header.num_frags, f.header.blob_num),
        (0x8000_0000_0000_0002, 2, 1)
    );
    assert_eq!(
        c.send(4, &[1, 0, 0, 0], LocalTime(0.3)),
        Err(CodecError::Queue.into())
    );
}

#[test]
fn a_repeated_grant_is_acknowledged_again() {
    let mut c = connection();
    c.feed(&control(0, 0x202, &[]), endpoint(), LocalTime(0.1))
        .unwrap();
    let out = sent(&mut c);
    let ack = Packet::decode(&out[0]).unwrap();
    assert_eq!((ack.header.flags, ack.header.recipient), (0x202, RECIPIENT));
    assert_eq!(c.poll(), None);
}

#[test]
fn the_servers_disconnect_is_answered_and_closes_the_connection() {
    let mut c = connection();
    let notice = encode_generic(
        Header {
            flags: 0x2_0002,
            recipient: SERVER,
            ..Default::default()
        },
        &[],
    )
    .unwrap();
    c.feed(&notice, endpoint(), LocalTime(0.1)).unwrap();
    assert!(c.is_closed());
    assert_eq!(c.poll(), Some(Event::Closed));
}

#[test]
fn the_goodbye_is_the_log_off_then_the_reset() {
    let c = connection();
    let out: Vec<Vec<u8>> = c.goodbye().into_iter().map(|(b, _)| b).collect();
    assert_eq!(flags_of(&out), [0x42, 0x8002]);
    for p in &out {
        let p = Packet::decode(p).unwrap();
        assert_eq!((p.header.recipient, p.header.sequence), (RECIPIENT, 0));
        assert!(p.body.is_empty() && p.generic_checksum_ok());
    }
}

#[test]
fn packets_the_client_would_not_accept_commit_nothing() {
    let mut c = connection();
    let good = message(2);
    // No checksum bit: ac1999's bare acknowledgment form.
    let mut unflagged = control(3, 0x4002, &3u32.to_le_bytes());
    unflagged[4] = 0;
    unflagged[5] = 0x40;
    assert_eq!(
        c.feed(&unflagged, endpoint(), LocalTime(0.1)),
        Err(Error::Peer)
    );
    let mut table = good.clone();
    table[19] = 1;
    assert_eq!(c.feed(&table, endpoint(), LocalTime(0.1)), Err(Error::Peer));
    let other: SocketAddr = "127.0.0.1:9001".parse().unwrap();
    assert_eq!(c.feed(&good, other, LocalTime(0.1)), Err(Error::Peer));
    let mut corrupt = good.clone();
    corrupt[12] ^= 1;
    assert!(c.feed(&corrupt, endpoint(), LocalTime(0.1)).is_err());
    assert_eq!(
        c.feed(&message(2 + WINDOW + 1), endpoint(), LocalTime(0.1)),
        Err(Error::Sequence)
    );
    assert_eq!(c.poll(), None);
    // The key for sequence two was not consumed by the rejected copy.
    c.feed(&good, endpoint(), LocalTime(0.2)).unwrap();
    assert_eq!(messages(&mut c), [2]);
}

#[test]
fn fragments_concatenate_and_keep_queues_separate() {
    let mut c = connection();
    c.feed(
        &data(2, &[fragment(1, 7, 3, 2, b"tail")]),
        endpoint(),
        LocalTime(0.0),
    )
    .unwrap();
    c.feed(
        &data(4, &[fragment(1, 7, 3, 1, b"middle")]),
        endpoint(),
        LocalTime(0.1),
    )
    .unwrap();
    c.feed(
        &data(
            3,
            &[
                fragment(1, 7, 3, 0, &[0xb8, 0xf7, 0, 0]),
                fragment(2, 4, 1, 0, &[0x13, 0, 0, 0]),
            ],
        ),
        endpoint(),
        LocalTime(0.2),
    )
    .unwrap();
    let mut got = Vec::new();
    while let Some(e) = c.poll() {
        if let Event::Message(m) = e {
            got.push((m.queue, m.bytes));
        }
    }
    assert_eq!(got[0], (NetQueue::UiQueue, vec![0x13, 0, 0, 0]));
    assert_eq!(
        got[1],
        (NetQueue::ClientCache, b"\xb8\xf7\0\0middletail".to_vec())
    );
}

#[test]
fn silence_closes_the_connection_with_an_injected_clock() {
    let mut c = connection();
    c.tick(LocalTime(29.9)).unwrap();
    assert!(!c.is_closed());
    c.tick(LocalTime(30.0)).unwrap();
    assert!(c.is_closed());
    assert_eq!(c.tick(LocalTime(31.0)), Err(Error::Closed));
}

/// A server stream from sequence two, keying each packet in order as the server does.
struct Stream {
    keys: KeyStream,
    seq: u32,
}

impl Stream {
    fn new() -> Self {
        Self {
            keys: KeyStream::new(0),
            seq: 1,
        }
    }

    fn next(&mut self, fragments: &[Fragment]) -> Vec<u8> {
        self.seq += 1;
        let body: Vec<u8> = fragments.iter().flat_map(Fragment::to_bytes).collect();
        let hash = fragments
            .iter()
            .fold(0u32, |hash, f| hash.wrapping_add(f.hash()));
        let mut h = Header {
            sequence: self.seq,
            flags: 0x802,
            recipient: SERVER,
            time: 1,
            payload_len: u16::try_from(body.len()).unwrap(),
            ..Default::default()
        };
        h.checksum = h.checksum_base().wrapping_add(hash ^ self.keys.next());
        let mut packet = h.encode().to_vec();
        packet.extend(body);
        packet
    }
}

#[test]
fn a_dat_file_of_thousands_of_fragments_reassembles() {
    let mut c = connection();
    let mut stream = Stream::new();
    let file: Vec<u8> = (0..3_000_000u32)
        .map(|i| (i % 251).to_le_bytes()[0])
        .collect();
    let parts: Vec<&[u8]> = file.chunks(444).collect();
    let count = u16::try_from(parts.len()).unwrap();
    for (index, part) in parts.iter().enumerate() {
        let f = fragment(9, 7, count, u16::try_from(index).unwrap(), part);
        c.feed(&stream.next(&[f]), endpoint(), LocalTime(0.0))
            .unwrap();
    }
    assert_eq!(c.poll(), Some(Event::Connected));
    match c.poll() {
        Some(Event::Message(m)) => {
            assert_eq!(m.queue, NetQueue::ClientCache);
            assert!(m.bytes == file, "the file arrives whole and in order");
        }
        other => panic!("expected the file, got {other:?}"),
    }
}

#[test]
fn a_message_that_keeps_gaining_fragments_outlives_the_silence_limit() {
    let mut c = connection();
    let mut stream = Stream::new();
    let count = 80u16;
    for index in 0..count - 1 {
        let now = LocalTime(f64::from(index));
        c.feed(
            &stream.next(&[fragment(5, 7, count, index, &index.to_le_bytes())]),
            endpoint(),
            now,
        )
        .unwrap();
        c.tick(now).unwrap();
        sent(&mut c);
        assert!(!c.is_closed(), "closed after {index} seconds");
    }
    // A message that stops gaining fragments still ends the connection, even while the server
    // keeps it alive.
    for second in 79..=110u32 {
        let now = LocalTime(f64::from(second));
        if c.is_closed() {
            break;
        }
        c.feed(&control(0, 0x22, &[0; 4]), endpoint(), now).unwrap();
        c.tick(now).unwrap();
        sent(&mut c);
    }
    assert!(c.is_closed());
}
