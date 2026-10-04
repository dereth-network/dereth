//! `Indicator` — per-connection reassembly, ephemeral supersession and the 5-second flush.
//!
//! One `Indicator` per network recipient, created in
//! the recipient's constructor.
//!
//! The mechanism worth understanding before reading the code: the ephemeral flag is meant to let a
//! half-received multi-fragment blob be thrown away as soon as a newer blob for the same stream
//! starts arriving — a position update still in flight abandoned when a fresher one appears.
//!
//! **The shipped client has that comparison inverted** and drops the arriving newer fragment
//! instead, deadlocking the stream. It is transcribed here as the client has it.
//!
//! See `docs/networking/04-netblobs-and-queues.md` §3.

use std::collections::BTreeMap;

use dereth_primitives::LocalTime;

use crate::blob::{BlobState, NetBlob, NetBlobId};
use crate::wire::Fragment;

/// An entry is stale after 5 seconds.
pub const EPH_INFO_TIMEOUT: f64 = 5.0;

/// The flush runs at most once every 5 seconds.
pub const EPH_FLUSH_INTERVAL: f64 = 5.0;

/// Per ephemeral stream, the newest blob id seen and when.
///
/// The client stores the key, the latest blob id and a timestamp, and threads the entries on an
/// intrusive list only so the timed-out flush can walk the table cheaply; a map here does the same
/// job and the flush iterates it.
#[derive(Debug, Clone, Copy)]
pub struct ReceivedEphemeral {
    pub key: u64,
    pub latest_net_blob_id: NetBlobId,
    pub time_stamp: LocalTime,
}

impl ReceivedEphemeral {
    /// The staleness test.
    #[must_use]
    pub fn timed_out(&self, now: LocalTime) -> bool {
        now.seconds_since(self.time_stamp) > EPH_INFO_TIMEOUT
    }
}

/// One completed blob, on its way to a net queue.
#[derive(Debug)]
pub struct CompletedBlob {
    /// The **original** id: handing a blob to its queue restores `id = saved_net_blob_id` first,
    /// so the consumer sees the real id and not the ephemeral stream key.
    pub id: NetBlobId,
    pub queue_id: u16,
    pub sender: u16,
    pub payload: Vec<u8>,
}

/// `Indicator`.
#[derive(Debug, Default)]
pub struct Indicator {
    /// `waiting_blobs` — partially received blobs, keyed by the blob id's `sequence_id` for ephemeral
    /// blobs and by the whole id otherwise.
    ///
    /// A `BTreeMap` and not a `HashMap`. Nothing iterates it today — every access is a keyed
    /// `get`/`insert`/`remove` — but this is the packet path, where the order things happen in
    /// is observable, and Rust randomises `HashMap` order **per process**, so a future iteration here would
    /// make two runs of this same code disagree with each other. Deterministic by construction is
    /// cheaper than an argument, at these sizes (a handful of half-received blobs per connection).
    waiting_blobs: BTreeMap<u64, NetBlob>,
    /// `arrived_eph_blobs` — per ephemeral stream, the newest blob id seen.
    ///
    /// Also a `BTreeMap`, for the same reason and one more: the timed-out ephemeral-info flush
    /// *does* walk it. Its order cannot reach the wire even so.
    ///
    /// Note that neither container reproduces the original's order and neither could: the client
    /// keeps these in a 64-bit hash table whose bucket order is its own 64-bit fold, with an
    /// intrusive list threaded through in insertion order. What matters is that our order is
    /// deterministic and unobservable, both of which hold.
    arrived_eph_blobs: BTreeMap<u64, ReceivedEphemeral>,
    /// `flush_stamp`.
    flush_stamp: LocalTime,
    /// Whether `flush_stamp` has ever been set, so the first flush is not deferred by five seconds
    /// from time zero.
    flushed_once: bool,
}

impl Indicator {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Process one arriving packet.
    ///
    /// Each fragment whose blob `frag_is_obsolete_ephemeral` does not reject is passed to
    /// `accept_frag` with the sender's receiver id.
    ///
    /// Returns every blob that completed. Note the order: the obsolescence test runs *first* and
    /// updates the arrival table even for fragments it then accepts, so a stream's "newest seen"
    /// advances on the first fragment of a new blob, not on its completion.
    pub fn check_in_packet(
        &mut self,
        fragments: &[Fragment],
        sender_rec_id: u16,
        now: LocalTime,
    ) -> Vec<CompletedBlob> {
        let mut done = Vec::new();
        for frag in fragments {
            let blob_id = NetBlobId(frag.header.blob_id());
            if self.frag_is_obsolete_ephemeral(blob_id, now) {
                continue;
            }
            if let Some(blob) = self.accept_frag(frag, blob_id, sender_rec_id) {
                done.push(blob);
            }
        }
        done
    }

    /// The obsolete-ephemeral test.
    ///
    /// A non-ephemeral blob is always accepted. Otherwise the arrival table is looked up by the
    /// blob's sequence id: a stream not seen before is recorded with this blob id and accepted. If
    /// the id differs from the stream's latest, it is obsolete when the latest is newer under
    /// `lhs_newer`; otherwise it becomes the stream's latest. Anything else is accepted.
    pub fn frag_is_obsolete_ephemeral(&mut self, blob_id: NetBlobId, now: LocalTime) -> bool {
        if !blob_id.is_ephemeral() {
            return false;
        }
        let key = blob_id.sequence_id();
        match self.arrived_eph_blobs.get_mut(&key) {
            None => {
                self.arrived_eph_blobs.insert(
                    key,
                    ReceivedEphemeral {
                        key,
                        latest_net_blob_id: blob_id,
                        time_stamp: now,
                    },
                );
                false
            }
            Some(info) => {
                if blob_id != info.latest_net_blob_id {
                    if info.latest_net_blob_id.lhs_newer_ordering_stamp(blob_id) {
                        return true;
                    }
                    info.latest_net_blob_id = blob_id;
                    info.time_stamp = now;
                }
                false
            }
        }
    }

    /// The fragment accept, plus the send-to-queue step when the blob
    /// completes.
    ///
    /// Returns the completed blob, if this fragment finished one.
    fn accept_frag(
        &mut self,
        frag: &Fragment,
        blob_id: NetBlobId,
        sender_rec_id: u16,
    ) -> Option<CompletedBlob> {
        let key = if blob_id.is_ephemeral() {
            blob_id.sequence_id()
        } else {
            blob_id.0
        };

        if let Some(existing) = self.waiting_blobs.get(&key) {
            if existing.saved_net_blob_id == blob_id {
                // Same blob, another fragment.
            } else if !existing.saved_net_blob_id.lhs_newer_ordering_stamp(blob_id) {
                // the supersession test is inverted in the
                // shipped client and this branch is the bug, transcribed.
                //
                // The fragment accept reads:
                //
                //     unless the saved blob id's ordering stamp is newer than the arriving
                //     fragment's blob id, return;                  // <-- here
                //     otherwise remove and release the waiting blob  // supersede
                //
                // so the client **drops the arriving fragment when the arriving blob is the newer
                // one**, and supersedes the stored blob only when the *stored* one is newer -- which
                // `frag_is_obsolete_ephemeral` has already made unreachable, since it drops any
                // fragment older than the stream's newest. The net effect is that an ephemeral
                // stream which half-receives a multi-fragment blob and then sees a newer blob is
                // dead for the rest of the connection: the new blob's fragments are dropped here and
                // the old blob's remaining fragments are dropped as obsolete upstream.
                //
                // "A half-received multi-fragment blob is thrown away as soon as a newer blob for
                // the same ephemeral stream starts arriving" describes the intent, not the code.
                //
                // Kept as the client has it, because it is unreachable
                // against ACE: `MessageFragment.CreateServerFragment` sends `Id = 0x80000000`, so
                // every blob is its own ephemeral stream and `saved != blob_id` never holds. Whether
                // a retail server ever exercised it is unknown, and no capture exists to say.
                return None;
            } else {
                self.waiting_blobs.remove(&key);
            }
        }

        let blob = self.waiting_blobs.entry(key).or_insert_with(|| {
            let mut b = NetBlob::for_recv(sender_rec_id);
            b.queue_id = frag.header.queue_id;
            b.saved_net_blob_id = blob_id;
            b.id = NetBlobId(key);
            b
        });

        blob.receive_add_fragment(frag);
        blob.id = NetBlobId(key);

        if blob.is_complete() {
            let mut blob = self.waiting_blobs.remove(&key)?;
            blob.state = BlobState::Received;
            // Handing the blob to its queue restores the real id first.
            blob.id = blob.saved_net_blob_id;
            return Some(CompletedBlob {
                id: blob.id,
                queue_id: blob.queue_id,
                sender: blob.sender,
                payload: blob.take_payload(),
            });
        }
        None
    }

    /// The timed-out flush, called from the recipient's periodic update.
    ///
    /// Runs at most once every 5 seconds and deletes entries older than 5 seconds, so a quiet
    /// stream costs nothing. Returns how many entries went, for the test.
    pub fn flush_timed_out_eph_info(&mut self, now: LocalTime) -> usize {
        // ORDER-OK: this is the only walk of `arrived_eph_blobs`, and its result is
        // order-independent. `retain`'s predicate reads one entry's own `time_stamp` and nothing
        // else -- no accumulator, no early exit, no output -- so the set that survives is the same
        // whatever order the entries are visited in, and the only value that escapes is a count.
        // Nothing is emitted, enqueued or written to a packet here. (It is a `BTreeMap` anyway, so
        // the order is deterministic; this note is about why it would not matter if it were not.)
        if self.flushed_once && now.seconds_since(self.flush_stamp) < EPH_FLUSH_INTERVAL {
            return 0;
        }
        self.flush_stamp = now;
        self.flushed_once = true;
        let before = self.arrived_eph_blobs.len();
        self.arrived_eph_blobs
            .retain(|_, info| !info.timed_out(now));
        before - self.arrived_eph_blobs.len()
    }

    /// How many blobs are half-received. Diagnostics and tests only.
    #[must_use]
    pub fn waiting_blob_count(&self) -> usize {
        self.waiting_blobs.len()
    }

    /// How many ephemeral streams are being tracked. Diagnostics and tests only.
    #[must_use]
    pub fn eph_stream_count(&self) -> usize {
        self.arrived_eph_blobs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::FragmentHeader;

    fn frag(id: NetBlobId, num_frags: u16, blob_num: u16, payload: Vec<u8>) -> Fragment {
        Fragment::new(
            FragmentHeader {
                blob_id_low: id.low32(),
                blob_id_high: id.high32(),
                num_frags,
                blob_frag_size: 0,
                blob_num,
                queue_id: 9,
            },
            payload,
        )
    }

    /// An ephemeral blob id: bit 63 set, ordering stamp in bits 32-47, sequence in the low dword.
    fn eph(stream: u32, stamp: u16) -> NetBlobId {
        NetBlobId(0x8000_0000_0000_0000 | (u64::from(stamp) << 32) | u64::from(stream))
    }

    /// Oracle: `docs/networking/04-netblobs-and-queues.md` §3.1, the fragment-accept
    /// description.
    #[test]
    fn a_single_fragment_blob_completes_immediately() {
        let mut ind = Indicator::new();
        let done = ind.check_in_packet(
            &[frag(NetBlobId(7), 1, 0, vec![1, 2, 3, 4])],
            3,
            LocalTime(0.0),
        );
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].id, NetBlobId(7));
        assert_eq!(done[0].queue_id, 9);
        assert_eq!(done[0].sender, 3);
        assert_eq!(done[0].payload, vec![1, 2, 3, 4]);
        assert_eq!(ind.waiting_blob_count(), 0);
    }

    /// A multi-fragment blob is held until every fragment arrives, and the completed blob carries
    /// the **original** id, not the ephemeral stream key it was filed under.
    #[test]
    fn multi_fragment_blob_completes_with_its_original_id_restored() {
        let mut ind = Indicator::new();
        let id = eph(0x11, 4);
        let t = LocalTime(0.0);
        assert!(ind
            .check_in_packet(&[frag(id, 2, 0, vec![0xAA; 448])], 1, t)
            .is_empty());
        assert_eq!(ind.waiting_blob_count(), 1);
        let done = ind.check_in_packet(&[frag(id, 2, 1, vec![0xBB; 10])], 1, t);
        assert_eq!(done.len(), 1);
        assert_eq!(
            done[0].id, id,
            "handing the blob to its queue restores saved_net_blob_id"
        );
        assert_eq!(done[0].payload.len(), 458);
        assert_eq!(ind.waiting_blob_count(), 0);
    }

    /// Ephemeral supersession, as the client actually implements it.
    ///
    /// Oracle: the fragment accept's early return, read together with the
    /// obsolete-ephemeral test.
    ///
    /// The comparison is inverted in the shipped client, so a newer blob does **not** displace a
    /// half-received one: its fragments are dropped, and the stale blob's own remaining fragments
    /// are then dropped upstream as obsolete. The stream deadlocks.
    /// `docs/networking/04-netblobs-and-queues.md` §3.1 describes this shipped, inverted rule.
    ///
    /// See the resolved compatibility notes #201. Unreachable against ACE, which gives every blob its
    /// own ephemeral stream.
    #[test]
    fn a_newer_ephemeral_blob_does_not_supersede_a_half_received_one() {
        let mut ind = Indicator::new();
        let t = LocalTime(0.0);
        let old = eph(0x11, 4);
        let new = eph(0x11, 5);

        // Half of the old blob arrives.
        assert!(ind
            .check_in_packet(&[frag(old, 3, 0, vec![0xAA; 448])], 1, t)
            .is_empty());
        assert_eq!(ind.waiting_blob_count(), 1);

        // The newer blob starts. Its fragments are dropped, and the stale partial blob stays.
        assert!(ind
            .check_in_packet(&[frag(new, 2, 0, vec![0xCC; 448])], 1, t)
            .is_empty());
        assert!(ind
            .check_in_packet(&[frag(new, 2, 1, vec![0xDD; 4])], 1, t)
            .is_empty());
        assert_eq!(ind.waiting_blob_count(), 1);

        // And the old blob's remaining fragments are now obsolete, so it never completes either.
        assert!(ind
            .check_in_packet(&[frag(old, 3, 1, vec![0xAA; 448])], 1, t)
            .is_empty());
        assert_eq!(ind.waiting_blob_count(), 1, "the stream is deadlocked");
    }

    /// The branch the inverted test *does* reach: a stored blob that is newer than the arriving one
    /// is discarded and replaced. `frag_is_obsolete_ephemeral` makes this unreachable through
    /// `check_in_packet`, so it is exercised through `accept_frag` directly.
    #[test]
    fn a_stored_newer_blob_is_superseded_by_an_older_arrival() {
        let mut ind = Indicator::new();
        let newer = eph(0x11, 5);
        let older = eph(0x11, 4);
        assert!(ind
            .accept_frag(&frag(newer, 2, 0, vec![0xCC; 448]), newer, 1)
            .is_none());
        assert_eq!(ind.waiting_blob_count(), 1);
        // The stored blob is newer, so it goes and the older one takes its place.
        assert!(ind
            .accept_frag(&frag(older, 1, 0, vec![0xAA; 4]), older, 1)
            .is_some());
        assert_eq!(ind.waiting_blob_count(), 0);
    }

    /// `frag_is_obsolete_ephemeral` drops an older stamp outright, so the stale blob's fragments
    /// never reach `accept_frag` at all.
    #[test]
    fn frag_is_obsolete_ephemeral_drops_an_older_stamp() {
        let mut ind = Indicator::new();
        let t = LocalTime(0.0);
        let newer = eph(0x22, 100);
        let older = eph(0x22, 99);

        assert!(!ind.frag_is_obsolete_ephemeral(newer, t));
        assert!(
            ind.frag_is_obsolete_ephemeral(older, t),
            "older stamp is obsolete"
        );
        // The same id again is not obsolete: it is the same blob, another fragment.
        assert!(!ind.frag_is_obsolete_ephemeral(newer, t));
        // And an even newer one advances the record.
        let newest = eph(0x22, 101);
        assert!(!ind.frag_is_obsolete_ephemeral(newest, t));
        assert!(ind.frag_is_obsolete_ephemeral(newer, t));

        // Non-ephemeral ids are never obsolete and never enter the table.
        assert!(!ind.frag_is_obsolete_ephemeral(NetBlobId(5), t));
        assert_eq!(ind.eph_stream_count(), 1);
    }

    /// The stamp comparison wraps, so a stream that has run past 0xFFFF keeps working.
    #[test]
    fn ephemeral_supersession_wraps_at_0xffff() {
        let mut ind = Indicator::new();
        let t = LocalTime(0.0);
        assert!(!ind.frag_is_obsolete_ephemeral(eph(1, 0xFFFF), t));
        assert!(
            !ind.frag_is_obsolete_ephemeral(eph(1, 0), t),
            "0 is newer than 0xFFFF"
        );
        assert!(ind.frag_is_obsolete_ephemeral(eph(1, 0xFFFF), t));
    }

    /// ACE gives every blob a unique sequence with `Id = 0x80000000`, so every blob is its own
    /// ephemeral stream and supersession is a no-op. `docs/networking/04-netblobs-and-queues.md`
    /// §2.2.
    #[test]
    fn aces_per_blob_streams_make_supersession_inert() {
        let mut ind = Indicator::new();
        let t = LocalTime(0.0);
        for seq in 0..5u32 {
            let id = NetBlobId(0x8000_0000_0000_0000 | u64::from(seq));
            assert!(!ind.frag_is_obsolete_ephemeral(id, t));
        }
        assert_eq!(ind.eph_stream_count(), 5, "one stream per blob");
    }

    /// The 5-second flush runs at most once per 5 seconds and drops entries older than 5 seconds.
    #[test]
    fn eph_info_flush_runs_at_most_once_per_five_seconds() {
        let mut ind = Indicator::new();
        ind.frag_is_obsolete_ephemeral(eph(1, 0), LocalTime(0.0));
        ind.frag_is_obsolete_ephemeral(eph(2, 0), LocalTime(0.0));
        assert_eq!(ind.eph_stream_count(), 2);

        // The first call establishes the stamp; nothing is old enough yet.
        assert_eq!(ind.flush_timed_out_eph_info(LocalTime(0.0)), 0);
        // Too soon to run again, even though the entries are now stale.
        assert_eq!(ind.flush_timed_out_eph_info(LocalTime(4.9)), 0);
        assert_eq!(ind.eph_stream_count(), 2);
        // Five seconds on, it runs and both entries go.
        assert_eq!(ind.flush_timed_out_eph_info(LocalTime(6.0)), 2);
        assert_eq!(ind.eph_stream_count(), 0);
    }

    /// A stream touched inside the window survives the flush.
    #[test]
    fn a_live_stream_survives_the_flush() {
        let mut ind = Indicator::new();
        ind.frag_is_obsolete_ephemeral(eph(1, 0), LocalTime(0.0));
        assert_eq!(ind.flush_timed_out_eph_info(LocalTime(0.0)), 0);
        // Touched at t = 5.5 by a newer stamp, so at t = 6.0 it is only 0.5 s old.
        ind.frag_is_obsolete_ephemeral(eph(1, 1), LocalTime(5.5));
        assert_eq!(ind.flush_timed_out_eph_info(LocalTime(6.0)), 0);
        assert_eq!(ind.eph_stream_count(), 1);
    }

    /// Non-ephemeral blobs are keyed by their whole id, so two different ids coexist rather than
    /// superseding each other.
    #[test]
    fn non_ephemeral_blobs_do_not_supersede() {
        let mut ind = Indicator::new();
        let t = LocalTime(0.0);
        let a = NetBlobId(0x0300_0004_0000_0001);
        let b = NetBlobId(0x0300_0005_0000_0002);
        assert!(ind
            .check_in_packet(&[frag(a, 2, 0, vec![1; 448])], 1, t)
            .is_empty());
        assert!(ind
            .check_in_packet(&[frag(b, 2, 0, vec![2; 448])], 1, t)
            .is_empty());
        assert_eq!(ind.waiting_blob_count(), 2);
        assert_eq!(ind.eph_stream_count(), 0);
    }
}
