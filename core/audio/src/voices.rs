//! A general mixer: any output rate, any number of voices, looping, and each voice's gains and
//! playback rate changed while it plays.
//!
//! The first game's client has its own pool ([`crate::mixer`]): sixteen voices, whole samples at
//! one rate, nothing looping, nothing changed once started. This is the mixer a client with other
//! rules builds on, and it leaves that pool as it is.
//!
//! A [`Source`] is interleaved stereo `f32` at its own rate, decoded once and shared between the
//! voices that play it. A [`Stream`] is a sound made as it plays (a synthesizer): the voice reads
//! its frames as it needs them, and it ends when the stream says it has. A voice reads its source at its own rate: `rate / mixer rate` source frames
//! for every output frame, interpolating linearly between neighbouring frames. A source at the
//! mixer's rate played at that rate therefore comes out sample for sample, and a voice's pitch (or
//! a Doppler shift) is only its rate. A voice's two gains multiply its left and right channels;
//! changing them, or the rate, takes effect from the next block mixed, with no ramp.

use std::sync::Arc;

/// A sound ready to mix: interleaved stereo frames at `rate`.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    frames: Arc<[f32]>,
    rate: u32,
}

impl Source {
    /// A source of interleaved stereo `frames` at `rate` (a trailing half frame is dropped).
    #[must_use]
    pub fn new(frames: Arc<[f32]>, rate: u32) -> Self {
        if frames.len().is_multiple_of(2) {
            Self { frames, rate }
        } else {
            let even: Arc<[f32]> = frames[..frames.len() - 1].into();
            Self { frames: even, rate }
        }
    }

    /// A source from interleaved samples of `channels` channels: one channel goes to both sides,
    /// two keep their sides, and of more only the first two are kept. A trailing partial frame is
    /// dropped.
    #[must_use]
    pub fn from_interleaved(samples: &[f32], channels: u16, rate: u32) -> Self {
        let c = usize::from(channels.max(1));
        let n = samples.len() / c;
        let mut out = Vec::with_capacity(n * 2);
        for f in samples.chunks_exact(c) {
            let l = f[0];
            let r = if c == 1 { l } else { f[1] };
            out.push(l);
            out.push(r);
        }
        Self {
            frames: out.into(),
            rate,
        }
    }

    /// The interleaved stereo frames.
    #[must_use]
    pub fn frames(&self) -> &[f32] {
        &self.frames
    }

    /// The rate the frames were recorded at, in frames a second.
    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// How many stereo frames it holds.
    #[must_use]
    pub fn len_frames(&self) -> usize {
        self.frames.len() / 2
    }

    /// Its length in seconds at its own rate (0 for a rate of 0).
    #[must_use]
    pub fn seconds(&self) -> f64 {
        if self.rate == 0 {
            return 0.0;
        }
        let n = u32::try_from(self.len_frames()).unwrap_or(u32::MAX);
        f64::from(n) / f64::from(self.rate)
    }
}

/// A sound made as it plays: interleaved stereo frames at its own rate, read in order.
pub trait Stream: Send + std::fmt::Debug {
    /// The rate its frames are made at, in frames a second.
    fn rate(&self) -> u32;

    /// Write its next frames into `out` (interleaved stereo); how many frames it wrote. Fewer than
    /// `out` holds means it has ended.
    fn read(&mut self, out: &mut [f32]) -> usize;
}

/// Frames a stream's voice reads at a time.
const STREAM_CHUNK: usize = 256;

/// A stream as a voice plays it: the frames read and not yet passed.
#[derive(Debug)]
struct Feed {
    stream: Box<dyn Stream>,
    /// Interleaved frames from `base` on.
    buf: Vec<f32>,
    base: u64,
    ended: bool,
}

impl Feed {
    /// Make frames `[from, to)` readable (as far as the stream goes); drop those before `from`.
    fn cover(&mut self, from: u64, to: u64) {
        if from > self.base {
            let drop = usize::try_from(from - self.base)
                .unwrap_or(usize::MAX)
                .min(self.buf.len() / 2);
            self.buf.drain(..drop * 2);
            self.base += drop as u64;
        }
        while !self.ended && self.base + (self.buf.len() / 2) as u64 <= to {
            let at = self.buf.len();
            self.buf.resize(at + STREAM_CHUNK * 2, 0.0);
            let got = self.stream.read(&mut self.buf[at..]).min(STREAM_CHUNK);
            self.buf.truncate(at + got * 2);
            if got < STREAM_CHUNK {
                self.ended = true;
            }
        }
    }

    fn frame(&self, i: u64) -> Option<(f32, f32)> {
        let k = usize::try_from(i.checked_sub(self.base)?).ok()?;
        Some((*self.buf.get(2 * k)?, *self.buf.get(2 * k + 1)?))
    }
}

/// A playing voice, as [`Mixer::start`] named it. Never reused for another voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VoiceId(u64);

/// How a voice plays: its left and right gains, the rate its source is read at (frames a second;
/// the source's own rate plays it as recorded), and whether it starts again from the beginning
/// when it reaches its end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceSettings {
    pub gains: [f32; 2],
    pub rate: f64,
    pub looping: bool,
}

impl VoiceSettings {
    /// `source` as recorded, at full gain, once.
    #[must_use]
    pub fn of(source: &Source) -> Self {
        Self {
            gains: [1.0, 1.0],
            rate: f64::from(source.rate),
            looping: false,
        }
    }
}

#[derive(Debug)]
struct Voice {
    id: VoiceId,
    source: Source,
    /// What it plays when it plays a stream (its `source` then empty, at the stream's rate).
    feed: Option<Feed>,
    /// Where it reads, in source frames.
    pos: f64,
    settings: VoiceSettings,
}

/// The mixer: a fixed number of voices mixed into interleaved stereo at one output rate.
#[derive(Debug)]
pub struct Mixer {
    rate: u32,
    voices: Vec<Option<Voice>>,
    next: u64,
}

impl Mixer {
    /// A mixer at `rate` output frames a second with room for `voices` voices.
    #[must_use]
    pub fn new(rate: u32, voices: usize) -> Self {
        Self {
            rate: rate.max(1),
            voices: (0..voices).map(|_| None).collect(),
            next: 1,
        }
    }

    /// The output rate.
    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// How many voices it has room for.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.voices.len()
    }

    /// How many voices are playing.
    #[must_use]
    pub fn active(&self) -> usize {
        self.voices.iter().flatten().count()
    }

    /// Start `source` with `settings` in the first free voice; `None` when every voice is busy
    /// (nothing is stopped to make room: which sound gives way is the caller's rule) or the source
    /// is empty.
    pub fn start(&mut self, source: Source, settings: VoiceSettings) -> Option<VoiceId> {
        if source.len_frames() == 0 {
            return None;
        }
        let slot = self.voices.iter().position(Option::is_none)?;
        let id = VoiceId(self.next);
        self.next += 1;
        self.voices[slot] = Some(Voice {
            id,
            source,
            feed: None,
            pos: 0.0,
            settings,
        });
        Some(id)
    }

    /// Start a stream with `settings` in the first free voice (its `looping` is ignored: a stream
    /// ends when it says so); `None` when every voice is busy.
    pub fn start_stream(
        &mut self,
        stream: Box<dyn Stream>,
        settings: VoiceSettings,
    ) -> Option<VoiceId> {
        let slot = self.voices.iter().position(Option::is_none)?;
        let id = VoiceId(self.next);
        self.next += 1;
        let rate = stream.rate();
        self.voices[slot] = Some(Voice {
            id,
            source: Source::new(Arc::from(Vec::new()), rate),
            feed: Some(Feed {
                stream,
                buf: Vec::new(),
                base: 0,
                ended: false,
            }),
            pos: 0.0,
            settings: VoiceSettings {
                looping: false,
                ..settings
            },
        });
        Some(id)
    }

    fn voice_mut(&mut self, id: VoiceId) -> Option<&mut Voice> {
        self.voices.iter_mut().flatten().find(|v| v.id == id)
    }

    fn voice(&self, id: VoiceId) -> Option<&Voice> {
        self.voices.iter().flatten().find(|v| v.id == id)
    }

    /// Stop a voice; whether it was playing.
    pub fn stop(&mut self, id: VoiceId) -> bool {
        for v in &mut self.voices {
            if v.as_ref().is_some_and(|x| x.id == id) {
                *v = None;
                return true;
            }
        }
        false
    }

    /// Stop every voice.
    pub fn stop_all(&mut self) {
        for v in &mut self.voices {
            *v = None;
        }
    }

    /// Whether a voice is still playing (a voice that reached its end without looping is not).
    #[must_use]
    pub fn is_playing(&self, id: VoiceId) -> bool {
        self.voice(id).is_some()
    }

    /// A playing voice's settings.
    #[must_use]
    pub fn settings(&self, id: VoiceId) -> Option<VoiceSettings> {
        self.voice(id).map(|v| v.settings)
    }

    /// Where a playing voice reads, in source frames.
    #[must_use]
    pub fn position(&self, id: VoiceId) -> Option<f64> {
        self.voice(id).map(|v| v.pos)
    }

    /// Change a playing voice's gains; whether it is playing.
    pub fn set_gains(&mut self, id: VoiceId, gains: [f32; 2]) -> bool {
        self.voice_mut(id)
            .map(|v| v.settings.gains = gains)
            .is_some()
    }

    /// Change the rate a playing voice reads its source at; whether it is playing.
    pub fn set_rate(&mut self, id: VoiceId, rate: f64) -> bool {
        self.voice_mut(id)
            .map(|v| v.settings.rate = rate.max(0.0))
            .is_some()
    }

    /// Change whether a playing voice loops; whether it is playing.
    pub fn set_looping(&mut self, id: VoiceId, looping: bool) -> bool {
        self.voice_mut(id)
            .map(|v| v.settings.looping = looping)
            .is_some()
    }

    /// Add every playing voice into `out`, interleaved stereo at the output rate. `out` is not
    /// cleared first. A voice that reaches its end stops, unless it loops.
    pub fn mix(&mut self, out: &mut [f32]) {
        let frames = out.len() / 2;
        let out_rate = f64::from(self.rate);
        for slot in &mut self.voices {
            let Some(v) = slot.as_mut() else { continue };
            if let Some(feed) = v.feed.as_mut() {
                if mix_feed(feed, &mut v.pos, &v.settings, out_rate, out) {
                    *slot = None;
                }
                continue;
            }
            let n = v.source.len_frames();
            let src = &v.source.frames;
            let step = v.settings.rate / out_rate;
            let [gl, gr] = v.settings.gains;
            let len = n as f64;
            let mut done = false;
            for f in 0..frames {
                if v.pos >= len {
                    if v.settings.looping {
                        v.pos %= len;
                    } else {
                        done = true;
                        break;
                    }
                }
                let i = v.pos.floor();
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // LINT-OK: a frame index below the source's length, which is a `usize`.
                let i0 = i as usize;
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: an interpolation weight in [0, 1).
                let t = (v.pos - i) as f32;
                let i1 = if i0 + 1 < n {
                    i0 + 1
                } else if v.settings.looping {
                    0
                } else {
                    i0
                };
                let (l0, r0) = (src[2 * i0], src[2 * i0 + 1]);
                let (l, r) = if t == 0.0 {
                    (l0, r0)
                } else {
                    let (l1, r1) = (src[2 * i1], src[2 * i1 + 1]);
                    (l0 + (l1 - l0) * t, r0 + (r1 - r0) * t)
                };
                out[2 * f] += l * gl;
                out[2 * f + 1] += r * gr;
                v.pos += step;
            }
            if done || (!v.settings.looping && v.pos >= len) {
                *slot = None;
            }
        }
    }

    /// Fill a device's block of `channels` interleaved channels: the mix on the first two (a
    /// one-channel device gets the two sides' mean) and silence on any others. `scratch` is the
    /// stereo block, kept by the caller so the device's thread does not allocate.
    pub fn fill(&mut self, out: &mut [f32], channels: usize, scratch: &mut Vec<f32>) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        scratch.clear();
        scratch.resize(frames * 2, 0.0);
        self.mix(scratch);
        out.fill(0.0);
        for f in 0..frames {
            let (l, r) = (scratch[2 * f], scratch[2 * f + 1]);
            if channels == 1 {
                out[f] = (l + r) * 0.5;
            } else {
                out[f * channels] = l;
                out[f * channels + 1] = r;
            }
        }
    }
}

/// Mix a stream's voice into `out`; whether it has ended.
fn mix_feed(
    feed: &mut Feed,
    pos: &mut f64,
    settings: &VoiceSettings,
    out_rate: f64,
    out: &mut [f32],
) -> bool {
    let frames = out.len() / 2;
    let step = settings.rate / out_rate;
    let [gl, gr] = settings.gains;
    #[allow(clippy::cast_precision_loss)]
    // LINT-OK: a block's frame count.
    let span = step * frames as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // LINT-OK: frame positions, never negative, far below u64's range.
    let (first, last) = (pos.floor() as u64, (*pos + span).ceil() as u64 + 1);
    feed.cover(first, last);
    for f in 0..frames {
        let i = pos.floor();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a frame position, never negative.
        let i0 = i as u64;
        let Some((l0, r0)) = feed.frame(i0) else {
            return feed.ended;
        };
        #[allow(clippy::cast_possible_truncation)]
        // LINT-OK: an interpolation weight in [0, 1).
        let t = (*pos - i) as f32;
        let (l1, r1) = feed.frame(i0 + 1).unwrap_or((l0, r0));
        out[2 * f] += (l0 + (l1 - l0) * t) * gl;
        out[2 * f + 1] += (r0 + (r1 - r0) * t) * gr;
        *pos += step;
    }
    false
}

/// A gain from a level in millibels (hundredths of a decibel, as DirectSound takes its volumes):
/// `10^(mb / 2000)`.
#[must_use]
pub fn gain_from_millibels(mb: i32) -> f32 {
    dereth_primitives::num::math::powf(10f32, mb as f32 / 2000.0)
}

/// DirectSound's pan in millibels as two gains: a positive pan attenuates the left channel by
/// that much, a negative one the right, and the other side stays at full level.
#[must_use]
pub fn pan_gains_millibels(pan: i32) -> [f32; 2] {
    let att = gain_from_millibels(-pan.abs());
    if pan >= 0 {
        [att, 1.0]
    } else {
        [1.0, att]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(frames: usize, rate: u32) -> Source {
        let mut v = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let s = i as f32 / 4.0;
            v.push(s);
            v.push(-s);
        }
        Source::new(v.into(), rate)
    }

    /// A source at the mixer's rate, read at that rate, comes out sample for sample times its
    /// gains.
    #[test]
    fn a_source_at_the_mixer_rate_comes_out_sample_for_sample() {
        let mut m = Mixer::new(22050, 4);
        let s = ramp(5, 22050);
        let mut set = VoiceSettings::of(&s);
        set.gains = [0.5, 2.0];
        let id = m.start(s, set).expect("a free voice");
        let mut out = vec![0.0f32; 10];
        m.mix(&mut out);
        let want: Vec<f32> = (0..5)
            .flat_map(|i| [i as f32 / 4.0 * 0.5, -(i as f32) / 4.0 * 2.0])
            .collect();
        assert_eq!(out, want);
        assert!(!m.is_playing(id), "it ended exactly at the block's end");
    }

    /// A source at half the mixer's rate is read half a frame a step, the odd frames the mean of
    /// their neighbours.
    #[test]
    fn a_source_at_half_the_rate_is_interpolated_between_frames() {
        let mut m = Mixer::new(22050, 1);
        let s = ramp(3, 11025);
        let set = VoiceSettings::of(&s);
        m.start(s, set);
        let mut out = vec![0.0f32; 12];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        // 0, 0.125, 0.25, 0.375, 0.5 and the last frame held (no next frame to lean on).
        assert_eq!(left, vec![0.0, 0.125, 0.25, 0.375, 0.5, 0.5]);
    }

    /// A looping voice starts again from its first frame and never ends; a voice that does not
    /// loop stops and leaves silence.
    #[test]
    fn a_looping_voice_wraps_and_one_that_does_not_ends() {
        let mut m = Mixer::new(100, 2);
        let s = ramp(3, 100);
        let mut set = VoiceSettings::of(&s);
        set.looping = true;
        let looping = m.start(s.clone(), set).expect("free");
        let mut out = vec![0.0f32; 16];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left, vec![0.0, 0.25, 0.5, 0.0, 0.25, 0.5, 0.0, 0.25]);
        assert!(m.is_playing(looping));
        assert!(m.set_looping(looping, false));
        let mut out = vec![0.0f32; 8];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left, vec![0.5, 0.0, 0.0, 0.0]);
        assert!(!m.is_playing(looping));
    }

    /// Gains and the rate change while the voice plays: the next block hears them.
    #[test]
    fn gains_and_rate_change_while_the_voice_plays() {
        let mut m = Mixer::new(100, 1);
        let s = Source::new(vec![1.0; 40].into(), 100);
        let set = VoiceSettings::of(&s);
        let id = m.start(s, set).expect("free");
        let mut out = vec![0.0f32; 4];
        m.mix(&mut out);
        assert_eq!(out, vec![1.0; 4]);
        m.set_gains(id, [0.25, 0.0]);
        m.set_rate(id, 200.0);
        let mut out = vec![0.0f32; 4];
        m.mix(&mut out);
        assert_eq!(out, vec![0.25, 0.0, 0.25, 0.0]);
        assert_eq!(m.position(id), Some(6.0), "two frames at 1, then two at 2");
    }

    /// With every voice busy a new one is refused, and nothing playing is stopped for it.
    #[test]
    fn a_full_mixer_refuses_and_stops_nothing() {
        let mut m = Mixer::new(100, 2);
        let s = ramp(10, 100);
        let a = m.start(s.clone(), VoiceSettings::of(&s));
        let b = m.start(s.clone(), VoiceSettings::of(&s));
        assert!(a.is_some() && b.is_some() && a != b);
        assert_eq!(m.start(s.clone(), VoiceSettings::of(&s)), None);
        assert_eq!(m.active(), 2);
        assert!(m.stop(a.expect("a")));
        assert!(m.start(s.clone(), VoiceSettings::of(&s)).is_some());
    }

    /// Voices sum into the caller's block, which is not cleared.
    #[test]
    fn voices_sum_into_the_block() {
        let mut m = Mixer::new(10, 2);
        let s = Source::new(vec![1.0, 2.0].into(), 10);
        m.start(s.clone(), VoiceSettings::of(&s));
        m.start(s.clone(), VoiceSettings::of(&s));
        let mut out = vec![0.5, 0.5, 0.5, 0.5];
        m.mix(&mut out);
        assert_eq!(out, vec![2.5, 4.5, 0.5, 0.5]);
    }

    /// A device block takes the mix on its first two channels and silence on the rest; a
    /// one-channel device takes the sides' mean.
    #[test]
    fn a_device_block_takes_the_mix_on_its_first_two_channels() {
        let s = Source::new(vec![1.0, 3.0, 1.0, 3.0].into(), 10);
        let mut m = Mixer::new(10, 1);
        m.start(s.clone(), VoiceSettings::of(&s));
        let mut scratch = Vec::new();
        let mut out = vec![9.0f32; 8];
        m.fill(&mut out, 4, &mut scratch);
        assert_eq!(out, vec![1.0, 3.0, 0.0, 0.0, 1.0, 3.0, 0.0, 0.0]);
        let mut m = Mixer::new(10, 1);
        m.start(s.clone(), VoiceSettings::of(&s));
        let mut out = vec![0.0f32; 2];
        m.fill(&mut out, 1, &mut scratch);
        assert_eq!(out, vec![2.0, 2.0]);
    }

    /// Mono is spread to both sides, stereo keeps its sides, and more channels keep the first two.
    #[test]
    fn sources_are_widened_to_stereo() {
        let s = Source::from_interleaved(&[0.1, 0.2], 1, 8000);
        assert_eq!(s.frames(), &[0.1, 0.1, 0.2, 0.2]);
        let s = Source::from_interleaved(&[0.1, 0.2, 0.3, 0.4], 2, 8000);
        assert_eq!(s.frames(), &[0.1, 0.2, 0.3, 0.4]);
        let s = Source::from_interleaved(&[0.1, 0.2, 0.9, 0.3, 0.4, 0.9, 0.5], 3, 8000);
        assert_eq!(s.frames(), &[0.1, 0.2, 0.3, 0.4]);
        assert!((s.seconds() - 2.0 / 8000.0).abs() < 1e-12);
    }

    /// A ramp made as it is read, `limit` frames long.
    #[derive(Debug)]
    struct Ramp {
        next: usize,
        limit: usize,
        rate: u32,
    }

    impl Stream for Ramp {
        fn rate(&self) -> u32 {
            self.rate
        }

        fn read(&mut self, out: &mut [f32]) -> usize {
            let n = (out.len() / 2).min(self.limit - self.next);
            for k in 0..n {
                let s = (self.next + k) as f32 / 4.0;
                out[2 * k] = s;
                out[2 * k + 1] = -s;
            }
            self.next += n;
            n
        }
    }

    /// A stream at the mixer's rate comes out sample for sample, read as it is needed across
    /// blocks, and its voice ends when the stream does.
    #[test]
    fn a_stream_is_read_as_it_plays_and_ends_with_its_stream() {
        let mut m = Mixer::new(22050, 2);
        let ramp = Ramp {
            next: 0,
            limit: 300,
            rate: 22050,
        };
        let set = VoiceSettings {
            gains: [1.0, 0.5],
            rate: 22050.0,
            looping: false,
        };
        let id = m.start_stream(Box::new(ramp), set).expect("a free voice");
        let mut out = vec![0.0f32; 2 * 200];
        m.mix(&mut out);
        assert_eq!(out[2 * 199], 199.0 / 4.0);
        assert_eq!(out[2 * 199 + 1], -199.0 / 8.0);
        assert!(m.is_playing(id));
        let mut out = vec![0.0f32; 2 * 200];
        m.mix(&mut out);
        assert_eq!(
            out[0],
            200.0 / 4.0,
            "the next block goes on where the last one stopped"
        );
        assert_eq!(out[2 * 99], 299.0 / 4.0);
        assert_eq!(out[2 * 100], 0.0, "silence after its end");
        assert!(!m.is_playing(id), "the voice ended with its stream");
    }

    /// A stream read at half the mixer's rate is interpolated between its frames.
    #[test]
    fn a_stream_read_at_another_rate_is_interpolated() {
        let mut m = Mixer::new(200, 1);
        let ramp = Ramp {
            next: 0,
            limit: 1000,
            rate: 100,
        };
        let set = VoiceSettings {
            gains: [1.0, 1.0],
            rate: 100.0,
            looping: false,
        };
        m.start_stream(Box::new(ramp), set);
        let mut out = vec![0.0f32; 2 * 5];
        m.mix(&mut out);
        let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
        assert_eq!(left, vec![0.0, 0.125, 0.25, 0.375, 0.5]);
    }

    /// Millibels: 0 is unity, -2000 a tenth, and a positive pan quietens the left side only.
    #[test]
    fn millibels_and_pan_follow_directsound() {
        assert_eq!(gain_from_millibels(0), 1.0);
        assert!((gain_from_millibels(-2000) - 0.1).abs() < 1e-6);
        let [l, r] = pan_gains_millibels(2000);
        assert!((l - 0.1).abs() < 1e-6 && r == 1.0);
        let [l, r] = pan_gains_millibels(-2000);
        assert!(l == 1.0 && (r - 0.1).abs() < 1e-6);
    }
}
