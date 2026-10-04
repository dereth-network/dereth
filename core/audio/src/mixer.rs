//! The voice pool: 16 fixed voices, the round-robin free-slot scan, and the DirectSound pan law.
//!
//! This is the client's fixed pool of 16 playing-sound slots (16 bytes each) and the scan over
//! it, which is the single choke point every sound in the game passes through.
//!
//! Three shipped behaviours live here and all three must be kept:
//!
//! * **The scan is round-robin and stateful.** It starts at the saved cursor and
//!   wraps, and the cursor is left one past the slot just used. It decides *which* sound is dropped
//!   when the world is noisy, which a player can hear.
//! * **The priority system is inert, so the 17th simultaneous sound is dropped.** Both sides of the
//!   stealing test read the voice's own priority, and that field is written only by the play
//!   path, which sets it to
//!   0.0. The comparison is a **strict** less-than, so equal
//!   priorities never steal.
//! * **Nothing loops, ever.** `IDirectSoundBuffer::Play(0, 0, 0)` -- `dwFlags` is hard zero at
//!   the one call site. `DSBCAPS_CTRLFREQUENCY` is requested and `SetFrequency` is never
//!   called, so there is no pitch variation either.
//!
//! Each playing sound's `start_time` is written and never read; there is no
//! age-based stealing and none is modelled here.

use dereth_primitives::num::math;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The playing-sound pool is a fixed array of 16.
pub const NUM_VOICES: usize = 16;

/// One playing voice.
///
/// Each playing sound shares its sample memory and has its own play cursor, volume and pan.
/// Volume and pan are chosen when playback starts and never changed mid-play.
/// The gains are therefore precomputed here, with no per-sample ramp.
/// Adding a ramp would change the playback behavior.
#[derive(Debug, Clone)]
pub struct Voice {
    /// Interleaved stereo at [`crate::wave::MIX_RATE`].
    pub sample: Arc<[f32]>,
    /// Sample index, in individual `f32`s (so it advances by 2 per frame).
    pub pos: usize,
    /// Left gain, after the dB volume and the pan law.
    pub gl: f32,
    /// Right gain.
    pub gr: f32,
    /// Priority copied from the cached buffer metadata.
    ///
    /// Always 0.0 for every sound the retail client plays; kept because the *field* is what the
    /// stealing test reads, and a test that asserts the drop must be able to show that.
    pub priority: f32,
}

impl Voice {
    /// `GetStatus() & DSBSTATUS_PLAYING`. A voice that has run off the end of its sample is a free
    /// slot; the original `delete`s the duplicated buffer at that point.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.pos < self.sample.len()
    }
}

/// `10^(db/20)` — DirectSound's `SetVolume` takes hundredths of a dB, and the engine's whole volume
/// resolution is 1 dB because the attenuation calculation returns an integer.
#[must_use]
pub fn gain_from_db(db: i32) -> f32 {
    math::powf(10f32, db as f32 / 20.0)
}

/// The DirectSound pan law, as the client uses it.
///
/// `SetPan(p)` leaves one channel at full level and attenuates the other by `|p|` — **positive pan
/// attenuates the left channel** (`DSBPAN_RIGHT = +10000`). It is linear in dB with no centre
/// compensation, and because the client clamps to +/-15 the far channel is never
/// silenced: at most -15 dB.
#[must_use]
pub fn pan_gains(db: i32, pan: i32) -> (f32, f32) {
    let g = gain_from_db(db);
    let att = gain_from_db(-pan.abs());
    if pan >= 0 {
        (g * att, g)
    } else {
        (g, g * att)
    }
}

/// The 16-voice pool.
///
/// `muted` is the mechanism DirectSound gave the original for free: no secondary buffer carries
/// `DSBCAPS_GLOBALFOCUS`, so DirectSound silenced every one of them while the client was not the
/// foreground application, while the client's own state machine carried on. A modern API has no such
/// rule, so the mute is explicit — and it must silence the *output*, not stop the voices, or
/// already-playing sounds would resume where they left off instead of having played on inaudibly.
/// See [`crate::focus`].
#[derive(Debug)]
pub struct VoicePool {
    voices: [Option<Voice>; NUM_VOICES],
    /// The round-robin cursor the client keeps, always 0..15.
    cursor: usize,
    muted: Arc<AtomicBool>,
}

impl Default for VoicePool {
    fn default() -> Self {
        Self::new(Arc::new(AtomicBool::new(false)))
    }
}

/// Why a [`VoicePool::start`] call did not produce a voice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartOutcome {
    /// The slot the sound was placed in.
    Started(usize),
    /// All 16 voices were busy and no victim had a strictly lower priority. In the retail client
    /// this is *every* overflow, because every priority is 0.0.
    Dropped,
}

impl VoicePool {
    #[must_use]
    pub fn new(muted: Arc<AtomicBool>) -> Self {
        Self {
            voices: [const { None }; NUM_VOICES],
            cursor: 0,
            muted,
        }
    }

    /// The currently playing buffer.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// How many voices are still playing.
    #[must_use]
    pub fn active(&self) -> usize {
        self.voices
            .iter()
            .flatten()
            .filter(|v| v.is_playing())
            .count()
    }

    /// Read a slot, for tests.
    #[must_use]
    pub fn voice(&self, slot: usize) -> Option<&Voice> {
        self.voices.get(slot).and_then(Option::as_ref)
    }

    /// Start a sound, minus the focus test its callers already did.
    ///
    /// Pass 1 scans for a free slot from the cursor, wrapping: `(cursor + i) & 15` for `i` in 0..16.
    /// Pass 2 looks for `victim.priority < incoming` and, finding none, **drops the sound**. Model it
    /// as drop — every shipped `priority` is 0.0 and the test is strict.
    pub fn start(&mut self, sample: Arc<[f32]>, db: i32, pan: i32, priority: f32) -> StartOutcome {
        let (gl, gr) = pan_gains(db, pan);
        // Pass 1: a slot with no buffer, and a slot whose buffer is no longer `DSBSTATUS_PLAYING`,
        // are both "this slot is available".
        let free = (0..NUM_VOICES)
            .map(|i| (self.cursor + i) & (NUM_VOICES - 1))
            .find(|&s| self.voices[s].as_ref().is_none_or(|v| !v.is_playing()));

        let slot = match free {
            Some(s) => s,
            None => {
                // Pass 2: the same wrapping scan, looking for a strictly lower priority.
                let victim = (0..NUM_VOICES)
                    .map(|i| (self.cursor + i) & (NUM_VOICES - 1))
                    .find(|&s| {
                        self.voices[s]
                            .as_ref()
                            .is_some_and(|v| v.priority < priority)
                    });
                match victim {
                    // Steal: stop the victim's buffer, delete it, then allocate a new one.
                    Some(s) => s,
                    // No victim -- the sound is dropped.
                    None => return StartOutcome::Dropped,
                }
            }
        };

        self.voices[slot] = Some(Voice {
            sample,
            pos: 0,
            gl,
            gr,
            priority,
        });
        // The client leaves its scan cursor at `(slot + 1) & 15`.
        self.cursor = (slot + 1) & (NUM_VOICES - 1);
        StartOutcome::Started(slot)
    }

    /// Mix every playing voice into an interleaved stereo buffer.
    ///
    /// `out` is *not* cleared: the caller owns the block. Voices advance whether or not the output is
    /// muted, because DirectSound's focus muting silenced the mix and did not pause the buffers.
    pub fn mix(&mut self, out: &mut [f32]) {
        let n = out.len() & !1;
        for v in self.voices.iter_mut().flatten() {
            let take = (v.sample.len() - v.pos.min(v.sample.len())).min(n);
            for i in 0..take / 2 {
                out[2 * i] += v.sample[v.pos + 2 * i] * v.gl;
                out[2 * i + 1] += v.sample[v.pos + 2 * i + 1] * v.gr;
            }
            v.pos += take;
        }
        if self.muted.load(Ordering::Relaxed) {
            out.fill(0.0);
        }
    }

    /// Shutdown step 2: stop and delete all 16 slots.
    pub fn stop_all(&mut self) {
        self.voices = [const { None }; NUM_VOICES];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(frames: usize) -> Arc<[f32]> {
        vec![1.0f32; frames * 2].into()
    }

    /// Oracle: the recovered slot-scan behaviour and the client's own scan -- the
    /// slot index is masked to 0..15 on every step, and the cursor is left one past the slot just
    /// used.
    #[test]
    fn the_free_slot_scan_is_round_robin_and_leaves_the_cursor_one_past() {
        let mut p = VoicePool::default();
        // Sixteen starts fill 0..15 in order and leave the cursor back at 0.
        for expect in 0..NUM_VOICES {
            assert_eq!(
                p.start(sample(1000), 0, 0, 0.0),
                StartOutcome::Started(expect)
            );
            assert_eq!(p.cursor(), (expect + 1) & 15);
        }
        assert_eq!(p.cursor(), 0);
        assert_eq!(p.active(), NUM_VOICES);
    }

    /// The whole point of the round robin: after a sound ends, the next start does **not** go to slot
    /// 0, it goes to the first free slot at or after the cursor. Oracle: the same scan, and
    /// the voice-dropping rule when the world is noisy.
    #[test]
    fn the_scan_starts_at_the_cursor_not_at_zero() {
        let mut p = VoicePool::default();
        for _ in 0..NUM_VOICES {
            p.start(sample(1000), 0, 0, 0.0);
        }
        // Free slots 0 and 9 by leaving their voices at the end of their samples, which is exactly
        // what `GetStatus() & DSBSTATUS_PLAYING` going false looks like to the scan.
        for s in [0usize, 9] {
            p.voices[s] = Some(Voice {
                sample: sample(1),
                pos: 2,
                gl: 1.0,
                gr: 1.0,
                priority: 0.0,
            });
        }
        assert_eq!(p.cursor(), 0);
        // Cursor is 0, so slot 0 wins.
        assert_eq!(p.start(sample(1000), 0, 0, 0.0), StartOutcome::Started(0));
        assert_eq!(p.cursor(), 1);
        // Cursor is 1, so the scan walks 1..8 (all busy) and lands on 9, not back on 0.
        assert_eq!(p.start(sample(1000), 0, 0, 0.0), StartOutcome::Started(9));
        assert_eq!(p.cursor(), 10);
    }

    /// The 17th simultaneous sound is **dropped**, not stolen, regardless of how
    /// important it is, because every shipped priority is 0.0 and the test is strictly less-than.
    /// A whole-image writer census found no sound-buffer metadata writer outside its constructor.
    #[test]
    fn the_seventeenth_simultaneous_sound_is_dropped_not_stolen() {
        let mut p = VoicePool::default();
        for _ in 0..NUM_VOICES {
            p.start(sample(1000), 0, 0, 0.0);
        }
        let before = p.cursor();
        // Everything in the pool has priority 0.0 -- the value the play path
        // writes -- and so does the incoming sound, so `victim.priority < incoming` is false for all
        // sixteen slots.
        assert_eq!(p.start(sample(1000), 0, 0, 0.0), StartOutcome::Dropped);
        assert_eq!(
            p.cursor(),
            before,
            "a dropped sound must not move the cursor"
        );
        assert_eq!(p.active(), NUM_VOICES);
    }

    /// The plumbing is *there* — this is what would happen if the `priority_` field were ever
    /// written. Fixing that is a deviation-ledger entry, not a bug fix; this test exists so that the
    /// difference between "inert" and "absent" stays visible.
    #[test]
    fn stealing_would_work_if_a_priority_were_ever_written_but_equal_priorities_never_steal() {
        let mut p = VoicePool::default();
        for _ in 0..NUM_VOICES {
            p.start(sample(1000), 0, 0, 0.0);
        }
        // Strictly greater steals the first slot at or after the cursor.
        assert_eq!(p.start(sample(1000), 0, 0, 0.5), StartOutcome::Started(0));
        // Equal does not: the client's comparison is a strict `<`.
        let mut q = VoicePool::default();
        for _ in 0..NUM_VOICES {
            q.start(sample(1000), 0, 0, 1.0);
        }
        assert_eq!(q.start(sample(1000), 0, 0, 1.0), StartOutcome::Dropped);
    }

    /// Oracle: the recovered pan law — one channel at full level, the other
    /// attenuated by `|pan|` dB, positive attenuating the **left**.
    #[test]
    fn the_pan_law_attenuates_one_side_and_never_cuts_it() {
        let (l, r) = pan_gains(0, 0);
        assert_eq!((l, r), (1.0, 1.0));
        let (l, r) = pan_gains(0, 15);
        assert_eq!(
            r, 1.0,
            "positive pan leaves the right channel at full level"
        );
        assert!(
            (l - math::powf(10f32, -15.0 / 20.0)).abs() < 1e-6,
            "left is -15 dB, not silent"
        );
        assert!(
            l > 0.17,
            "the far channel is only 15 dB down, never a hard cut"
        );
        let (l, r) = pan_gains(0, -15);
        assert_eq!(l, 1.0);
        assert!((r - math::powf(10f32, -15.0 / 20.0)).abs() < 1e-6);
        // The dB volume multiplies both sides.
        let (l, r) = pan_gains(-6, 0);
        let g = math::powf(10f32, -6.0 / 20.0);
        assert!((l - g).abs() < 1e-6 && (r - g).abs() < 1e-6);
    }

    /// Nothing loops: a voice runs off the end of its sample and frees its slot.
    #[test]
    fn a_voice_stops_at_the_end_of_its_sample_and_never_wraps() {
        let mut p = VoicePool::default();
        p.start(sample(4), 0, 0, 0.0);
        let mut out = vec![0.0f32; 8];
        p.mix(&mut out);
        assert_eq!(out, vec![1.0f32; 8]);
        assert_eq!(
            p.active(),
            0,
            "four frames of sample, four frames of output, done"
        );
        let mut out2 = vec![0.0f32; 8];
        p.mix(&mut out2);
        assert_eq!(
            out2,
            vec![0.0f32; 8],
            "a finished voice contributes nothing, it does not loop"
        );
    }

    /// The explicit master mute replaces DirectSound's focus behaviour: the output is silenced but
    /// the voices keep advancing, so regaining focus does not replay what was missed.
    /// Oracle: the recovered focus behavior and the rebuild's explicit master-mute rule.
    #[test]
    fn muting_silences_the_output_without_pausing_the_voices() {
        let muted = Arc::new(AtomicBool::new(true));
        let mut p = VoicePool::new(Arc::clone(&muted));
        p.start(sample(4), 0, 0, 0.0);
        let mut out = vec![0.0f32; 4];
        p.mix(&mut out);
        assert_eq!(out, vec![0.0f32; 4], "muted output is silent");
        muted.store(false, Ordering::Relaxed);
        let mut out = vec![0.0f32; 8];
        p.mix(&mut out);
        // Only the last two frames are left: the first two played, inaudibly.
        assert_eq!(&out[0..4], &[1.0, 1.0, 1.0, 1.0]);
        assert_eq!(&out[4..8], &[0.0, 0.0, 0.0, 0.0]);
    }

    /// Voices sum, and the mixer adds into the caller's buffer rather than overwriting it.
    #[test]
    fn voices_sum_into_the_output_block() {
        let mut p = VoicePool::default();
        p.start(sample(2), 0, 0, 0.0);
        p.start(sample(2), 0, 0, 0.0);
        let mut out = vec![0.5f32; 4];
        p.mix(&mut out);
        assert_eq!(out, vec![2.5f32; 4]);
    }
}
