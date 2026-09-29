//! `SequenceGate` — the per-object blob ordering primitive, with the 19/300 s deadlock breaker.
//!
//! The packet layer does not reorder anything. Ordering happens here, per weenie object, on the
//! `OrderedEventHeader` (object id, then stamp) prefix that ordered UI-queue blobs carry.
//!
//! The deadlock breaker is not an implementation detail: it produces the retail behaviour of an
//! object that stalls for up to 300 seconds and then force-skips, which is **visible in play**.
//!
//! See `docs/networking/02-reliability-and-flow.md` §5.

use dereth_primitives::LocalTime;

/// `SequenceGateMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceGateMode {
    /// `TSRECV_BLOCK = 0` — hold out-of-order entries until the gap fills.
    Block = 0,
    /// `TSRECV_LATESTONLY = 1` — keep only the newest, discard anything older.
    LatestOnly = 1,
}

/// What `add_entry_blocking` / `add_entry_latest` return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceGateResult {
    /// `1` — deliver now.
    Deliver = 1,
    /// `2` — queued on the blocked list.
    Queued = 2,
    /// `3` — old or duplicate.
    Old = 3,
}

/// More than this many blocked stamps is half the deadlock-breaker condition. `0x13` = 19, so the
/// breaker needs at least **20** blocked stamps.
pub const DEADLOCK_BLOCKED_STAMPS: usize = 19;

/// The other half: they must have been blocked for longer than this.
pub const DEADLOCK_SECONDS: f64 = 300.0;

/// `SequenceGate`.
#[derive(Debug)]
pub struct SequenceGate {
    mode: SequenceGateMode,
    /// The highest stamp delivered so far.
    highest_stamp: u32,
    /// Whether any entry has arrived: the very first entry skips the `stamp <= highest_stamp`
    /// staleness rejection.
    received_first_entry: bool,
    /// The sorted blocked list. Ascending by stamp.
    blocked: Vec<u32>,
    /// When the list became non-empty. `None` is the client's `0.0`.
    blocked_since: Option<LocalTime>,
    /// The overflow limit — set by the constructor and **never read anywhere in the client**.
    /// Stored so the object matches; the blocked-queue
    /// overflow is governed by the 19/300 s rule, which *is* read.
    overflow_limit: u32,
}

impl SequenceGate {
    #[must_use]
    pub fn new(mode: SequenceGateMode) -> Self {
        Self::with_overflow_limit(mode, 0)
    }

    #[must_use]
    pub fn with_overflow_limit(mode: SequenceGateMode, overflow_limit: u32) -> Self {
        Self {
            mode,
            highest_stamp: 0,
            received_first_entry: false,
            blocked: Vec::new(),
            blocked_since: None,
            overflow_limit,
        }
    }

    /// The overflow limit. Nothing reads it.
    #[must_use]
    pub fn overflow_limit(&self) -> u32 {
        self.overflow_limit
    }

    #[must_use]
    pub fn highest_stamp(&self) -> u32 {
        self.highest_stamp
    }

    #[must_use]
    pub fn blocked_count(&self) -> usize {
        self.blocked.len()
    }

    /// Dispatch on the mode.
    pub fn add_entry(&mut self, stamp: u32, now: LocalTime) -> SequenceGateResult {
        match self.mode {
            SequenceGateMode::Block => self.add_entry_blocking(stamp, now),
            SequenceGateMode::LatestOnly => self.add_entry_latest(stamp),
        }
    }

    /// The sorted insert, transcribed.
    ///
    /// Note two things that are easy to miss:
    ///
    /// - the `stamp <= highest_stamp` rejection is a **plain unsigned** comparison, not a wrapping
    ///   one, so this stamp space genuinely does not wrap;
    /// - an entry whose stamp *equals* `highest_stamp` is **delivered**, not treated as a
    ///   duplicate. Only a strictly smaller stamp is `Old`.
    pub fn add_entry_blocking(&mut self, stamp: u32, now: LocalTime) -> SequenceGateResult {
        if self.received_first_entry {
            if stamp <= self.highest_stamp {
                return SequenceGateResult::Old;
            }
        } else {
            self.received_first_entry = true;
        }

        if stamp == self.highest_stamp || stamp == self.highest_stamp.wrapping_add(1) {
            self.highest_stamp = stamp;
            return SequenceGateResult::Deliver;
        }

        let was_empty = self.blocked.is_empty();
        match self.blocked.binary_search(&stamp) {
            Ok(_) => return SequenceGateResult::Old, // the sorted insert's duplicate result
            Err(at) => self.blocked.insert(at, stamp),
        }
        if was_empty && self.blocked.len() == 1 {
            self.blocked_since = Some(now);
        }

        // The deadlock breaker: more than 0x13 blocked stamps, a non-zero blocked-since time, and
        // more than 300 s since it. Force-insert the missing `highest_stamp + 1` as a null entry
        // and let the queue drain.
        if self.blocked.len() > DEADLOCK_BLOCKED_STAMPS {
            if let Some(since) = self.blocked_since {
                if now.seconds_since(since) > DEADLOCK_SECONDS {
                    let next = self.highest_stamp.wrapping_add(1);
                    self.add_entry_blocking(next, now);
                    return if self.next_ready_entry(now).is_some() {
                        SequenceGateResult::Deliver
                    } else {
                        SequenceGateResult::Queued
                    };
                }
            }
        }
        SequenceGateResult::Queued
    }

    /// Keep the newest, discard anything older.
    ///
    /// The wrapping compare here uses `0x7FFFFFFE` as its threshold, **not** the `0x7FFFFFFF` the
    /// transport uses. The one-off is in the shipped code; it means a stamp
    /// exactly `0x7FFFFFFF` behind is treated as newer here and as older there.
    pub fn add_entry_latest(&mut self, stamp: u32) -> SequenceGateResult {
        self.received_first_entry = true;
        if stamp == self.highest_stamp {
            return SequenceGateResult::Old;
        }
        let (diff, mut sign) = if stamp < self.highest_stamp {
            (self.highest_stamp - stamp, -1i32)
        } else {
            (stamp - self.highest_stamp, 1i32)
        };
        if diff > 0x7FFF_FFFE {
            sign = -sign;
        }
        if sign > 0 {
            self.highest_stamp = stamp;
            return SequenceGateResult::Deliver;
        }
        SequenceGateResult::Old
    }

    /// Pop the head of the blocked list if it is now
    /// contiguous.
    ///
    /// Note the last branch, transcribed as-is: when the head is *not* ready and the list is not
    /// empty, `blocked_since` is **reset to now**. A caller that polls this every frame therefore
    /// pushes the 300-second deadline out indefinitely and the breaker never fires. That is what
    /// the shipped code does.
    pub fn next_ready_entry(&mut self, now: LocalTime) -> Option<u32> {
        let &first = self.blocked.first()?;
        if first == self.highest_stamp || first == self.highest_stamp.wrapping_add(1) {
            self.blocked.remove(0);
            self.highest_stamp = first;
            if self.blocked.is_empty() {
                self.blocked_since = None;
            }
            return Some(first);
        }
        if !self.blocked.is_empty() {
            self.blocked_since = Some(now);
        }
        None
    }

    /// Everything that is ready now, in order. A convenience over repeated
    /// the next-ready-entry query.
    ///
    /// It takes `now` and returns a `Vec` rather than a lazy iterator, because the next-ready-entry
    /// step reads and writes the clock-dependent `blocked_since` and a lazy iterator would make
    /// *when* that happens depend on how the caller consumes it.
    pub fn drain_ready(&mut self, now: LocalTime) -> Vec<u32> {
        let mut out = Vec::new();
        while let Some(stamp) = self.next_ready_entry(now) {
            out.push(stamp);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the client's own receive-side time stamper, and
    /// `docs/networking/02-reliability-and-flow.md` §5.
    #[test]
    fn blocking_delivers_highest_and_highest_plus_one_and_blocks_the_rest() {
        let mut ts = SequenceGate::new(SequenceGateMode::Block);
        let t = LocalTime(0.0);

        // The very first entry skips the `stamp <= highest` rejection; with highest = 0 a stamp of
        // 1 delivers.
        assert_eq!(ts.add_entry(1, t), SequenceGateResult::Deliver);
        assert_eq!(ts.highest_stamp(), 1);

        // Equal to highest: delivered, not treated as a duplicate.
        assert_eq!(
            ts.add_entry(1, t),
            SequenceGateResult::Old,
            "not the first entry any more"
        );
        assert_eq!(ts.add_entry(2, t), SequenceGateResult::Deliver);

        // A gap blocks.
        assert_eq!(ts.add_entry(5, t), SequenceGateResult::Queued);
        assert_eq!(ts.add_entry(4, t), SequenceGateResult::Queued);
        assert_eq!(ts.blocked_count(), 2);
        assert_eq!(ts.highest_stamp(), 2);

        // Filling the gap delivers 3 and then drains 4 and 5 in order.
        assert_eq!(ts.add_entry(3, t), SequenceGateResult::Deliver);
        assert_eq!(ts.drain_ready(t), vec![4, 5]);
        assert_eq!(ts.blocked_count(), 0);
        assert_eq!(ts.highest_stamp(), 5);
    }

    /// A strictly smaller stamp is `Old`; a duplicate already on the blocked list is too.
    #[test]
    fn blocking_rejects_old_and_duplicate_stamps() {
        let mut ts = SequenceGate::new(SequenceGateMode::Block);
        let t = LocalTime(0.0);
        // Walk up to 10 the only way the blocking mode allows: contiguously.
        for s in 1..=10u32 {
            assert_eq!(ts.add_entry(s, t), SequenceGateResult::Deliver, "stamp {s}");
        }
        assert_eq!(ts.add_entry(9, t), SequenceGateResult::Old);
        assert_eq!(ts.add_entry(20, t), SequenceGateResult::Queued);
        assert_eq!(
            ts.add_entry(20, t),
            SequenceGateResult::Old,
            "duplicate on the blocked list"
        );
        assert_eq!(ts.blocked_count(), 1);
    }

    /// The first entry is accepted whatever its stamp, but is only *delivered* if it happens to be
    /// 0 or 1 — anything else goes straight onto the blocked list.
    #[test]
    fn the_very_first_entry_is_not_special_cased_into_delivery() {
        let mut ts = SequenceGate::new(SequenceGateMode::Block);
        assert_eq!(
            ts.add_entry(100, LocalTime(0.0)),
            SequenceGateResult::Queued
        );
        assert_eq!(ts.highest_stamp(), 0);
        assert_eq!(ts.blocked_count(), 1);
    }

    /// Latest-only keeps the newest and returns `Old` for anything else.
    #[test]
    fn latest_only_keeps_the_newest() {
        let mut ts = SequenceGate::new(SequenceGateMode::LatestOnly);
        let t = LocalTime(0.0);
        assert_eq!(ts.add_entry(5, t), SequenceGateResult::Deliver);
        assert_eq!(ts.highest_stamp(), 5);
        assert_eq!(ts.add_entry(4, t), SequenceGateResult::Old);
        assert_eq!(ts.add_entry(5, t), SequenceGateResult::Old);
        assert_eq!(ts.add_entry(9, t), SequenceGateResult::Deliver);
        assert_eq!(ts.highest_stamp(), 9);
        assert_eq!(ts.blocked_count(), 0, "latest-only never blocks");
    }

    /// `add_entry_latest`'s threshold is `0x7FFFFFFE`, one less than `lhs_newer`'s. The difference is
    /// observable at exactly `0x7FFFFFFF`.
    #[test]
    fn latest_only_uses_0x7ffffffe_not_0x7fffffff() {
        let mut ts = SequenceGate::new(SequenceGateMode::LatestOnly);
        ts.add_entry_latest(0);
        // stamp - highest = 0x7FFFFFFF, which is NOT > 0x7FFFFFFE... it is. So the sign flips and
        // this reads as older.
        assert_eq!(ts.add_entry_latest(0x7FFF_FFFF), SequenceGateResult::Old);
        // While `lhs_newer(0x7FFFFFFF, 0)` is true, because its threshold is 0x7FFFFFFF.
        assert!(dereth_transport::session::lhs_newer(0x7FFF_FFFF, 0));
    }

    /// The deadlock breaker: more than 19 blocked stamps held for more than 300 seconds
    /// force-inserts the highest stamp plus one and the queue drains.
    ///
    /// This is the mechanism behind the retail behaviour of an object that stalls for up to five
    /// minutes and then force-skips. `docs/networking/messages/00-dispatch-and-queues.md` §4.
    #[test]
    fn the_deadlock_breaker_fires_at_more_than_19_stamps_held_more_than_300_seconds() {
        let mut ts = SequenceGate::new(SequenceGateMode::Block);
        let t0 = LocalTime(0.0);
        assert_eq!(ts.add_entry(1, t0), SequenceGateResult::Deliver);

        // Stamps 3..=21 block: nineteen of them, one short of the threshold.
        for s in 3..=21u32 {
            assert_eq!(ts.add_entry(s, t0), SequenceGateResult::Queued, "stamp {s}");
        }
        assert_eq!(ts.blocked_count(), 19);

        // 400 seconds later, still only nineteen: `0x13 < blocked count` is false.
        assert_eq!(
            ts.add_entry(2, LocalTime(400.0)),
            SequenceGateResult::Deliver
        );
        assert_eq!(ts.highest_stamp(), 2);
        // (that one filled the gap, so reset and rebuild the jam)

        let mut ts = SequenceGate::new(SequenceGateMode::Block);
        assert_eq!(ts.add_entry(1, t0), SequenceGateResult::Deliver);
        for s in 3..=21u32 {
            ts.add_entry(s, t0);
        }
        // The twentieth blocked stamp, but only 299 seconds in: not yet.
        assert_eq!(
            ts.add_entry(22, LocalTime(299.0)),
            SequenceGateResult::Queued
        );
        assert_eq!(ts.blocked_count(), 20);
        assert_eq!(ts.highest_stamp(), 1, "still stuck");

        // The twenty-first, past 300 seconds: the breaker fires. Stamp 2 is force-inserted as a
        // null entry, so highest advances to 2 and then drains the run 3..=23.
        assert_eq!(
            ts.add_entry(23, LocalTime(301.0)),
            SequenceGateResult::Deliver
        );
        assert_eq!(ts.highest_stamp(), 3, "2 force-inserted, then 3 drained");
        assert_eq!(
            ts.drain_ready(LocalTime(301.0)),
            (4..=23).collect::<Vec<u32>>()
        );
        assert_eq!(ts.blocked_count(), 0);
    }

    /// Transcribed: polling while the head is not ready resets
    /// the blocked-since time, so a caller that drains every frame pushes the 300-second deadline
    /// out forever and the breaker never fires. Pinned here so a future "cleanup" is a deliberate act.
    #[test]
    fn polling_a_blocked_queue_resets_the_deadlock_timer() {
        let mut ts = SequenceGate::new(SequenceGateMode::Block);
        ts.add_entry(1, LocalTime(0.0));
        for s in 3..=22u32 {
            ts.add_entry(s, LocalTime(0.0));
        }
        assert_eq!(ts.blocked_count(), 20);

        // A poll at t = 299 finds nothing ready and restarts the clock.
        assert_eq!(ts.next_ready_entry(LocalTime(299.0)), None);
        // So at t = 301 only 2 seconds have elapsed since the blocked-since time, and the breaker
        // does not fire even though twenty stamps have been blocked for 301 seconds of wall time.
        assert_eq!(
            ts.add_entry(23, LocalTime(301.0)),
            SequenceGateResult::Queued
        );
        assert_eq!(ts.highest_stamp(), 1);
    }

    /// The overflow limit is stored and never consulted. Compatibility note #58.
    #[test]
    fn overflow_limit_is_stored_and_never_read() {
        let mut ts = SequenceGate::with_overflow_limit(SequenceGateMode::Block, 3);
        assert_eq!(ts.overflow_limit(), 3);
        ts.add_entry(1, LocalTime(0.0));
        for s in 3..=10u32 {
            assert_eq!(ts.add_entry(s, LocalTime(0.0)), SequenceGateResult::Queued);
        }
        assert_eq!(
            ts.blocked_count(),
            8,
            "the blocked list grows past the overflow limit without complaint"
        );
    }
}
