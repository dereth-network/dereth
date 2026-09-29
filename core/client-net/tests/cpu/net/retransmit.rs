//! Over an in-process lossy link: no loss means no NAKs; drops, corruption and late arrivals are
//! NAKed and recovered without desyncing ISAAC; a NAK is a cumulative ack; RejectRetransmit forgets
//! only named ids, resets the 140 s clock, is refused with the encrypted bit, and an empty one is a
//! no-op.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_client_net::{Net, NetConfig};
use dereth_primitives::LocalTime;
use dereth_primitives::{NetQueue, Transport};
use dereth_transport::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};

/// A's slot in its own receiver table, and the id B stamps on packets addressed to A.
const A_REC: u16 = 1;
/// B's slot, and the id A stamps on packets addressed to B.
const B_REC: u16 = 2;
const SEED_A: u32 = 0xDEAD_BEEF;
const SEED_B: u32 = 0x1234_5678;

/// The link. `a` is the endpoint under test; `b` is its peer.
struct Link {
    a: Net,
    b: Net,
    /// Sequence ids of A→B **data** datagrams the link eats, consumed on first match.
    drop_seqs: Vec<u32>,
    /// Sequence ids of A→B data datagrams held back rather than dropped, and the bytes.
    hold_seqs: Vec<u32>,
    held: Vec<Vec<u8>>,
    /// Sequence ids of A→B data datagrams the link corrupts (their last byte flipped) on first
    /// transmission, consumed on first match: delivered, but failing the checksum.
    corrupt_seqs: Vec<u32>,
    /// Blob payloads B reassembled, in arrival order.
    b_received: Vec<Vec<u8>>,

    // Counters. Every assertion below is one of these, so that a test which stopped exercising its
    // subject fails rather than passing on a smaller number.
    /// A→B datagrams the link delivered.
    delivered: usize,
    /// A→B datagrams the link ate.
    dropped: usize,
    /// A→B datagrams carrying `header_ & Retransmission`.
    resends: usize,
    /// B→A datagrams carrying a `RequestRetransmit` section, and every id named across them.
    naks: usize,
    naked_ids: Vec<u32>,
    /// A→B datagrams carrying a `RejectRetransmit` section, and the ids in them.
    rejects: usize,
    rejected_ids: Vec<u32>,
    /// The sequence number of every A→B data datagram, in the order A built them.
    data_seqs: Vec<u32>,
}

impl Link {
    fn new() -> Self {
        let mut a = Net::new(NetConfig::default());
        // `add_connection(rec_id, net_id, iteration, outgoing_seed, incoming_seed, addr)`.
        // makes `crypto_incoming` from the *outgoing* seed and vice versa, so
        // the pair is mirrored: A's outgoing stream and B's incoming stream must share a seed.
        a.add_connection(A_REC, B_REC, 1, SEED_A, SEED_B, None);
        let mut b = Net::new(NetConfig::default());
        b.add_connection(B_REC, A_REC, 1, SEED_B, SEED_A, None);
        Self {
            a,
            b,
            drop_seqs: Vec::new(),
            hold_seqs: Vec::new(),
            held: Vec::new(),
            corrupt_seqs: Vec::new(),
            b_received: Vec::new(),
            delivered: 0,
            dropped: 0,
            resends: 0,
            naks: 0,
            naked_ids: Vec::new(),
            rejects: 0,
            rejected_ids: Vec::new(),
            data_seqs: Vec::new(),
        }
    }

    /// One frame of A: tick, then move whatever it produced across the link.
    fn tick_a(&mut self, t: f64) {
        self.a
            .tick(LocalTime(t), std::time::Duration::from_millis(50));
        for (dg, _) in self.a.take_outgoing() {
            let p = ParsedPacket::parse(&dg).expect("A emitted an unparseable datagram");
            if let Some(body) = p.optional.get(&PacketFlags::REJECT_RETRANSMIT) {
                self.rejects += 1;
                self.rejected_ids
                    .extend(dereth_transport::wire::optional::seq_ids(body));
            }
            let seq = p.header.seq_id;
            if p.header.header.has_fragments() {
                self.data_seqs.push(seq);
                if let Some(i) = self.drop_seqs.iter().position(|s| *s == seq) {
                    self.drop_seqs.remove(i);
                    self.dropped += 1;
                    continue;
                }
                if let Some(i) = self.hold_seqs.iter().position(|s| *s == seq) {
                    self.hold_seqs.remove(i);
                    self.held.push(dg);
                    continue;
                }
                if let Some(i) = self.corrupt_seqs.iter().position(|s| *s == seq) {
                    self.corrupt_seqs.remove(i);
                    let mut bad = dg;
                    if let Some(last) = bad.last_mut() {
                        *last ^= 0xFF;
                    }
                    self.delivered += 1;
                    self.deliver_to_b(&bad);
                    continue;
                }
            }
            if p.header.header.contains(PacketFlags::RETRANSMISSION) {
                self.resends += 1;
            }
            self.delivered += 1;
            self.deliver_to_b(&dg);
        }
    }

    /// Release every held datagram — the out-of-order arrival.
    fn release_held(&mut self) {
        for dg in std::mem::take(&mut self.held) {
            self.delivered += 1;
            self.deliver_to_b(&dg);
        }
    }

    fn deliver_to_b(&mut self, dg: &[u8]) {
        // A rejected datagram is not an error here: a duplicate arriving after its retransmission
        // is exactly what `RejectReason::DuplicateSequence` is for.
        let _ = self.b.feed(dg, None, LocalTime(0.0));
        while let Some(m) = self.b.poll() {
            let mut payload = m.opcode.to_le_bytes().to_vec();
            payload.extend_from_slice(&m.body);
            self.b_received.push(payload);
        }
    }

    /// One frame of B, delivering everything it produces to A.
    fn tick_b(&mut self, t: f64) {
        self.b
            .tick(LocalTime(t), std::time::Duration::from_millis(50));
        for (dg, _) in self.b.take_outgoing() {
            let p = ParsedPacket::parse(&dg).expect("B emitted an unparseable datagram");
            if let Some(body) = p.optional.get(&PacketFlags::REQUEST_RETRANSMIT) {
                self.naks += 1;
                self.naked_ids
                    .extend(dereth_transport::wire::optional::seq_ids(body));
            }
            let _ = self.a.feed(&dg, None, LocalTime(t));
        }
    }

    /// `n` blobs, one datagram each, 0.1 s apart from `t0`. One `send` plus one `tick_a` is one
    /// datagram: `coalesce_data` packs everything queued at tick time into as few packets as it
    /// can, so anything sent together would travel together and could not be lost separately.
    ///
    /// Returns the sequence ids A stamped on them, which is what the drop lists name.
    fn burst(&mut self, n: usize, t0: f64) -> Vec<u32> {
        let before = self.data_seqs.len();
        for i in 0..n {
            self.a.send(NetQueue::UiQueue, false, &opcode(i));
            self.tick_a(t0 + 0.1 * (i as f64));
        }
        assert_eq!(
            self.data_seqs.len() - before,
            n,
            "the burst must be one datagram per blob, or the drop list means nothing"
        );
        self.data_seqs[before..].to_vec()
    }
}

/// An eight-byte blob body: a four-byte opcode, which `Net::poll` lifts out, and a payload.
fn opcode(i: usize) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[..4].copy_from_slice(&(0xF700u32 + u32::try_from(i).unwrap_or(0)).to_le_bytes());
    b[4..].copy_from_slice(&u32::try_from(i).unwrap_or(0).to_le_bytes());
    b
}

/// A datagram carrying one hand-built optional header and nothing else.
///
/// Disposable sections do not set `NEEDS_ENCRYPTION`, so this serialises in the clear with
/// `seq_id = 0` — the shape builds.
fn control(to_rec_id: u16, mask: u32, ids: &[u32]) -> Vec<u8> {
    let mut body = u32::try_from(ids.len()).unwrap_or(0).to_le_bytes().to_vec();
    for id in ids {
        body.extend_from_slice(&id.to_le_bytes());
    }
    let mut p = OutPacket::new(ProtoHeader {
        rec_id: to_rec_id,
        iteration: 1,
        ..Default::default()
    });
    p.add_optional_header(mask, body).expect("section");
    p.serialize(None)
        .expect("a disposable section needs no key")
}

// -------------------------------------------------------------------------------------------
// Calibration
// -------------------------------------------------------------------------------------------

/// **The known-negative.** The identical driver with an empty drop list must report zero on every
/// counter, so the non-zero counts in every other test below are attributable to the injected loss
/// and not to the harness. §7.8: an instrument that reports a one is worth nothing until it has
/// been shown reporting a zero on something known to be zero, and the other way round.
#[test]
fn no_loss_produces_no_naks_and_no_resends() {
    let mut link = Link::new();
    let seqs = link.burst(6, 0.1);
    assert_eq!(
        seqs,
        vec![2, 3, 4, 5, 6, 7],
        "`highest_id_sent` starts at 1 and pre-increments, so the first data packet is 2"
    );
    for i in 0..6 {
        link.tick_b(1.0 + 0.7 * f64::from(i));
        link.tick_a(1.05 + 0.7 * f64::from(i));
    }
    assert_eq!(link.dropped, 0, "nothing was asked to be dropped");
    assert_eq!(link.naks, 0, "a lossless link must produce no NAK");
    assert_eq!(link.resends, 0, "and therefore no resend");
    assert_eq!(link.rejects, 0);
    assert_eq!(link.b_received.len(), 6, "every blob arrived once");
}

/// Behaviour: link.retransmit.a-lost-datagram-is-naked-and-resent
/// **Scenario 1: a single dropped datagram.**
///
/// The peer notices the gap, NAKs it once, and the client puts the cached packet back on the wire
/// with the `Retransmission` bit set and the original sequence number.
///
/// Oracle: → →
#[test]
fn a_single_dropped_datagram_is_naked_and_resent() {
    let mut link = Link::new();
    link.drop_seqs = vec![3];
    let seqs = link.burst(3, 0.1);
    assert_eq!(seqs, vec![2, 3, 4]);
    assert_eq!(link.dropped, 1);
    assert_eq!(link.b_received.len(), 2, "the peer is one blob short");

    // The peer is missing exactly one sequence and says so.
    link.tick_b(1.0);
    assert_eq!(link.naks, 1, "one NAK burst");
    assert_eq!(link.naked_ids, vec![3], "naming the lost sequence");

    // And the client answers it.
    link.tick_a(1.1);
    assert_eq!(link.resends, 1, "a non-zero resend count");
    assert_eq!(
        link.b_received.len(),
        3,
        "all three blobs reached the peer, the lost one by retransmission"
    );
    assert_eq!(
        link.b_received[2],
        opcode(1).to_vec(),
        "and it is the right one"
    );

    // A second NAK cycle finds nothing left to ask for.
    link.tick_b(2.0);
    link.tick_a(2.1);
    assert_eq!(
        link.naks, 1,
        "the peer stopped asking once it was satisfied"
    );
    assert_eq!(link.resends, 1);
}

/// **Scenario 2: a run of drops.**
///
/// Four consecutive datagrams vanish. The NAK builder walks the whole set in
/// order, so they come back as **one** section naming four ids, and `transmit_acks` drains the whole
/// ack list in one pass rather than one id per tick.
#[test]
fn a_run_of_drops_is_naked_in_one_burst_and_wholly_recovered() {
    let mut link = Link::new();
    link.drop_seqs = vec![4, 5, 6, 7];
    let seqs = link.burst(8, 0.1);
    assert_eq!(seqs, vec![2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(link.dropped, 4);
    assert_eq!(link.b_received.len(), 4);

    link.tick_b(1.0);
    assert_eq!(link.naks, 1, "one burst, not one per lost id");
    assert_eq!(link.naked_ids, vec![4, 5, 6, 7], "in ascending order");

    link.tick_a(1.1);
    assert_eq!(link.resends, 4, "all four resent in one pass");
    assert_eq!(link.b_received.len(), 8, "every blob arrived exactly once");
}

/// A corrupted datagram is naked under its own key and recovered.
#[test]
fn a_corrupted_datagram_is_naked_under_its_own_key_and_recovered() {
    let mut link = Link::new();
    link.corrupt_seqs = vec![3];
    let seqs = link.burst(4, 0.1);
    assert_eq!(seqs, vec![2, 3, 4, 5]);
    assert_eq!(
        link.b_received.len(),
        3,
        "the packets after the corrupt one still decrypt"
    );

    link.tick_b(1.0);
    assert_eq!(link.naks, 1);
    assert_eq!(link.naked_ids, vec![3], "the corrupt sequence is asked for");

    link.tick_a(1.1);
    assert_eq!(link.resends, 1);
    assert_eq!(
        link.b_received.len(),
        4,
        "and its resend decrypts under the parked key"
    );
    assert_eq!(link.b_received[3], opcode(1).to_vec());

    link.tick_b(2.0);
    link.tick_a(2.1);
    assert_eq!(link.naks, 1, "nothing left to ask for");
}

/// **Scenario 3: an out-of-order arrival.**
///
/// The datagram was never lost, only late. It arrives after the peer has already NAKed it and after
/// the client has already resent it, so the peer sees the original **and** the retransmission. The
/// second must be dropped as a duplicate **without drawing an ISAAC value** — that is
/// the duplicate-suppression branch, and getting it wrong
/// desynchronises the key stream permanently rather than losing one packet.
///
/// So the assertion that matters is not "the blob arrived" but "**the next** blob arrived": a
/// desynchronised stream fails the checksum on everything after it.
#[test]
fn a_late_arrival_and_its_retransmission_do_not_desynchronise_the_key_stream() {
    let mut link = Link::new();
    link.hold_seqs = vec![3];
    link.burst(3, 0.1);
    assert_eq!(link.held.len(), 1, "one datagram is in flight, not lost");
    assert_eq!(link.dropped, 0, "nothing was lost in this scenario");

    link.tick_b(1.0);
    assert_eq!(link.naks, 1, "the peer cannot tell late from lost");
    assert_eq!(link.naked_ids, vec![3]);

    // The original turns up first, and then the retransmission of the same sequence.
    link.release_held();
    assert_eq!(
        link.b_received.len(),
        3,
        "the late one is accepted under its parked key"
    );
    link.tick_a(1.1);
    assert_eq!(link.resends, 1);
    assert_eq!(
        link.b_received.len(),
        3,
        "and the retransmission is a duplicate, not a second delivery"
    );

    // The stream is still aligned, which only the next encrypted packet can show.
    link.a.send(NetQueue::UiQueue, false, &opcode(99));
    link.tick_a(1.2);
    assert_eq!(
        link.b_received.len(),
        4,
        "a packet after the duplicate still decrypts -- the key stream kept its position"
    );
    assert_eq!(link.b_received[3], opcode(99).to_vec());
}

/// **Scenario 4: a NAK for something the cumulative ACK already retired.**
///
/// The receive path records the peer's `AckSequence`, then applies it through the recipient update
/// after emptying the flow queue. Once a sequence has left
/// `SentPacketHistory` there is nothing to resend, so `enqueue_acks` routes it to the empty-ack list and
/// it comes back as `RejectRetransmit` — "stop asking, it is gone" — rather than as silence.
///
/// The `AckSequence` is genuine peer traffic. The trailing NAK is hand-built, because a peer that
/// has just acknowledged a sequence has no reason to ask for it in the same breath; what this
/// models is the ordinary reordering case, a NAK queued before the ACK and delivered after it.
#[test]
fn a_nak_for_a_sequence_the_cumulative_ack_retired_is_rejected_not_resent() {
    let mut link = Link::new();
    let seqs = link.burst(3, 0.1);
    assert_eq!(link.b_received.len(), 3);

    // The peer acknowledges everything, and the client retires its cache on the next tick.
    link.tick_b(2.5);
    link.tick_a(2.6);

    // Now the stale NAK lands.
    let nak = control(A_REC, PacketFlags::REQUEST_RETRANSMIT, &[seqs[1]]);
    link.a
        .feed(&nak, None, LocalTime(2.7))
        .expect("the NAK is accepted");
    link.tick_a(2.8);

    assert_eq!(
        link.resends, 0,
        "there is nothing left in the cache to resend"
    );
    assert_eq!(
        link.rejects, 1,
        "and the client says so rather than going quiet"
    );
    assert_eq!(link.rejected_ids, vec![seqs[1]]);
}

/// **Scenario 4b: the flush is deferred by a whole tick, and that is load-bearing.**
///
/// One datagram carrying `AckSequence(N)` **and** `RequestRetransmit(M)` with `M < N`. The client
/// must still resend M: the recipient's per-frame step empties the flow queue before it flushes
/// the cumulative ACK. The flow queue's ACK transmitter runs at that point and only *then* applies
/// `flush_num`. A rebuild that flushed inside
/// the receive path would drop M first and answer a perfectly recoverable NAK with
/// `RejectRetransmit`, which is a silent, permanent data loss on a lossy link.
#[test]
fn an_ack_and_a_nak_in_one_datagram_still_resend_because_the_flush_is_deferred() {
    let mut link = Link::new();
    let seqs = link.burst(3, 0.1);

    let mut p = OutPacket::new(ProtoHeader {
        rec_id: A_REC,
        iteration: 1,
        ..Default::default()
    });
    p.add_optional_header(PacketFlags::ACK_SEQUENCE, seqs[2].to_le_bytes().to_vec())
        .expect("ack");
    let mut nak = 1u32.to_le_bytes().to_vec();
    nak.extend_from_slice(&seqs[1].to_le_bytes());
    p.add_optional_header(PacketFlags::REQUEST_RETRANSMIT, nak)
        .expect("nak");
    let bytes = p.serialize(None).expect("clear");
    link.a.feed(&bytes, None, LocalTime(1.0)).expect("accepted");

    link.tick_a(1.1);
    assert_eq!(
        link.resends, 1,
        "the NAK was answered before the ACK retired it"
    );
    assert_eq!(link.rejects, 0);

    // And the flush did happen -- a second copy of the same NAK now finds nothing.
    let again = control(A_REC, PacketFlags::REQUEST_RETRANSMIT, &[seqs[1]]);
    link.a.feed(&again, None, LocalTime(1.2)).expect("accepted");
    link.tick_a(1.3);
    assert_eq!(
        link.resends, 1,
        "the cache was retired on the previous tick"
    );
    assert_eq!(link.rejects, 1, "so the second ask is rejected");
    assert_eq!(link.rejected_ids, vec![seqs[1]]);
}

/// A nak is a cumulative ack of everything below its first id.
#[test]
fn a_nak_is_a_cumulative_ack_of_everything_below_its_first_id() {
    let mut link = Link::new();
    let seqs = link.burst(5, 0.1);
    assert_eq!(seqs, vec![2, 3, 4, 5, 6]);
    assert_eq!(
        link.b_received.len(),
        5,
        "nothing was lost; the cache holds all five"
    );

    // "I am missing 5" — which also says 2, 3 and 4 arrived.
    let nak = control(A_REC, PacketFlags::REQUEST_RETRANSMIT, &[seqs[3]]);
    link.a.feed(&nak, None, LocalTime(1.0)).expect("accepted");
    link.tick_a(1.1);
    assert_eq!(link.resends, 1, "5 is still cached and goes back out");
    assert_eq!(link.rejects, 0);

    // And now 3 is gone, because the NAK for 5 retired it.
    let stale = control(A_REC, PacketFlags::REQUEST_RETRANSMIT, &[seqs[1]]);
    link.a.feed(&stale, None, LocalTime(1.2)).expect("accepted");
    link.tick_a(1.3);
    assert_eq!(link.resends, 1, "nothing more to resend");
    assert_eq!(
        link.rejects, 1,
        "the implicit ACK retired it, so it is rejected"
    );
    assert_eq!(link.rejected_ids, vec![seqs[1]]);
}

/// Behaviour: link.retransmit.a-reject-retransmit-ends-the-retry-and-keeps-the-key-stream
/// A reject retransmit ends the peers retry loop.
#[test]
fn a_reject_retransmit_ends_the_peers_retry_loop() {
    let mut link = Link::new();
    link.drop_seqs = vec![3];
    link.burst(3, 0.1);
    assert_eq!(link.dropped, 1);

    // The retry loop, running. The NAK cadence is 0.6 s, so three ticks a second apart give three
    // bursts -- and it would give three thousand, because nothing else stops it.
    for i in 0..3 {
        link.tick_b(1.0 + f64::from(i));
        link.a.take_outgoing(); // the client's answer is not the subject here
    }
    assert_eq!(link.naks, 3, "the peer re-asks on every NAK cadence");
    assert_eq!(link.naked_ids, vec![3, 3, 3]);

    // "It is gone." One datagram, and the loop must stop.
    let reject = control(B_REC, PacketFlags::REJECT_RETRANSMIT, &[3]);
    link.b
        .feed(&reject, None, LocalTime(4.0))
        .expect("accepted");

    let before = link.naks;
    for i in 0..4 {
        link.tick_b(5.0 + f64::from(i));
    }
    assert_eq!(
        link.naks, before,
        "after RejectRetransmit the peer must never ask for that sequence again"
    );
}

/// B's own receiver -- the end of the link that has the gap.
fn b_receiver(link: &Link) -> &dereth_transport::session::ReceiverData {
    link.b
        .receiver(dereth_client_net::RecipientId(B_REC))
        .expect("B has a connection")
}

/// **Only the ids it names.** Two sequences are lost; the server can still produce one of them
/// and rejects the other. The retry loop must stop for the rejected id and keep running for the
/// other, because the empty-ack handler's loop removes exactly the listed ids and nothing wider.
///
/// A rebuild that answered a reject by clearing the NAKed-id set, or by treating it as
/// "everything below this is gone" the way the NAK processing's tail treats a NAK, would pass
/// [`a_reject_retransmit_ends_the_peers_retry_loop`] and fail here.
#[test]
fn a_reject_retransmit_forgets_only_the_ids_it_names() {
    let mut link = Link::new();
    link.drop_seqs = vec![3, 5];
    link.burst(5, 0.1);
    assert_eq!(link.dropped, 2);

    link.tick_b(1.0);
    link.a.take_outgoing();
    assert_eq!(
        link.naked_ids,
        vec![3, 5],
        "both gaps are asked for, in order"
    );
    assert_eq!(b_receiver(&link).get_naks(), vec![3, 5]);

    // "3 is gone." 5 is not mentioned.
    let reject = control(B_REC, PacketFlags::REJECT_RETRANSMIT, &[3]);
    link.b
        .feed(&reject, None, LocalTime(2.0))
        .expect("accepted");

    let r = b_receiver(&link);
    assert_eq!(
        r.get_naks(),
        vec![5],
        "the reject took more than the id it named"
    );
    assert_eq!(
        r.nak_state,
        dereth_transport::session::ReceiverState::Nak,
        "the set is not empty, so the connection walk leaves it in NAK_STATE"
    );

    // And the next cadence asks for 5 alone, which is correct: nobody has said 5 is gone.
    let before = link.naks;
    link.tick_b(3.0);
    link.a.take_outgoing();
    assert_eq!(
        link.naks,
        before + 1,
        "the peer must keep asking for the id nobody rejected"
    );
    assert_eq!(link.naked_ids, vec![3, 5, 5]);
}

/// **Nothing advances, and the key stream keeps its place.**
///
/// the resend only reaches for `seq_id != 0`, and a
/// `RejectRetransmit` is a `seq_id == 0` control datagram, so `highest_id_received` is untouched.
/// That is load-bearing rather than incidental: the ISAAC key for each NAKed sequence was already
/// drawn by when the gap was noticed, so forgetting the id without
/// consuming anything is what leaves the stream aligned. The assertion that shows it is not the
/// counter but **the next encrypted packet still decrypting**.
#[test]
fn a_reject_retransmit_advances_nothing_and_keeps_the_key_stream_aligned() {
    let mut link = Link::new();
    link.drop_seqs = vec![3];
    let seqs = link.burst(4, 0.1);
    assert_eq!(seqs, vec![2, 3, 4, 5]);
    assert_eq!(link.b_received.len(), 3, "the peer is one blob short");
    let highest = b_receiver(&link).window.highest_id_received;
    assert_eq!(highest, 5, "the newest sequence accepted, gap and all");

    link.tick_b(1.0);
    link.a.take_outgoing();

    let reject = control(B_REC, PacketFlags::REJECT_RETRANSMIT, &[3]);
    link.b
        .feed(&reject, None, LocalTime(2.0))
        .expect("accepted");
    assert_eq!(
        b_receiver(&link).window.highest_id_received,
        highest,
        "the empty-ack handler wrote highest_id_received; it only removes ids from the NAK set"
    );
    assert!(b_receiver(&link).window.seq_ids_we_naked.is_empty());

    // The proof the stream never lost its place: two more encrypted datagrams arrive and are
    // delivered. A consumed key would fail the checksum on everything after the reject.
    link.a.send(NetQueue::UiQueue, false, &opcode(90));
    link.tick_a(2.1);
    link.a.send(NetQueue::UiQueue, false, &opcode(91));
    link.tick_a(2.2);
    assert_eq!(
        link.b_received.len(),
        5,
        "a packet after the RejectRetransmit failed to decrypt -- the key stream was consumed"
    );
    assert_eq!(link.b_received[3], opcode(90).to_vec());
    assert_eq!(link.b_received[4], opcode(91).to_vec());
    // The blob in the rejected sequence is gone for good, and nothing pretends otherwise.
    assert_eq!(
        link.b_received
            .iter()
            .filter(|b| *b == &opcode(1).to_vec())
            .count(),
        0,
        "the rejected blob must not appear from anywhere"
    );
}

/// **Two arms.** Once the reject empties the NAKed-id set the
/// empty-set test sends the connection down the ack side,
/// so the burst the peer had been sending every 0.6 s is replaced by the ordinary 2 s
/// `AckSequence` -- and that ack carries the highest received id, the value the reject did not move.
#[test]
fn a_reject_retransmit_that_empties_the_set_turns_the_nak_burst_into_an_ack() {
    let mut link = Link::new();
    link.drop_seqs = vec![3];
    link.burst(3, 0.1);

    link.tick_b(1.0);
    link.a.take_outgoing();
    assert_eq!(link.naks, 1);

    let reject = control(B_REC, PacketFlags::REJECT_RETRANSMIT, &[3]);
    link.b
        .feed(&reject, None, LocalTime(2.0))
        .expect("accepted");
    assert_eq!(
        b_receiver(&link).nak_state,
        dereth_transport::session::ReceiverState::NoNak
    );

    // Drive B past the 2 s ACK cadence and read what it puts on the wire.
    let mut acks: Vec<u32> = Vec::new();
    for i in 0..4 {
        let t = 3.0 + f64::from(i);
        link.b
            .tick(LocalTime(t), std::time::Duration::from_millis(50));
        for (dg, _) in link.b.take_outgoing() {
            let p = ParsedPacket::parse(&dg).expect("B emitted an unparseable datagram");
            assert!(
                !p.optional.contains_key(&PacketFlags::REQUEST_RETRANSMIT),
                "the peer asked again after the RejectRetransmit"
            );
            if let Some(body) = p.optional.get(&PacketFlags::ACK_SEQUENCE) {
                acks.push(u32::from_le_bytes([body[0], body[1], body[2], body[3]]));
            }
        }
    }
    assert!(
        !acks.is_empty(),
        "the cumulative-ack enqueue never ran: the NAK set is empty, so the cadence owes an AckSequence"
    );
    assert!(
        acks.iter().all(|a| *a == 4),
        "the ack must carry highest_id_received, which the reject left at 4: {acks:?}"
    );
}

/// **The link does not die -- it lives longer.** The accepted-packet tail writes the current time
/// to the receiver's last-data timestamp for every packet that reaches it,
/// a `RejectRetransmit`-only control datagram included, and that field is the **only** input to
/// the 140-second timeout. The empty-ack handler does not tear
/// anything down.
#[test]
fn a_reject_retransmit_keeps_the_link_alive_and_resets_its_140_second_clock() {
    let mut link = Link::new();
    link.drop_seqs = vec![3];
    link.burst(3, 0.1);
    link.tick_b(1.0);
    link.a.take_outgoing();

    let before = b_receiver(&link).local_time_last_got_data;
    let reject = control(B_REC, PacketFlags::REJECT_RETRANSMIT, &[3]);
    link.b
        .feed(&reject, None, LocalTime(60.0))
        .expect("accepted");

    assert_eq!(
        b_receiver(&link).local_time_last_got_data,
        LocalTime(60.0),
        "the reject did not refresh the 140 s clock: it was {before:?}"
    );
    assert_eq!(
        link.b
            .connection_state(dereth_client_net::RecipientId(B_REC)),
        dereth_transport::conn::ConnectionState::Connected,
        "the empty-ack handler disconnected something; its bytes are AVL::Remove and ret 8"
    );
    // And the connection is still usable in both directions.
    link.a.send(NetQueue::UiQueue, false, &opcode(71));
    link.tick_a(60.2);
    assert_eq!(link.b_received.last(), Some(&opcode(71).to_vec()));
}

/// **A reject that claims to be encrypted never reaches the arm.** Retail guards that twice, and
/// both guards are ahead of optional-header processing:
///
/// * the encrypted-iff rule -- header bit 1 must equal the local needs-encryption bit --
///   which a `RejectRetransmit` fails on its own, because its section is **disposable** (flag `1`)
///   and therefore sets no `NEEDS_ENCRYPTION`;
/// * the sequence-0 split refusing a packet with header bit 2 set, which catches the same shape
///   carrying a non-disposable section.
///
/// This build applies the first of those inside [`ParsedPacket::parse`], one gate earlier than
/// retail applies the second, so the reason differs and the outcome does not: the datagram is
/// thrown away whole and the NAKed-id set is untouched. A rebuild that read the optional headers
/// first would let a peer clear the set with a packet the client never authenticated.
#[test]
fn a_reject_retransmit_carrying_the_encrypted_bit_is_refused_before_the_arm() {
    let mut link = Link::new();
    link.drop_seqs = vec![3];
    link.burst(3, 0.1);
    link.tick_b(1.0);
    link.a.take_outgoing();
    assert_eq!(b_receiver(&link).get_naks(), vec![3]);

    // `OutPacket::serialize` rebuilds `header_` from the sections it carries (a disposable
    // section sets no `NEEDS_ENCRYPTION`), so the bit is set on the wire bytes afterwards --
    // which is also the only way a peer could send this shape at all.
    let mut bytes = control(B_REC, PacketFlags::REJECT_RETRANSMIT, &[3]);
    let flags = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    bytes[4..8].copy_from_slice(&(flags | PacketFlags::ENCRYPTED_CHECKSUM).to_le_bytes());
    assert!(
        link.b.feed(&bytes, None, LocalTime(2.0)).is_err(),
        "an unsequenced packet carrying EncryptedChecksum must be refused before the arm"
    );
    assert_eq!(
        b_receiver(&link).get_naks(),
        vec![3],
        "a refused packet reached the empty-ack handler anyway"
    );
}

/// **A zero count.** A section whose count is zero names nothing and the
/// loop is not entered: a reject is never an instruction to forget anything but the ids it
/// carries.
#[test]
fn a_reject_retransmit_naming_nothing_is_a_no_op() {
    let mut link = Link::new();
    link.drop_seqs = vec![3];
    link.burst(3, 0.1);
    link.tick_b(1.0);
    link.a.take_outgoing();

    let reject = control(B_REC, PacketFlags::REJECT_RETRANSMIT, &[]);
    link.b
        .feed(&reject, None, LocalTime(2.0))
        .expect("accepted");
    assert_eq!(
        b_receiver(&link).get_naks(),
        vec![3],
        "an empty reject emptied the NAK set"
    );
    assert_eq!(
        b_receiver(&link).nak_state,
        dereth_transport::session::ReceiverState::Nak
    );
}
