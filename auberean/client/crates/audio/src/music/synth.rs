//! The instrument synthesizer: it plays notes on channels with the instruments of the collections
//! and renders them at its own rate, as the software synthesizer the music was written for renders
//! them.
//!
//! A note finds the region of its channel's instrument whose key and velocity ranges hold it, and
//! plays that region's wave from its start, pitched by the key against the wave's root note and its
//! fine tuning (a semitone a key), looping its loop while it lasts. Its level is the wave's
//! attenuation, the velocity's and the channel's volume and expression (each `40 log10(v / 127)`
//! decibels), the volume envelope and any tremolo; its pitch moves with the pitch wheel and any
//! vibrato; a low-pass filter darkens it when the instrument has one. The volume envelope waits its
//! delay, rises in its attack linearly in amplitude, holds, falls at 96 decibels per decay time to
//! its sustain level (a fraction of that 96-decibel range) and, once the note is released, falls at
//! 96 decibels per release time; the voice ends when it reaches -96 decibels or the wave ends.
//!
//! Articulation is the instrument's or, when a region has its own, the region's; connections it
//! does not hold take the format's defaults.

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_primitives::num::math;

use super::dls::{self, dst, Articulation, Collection, Sampler};

/// How the synthesizer renders.
pub const RATE: u32 = 22_050;

/// Voices one synthesizer plays at once; a note past them takes the voice longest released, or
/// else the oldest.
pub const MAX_VOICES: usize = 48;

/// Frames between control updates (envelopes, LFOs, pitch); the level is ramped between them.
const CONTROL_FRAMES: usize = 16;

/// The floor of the level, in decibels: a voice there is silent and done.
const FLOOR_DB: f32 = -96.0;

/// An instrument in its collection.
#[derive(Clone, Debug)]
pub struct Patch {
    pub collection: Arc<Collection>,
    pub index: usize,
}

impl PartialEq for Patch {
    fn eq(&self, o: &Self) -> bool {
        Arc::ptr_eq(&self.collection, &o.collection) && self.index == o.index
    }
}

/// What the synthesizer is told.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    NoteOn {
        channel: u32,
        key: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u32,
        key: u8,
    },
    /// A channel's instrument (`None`: no instrument, its notes are silent).
    Program {
        channel: u32,
        patch: Option<Patch>,
    },
    /// A controller: 7 volume, 10 pan, 11 expression, 1 the modulation wheel.
    Control {
        channel: u32,
        controller: u8,
        value: u8,
    },
    /// The pitch wheel, 0 to 16,383 with 8,192 at rest.
    PitchBend {
        channel: u32,
        value: u16,
    },
    /// The pitch wheel's range in semitones.
    BendRange {
        channel: u32,
        semitones: u8,
    },
    /// Every note released.
    AllNotesOff,
}

#[derive(Clone, Debug)]
struct ChannelState {
    patch: Option<Patch>,
    volume: u8,
    expression: u8,
    pan: u8,
    modulation: u8,
    bend: u16,
    /// The pitch wheel's range in cents.
    bend_range: f32,
    /// The sustain pedal is down.
    sustain: bool,
    /// The registered parameter selected (controllers 101 and 100).
    rpn: (u8, u8),
}

impl Default for ChannelState {
    fn default() -> Self {
        Self {
            patch: None,
            volume: 100,
            expression: 127,
            pan: 64,
            modulation: 0,
            bend: 8192,
            bend_range: 200.0,
            sustain: false,
            rpn: (127, 127),
        }
    }
}

/// An envelope's times (seconds) and sustain (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Envelope {
    pub delay: f32,
    pub attack: f32,
    pub hold: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

/// What a note's articulation comes to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteSettings {
    pub eg1: Envelope,
    pub lfo_hz: f32,
    pub lfo_delay: f32,
    pub vib_hz: f32,
    pub vib_delay: f32,
    /// Cents of pitch the LFO and the vibrato LFO move by at their peaks.
    pub lfo_pitch: f32,
    pub vib_pitch: f32,
    /// Decibels the LFO moves the level by at its peak.
    pub lfo_level: f32,
    /// Pan, -0.5 (left) to 0.5 (right).
    pub pan: f32,
    /// Cents added to the pitch.
    pub pitch: f32,
    /// Decibels added to the level.
    pub level: f32,
    /// The filter's cutoff in absolute cents, when it filters.
    pub cutoff: Option<f32>,
    pub lfo_cutoff: f32,
    /// The filter's resonance in decibels.
    pub q: f32,
}

/// Time cents (stored times 65,536) as seconds; the smallest value is no time.
fn seconds(scale: i32) -> f32 {
    if scale == i32::MIN {
        return 0.0;
    }
    let tc = f64::from(scale) / 65536.0;
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a duration in seconds, far inside f32's range.
    let s = math::pow(2.0, tc / 1200.0) as f32;
    s
}

/// Absolute pitch cents (stored times 65,536) as a frequency.
fn hertz(scale: i32) -> f32 {
    let pc = f64::from(scale) / 65536.0;
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a frequency in hertz, far inside f32's range.
    let f = (440.0 * math::pow(2.0, (pc - 6900.0) / 1200.0)) as f32;
    f
}

fn unit(scale: i32) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: a stored amount over 65,536, well inside f32's precision for these ranges.
    let v = (f64::from(scale) / 65536.0) as f32;
    v
}

/// A level in decibels as a gain.
#[must_use]
pub fn gain_of_db(db: f32) -> f32 {
    if db <= FLOOR_DB {
        return 0.0;
    }
    math::powf(10.0, db / 20.0)
}

/// A controller's or a velocity's level in decibels: `40 log10(v / 127)`, at most 96 down.
#[must_use]
pub fn curve_db(v: u8) -> f32 {
    if v == 0 {
        return FLOOR_DB;
    }
    narrow(40.0 * math::log10(f64::from(v) / 127.0)).max(FLOOR_DB)
}

/// A gain as decibels.
fn db_of_gain(a: f32) -> f32 {
    if a <= 0.0 {
        return FLOOR_DB;
    }
    narrow(20.0 * math::log10(f64::from(a))).max(FLOOR_DB)
}

/// An `f64` computed for precision, kept as the `f32` the voices run in.
fn narrow(x: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    // LINT-OK: levels, pitches and times well inside f32's range.
    let y = x as f32;
    y
}

/// The defaults of connections an articulation does not hold: (source, control, destination,
/// scale).
const DEFAULTS: &[(u16, u16, u16, i32)] = &[
    // 5 Hz and 10 ms for both LFOs.
    (dls::SRC_NONE, 0, dst::LFO_FREQUENCY, -55_791_973),
    (dls::SRC_NONE, 0, dst::LFO_DELAY, -522_494_200),
    (dls::SRC_NONE, 0, dst::VIB_FREQUENCY, -55_791_973),
    (dls::SRC_NONE, 0, dst::VIB_DELAY, -522_494_200),
    // The modulation wheel's vibrato: 50 cents.
    (dls::SRC_LFO, dls::SRC_CC1, dst::PITCH, 50 * 65536),
    (dls::SRC_NONE, 0, dst::EG1_DELAY, i32::MIN),
    (dls::SRC_NONE, 0, dst::EG1_ATTACK, i32::MIN),
    (dls::SRC_NONE, 0, dst::EG1_HOLD, i32::MIN),
    (dls::SRC_NONE, 0, dst::EG1_DECAY, i32::MIN),
    (dls::SRC_NONE, 0, dst::EG1_SUSTAIN, 1000 * 65536),
    (dls::SRC_NONE, 0, dst::EG1_RELEASE, i32::MIN),
    (dls::SRC_NONE, 0, dst::FILTER_CUTOFF, i32::MAX),
];

/// What `art` (or the defaults) makes of a note of `key` at `velocity` on a channel with the
/// modulation wheel at `modulation`.
#[must_use]
pub fn note_settings(
    art: Option<&Articulation>,
    key: u8,
    velocity: u8,
    modulation: u8,
) -> NoteSettings {
    let mut table: Vec<(u16, u16, u16, i32)> = DEFAULTS.to_vec();
    if let Some(a) = art {
        for c in &a.connections {
            match table
                .iter_mut()
                .find(|t| t.0 == c.source && t.1 == c.control && t.2 == c.destination)
            {
                Some(t) => t.3 = c.scale,
                None => table.push((c.source, c.control, c.destination, c.scale)),
            }
        }
    }
    let v = f32::from(velocity) / 128.0;
    let k = f32::from(key) / 128.0;
    let m = f32::from(modulation) / 128.0;
    // A connection's source times its control, as known at the note's start; the LFOs' are their
    // depths (their motion is the voice's).
    let gauge = |src: u16, ctl: u16| -> Option<f32> {
        let s = match src {
            dls::SRC_NONE | dls::SRC_LFO | dls::SRC_VIBRATO | dls::SRC_EG2 => 1.0,
            dls::SRC_VELOCITY => v,
            dls::SRC_KEY => k,
            _ => return None,
        };
        let c = match ctl {
            dls::SRC_NONE => 1.0,
            dls::SRC_CC1 => m,
            dls::SRC_VELOCITY => v,
            dls::SRC_KEY => k,
            // The channel's pressure and anything else the music never sends.
            _ => 0.0,
        };
        Some(s * c)
    };
    // The sum of a destination's static connections (in stored units over 65,536), and the
    // parameter's own constant connection kept as stored for its special values.
    let sum = |d: u16, from: &[u16]| -> f32 {
        table
            .iter()
            .filter(|t| t.2 == d && from.contains(&t.0))
            .filter_map(|t| gauge(t.0, t.1).map(|g| g * unit(t.3)))
            .sum()
    };
    let constant = |d: u16| -> i32 {
        table
            .iter()
            .find(|t| t.2 == d && t.0 == dls::SRC_NONE && t.1 == 0)
            .map_or(i32::MIN, |t| t.3)
    };
    let modifiers = [dls::SRC_VELOCITY, dls::SRC_KEY];
    let time = |d: u16| -> f32 {
        let base = constant(d);
        let extra = sum(d, &modifiers);
        if base == i32::MIN && extra == 0.0 {
            0.0
        } else {
            let tc = if base == i32::MIN {
                -32768.0
            } else {
                unit(base)
            } + extra;
            narrow(math::pow(2.0, f64::from(tc) / 1200.0))
        }
    };
    let eg1 = Envelope {
        delay: time(dst::EG1_DELAY),
        attack: time(dst::EG1_ATTACK),
        hold: time(dst::EG1_HOLD),
        decay: time(dst::EG1_DECAY),
        sustain: (unit(constant(dst::EG1_SUSTAIN)) / 1000.0).clamp(0.0, 1.0),
        release: time(dst::EG1_RELEASE),
    };
    let lfo = |src: u16, d: u16| -> f32 {
        table
            .iter()
            .filter(|t| t.0 == src && t.2 == d)
            .filter_map(|t| gauge(t.0, t.1).map(|g| g * unit(t.3)))
            .sum()
    };
    let cutoff = constant(dst::FILTER_CUTOFF);
    NoteSettings {
        eg1,
        lfo_hz: hertz(constant(dst::LFO_FREQUENCY)),
        lfo_delay: seconds(constant(dst::LFO_DELAY)),
        vib_hz: hertz(constant(dst::VIB_FREQUENCY)),
        vib_delay: seconds(constant(dst::VIB_DELAY)),
        lfo_pitch: lfo(dls::SRC_LFO, dst::PITCH),
        vib_pitch: lfo(dls::SRC_VIBRATO, dst::PITCH),
        lfo_level: lfo(dls::SRC_LFO, dst::ATTENUATION) / 10.0,
        pan: sum(dst::PAN, &[dls::SRC_NONE]) / 1000.0,
        pitch: sum(dst::PITCH, &[dls::SRC_NONE, dls::SRC_VELOCITY]),
        level: sum(
            dst::ATTENUATION,
            &[dls::SRC_NONE, dls::SRC_VELOCITY, dls::SRC_KEY],
        ) / 10.0,
        cutoff: if cutoff == i32::MAX {
            None
        } else {
            Some(unit(cutoff) + sum(dst::FILTER_CUTOFF, &modifiers))
        },
        lfo_cutoff: lfo(dls::SRC_LFO, dst::FILTER_CUTOFF),
        q: sum(dst::FILTER_Q, &[dls::SRC_NONE]) / 10.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Delay,
    Attack,
    Hold,
    Decay,
    Sustain,
    Release,
    Done,
}

/// The volume envelope as it runs: its stage, the seconds spent in it, and its level in decibels.
#[derive(Clone, Copy, Debug)]
struct Eg {
    stage: Stage,
    t: f32,
    db: f32,
}

impl Eg {
    fn new() -> Self {
        Self {
            stage: Stage::Delay,
            t: 0.0,
            db: FLOOR_DB,
        }
    }

    fn release(&mut self) {
        if !matches!(self.stage, Stage::Release | Stage::Done) {
            self.stage = Stage::Release;
            self.t = 0.0;
        }
    }

    /// Advance by `dt` seconds; the level as a gain.
    fn step(&mut self, e: &Envelope, dt: f32) -> f32 {
        let sustain_db = FLOOR_DB * (1.0 - e.sustain);
        let mut left = dt;
        loop {
            match self.stage {
                Stage::Delay => {
                    if self.t + left < e.delay {
                        self.t += left;
                        return 0.0;
                    }
                    left -= e.delay - self.t;
                    self.stage = Stage::Attack;
                    self.t = 0.0;
                }
                Stage::Attack => {
                    if self.t + left < e.attack {
                        self.t += left;
                        let a = self.t / e.attack;
                        self.db = db_of_gain(a);
                        return a;
                    }
                    left -= e.attack - self.t;
                    self.stage = Stage::Hold;
                    self.t = 0.0;
                    self.db = 0.0;
                }
                Stage::Hold => {
                    if self.t + left < e.hold {
                        self.t += left;
                        return 1.0;
                    }
                    left -= e.hold - self.t;
                    self.stage = Stage::Decay;
                    self.t = 0.0;
                }
                Stage::Decay => {
                    let rate = if e.decay > 0.0 {
                        -FLOOR_DB / e.decay
                    } else {
                        f32::INFINITY
                    };
                    self.db -= rate * left;
                    if self.db <= sustain_db {
                        self.db = sustain_db;
                        self.stage = if sustain_db <= FLOOR_DB {
                            Stage::Done
                        } else {
                            Stage::Sustain
                        };
                    }
                    return gain_of_db(self.db);
                }
                Stage::Sustain => return gain_of_db(self.db),
                Stage::Release => {
                    let rate = if e.release > 0.0 {
                        -FLOOR_DB / e.release
                    } else {
                        f32::INFINITY
                    };
                    self.db -= rate * left;
                    if self.db <= FLOOR_DB {
                        self.db = FLOOR_DB;
                        self.stage = Stage::Done;
                    }
                    return gain_of_db(self.db);
                }
                Stage::Done => return 0.0,
            }
        }
    }
}

/// A two-pole resonant low-pass filter.
#[derive(Clone, Copy, Debug, Default)]
struct Lowpass {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    on: bool,
}

impl Lowpass {
    fn tune(&mut self, cutoff_cents: f32, q_db: f32, rate: f32) {
        let f = 440.0 * math::powf(2.0, (cutoff_cents - 6900.0) / 1200.0);
        if f >= rate * 0.45 {
            self.on = false;
            return;
        }
        self.on = true;
        let f = f.max(10.0);
        let q = (std::f32::consts::FRAC_1_SQRT_2 * math::powf(10.0, q_db / 20.0)).max(0.5);
        let w = 2.0 * std::f32::consts::PI * f / rate;
        let (s, c) = (math::sinf(w), math::cosf(w));
        let alpha = s / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = (1.0 - c) / 2.0 / a0;
        self.b1 = (1.0 - c) / a0;
        self.b2 = self.b0;
        self.a1 = -2.0 * c / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn run(&mut self, x: f32) -> f32 {
        if !self.on {
            return x;
        }
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

#[derive(Clone, Debug)]
struct Voice {
    channel: u32,
    key: u8,
    key_group: u16,
    /// The patch's collection and wave, kept alive while it sounds.
    samples: Arc<[f32]>,
    /// Source samples per output frame at the note's own pitch, and as last moved by the pitch
    /// wheel and the LFOs.
    step: f64,
    step_now: f64,
    /// The synthesizer's rate, for retuning the filter.
    rate: f32,
    loop_span: Option<(f64, f64)>,
    pos: f64,
    settings: NoteSettings,
    /// Decibels fixed at its start: the wave's attenuation, the velocity's, the articulation's.
    level_db: f32,
    eg: Eg,
    released: bool,
    /// Its key was let go while the sustain pedal was down.
    held: bool,
    /// Seconds since its start.
    age: f32,
    started: u64,
    released_at: u64,
    gain: f32,
    filter: Lowpass,
}

/// The synthesizer.
#[derive(Debug)]
pub struct Synth {
    rate: u32,
    mono: bool,
    voices: Vec<Voice>,
    channels: BTreeMap<u32, ChannelState>,
    frames: u64,
}

impl Synth {
    /// A synthesizer rendering at `rate`, in stereo or (for a placed sound) mono, pan ignored.
    #[must_use]
    pub fn new(rate: u32, mono: bool) -> Self {
        Self {
            rate: rate.max(1),
            mono,
            voices: Vec::new(),
            channels: BTreeMap::new(),
            frames: 0,
        }
    }

    /// The rate it renders at.
    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// How many voices sound.
    #[must_use]
    pub fn active(&self) -> usize {
        self.voices.len()
    }

    /// Apply an event at once.
    pub fn apply(&mut self, e: &Event) {
        match e {
            Event::NoteOn {
                channel,
                key,
                velocity,
            } => {
                if *velocity == 0 {
                    self.note_off(*channel, *key);
                } else {
                    self.note_on(*channel, *key, *velocity);
                }
            }
            Event::NoteOff { channel, key } => self.note_off(*channel, *key),
            Event::Program { channel, patch } => {
                self.channels
                    .entry(*channel)
                    .or_default()
                    .patch
                    .clone_from(patch);
            }
            Event::Control {
                channel,
                controller,
                value,
            } => {
                let ch = self.channels.entry(*channel).or_default();
                match controller {
                    1 => ch.modulation = *value,
                    7 => ch.volume = *value,
                    10 => ch.pan = *value,
                    11 => ch.expression = *value,
                    101 => ch.rpn.0 = *value,
                    100 => ch.rpn.1 = *value,
                    6 if ch.rpn == (0, 0) => ch.bend_range = f32::from(*value) * 100.0,
                    64 => {
                        let down = *value >= 64;
                        let was = ch.sustain;
                        ch.sustain = down;
                        if was && !down {
                            let (c, now) = (*channel, self.frames);
                            for v in self.voices.iter_mut().filter(|v| v.channel == c && v.held) {
                                v.held = false;
                                v.released = true;
                                v.released_at = now;
                                v.eg.release();
                            }
                        }
                    }
                    121 => {
                        *ch = ChannelState {
                            patch: ch.patch.clone(),
                            ..ChannelState::default()
                        };
                    }
                    120 | 123 => {
                        let c = *channel;
                        for v in self.voices.iter_mut().filter(|v| v.channel == c) {
                            v.released = true;
                            v.eg.release();
                        }
                    }
                    _ => {}
                }
            }
            Event::PitchBend { channel, value } => {
                self.channels.entry(*channel).or_default().bend = *value;
            }
            Event::BendRange { channel, semitones } => {
                self.channels.entry(*channel).or_default().bend_range =
                    f32::from(*semitones) * 100.0;
            }
            Event::AllNotesOff => {
                let now = self.frames;
                for v in &mut self.voices {
                    if !v.released {
                        v.released = true;
                        v.released_at = now;
                        v.eg.release();
                    }
                }
            }
        }
    }

    fn note_off(&mut self, channel: u32, key: u8) {
        let now = self.frames;
        let sustain = self.channels.get(&channel).is_some_and(|c| c.sustain);
        for v in self
            .voices
            .iter_mut()
            .filter(|v| v.channel == channel && v.key == key && !v.released && !v.held)
        {
            if sustain {
                v.held = true;
                continue;
            }
            v.released = true;
            v.released_at = now;
            v.eg.release();
        }
    }

    fn note_on(&mut self, channel: u32, key: u8, velocity: u8) {
        let ch = self.channels.entry(channel).or_default().clone();
        let Some(patch) = ch.patch else { return };
        let Some(ins) = patch.collection.instruments.get(patch.index) else {
            return;
        };
        let (k, vel) = (u16::from(key), u16::from(velocity));
        for r in ins.regions.iter().filter(|r| {
            (r.keys.0..=r.keys.1).contains(&k) && (r.velocities.0..=r.velocities.1).contains(&vel)
        }) {
            let Some(wave) = patch.collection.waves.get(r.wave as usize) else {
                continue;
            };
            if wave.samples.is_empty() {
                continue;
            }
            let now = self.frames;
            // A note of a key already sounding in a region that is not self-non-exclusive, or of
            // the same key group, stops the one sounding.
            for v in self.voices.iter_mut().filter(|v| v.channel == channel) {
                let same_key = v.key == key && !r.self_non_exclusive;
                let same_group = r.key_group != 0 && v.key_group == r.key_group;
                if (same_key || same_group) && v.eg.stage != Stage::Done {
                    v.released = true;
                    v.released_at = now;
                    v.eg = Eg {
                        stage: Stage::Release,
                        t: 0.0,
                        db: v.eg.db,
                    };
                    v.settings.eg1.release = v.settings.eg1.release.min(0.005);
                }
            }
            let sampler: Sampler = r
                .sampler
                .clone()
                .or_else(|| wave.sampler.clone())
                .unwrap_or_default();
            let art = r.articulation.as_deref().or(ins.articulation.as_deref());
            let settings = note_settings(art, key, velocity, ch.modulation);
            let cents = (f32::from(key) - f32::from(sampler.unity_note)) * 100.0
                + f32::from(sampler.fine_tune)
                + settings.pitch;
            let step = f64::from(wave.rate) / f64::from(self.rate)
                * math::pow(2.0, f64::from(cents) / 1200.0);
            let len = wave.samples.len() as f64;
            let loop_span = sampler.loops.first().and_then(|l| {
                let s = f64::from(l.start);
                let e = s + f64::from(l.length);
                (l.length > 0 && e <= len).then_some((s, e))
            });
            let level_db = unit(sampler.attenuation) / 10.0 + curve_db(velocity) + settings.level;
            let mut filter = Lowpass::default();
            if let Some(c) = settings.cutoff {
                filter.tune(c, settings.q, self.rate as f32);
            }
            if self.voices.len() >= MAX_VOICES {
                self.steal();
            }
            self.voices.push(Voice {
                channel,
                key,
                key_group: r.key_group,
                samples: wave.samples.clone(),
                step,
                step_now: step,
                rate: self.rate as f32,
                loop_span,
                pos: 0.0,
                settings,
                level_db,
                eg: Eg::new(),
                released: false,
                held: false,
                age: 0.0,
                started: now,
                released_at: u64::MAX,
                gain: 0.0,
                filter,
            });
        }
    }

    /// Make room: the voice released longest ago goes, or else the oldest.
    fn steal(&mut self) {
        let pick = self
            .voices
            .iter()
            .enumerate()
            .min_by_key(|(_, v)| (v.released_at == u64::MAX, v.released_at, v.started))
            .map(|(i, _)| i);
        if let Some(i) = pick {
            self.voices.swap_remove(i);
        }
    }

    /// Render `out.len()` frames (interleaved stereo pairs, or mono samples for a mono
    /// synthesizer), added into `out`.
    pub fn render(&mut self, out: &mut [f32]) {
        let width = if self.mono { 1 } else { 2 };
        let frames = out.len() / width;
        let rate = self.rate as f32;
        let mut done = 0;
        while done < frames {
            let n = CONTROL_FRAMES.min(frames - done);
            let dt = n as f32 / rate;
            for v in &mut self.voices {
                let ch = self.channels.get(&v.channel).cloned().unwrap_or_default();
                let target = Self::control(v, &ch, dt);
                let block = &mut out[done * width..(done + n) * width];
                Self::run_voice(v, &ch, target, block, width);
            }
            self.voices.retain(|v| {
                v.eg.stage != Stage::Done
                    && (v.loop_span.is_some() || v.pos < v.samples.len() as f64)
            });
            done += n;
            self.frames += n as u64;
        }
    }

    /// A voice's control turn: its envelope and LFOs advanced, its pitch set; the gain it ramps
    /// to.
    fn control(v: &mut Voice, ch: &ChannelState, dt: f32) -> f32 {
        let s = &v.settings;
        let env = v.eg.step(&s.eg1, dt);
        v.age += dt;
        let wave = |hz: f32, delay: f32, age: f32| -> f32 {
            if age < delay {
                return 0.0;
            }
            // A triangle from 0, rising first.
            let p = ((age - delay) * hz).fract();
            if p < 0.25 {
                4.0 * p
            } else if p < 0.75 {
                2.0 - 4.0 * p
            } else {
                4.0 * p - 4.0
            }
        };
        let lfo = wave(s.lfo_hz, s.lfo_delay, v.age);
        let vib = wave(s.vib_hz, s.vib_delay, v.age);
        let bend = (f32::from(ch.bend) - 8192.0) / 8192.0 * ch.bend_range;
        let cents = lfo * s.lfo_pitch + vib * s.vib_pitch + bend;
        v.step_now = if cents == 0.0 {
            v.step
        } else {
            v.step * math::pow(2.0, f64::from(cents) / 1200.0)
        };
        let db = v.level_db + curve_db(ch.volume) + curve_db(ch.expression) + lfo * s.lfo_level;
        if let Some(c) = s.cutoff {
            if s.lfo_cutoff != 0.0 {
                v.filter.tune(c + lfo * s.lfo_cutoff, s.q, v.rate);
            }
        }
        env * gain_of_db(db)
    }

    fn run_voice(v: &mut Voice, ch: &ChannelState, target: f32, block: &mut [f32], width: usize) {
        let n = block.len() / width;
        let len = v.samples.len() as f64;
        let start_gain = v.gain;
        let (pl, pr) = if width == 2 {
            let pan = (v.settings.pan + (f32::from(ch.pan) - 64.0) / 127.0).clamp(-0.5, 0.5);
            let a = (pan + 0.5) * std::f32::consts::FRAC_PI_2;
            (math::cosf(a), math::sinf(a))
        } else {
            (1.0, 1.0)
        };
        for f in 0..n {
            if let Some((ls, le)) = v.loop_span {
                while v.pos >= le {
                    v.pos -= le - ls;
                }
            }
            if v.pos >= len {
                break;
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // LINT-OK: a sample index below the wave's length, which is a `usize`.
            let i = v.pos as usize;
            let t = narrow(v.pos - i as f64);
            let next = if i + 1 < v.samples.len() {
                v.samples[i + 1]
            } else if let Some((ls, _)) = v.loop_span {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                // LINT-OK: the loop's start, below the wave's length.
                let j = ls as usize;
                v.samples[j]
            } else {
                0.0
            };
            let s = v.samples[i] + (next - v.samples[i]) * t;
            let g = start_gain + (target - start_gain) * ((f + 1) as f32 / n as f32);
            let x = v.filter.run(s) * g;
            if width == 2 {
                block[2 * f] += x * pl;
                block[2 * f + 1] += x * pr;
            } else {
                block[f] += x;
            }
            v.pos += v.step_now;
        }
        v.gain = target;
    }
}
