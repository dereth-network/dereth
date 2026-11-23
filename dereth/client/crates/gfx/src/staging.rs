//! The staging arenas uploads pass through, the same on every backend: filled for work not yet
//! recorded, then waiting for the recorded work that reads them, then free to fill again.
//!
//! A frame of steady play fills one small arena and gets back the one it filled a few frames
//! before, so a short free list serves it. A burst (a world written to the device at once) fills
//! many arenas in one go; once the device has read them, those beyond [`KEPT_BYTES`] go back to
//! the system instead of staying on the free list for the life of the device.

/// The smallest staging arena, bytes.
pub(crate) const ARENA_BYTES: u64 = 8 << 20;

/// The most bytes of free arenas kept for later uploads: room for a few frames that fill more
/// than one arena, past the one a frame of steady play takes back.
pub(crate) const KEPT_BYTES: u64 = 4 * ARENA_BYTES;

/// A staging arena's size, the one thing the bookkeeping needs of it.
pub(crate) trait Arena {
    /// Its bytes.
    fn capacity(&self) -> u64;
}

/// The arenas, by where each is in its round, and `R`, the backend's mark of when recorded work
/// is finished.
pub(crate) struct Staging<A, R> {
    /// Being filled, for work not yet recorded; the last is the one filled next.
    pub current: Vec<A>,
    /// Read by recorded work, each with when that work is finished.
    waiting: Vec<(R, A)>,
    /// Free to fill again.
    free: Vec<A>,
    /// The free arenas' bytes.
    free_bytes: u64,
}

impl<A, R> Default for Staging<A, R> {
    fn default() -> Self {
        Self {
            current: Vec::new(),
            waiting: Vec::new(),
            free: Vec::new(),
            free_bytes: 0,
        }
    }
}

impl<A: Arena, R: Copy> Staging<A, R> {
    /// A free arena of at least `len` bytes, to fill from its start.
    pub(crate) fn take(&mut self, len: u64) -> Option<A> {
        let i = self.free.iter().position(|a| a.capacity() >= len)?;
        let a = self.free.swap_remove(i);
        self.free_bytes -= a.capacity();
        Some(a)
    }

    /// The arenas filled so far are read by work that is finished at `at`.
    pub(crate) fn recorded(&mut self, at: R) {
        self.waiting.extend(
            std::mem::take(&mut self.current)
                .into_iter()
                .map(|a| (at, a)),
        );
    }

    /// Whether any arena waits for recorded work.
    pub(crate) fn waits(&self) -> bool {
        !self.waiting.is_empty()
    }

    /// Take back the arenas whose work `done` says is finished: onto the free list while it
    /// holds no more than [`KEPT_BYTES`], the rest returned for the caller to release. Only
    /// arenas whose work is finished are ever returned.
    pub(crate) fn collect(&mut self, done: impl Fn(&R) -> bool) -> Vec<A> {
        let mut release = Vec::new();
        let (back, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting)
            .into_iter()
            .partition(|(r, _)| done(r));
        self.waiting = keep;
        for (_, a) in back {
            if self.free_bytes + a.capacity() <= KEPT_BYTES {
                self.free_bytes += a.capacity();
                self.free.push(a);
            } else {
                release.push(a);
            }
        }
        release
    }

    /// Every arena, wherever it is in its round, for a device that is done with all of them.
    pub(crate) fn into_arenas(self) -> impl Iterator<Item = A> {
        self.current
            .into_iter()
            .chain(self.waiting.into_iter().map(|(_, a)| a))
            .chain(self.free)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the device's own memory bookkeeping; no retail counterpart)
    use super::*;

    /// A fake arena: its bytes, and a name to follow it by.
    #[derive(Debug)]
    struct Fake {
        id: u32,
        bytes: u64,
    }

    impl Arena for Fake {
        fn capacity(&self) -> u64 {
            self.bytes
        }
    }

    /// A device's staging driven frame by frame: frames are recorded in order and up to
    /// `in_flight` of them may still be running, so frame `n` is finished once frame
    /// `n + in_flight` is being recorded.
    struct Frames {
        staging: Staging<Fake, u64>,
        in_flight: u64,
        frame: u64,
        made: u32,
        /// When each arena's last reader finishes.
        read_until: std::collections::HashMap<u32, u64>,
        released: Vec<Fake>,
    }

    impl Frames {
        fn new(in_flight: u64) -> Self {
            Self {
                staging: Staging::default(),
                in_flight,
                frame: 0,
                made: 0,
                read_until: std::collections::HashMap::new(),
                released: Vec::new(),
            }
        }

        /// Fill `arenas` arenas of `bytes` each, from the free list or made new.
        fn fill(&mut self, arenas: u32, bytes: u64) {
            for _ in 0..arenas {
                let a = self.staging.take(bytes).unwrap_or_else(|| {
                    self.made += 1;
                    Fake {
                        id: self.made,
                        bytes,
                    }
                });
                self.staging.current.push(a);
            }
        }

        /// The next frame's recording: what finished is taken back, then what was filled is
        /// recorded into this frame.
        fn record(&mut self) {
            self.frame += 1;
            let finished = self.frame.saturating_sub(self.in_flight);
            let back = self.staging.collect(|r| *r <= finished);
            for a in &back {
                let until = self.read_until[&a.id];
                assert!(
                    until <= finished,
                    "arena {} released while frame {until} may still read it (finished {finished})",
                    a.id
                );
            }
            self.released.extend(back);
            for a in &self.staging.current {
                self.read_until.insert(a.id, self.frame);
            }
            self.staging.recorded(self.frame);
        }
    }

    /// Steady play, one arena a frame with two frames in flight, makes its arenas once and then
    /// only reuses them: nothing is released and nothing new is made.
    #[test]
    fn steady_frames_reuse_their_arenas_and_release_none() {
        let mut f = Frames::new(2);
        for _ in 0..1000 {
            f.fill(1, ARENA_BYTES);
            f.record();
        }
        assert_eq!(
            f.made, 3,
            "one arena for each frame that may be running, and the next"
        );
        assert!(f.released.is_empty(), "released {:?}", f.released);
    }

    /// A frame now and then that fills a few arenas more than steady play makes them once and
    /// keeps them for the next such frame.
    #[test]
    fn an_occasional_heavier_frame_keeps_its_extra_arenas() {
        let mut f = Frames::new(2);
        for n in 0..1000 {
            f.fill(if n % 50 == 0 { 4 } else { 1 }, ARENA_BYTES);
            f.record();
        }
        assert_eq!(f.made, 6);
        assert!(f.released.is_empty(), "released {:?}", f.released);
    }

    /// A burst of arenas waits while the frame that reads them may still run, then all but the
    /// bytes the free list keeps go back, and steady play after it makes nothing new.
    #[test]
    fn a_bursts_arenas_are_released_once_the_device_has_read_them() {
        let mut f = Frames::new(2);
        for _ in 0..10 {
            f.fill(1, ARENA_BYTES);
            f.record();
        }
        f.fill(57, ARENA_BYTES);
        f.record();
        // The next frame: the burst's may still be running.
        f.fill(1, ARENA_BYTES);
        f.record();
        assert!(
            f.released.is_empty(),
            "released before the burst's frame finished"
        );
        // The one after: the burst's frame is finished.
        f.fill(1, ARENA_BYTES);
        f.record();
        let made = f.made;
        let kept = (KEPT_BYTES / ARENA_BYTES) as usize;
        assert_eq!(
            f.released.len(),
            57 - kept,
            "all but what the free list keeps"
        );
        assert_eq!(f.staging.free_bytes, KEPT_BYTES);
        for _ in 0..1000 {
            f.fill(1, ARENA_BYTES);
            f.record();
        }
        assert_eq!(f.made, made, "steady play after the burst makes nothing");
        assert_eq!(f.released.len(), 57 - kept, "and releases nothing more");
    }

    /// An arena bigger than the free list keeps, made for one large upload, is released once read
    /// rather than kept.
    #[test]
    fn an_arena_larger_than_the_free_list_keeps_is_released_once_read() {
        let mut f = Frames::new(2);
        f.fill(1, 8 * KEPT_BYTES);
        f.record();
        f.record();
        assert!(f.released.is_empty());
        f.record();
        assert_eq!(f.released.len(), 1);
        assert_eq!(f.released[0].bytes, 8 * KEPT_BYTES);
        assert_eq!(f.staging.free_bytes, 0);
    }

    /// Arenas being filled, and those whose work may still run, are never released, however
    /// many there are; a device that is done with everything gets every one back.
    #[test]
    fn arenas_the_device_may_still_read_are_never_released() {
        let mut s: Staging<Fake, u64> = Staging::default();
        for id in 0..40 {
            s.current.push(Fake {
                id,
                bytes: ARENA_BYTES,
            });
        }
        s.recorded(7);
        for id in 40..80 {
            s.current.push(Fake {
                id,
                bytes: ARENA_BYTES,
            });
        }
        assert!(s.collect(|r| *r <= 6).is_empty());
        assert!(s.waits());
        assert_eq!(s.free_bytes, 0);
        let back = s.collect(|r| *r <= 7);
        assert!(back.iter().all(|a| a.id < 40), "only the recorded ones");
        assert_eq!(back.len(), 40 - (KEPT_BYTES / ARENA_BYTES) as usize);
        assert!(!s.waits());
        assert_eq!(s.current.len(), 40, "the ones being filled stay");
        assert_eq!(
            s.into_arenas().count(),
            40 + (KEPT_BYTES / ARENA_BYTES) as usize
        );
    }
}
