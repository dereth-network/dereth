//! A server's large messages over a virtual link, on a virtual clock: the pace they go at, the
//! window that keeps them recoverable, and the other queues they do not hold up.

use std::collections::BTreeMap;

use dereth_transport::flow::NAK_INTERVAL;
use dereth_transport::session::{MAX_NAK_IDS, SENT_CACHE_MAX_AGE, SENT_CACHE_MAX_PACKETS};
use dereth_transport::wire::{PacketFlags, ParsedPacket, ProtoHeader, MAX_DATAGRAM_OUT};
use yalain_net::endpoint::Endpoint;
use yalain_net::pace::{
    check_rate, Pace, BYTES_PER_INTERVAL, MAX_BYTES_PER_SECOND, MIN_BYTES_PER_SECOND, WINDOW,
};
use yalain_net::LocalTime;

use super::pair;

/// The data download's queue.
const DATABASE: u16 = 5;

/// The length of the largest message the January 2013 servers sent: the script library, seven
/// megabytes compressed, 16,105 fragments.
const LIBRARY: usize = 7_214_793;

/// The virtual clock's step: the server's loop runs far more often, the client's once a frame.
const STEP: f64 = 0.01;

/// A small deterministic generator for the link's losses and the messages' bytes.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// True for a share `p` of the calls.
    fn chance(&mut self, p: f64) -> bool {
        #[allow(clippy::cast_precision_loss)]
        let x = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        x < p
    }

    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next().to_le_bytes()[0]).collect()
    }
}

/// The link between the two ends: it loses a share of the datagrams each way, holds at most
/// `buffer` of the server's datagrams in a step (the client's socket: the rest of a burst is lost),
/// and between `outage.0` and `outage.1` seconds loses everything.
struct Link {
    loss: f64,
    buffer: usize,
    outage: (f64, f64),
    rng: Rng,
}

impl Link {
    fn passes(&mut self, now: f64) -> bool {
        !(now >= self.outage.0 && now < self.outage.1) && !self.rng.chance(self.loss)
    }
}

/// What a run saw.
#[derive(Debug, Default)]
struct Run {
    /// The messages the client took, by queue, in the order they completed, with the time.
    got: BTreeMap<u16, Vec<(f64, Vec<u8>)>>,
    /// Bytes of first sends (not resends) the server put out in each of its intervals.
    new_bytes: BTreeMap<u16, usize>,
    /// Packets the server resent, and the most in one step.
    resent: u64,
    most_resent: u64,
    /// Retransmit requests the server took that its next step answered with nothing.
    unanswered_requests: u32,
    /// The server said a packet was gone for good.
    rejected: bool,
    /// The most sequenced packets the server had out unacknowledged.
    most_out: u32,
    /// The most sequenced packets out while the link was down.
    out_in_outage: u32,
    /// After the link was down, the server sent more of the message while the client still
    /// lacked some of what it had sent.
    went_on_with_holes: bool,
}

/// Run both ends until the client holds every message in `sent` (queue, bytes) or `limit`
/// virtual seconds pass; the server queues each message at the start.
fn run(sent: &[(u16, Vec<u8>)], link: Link, limit: f64) -> (Run, Endpoint) {
    run_at(Pace::default(), sent, link, limit)
}

/// [`run`], the server's large messages at `pace`.
fn run_at(pace: Pace, sent: &[(u16, Vec<u8>)], link: Link, limit: f64) -> (Run, Endpoint) {
    let kinds = vec![0x23; sent.len()];
    run_kinds(pace, sent, &kinds, link, limit)
}

/// [`run_at`], each message of this ordering kind.
fn run_kinds(
    pace: Pace,
    sent: &[(u16, Vec<u8>)],
    kinds: &[u8],
    mut link: Link,
    limit: f64,
) -> (Run, Endpoint) {
    let mut now = LocalTime(1.0);
    let (mut server, mut client) = pair(now);
    server.pace = pace;
    for (i, (queue, m)) in sent.iter().enumerate() {
        let id = u64::from(kinds[i]) << 56 | 0x00F1_0000_0000_0000 | i as u64;
        server.send_blob(*queue, id, m.clone());
    }
    let mut r = Run::default();
    let mut asked = false;
    while now.0 < limit && r.got.values().map(Vec::len).sum::<usize>() < sent.len() {
        let out = server.tick(now);
        let mut resends = 0;
        let mut first_sends = false;
        for dg in &out {
            let h = ProtoHeader::from_bytes(dg).expect("a header");
            if h.header.contains(PacketFlags::RETRANSMISSION) {
                resends += 1;
            } else if h.header.has_fragments() {
                *r.new_bytes.entry(h.interval).or_default() += dg.len();
                first_sends = true;
            }
            if h.header.contains(PacketFlags::REJECT_RETRANSMIT) {
                r.rejected = true;
            }
        }
        if asked && resends == 0 {
            r.unanswered_requests += 1;
        }
        r.resent += resends;
        r.most_resent = r.most_resent.max(resends);
        if link.outage.1 > 0.0 && now.0 >= link.outage.1 && first_sends {
            r.went_on_with_holes |= !client.recv.get_naks().is_empty();
        }
        let out_now = server.flow.highest_id_sent.wrapping_sub(server.peer_acked);
        r.most_out = r.most_out.max(out_now);
        if now.0 >= link.outage.0 && now.0 < link.outage.1 {
            r.out_in_outage = r.out_in_outage.max(out_now);
        }
        for dg in out.iter().take(link.buffer) {
            if !link.passes(now.0) {
                continue;
            }
            let p = ParsedPacket::parse(dg).expect("the client reads what the server sends");
            if let Ok(inc) = client.feed(&p, dg.len(), now) {
                for b in inc.blobs {
                    r.got
                        .entry(b.queue_id)
                        .or_default()
                        .push((now.0, b.payload));
                }
            }
        }
        asked = false;
        for dg in client.tick(now) {
            if !link.passes(now.0) {
                continue;
            }
            let p = ParsedPacket::parse_from_client(&dg)
                .expect("the server reads what the client sends");
            asked |= p.optional.contains_key(&PacketFlags::REQUEST_RETRANSMIT);
            let _ = server.feed(&p, dg.len(), now);
        }
        now = LocalTime(now.0 + STEP);
    }
    (r, server)
}

/// A server sends a seven-megabyte message, the largest the 2013 servers sent, over a link that
/// loses one datagram in twenty each way and whose client socket holds 400 datagrams at a time.
/// It arrives whole, at the 2013 servers' pace: no more than 32 KB of first sends in any
/// half-second interval, so about two minutes, and never the whole message at once (which this
/// socket would mostly lose, and the server could not resend). Every request to send again is
/// answered at once, and nothing is ever let go.
/// Behaviour: server.net.large-message-paced
#[test]
fn a_seven_megabyte_message_arrives_whole_over_a_lossy_link_at_the_2013_servers_pace() {
    let mut rng = Rng(0x5EED_1234_ABCD_0001);
    let library = rng.bytes(LIBRARY);
    let link = Link {
        loss: 0.05,
        buffer: 400,
        outage: (0.0, 0.0),
        rng: Rng(0x0123_4567_89AB_CDEF),
    };
    let (r, _) = run(&[(DATABASE, library.clone())], link, 600.0);
    let got = r.got.get(&DATABASE).expect("the message arrived");
    assert!(got[0].1 == library, "intact");
    let took = got[0].0 - 1.0;
    let most = r.new_bytes.values().copied().max().unwrap_or(0);
    assert!(
        most <= BYTES_PER_INTERVAL + MAX_DATAGRAM_OUT,
        "at most an interval's budget of first sends in any interval: {most}"
    );
    #[allow(clippy::cast_precision_loss)]
    let at_full_rate = LIBRARY as f64 / (2.0 * BYTES_PER_INTERVAL as f64);
    assert!(
        took > at_full_rate && took < at_full_rate * 1.5,
        "paced: {took:.1} s, against {at_full_rate:.1} s at the full rate"
    );
    assert!(r.resent > 0, "losses were resent");
    assert_eq!(r.unanswered_requests, 0, "every request answered at once");
    assert!(!r.rejected, "no packet let go");
}

/// The window, under the worst a link does: the link goes down for twenty seconds in the middle of
/// a seven-megabyte message. The server goes on sending until it has 1,140 packets out that the
/// client has not acknowledged, ten of the client's requests to send again, then waits; once the
/// link is back the client asks for the lost ones again, 114 at a time, each request answered
/// whole, and has them all well inside the 120 s a server keeps a packet, which it never lets go;
/// each request acknowledges what comes before its first packet, so the window moves on as the
/// holes fill, and the message arrives whole.
/// Behaviour: server.net.large-message-window
#[test]
fn a_large_message_never_has_more_out_than_the_client_can_ask_for_again_before_it_is_let_go() {
    let mut rng = Rng(0x5EED_1234_ABCD_0002);
    let library = rng.bytes(LIBRARY);
    let link = Link {
        loss: 0.02,
        buffer: 400,
        outage: (20.0, 40.0),
        rng: Rng(0x0FED_CBA9_8765_4321),
    };
    let (r, _) = run(&[(DATABASE, library.clone())], link, 900.0);
    let got = r.got.get(&DATABASE).expect("the message arrived");
    assert!(got[0].1 == library, "intact");
    // A few packets of the connection's own besides the large message's.
    assert!(
        r.most_out <= WINDOW + 8,
        "never more out than the window: {}",
        r.most_out
    );
    assert!(
        r.out_in_outage >= WINDOW,
        "the window filled while the link was down: {}",
        r.out_in_outage
    );
    let ask_again = f64::from(
        r.most_out
            .div_ceil(u32::try_from(MAX_NAK_IDS).expect("small")),
    ) * NAK_INTERVAL;
    assert!(
        ask_again < SENT_CACHE_MAX_AGE / 10.0,
        "everything out is asked for again in {ask_again:.1} s, well before {SENT_CACHE_MAX_AGE} s"
    );
    assert!(r.most_out as usize <= SENT_CACHE_MAX_PACKETS / 2);
    assert!(
        r.most_resent >= MAX_NAK_IDS as u64,
        "a whole request of 114 answered at once: {}",
        r.most_resent
    );
    assert!(
        r.went_on_with_holes,
        "a request to send again acknowledges what comes before it, so the window moves on \
         before every hole is filled"
    );
    assert!(!r.rejected, "no packet let go");
}

/// A small message on another queue, sent after a large one, is not held behind it; one on the
/// large message's own queue waits for it, so each queue keeps its order.
/// Behaviour: server.net.large-message-holds-only-its-queue
#[test]
fn a_large_message_holds_back_only_the_messages_on_its_own_queue() {
    let mut rng = Rng(0x5EED_1234_ABCD_0003);
    let large = rng.bytes(300_000);
    let (earlier, after_on_its_queue, other_queue) = (rng.bytes(40), rng.bytes(60), rng.bytes(80));
    let link = Link {
        loss: 0.0,
        buffer: usize::MAX,
        outage: (0.0, 0.0),
        rng: Rng(1),
    };
    let (r, server) = run(
        &[
            (DATABASE, earlier.clone()),
            (DATABASE, large.clone()),
            (DATABASE, after_on_its_queue.clone()),
            (1, other_queue.clone()),
        ],
        link,
        60.0,
    );
    let order: Vec<&Vec<u8>> = r.got[&DATABASE].iter().map(|(_, m)| m).collect();
    assert_eq!(
        order,
        [&earlier, &large, &after_on_its_queue],
        "its queue's order"
    );
    let large_at = r.got[&DATABASE][1].0;
    let other_at = r.got[&1][0].0;
    assert!(
        other_at < 1.1 && large_at > 3.0,
        "the other queue's message went at once ({other_at:.2} s), the large one at its pace \
         ({large_at:.2} s)"
    );
    assert!(server.pace.is_empty());
}

/// On one queue, a small message of another ordering kind goes past a large one at once, in the order
/// the rest of its kind has; one of the large message's own kind waits behind it.
/// Behaviour: server.net.large-message-holds-only-its-kind
#[test]
fn a_large_message_holds_back_only_the_messages_of_its_kind_on_its_queue() {
    const PRIVATE: u8 = 0x01;
    const VISUAL: u8 = 0x02;
    const UNORDERED: u8 = 0x03;
    const EPHEMERAL: u8 = 0x83;
    let mut rng = Rng(0x5EED_1234_ABCD_0007);
    let large = rng.bytes(300_000);
    let (own, seen, also, moved) = (rng.bytes(20), rng.bytes(30), rng.bytes(40), rng.bytes(50));
    let link = Link {
        loss: 0.0,
        buffer: usize::MAX,
        outage: (0.0, 0.0),
        rng: Rng(1),
    };
    let (r, server) = run_kinds(
        Pace::default(),
        &[
            (1, large.clone()),
            (1, own.clone()),
            (1, seen.clone()),
            (1, also.clone()),
            (1, moved.clone()),
        ],
        &[PRIVATE, PRIVATE, VISUAL, UNORDERED, EPHEMERAL],
        link,
        60.0,
    );
    let got = &r.got[&1];
    let at = |m: &Vec<u8>| got.iter().find(|(_, x)| x == m).expect("arrived").0;
    assert!(at(&large) > 3.0, "the large one at its pace");
    for other in [&seen, &also, &moved] {
        assert!(at(other) < 1.1, "{:.2} s", at(other));
    }
    assert!(at(&own) >= at(&large), "its own kind waits");
    assert!(server.pace.is_empty());
}

/// A host's highest rate: a seven-megabyte message over the same lossy link, with the link down
/// for twenty seconds on the way, arrives whole in a fraction of the two minutes. No interval
/// carries more than the rate's budget of first sends; however fast the server goes it never has
/// more out than the window, far below the 4,096 packets it keeps for resending; and nothing is
/// let go.
/// Behaviour: server.net.large-message-rate-setting
#[test]
fn a_seven_megabyte_message_at_the_highest_rate_arrives_whole_and_loses_nothing_it_sent() {
    let mut rng = Rng(0x5EED_1234_ABCD_0004);
    let library = rng.bytes(LIBRARY);
    let link = Link {
        loss: 0.05,
        buffer: 400,
        outage: (10.0, 30.0),
        rng: Rng(0x0123_4567_89AB_CDEF),
    };
    let pace = Pace::at_rate(MAX_BYTES_PER_SECOND);
    let (r, _) = run_at(pace, &[(DATABASE, library.clone())], link, 600.0);
    let got = r.got.get(&DATABASE).expect("the message arrived");
    assert!(got[0].1 == library, "intact");
    let took = got[0].0 - 1.0;
    #[allow(clippy::cast_precision_loss)]
    let at_full_rate = LIBRARY as f64 / f64::from(MAX_BYTES_PER_SECOND);
    assert!(
        took > at_full_rate && took < at_full_rate + 60.0,
        "{took:.1} s, against {at_full_rate:.1} s at the full rate and 20 s down"
    );
    let budget = usize::try_from(MAX_BYTES_PER_SECOND / 2).expect("small");
    let most = r.new_bytes.values().copied().max().unwrap_or(0);
    assert!(
        most <= budget + MAX_DATAGRAM_OUT,
        "at most an interval's budget of first sends in any interval: {most}"
    );
    assert!(
        most > 2 * BYTES_PER_INTERVAL,
        "well past the default rate's: {most}"
    );
    assert!(
        r.most_out <= WINDOW + 8,
        "never more out than the window: {}",
        r.most_out
    );
    assert!(r.most_out as usize <= SENT_CACHE_MAX_PACKETS / 2);
    assert!(!r.rejected, "no packet let go");
}

/// A host's lowest rate: a large message over a lossy link still arrives, at that rate.
/// Behaviour: server.net.large-message-rate-setting
#[test]
fn a_large_message_at_the_lowest_rate_still_arrives() {
    let mut rng = Rng(0x5EED_1234_ABCD_0005);
    let large = rng.bytes(100_000);
    let link = Link {
        loss: 0.05,
        buffer: 400,
        outage: (0.0, 0.0),
        rng: Rng(0x0123_4567_89AB_CDEF),
    };
    let pace = Pace::at_rate(MIN_BYTES_PER_SECOND);
    let (r, _) = run_at(pace, &[(DATABASE, large.clone())], link, 300.0);
    let got = r.got.get(&DATABASE).expect("the message arrived");
    assert!(got[0].1 == large, "intact");
    let took = got[0].0 - 1.0;
    #[allow(clippy::cast_precision_loss)]
    let at_full_rate = large.len() as f64 / f64::from(MIN_BYTES_PER_SECOND);
    assert!(
        took > at_full_rate * 0.8 && took < at_full_rate * 1.5,
        "{took:.1} s, against {at_full_rate:.1} s at the full rate"
    );
    assert!(!r.rejected, "no packet let go");
}

/// What counts as large does not change with the rate: at the highest, a message longer than the
/// 2013 servers' budget still goes by the pace and the window.
/// Behaviour: server.net.large-message-rate-setting
#[test]
fn a_higher_rate_lets_no_large_message_skip_the_pace() {
    let pace = Pace::at_rate(MAX_BYTES_PER_SECOND);
    assert!(pace.takes(DATABASE, 0, BYTES_PER_INTERVAL + 1));
    assert!(!pace.takes(DATABASE, 0, BYTES_PER_INTERVAL));
}

/// The rates a host may set: the 2013 servers' and the bounds, and nothing beyond them.
/// Behaviour: server.net.large-message-rate-setting
#[test]
fn a_rate_out_of_bounds_is_refused() {
    for rate in [MIN_BYTES_PER_SECOND, 65_536, MAX_BYTES_PER_SECOND] {
        assert_eq!(check_rate(rate), Ok(()));
    }
    for rate in [0, MIN_BYTES_PER_SECOND - 1, MAX_BYTES_PER_SECOND + 1] {
        assert!(check_rate(rate).is_err(), "{rate}");
    }
}
