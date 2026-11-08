//! The pace a server sends its large messages at: a record from the data download, say, which can
//! be seven megabytes and sixteen thousand fragments.
//!
//! Sent all at once, such a message fills the client's socket and loses most of it, and the client
//! asks for lost packets again only 114 at a time, once every 0.6 s, while a server keeps a sent
//! packet for resending 120 s and only so many at most: the download never completes. So a
//! message longer than one interval's budget leaves the connection's own queue for this one and is
//! sent from it a part at a time:
//!
//! - **the rate:** at most half of the connection's rate in each half-second interval
//!   ([`BYTES_PER_INTERVAL`] unless the host sets another, within [`check_rate`]), counting what the
//!   connection resends and its other messages first, which go out at once as before: the budget
//!   is the large messages', and they take what the rest leave of it;
//! - **the window:** never more than [`WINDOW`] sequenced packets out that the client has not
//!   acknowledged (by its acknowledgement, or by asking for a later packet first), so that even a
//!   whole window lost is asked for again long before any of it is let go.
//!
//! A message on a queue that already has a large message of its ordering kind waiting waits behind
//! it, whatever its length, so each kind keeps its order on its queue; the other kinds on that queue
//! and the other queues' messages are not held (2017's servers let them by: over 852 long messages on
//! the event queue, 877 messages of other kinds came in before the long one's last packet). What
//! counts as large does not change with the rate, so no rate lets a large message skip the window.

use std::collections::{BTreeMap, VecDeque};

use dereth_primitives::LocalTime;
use dereth_transport::blob::NetBlob;
use dereth_transport::flow::FlowQueue;
use dereth_transport::session::{MAX_NAK_IDS, SENT_CACHE_MAX_PACKETS};
use dereth_transport::wire::{
    Fragment, OutPacket, ProtoHeader, MAX_BUILD_PAYLOAD, MAX_FRAGS_PER_PACKET,
};
use dereth_transport::CryptoSystem;

/// What a server sends of its large messages in one half-second interval. The January 2013
/// servers sent a seven-megabyte download in one burst each half second, 66 to 69 full packets,
/// about 32 KB: some 64 KB and 135 packets a second.
pub const BYTES_PER_INTERVAL: usize = 32 * 1024;

/// The most sequenced packets a server has out unacknowledged before its large messages wait: ten
/// of the client's requests to send again, which it makes one each 0.6 s, so a whole window lost
/// is asked for again within six seconds, far inside the time and the packets a server keeps
/// for resending.
#[allow(clippy::cast_possible_truncation)]
pub const WINDOW: u32 = 10 * MAX_NAK_IDS as u32;

const _: () = assert!((WINDOW as usize) < SENT_CACHE_MAX_PACKETS);

/// The most fragments a connection may have waiting before it is closed: about 45 MB, above what a
/// download's own limit and one more record of it ever leave waiting, so only a client that has
/// stopped taking what it is sent comes to it.
pub const MOST_WAITING: usize = 100_000;

/// The default rate: the January 2013 servers' ([`BYTES_PER_INTERVAL`] each half second).
#[allow(clippy::cast_possible_truncation)]
pub const DEFAULT_BYTES_PER_SECOND: u32 = 2 * BYTES_PER_INTERVAL as u32;

/// The lowest rate taken: four full packets each half second. Less is the connection's own
/// acknowledgements and clock messages spending most of the budget, and on a world connection its
/// other messages all of it, so a large message might never go.
pub const MIN_BYTES_PER_SECOND: u32 = 4 * 1024;

/// The highest rate taken: each half second's burst just fills the 128 KiB a client's socket holds
/// between two frames, and the four bursts between its acknowledgements just fit in the window. A
/// larger burst is mostly lost and asked for again at the client's 114 packets each 0.6 s, which
/// is slower.
pub const MAX_BYTES_PER_SECOND: u32 = 256 * 1024;

/// Refuse a rate no download does well at.
///
/// # Errors
/// The rate is outside [`MIN_BYTES_PER_SECOND`] to [`MAX_BYTES_PER_SECOND`].
pub fn check_rate(bytes_per_second: u32) -> Result<(), String> {
    if (MIN_BYTES_PER_SECOND..=MAX_BYTES_PER_SECOND).contains(&bytes_per_second) {
        Ok(())
    } else {
        Err(format!(
            "{bytes_per_second} bytes a second: it must be {MIN_BYTES_PER_SECOND} to \
             {MAX_BYTES_PER_SECOND}"
        ))
    }
}

/// A blob's ordering kind: the top byte of its id.
#[allow(clippy::cast_possible_truncation)]
const fn kind_of(blob_id: u64) -> u8 {
    (blob_id >> 56) as u8
}

/// One connection's large messages, waiting as fragments.
#[derive(Debug)]
pub struct Pace {
    /// What may be sent in one interval.
    budget: usize,
    /// The fragments waiting, in the order they go.
    fragments: VecDeque<Fragment>,
    /// How many of them each queue has, and each ordering kind on it.
    by_queue: BTreeMap<u16, usize>,
    by_kind: BTreeMap<(u16, u8), usize>,
    /// Bytes sent in the current interval.
    spent: usize,
    /// The interval `spent` counts.
    interval: Option<u16>,
}

impl Default for Pace {
    fn default() -> Self {
        Self::at_rate(DEFAULT_BYTES_PER_SECOND)
    }
}

impl Pace {
    /// A connection's large messages sent at `bytes_per_second` ([`check_rate`] says which rates
    /// do well).
    #[must_use]
    pub fn at_rate(bytes_per_second: u32) -> Self {
        Self {
            budget: usize::try_from(bytes_per_second / 2).unwrap_or(usize::MAX),
            fragments: VecDeque::new(),
            by_queue: BTreeMap::new(),
            by_kind: BTreeMap::new(),
            spent: 0,
            interval: None,
        }
    }

    /// Does a message of `len` bytes with this blob id on `queue` go by the pace: longer than the
    /// 2013 servers' budget of an interval, or of a kind with a large message waiting on the queue?
    #[must_use]
    pub fn takes(&self, queue: u16, blob_id: u64, len: usize) -> bool {
        len > BYTES_PER_INTERVAL
            || self
                .by_kind
                .get(&(queue, kind_of(blob_id)))
                .is_some_and(|n| *n > 0)
    }

    /// Queue a message, as its fragments.
    pub fn push(&mut self, blob: &NetBlob) {
        let frags = blob.fragmentize();
        *self.by_queue.entry(blob.queue_id).or_default() += frags.len();
        for f in &frags {
            *self
                .by_kind
                .entry((f.header.queue_id, kind_of(f.header.blob_id())))
                .or_default() += 1;
        }
        self.fragments.extend(frags);
    }

    /// Nothing waiting?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fragments.is_empty()
    }

    /// How many fragments are waiting.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.fragments.len()
    }

    /// How many fragments of one queue are waiting.
    #[must_use]
    pub fn waiting_on(&self, queue: u16) -> usize {
        self.by_queue.get(&queue).copied().unwrap_or(0)
    }

    /// The connection is in interval `interval`: a new one starts its budget again.
    pub fn at_interval(&mut self, interval: u16) {
        if self.interval != Some(interval) {
            self.interval = Some(interval);
            self.spent = 0;
        }
    }

    /// Count bytes the connection sent otherwise against this interval's budget.
    pub fn spend(&mut self, bytes: usize) {
        self.spent = self.spent.saturating_add(bytes);
    }

    /// Send what the budget and the window allow, numbering, keying and caching each packet as the
    /// connection's queue does its own: the datagrams, after whatever the queue sent this moment.
    /// `acknowledged` is the newest sequence the peer has said it has everything up to.
    pub fn send(
        &mut self,
        flow: &mut FlowQueue,
        crypto: &mut CryptoSystem,
        acknowledged: u32,
        now: LocalTime,
    ) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while !self.fragments.is_empty()
            && self.spent < self.budget
            && flow.highest_id_sent.wrapping_sub(acknowledged) < WINDOW
        {
            let mut packet = OutPacket::new(ProtoHeader {
                rec_id: flow.net_id,
                interval: flow.cur_local_interval,
                iteration: flow.iteration,
                ..Default::default()
            });
            while let Some(f) = self.fragments.front() {
                if packet.fragments.len() >= MAX_FRAGS_PER_PACKET
                    || packet.payload_len() + f.wire_len() > MAX_BUILD_PAYLOAD
                {
                    break;
                }
                let Some(f) = self.fragments.pop_front() else {
                    break;
                };
                if let Some(n) = self.by_queue.get_mut(&f.header.queue_id) {
                    *n -= 1;
                    if *n == 0 {
                        self.by_queue.remove(&f.header.queue_id);
                    }
                }
                let kind = (f.header.queue_id, kind_of(f.header.blob_id()));
                if let Some(n) = self.by_kind.get_mut(&kind) {
                    *n -= 1;
                    if *n == 0 {
                        self.by_kind.remove(&kind);
                    }
                }
                // The bounds were just checked, so this cannot fail.
                let _ = packet.add_fragment(f);
            }
            if packet.fragments.is_empty() {
                break;
            }
            flow.highest_id_sent = flow.highest_id_sent.wrapping_add(1);
            if flow.highest_id_sent == 0 {
                flow.highest_id_sent = 1;
            }
            packet.header.seq_id = flow.highest_id_sent;
            // A packet of fragments with its key always serializes.
            let Ok(bytes) = packet.serialize(Some(crypto.next())) else {
                break;
            };
            self.spend(bytes.len());
            out.push(bytes);
            flow.sent.add_sent_packet(packet, now);
        }
        out
    }
}
