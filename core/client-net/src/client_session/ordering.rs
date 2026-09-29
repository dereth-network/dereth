//! The ordering machinery above the blob: [`StampWindow`], the per-object parking table and the
//! crucial-events gate.
//!
//! Source: `docs/networking/messages/00-dispatch-and-queues.md` §4 and §5,
//! plus `docs/networking/02-reliability-and-flow.md` §5.
//! The algorithm below is transcribed from the client's own receive-side time stamper — its sorted
//! insert and its next-ready-entry step.
//!
//! # Why this is a re-implementation and not a shared type
//!
//! The same stamp-window algorithm serves the transport's `Indicator` and the UI queue's
//! per-weenie ordering in the client session. Sharing it would tie the session layer, which
//! reaches the network only through `Transport`, to the transport's own types, so this is a
//! deliberate ~100-line duplicate. Both implementations are tested against
//! `docs/networking/02-reliability-and-flow.md` §5.

use dereth_primitives::{LocalTime, ObjectId};
use std::collections::HashMap;

/// What [`StampWindow::add`] decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StampResult {
    /// In order: deliver now. `SequenceGate` returns 1.
    Deliver,
    /// Out of order: held on the blocked list. `SequenceGate` returns 2.
    Queued,
    /// Stale or duplicate: drop. `SequenceGate` returns 3.
    Old,
}

/// The 19-entry half of the deadlock breaker: **more than** 0x13 blocked stamps.
pub const DEADLOCK_BLOCKED_STAMPS: usize = 0x13;
/// The 300-second half.
pub const DEADLOCK_SECONDS: f64 = 300.0;
/// The wrap threshold used by the timestamp queue's sorted insert. Note it is `0x7FFFFFFE`, one
/// less than the transport's own newer-than half-range, `0x7FFFFFFF`; the client has the same
/// off-by-one, so each site is transcribed separately rather than sharing a helper.
pub const INSERT_WRAP_THRESHOLD: u32 = 0x7FFF_FFFE;

/// One blocked entry.
#[derive(Debug, Clone)]
struct Blocked {
    stamp: u32,
    /// `None` for the fabricated entry the deadlock breaker inserts.
    blob: Option<Vec<u8>>,
}

/// A re-implementation of `SequenceGate`'s `TSRECV_BLOCK` mode.
#[derive(Debug, Clone, Default)]
pub struct StampWindow {
    highest: u32,
    received_first: bool,
    blocked: Vec<Blocked>,
    /// `0.0` means "not blocked", which is how the client spells it.
    blocked_since: Option<LocalTime>,
}

impl StampWindow {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The highest stamp delivered so far.
    #[must_use]
    pub fn highest(&self) -> u32 {
        self.highest
    }

    /// How many stamps are currently held out of order.
    #[must_use]
    pub fn blocked_count(&self) -> usize {
        self.blocked.len()
    }

    /// The blocking sorted insert.
    ///
    /// 1. The first entry ever sets the received-first flag and skips **only** the staleness test.
    ///    The contiguity test below still applies, and `highest` starts at 0 — so a stream whose
    ///    first stamp is neither 0 nor 1 blocks from the outset; it is not accepted whatever the
    ///    stamp.
    /// 2. Otherwise `stamp <= highest` is stale — and note this is a **plain unsigned compare**, not
    ///    a wrap-aware one. The wrap-aware comparison appears only in the sorted insert.
    /// 3. `stamp == highest` or `highest + 1` delivers now and bumps `highest`.
    /// 4. Anything else goes on the sorted blocked list, and `blocked_since` is stamped **only** on
    ///    the transition from zero blocked entries to one.
    /// 5. The deadlock breaker: more than 19 blocked stamps **and** more than 300 s since
    ///    `blocked_since` fabricates the missing `highest + 1` and pulls the next ready entry.
    pub fn add(&mut self, stamp: u32, blob: Vec<u8>, now: LocalTime) -> StampResult {
        self.add_inner(stamp, Some(blob), now).0
    }

    /// [`StampWindow::add`], handing the blob back when the decision is [`StampResult::Deliver`].
    ///
    /// The blob comes back rather than being cloned by the caller, because the deadlock breaker can
    /// deliver a *different* blob from the one just added: it fabricates the missing stamp and then
    /// pulls whatever that unblocked.
    pub fn add_returning(
        &mut self,
        stamp: u32,
        blob: Vec<u8>,
        now: LocalTime,
    ) -> (StampResult, Option<Vec<u8>>) {
        self.add_inner(stamp, Some(blob), now)
    }

    /// The sorted insert **on its own** -- no contiguity
    /// test, no staleness test and no blocked-since stamp.
    ///
    /// This is the operation the UI queue's ordering router uses when it finds no object for the
    /// blob's object id:
    /// the object maintainer takes the **null placeholder weenie**
    /// for that id and parks the blob in *that object's own* ordering window. There is exactly one
    /// ordering window per object and the placeholder becomes the object, so a blob parked while
    /// the object was unknown and a blob that arrives after it is known go into the **same**
    /// window -- which is why delivery can replay the parked list through the ready-entry iterator
    /// and have the highest stamp advance across the seam.
    ///
    /// Note the three things the plain add deliberately does **not** do, all of which the
    /// blocking-add wrapper does around it: parking a stamp can never deliver, can never be
    /// rejected as stale, and does not start the 300-second deadlock clock.
    pub fn park(&mut self, stamp: u32, blob: Vec<u8>) {
        self.insert_sorted(stamp, Some(blob));
    }

    fn add_inner(
        &mut self,
        stamp: u32,
        blob: Option<Vec<u8>>,
        now: LocalTime,
    ) -> (StampResult, Option<Vec<u8>>) {
        if self.received_first {
            if stamp <= self.highest {
                return (StampResult::Old, None);
            }
        } else {
            self.received_first = true;
        }

        if stamp == self.highest || stamp == self.highest.wrapping_add(1) {
            self.highest = stamp;
            return (StampResult::Deliver, blob);
        }

        let was_empty = self.blocked.is_empty();
        self.insert_sorted(stamp, blob);
        if was_empty && self.blocked.len() == 1 {
            self.blocked_since = Some(now);
        }

        if self.blocked.len() > DEADLOCK_BLOCKED_STAMPS
            && self
                .blocked_since
                .is_some_and(|t| now.seconds_since(t) > DEADLOCK_SECONDS)
        {
            // Fabricate the missing stamp, then pull whatever that unblocked. This is the only way
            // the client ever skips a missing ordered event.
            let next = self.highest.wrapping_add(1);
            self.add_inner(next, None, now);
            if let Some(ready) = self.next_ready(now) {
                return (StampResult::Deliver, ready);
            }
        }
        (StampResult::Queued, None)
    }

    /// The sorted insert: walk the list while the new stamp is *after*
    /// the current one under the wrap-aware comparison, and insert there.
    fn insert_sorted(&mut self, stamp: u32, blob: Option<Vec<u8>>) {
        let mut at = self.blocked.len();
        for (i, e) in self.blocked.iter().enumerate() {
            if stamp == e.stamp || !stamp_after(stamp, e.stamp) {
                at = i;
                break;
            }
        }
        self.blocked.insert(at, Blocked { stamp, blob });
    }

    /// The next-ready-entry step.
    ///
    /// Pops the head of the blocked list when its stamp equals `highest` or `highest + 1`.
    ///
    /// **Reproduce the odd part.** When the head is *not* ready and something is still blocked, the
    /// client **resets `blocked_since` to the current time**. A caller that drains every frame
    /// therefore pushes the 300-second deadline out for ever and the breaker never fires. The
    /// force-skip is observable behavior, so a "cleanup" here would change the client silently.
    pub fn next_ready(&mut self, now: LocalTime) -> Option<Option<Vec<u8>>> {
        let head = self.blocked.first()?;
        if head.stamp == self.highest || head.stamp == self.highest.wrapping_add(1) {
            let e = self.blocked.remove(0);
            self.highest = e.stamp;
            if self.blocked.is_empty() {
                self.blocked_since = None;
            }
            Some(e.blob)
        } else {
            if !self.blocked.is_empty() {
                self.blocked_since = Some(now);
            }
            None
        }
    }

    /// Every blob the window can release now, in order. Fabricated entries are skipped, exactly as
    /// does when `SequenceGate` hands it a null.
    pub fn drain_ready(&mut self, now: LocalTime) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        while let Some(entry) = self.next_ready(now) {
            if let Some(b) = entry {
                out.push(b);
            }
        }
        out
    }
}

/// The timestamp queue's ordering predicate: `a` sorts after `b`.
///
/// The client computes the absolute difference and flips the sign when it exceeds `0x7FFFFFFE`, so
/// this is *not* the same expression as the client's other newer-stamp comparison.
#[must_use]
pub fn stamp_after(a: u32, b: u32) -> bool {
    if a == b {
        return false;
    }
    let (diff, forward) = if a > b { (a - b, true) } else { (b - a, false) };
    if diff > INSERT_WRAP_THRESHOLD {
        !forward
    } else {
        forward
    }
}

/// The placeholder deadline: current time plus 25.0 seconds.
///
/// The same constant `dereth_client_model::objects::DESTRUCTION_DELAY` holds, repeated here rather than
/// shared because `dereth-client-net` does not depend on `dereth-client-model` and a constant read through the
/// same symbol it is written through cannot detect a wrong constant.
pub const PARKED_BLOB_LIFETIME: f64 = 25.0;

/// How old a placeholder's ask stamp must be, strictly, before the client asks the server again
/// to describe the object (`0xF6EA`). The same constant as `dereth_client_model::objects::FORCE_OBJDESC_INTERVAL`,
/// repeated here for the reason [`PARKED_BLOB_LIFETIME`] is.
pub const FORCE_OBJDESC_AGE: f64 = 20.0;

/// What is parked on one object, and when the placeholder holding it dies.
#[derive(Debug, Clone)]
struct ParkedObject {
    /// Current time plus 25.0 seconds as of the **latest** park on this id. Every park
    /// reschedules the placeholder's destruction, so a placeholder lives as long as blobs keep
    /// arriving for it at least every 25 s, and dies 25 s after the last one.
    ///
    /// The retail recordings show it: in `Random10.pcap` an object that was never created got
    /// `0xF748` from t = 77.6 s to 328.1 s with gaps under 25 s, and the client asked about it
    /// every 20 s from 97.6 s to 348.4 s, which a deadline fixed at the first park (102.6 s)
    /// cannot produce.
    doomed_at: f64,
    /// When the placeholder was made, then when the client last asked the server about it. A new
    /// park does **not** move it.
    asked_at: f64,
    blobs: Vec<Vec<u8>>,
}

/// Blobs parked on an object that the client does not know yet.
///
/// The object maintainer creates a "null object" placeholder and hangs
/// the blob on it; the smart box's replay walks the list when the real
/// object arrives. Dropping "future" messages instead of parking them **loses objects**.
///
/// # The placeholder is doomed the moment it is made
///
/// The parking step is 59 bytes and calls exactly four things: fetch the object,
/// and failing that make a null placeholder, **mark that placeholder to be destroyed**, and then
/// queue the blob on it.
/// So in the client a placeholder dies 25 s after the last blob parked on it: if the real object
/// does not arrive, the maintainer's sweep reaches the placeholder's deadline, deletes the object,
/// and its parked-blob list goes with it. The list lives on the physics object itself rather
/// than in a separate lifetime-managed store.
///
/// Without that deadline the consequence is not hypothetical. **Any WorldObjects
/// message for an id the client has already culled parks its bytes here for the life of the
/// process** — and ACE produces exactly that stream: its `handle_visible_obj` expiry stops
/// tracking a player without sending `0xF747` (`ObjectMaint.cs:630..651`), a landblock reload
/// reissues dynamic guids and never names the old ones again (`Entity/Landblock.cs:118`), and the
/// client's own 25 s cull of a body whose placement failed can retire an id while the server is
/// still broadcasting about it. The long-session growth test measures it: one `Vec<u8>` retained
/// per leave/return cycle, for ever.
///
/// # The placeholder is also what makes the client ask the server about the object
///
/// Each placeholder carries a stamp set when it is made. The maintainer's pass
/// ([`Self::take_force_objdesc_asks`]) asks the server to describe every placeholder whose stamp
/// is more than 20 s old (`0xF6EA`) and restamps it. So the first ask comes 20 s after the first
/// park, not on it, and asks repeat every 20 s for as long as the placeholder lives; a create
/// (the placeholder is released) or the deadline ends them. Retail's recordings carry 968 k of
/// these asks, at 20.00 s spacing per object.
///
/// The clock is pushed in rather than read, because the client uses a global clock
/// and this crate has none; [`Self::set_time`] is that global's publisher and
/// [`Self::destroy_expired`] is the object-maintenance pass over the deadlines.
#[derive(Debug, Clone, Default)]
pub struct ParkedBlobs {
    by_object: HashMap<ObjectId, ParkedObject>,
    /// Current time as [`Self::set_time`] last published it.
    now: f64,
    /// Placeholders the deadline has destroyed, for the counters and the tests.
    destroyed: u64,
}

impl ParkedBlobs {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish current time, which is what [`Self::park`] stamps the deadline from.
    ///
    /// Called immediately before every dispatch that can park, so the stamp is the clock the
    /// admission ran at rather than the clock of whatever ran last.
    pub fn set_time(&mut self, now: f64) {
        self.now = now;
    }

    /// Park a blob on an object.
    ///
    /// The first park on an id creates the placeholder object and stamps it with current time.
    /// Every park, the first included, reschedules the placeholder's destruction to current time
    /// plus 25.0 seconds; no park moves the ask stamp.
    pub fn park(&mut self, id: ObjectId, blob: Vec<u8>) {
        let now = self.now;
        let parked = self.by_object.entry(id).or_insert_with(|| ParkedObject {
            doomed_at: now,
            asked_at: now,
            blobs: Vec::new(),
        });
        parked.doomed_at = now + PARKED_BLOB_LIFETIME;
        parked.blobs.push(blob);
    }

    /// The maintainer's ask pass: every placeholder whose stamp is more than 20 s older than
    /// `now` is restamped with `now`, and its id is returned for a `0xF6EA` ask. Ids come back
    /// in ascending order so a frame's asks are deterministic.
    ///
    /// Run it after [`Self::destroy_expired`] in the same pass, as the client does, so a
    /// placeholder that has just died is not asked about.
    pub fn take_force_objdesc_asks(&mut self, now: f64) -> Vec<ObjectId> {
        let mut asks: Vec<ObjectId> = self
            .by_object
            .iter_mut()
            .filter(|(_, parked)| now - parked.asked_at > FORCE_OBJDESC_AGE)
            .map(|(id, parked)| {
                parked.asked_at = now;
                *id
            })
            .collect();
        asks.sort_unstable_by_key(|id| id.0);
        asks
    }

    /// The maintainer's sweep over the placeholders this table stands for: destroy
    /// every one whose destroy deadline has passed, and answer how many.
    ///
    /// The blobs are dropped, not replayed. That is the object delete's behaviour and the
    /// only one available: there is no object to deliver them to, which is why the placeholder
    /// existed at all.
    pub fn destroy_expired(&mut self, now: f64) -> usize {
        let before = self.by_object.len();
        self.by_object.retain(|_, parked| parked.doomed_at > now);
        let destroyed = before - self.by_object.len();
        self.destroyed += destroyed as u64;
        destroyed
    }

    /// Take everything parked on an object, in arrival order.
    pub fn release(&mut self, id: ObjectId) -> Vec<Vec<u8>> {
        self.by_object
            .remove(&id)
            .map(|p| p.blobs)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn parked_on(&self, id: ObjectId) -> usize {
        self.by_object.get(&id).map_or(0, |p| p.blobs.len())
    }

    #[must_use]
    pub fn total(&self) -> usize {
        self.by_object.values().map(|p| p.blobs.len()).sum()
    }

    /// How many placeholders the 25 s deadline has destroyed this session.
    #[must_use]
    pub fn destroyed(&self) -> u64 {
        self.destroyed
    }
}

/// The "buffer everything until `Login_PlayerDescription`" gate.
///
/// The UI queue's ephemeral path refuses to dispatch until
/// the crucial-ordered-events flag is set, and that flag is set **only** by the `0x0013` arm
/// through. Until then every unordered UI event is
/// ref-counted onto a waiting list; when `0x0013` arrives the whole list is replayed through the
/// dispatcher **in order**.
#[derive(Debug, Clone, Default)]
pub struct CrucialEventsGate {
    received: bool,
    waiting: Vec<Vec<u8>>,
}

impl CrucialEventsGate {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `0x0013` has arrived.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.received
    }

    /// How many blobs are held.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.waiting.len()
    }

    /// Offer an ephemeral blob. Returns `false` when it was held instead of dispatched.
    pub fn admit(&mut self, blob: Vec<u8>) -> bool {
        if self.received {
            true
        } else {
            self.waiting.push(blob);
            false
        }
    }

    /// The crucial-events gate: open it and hand back everything held, in
    /// arrival order.
    pub fn open(&mut self) -> Vec<Vec<u8>> {
        self.received = true;
        std::mem::take(&mut self.waiting)
    }

    /// Ending the character session: entering the world again starts the gate closed.
    pub fn reset(&mut self) {
        self.received = false;
        self.waiting.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: f64) -> LocalTime {
        LocalTime(s)
    }

    /// Oracle: retail behaviour. A contiguous
    /// stamp delivers and a gap blocks.
    ///
    /// The stream starts at 1 because the highest stamp starts at 0 and the contiguity test is
    /// `stamp == highest || stamp == highest + 1` — see
    /// [`the_first_stamp_must_be_zero_or_one`].
    #[test]
    fn a_gap_blocks_and_then_fills() {
        let mut w = StampWindow::new();
        assert_eq!(w.add(1, vec![1], t(0.0)), StampResult::Deliver);
        assert_eq!(w.highest(), 1);
        assert_eq!(w.add(2, vec![2], t(0.0)), StampResult::Deliver);
        // 4 arrives before 3.
        assert_eq!(w.add(4, vec![4], t(0.0)), StampResult::Queued);
        assert_eq!(w.blocked_count(), 1);
        assert!(w.drain_ready(t(0.0)).is_empty(), "4 is not ready yet");
        // 3 fills the gap and 4 follows it out.
        assert_eq!(w.add(3, vec![3], t(0.0)), StampResult::Deliver);
        assert_eq!(w.drain_ready(t(0.0)), vec![vec![4]]);
        assert_eq!(w.highest(), 4);
        assert_eq!(w.blocked_count(), 0);
    }

    /// **The first entry is not special-cased into delivery**
    /// (`docs/networking/messages/00-dispatch-and-queues.md` §4, rule 1). The first-entry flag
    /// suppresses only the *staleness* test, and the contiguity test still runs against the highest
    /// stamp seen, which the receive-side stamper initialises to **0**.
    ///
    /// So a stream whose first stamp is neither 0 nor 1 blocks from the outset and stays blocked
    /// until the 300-second breaker. ACE starts `Session.GameEventSequence` at 1 — exactly
    /// `highest + 1` — so this is invisible against ACE and would stall a client against a server
    /// that numbered its first event anything else.
    #[test]
    fn the_first_stamp_must_be_zero_or_one() {
        for first in [0u32, 1] {
            let mut w = StampWindow::new();
            assert_eq!(
                w.add(first, vec![1], t(0.0)),
                StampResult::Deliver,
                "first = {first}"
            );
        }
        let mut w = StampWindow::new();
        assert_eq!(
            w.add(100, vec![1], t(0.0)),
            StampResult::Queued,
            "a first stamp of 100 is blocked, not accepted"
        );
        assert_eq!(w.highest(), 0);
    }

    /// The stale test is a **plain unsigned compare** against `highest`, not a wrap-aware one: a stamp
    /// at or below the highest seen is stale (result 3).
    #[test]
    fn a_stale_stamp_is_dropped_by_a_plain_compare() {
        let mut w = StampWindow::new();
        // Walk the stream up to 100 the way a real one would.
        for s in 1..=100u32 {
            assert_eq!(w.add(s, vec![1], t(0.0)), StampResult::Deliver);
        }
        assert_eq!(w.add(99, vec![0], t(0.0)), StampResult::Old);
        assert_eq!(
            w.add(100, vec![1], t(0.0)),
            StampResult::Old,
            "equal is stale too"
        );
        // A stamp far below is stale even though a wrap-aware reading would call it newer.
        assert_eq!(w.add(1, vec![0], t(0.0)), StampResult::Old);
    }

    /// `StampWindow` force-advances after **more than** 19 blocked stamps
    /// held for **more than** 300 s. Oracle: the blocking add
    /// force-advances when more than `0x13` stamps are blocked, the blocked-since time is non-zero,
    /// and more than 300.0 s have passed since it.
    #[test]
    fn the_deadlock_breaker_needs_both_nineteen_stamps_and_three_hundred_seconds() {
        let mut w = StampWindow::new();
        assert_eq!(w.add(1, vec![1], t(0.0)), StampResult::Deliver);
        // Twenty blocked stamps: 3..=22, leaving a hole at 2. The first one stamps blocked_since.
        for (i, s) in (3..=22u32).enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let r = w.add(s, vec![s as u8], t(0.0));
            assert_eq!(r, StampResult::Queued, "entry {i}");
        }
        assert_eq!(w.blocked_count(), 20);

        // Not yet 300 s: still blocked. Note this call must not be `next_ready`, which would push
        // `blocked_since` forward — that is the client's own behaviour, tested separately.
        assert_eq!(w.add(23, vec![23], t(299.0)), StampResult::Queued);

        // Past 300 s: the missing stamp 2 is fabricated and the queue drains from 3 upward.
        let r = w.add(24, vec![24], t(301.0));
        assert_eq!(r, StampResult::Deliver, "the breaker released the head");
        assert!(w.highest() >= 3, "the fabricated stamp 2 let 3 through");
    }

    /// Nineteen blocked stamps is **not** enough — the test is `>`, not `>=`.
    #[test]
    fn nineteen_blocked_stamps_do_not_trip_the_breaker() {
        let mut w = StampWindow::new();
        assert_eq!(w.add(1, vec![1], t(0.0)), StampResult::Deliver);
        // Eighteen blocked stamps, 3..=20, with the hole at 2. `blocked_since` is stamped at t = 0.
        #[allow(clippy::cast_possible_truncation)]
        for s in 3..=20u32 {
            assert_eq!(w.add(s, vec![s as u8], t(0.0)), StampResult::Queued);
        }
        assert_eq!(w.blocked_count(), 18);

        // The nineteenth blocks even after a very long wait: the test is `>`, not `>=`.
        assert_eq!(
            w.add(21, vec![21], t(10_000.0)),
            StampResult::Queued,
            "19 is not > 19"
        );
        assert_eq!(w.blocked_count(), 19);

        // The twentieth makes it 20 and the breaker fires.
        assert_eq!(w.add(22, vec![22], t(10_000.0)), StampResult::Deliver);
    }

    /// Polling every frame defers the deadlock breaker for ever.
    #[test]
    fn polling_every_frame_defers_the_deadlock_breaker_for_ever() {
        let mut w = StampWindow::new();
        assert_eq!(w.add(1, vec![1], t(0.0)), StampResult::Deliver);
        #[allow(clippy::cast_possible_truncation)]
        for s in 3..=23u32 {
            w.add(s, vec![s as u8], t(0.0));
        }
        assert_eq!(w.blocked_count(), 21);

        // A caller that drains every second keeps pushing the deadline forward.
        let mut now = 0.0;
        while now < 1000.0 {
            now += 1.0;
            assert!(w.drain_ready(t(now)).is_empty());
        }
        // A thousand seconds later the breaker still has not fired.
        assert_eq!(w.add(24, vec![24], t(now)), StampResult::Queued);
        assert_eq!(w.blocked_count(), 22);
    }

    /// The sorted insert's wrap threshold is `0x7FFFFFFE`, not `0x7FFFFFFF`.
    #[test]
    fn the_insert_ordering_wraps_at_one_less_than_half() {
        assert!(stamp_after(5, 4));
        assert!(!stamp_after(4, 5));
        assert!(!stamp_after(4, 4));
        // A difference of exactly 0x7FFFFFFE keeps the plain sense; 0x7FFFFFFF flips it.
        assert!(stamp_after(0x7FFF_FFFE, 0));
        assert!(!stamp_after(0x7FFF_FFFF, 0));
    }

    /// The blocked list must come out in stamp order however it went in.
    #[test]
    fn blocked_entries_are_kept_sorted() {
        let mut w = StampWindow::new();
        for s in 1..=10u32 {
            #[allow(clippy::cast_possible_truncation)]
            w.add(s, vec![s as u8], t(0.0));
        }
        // 11..=15 arrive shuffled and must come out in stamp order.
        for s in [15u32, 12, 14, 13, 11] {
            #[allow(clippy::cast_possible_truncation)]
            w.add(s, vec![s as u8], t(0.0));
        }
        assert_eq!(
            w.drain_ready(t(0.0)),
            vec![vec![12], vec![13], vec![14], vec![15]]
        );
    }

    /// A blob for an unknown object is parked on that object and
    /// replayed when it appears, not dropped.
    #[test]
    fn blobs_are_parked_on_the_object_they_name() {
        let mut p = ParkedBlobs::new();
        p.park(ObjectId(1), vec![0xAA]);
        p.park(ObjectId(1), vec![0xBB]);
        p.park(ObjectId(2), vec![0xCC]);
        assert_eq!(p.total(), 3);
        assert_eq!(p.parked_on(ObjectId(1)), 2);
        assert_eq!(
            p.release(ObjectId(1)),
            vec![vec![0xAA], vec![0xBB]],
            "in arrival order"
        );
        assert_eq!(p.parked_on(ObjectId(1)), 0);
        assert_eq!(p.total(), 1);
        assert!(
            p.release(ObjectId(99)).is_empty(),
            "an object with nothing parked"
        );
    }

    /// A placeholder asks every twenty seconds and lives twenty five past its last park.
    #[test]
    fn a_placeholder_asks_every_twenty_seconds_and_lives_twenty_five_past_its_last_park() {
        let mut p = ParkedBlobs::new();
        let id = ObjectId(0x8000_0201);
        p.set_time(0.0);
        p.park(id, vec![1]);
        p.set_time(10.0);
        p.park(id, vec![2]);
        assert!(p.take_force_objdesc_asks(10.0).is_empty());
        assert!(
            p.take_force_objdesc_asks(20.0).is_empty(),
            "the age test is strict"
        );
        assert_eq!(
            p.take_force_objdesc_asks(20.01),
            vec![id],
            "20 s after the first park, not the latest"
        );
        assert!(p.take_force_objdesc_asks(30.0).is_empty(), "restamped");
        // Alive at 34.9 because the 10 s park moved the deadline to 35 s.
        assert_eq!(p.destroy_expired(34.9), 0);
        p.set_time(34.0);
        p.park(id, vec![3]);
        assert_eq!(p.take_force_objdesc_asks(40.02), vec![id]);
        assert_eq!(p.destroy_expired(58.9), 0);
        assert_eq!(p.destroy_expired(59.1), 1, "25 s after the last park");
        assert!(
            p.take_force_objdesc_asks(80.0).is_empty(),
            "a dead placeholder asks nothing"
        );

        // Released on the create: no more asks.
        p.set_time(100.0);
        p.park(id, vec![4]);
        p.release(id);
        assert!(p.take_force_objdesc_asks(121.0).is_empty());
    }

    /// Nothing dispatches through the ephemeral path until `0x0013` has arrived, and
    /// everything held is then replayed in order.
    #[test]
    fn the_crucial_events_gate_holds_and_then_replays_in_order() {
        let mut g = CrucialEventsGate::new();
        assert!(!g.is_open());
        assert!(!g.admit(vec![1]));
        assert!(!g.admit(vec![2]));
        assert!(!g.admit(vec![3]));
        assert_eq!(g.waiting(), 3);

        let replayed = g.open();
        assert_eq!(replayed, vec![vec![1], vec![2], vec![3]]);
        assert!(g.is_open());
        assert_eq!(g.waiting(), 0);

        // Once open, blobs pass straight through.
        assert!(g.admit(vec![4]));
        assert_eq!(g.waiting(), 0);

        // Leaving the world closes it again.
        g.reset();
        assert!(!g.is_open());
        assert!(!g.admit(vec![5]));
    }
}
