//! The effects on the music's paths: a compressor on each, as the paths' configurations set it.
//!
//! The compressor follows the level of what passes (its peak, rising over the attack and falling
//! over the release), and above the threshold lets only a ratio of the excess through; the result
//! is raised by the gain, and the sound is held back by the predelay so the level is known before
//! the sound arrives.

use std::collections::VecDeque;

use dereth_primitives::num::math;

/// A compressor's settings: the gain (decibels), attack and release (milliseconds), threshold
/// (decibels), ratio and predelay (milliseconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressorSettings {
    pub gain: f32,
    pub attack: f32,
    pub release: f32,
    pub threshold: f32,
    pub ratio: f32,
    pub predelay: f32,
}

/// The compressor of the path all music heard everywhere shares.
pub const FLAT_MUSIC: CompressorSettings = CompressorSettings {
    gain: 10.0,
    attack: 10.0,
    release: 200.0,
    threshold: -30.0,
    ratio: 2.2,
    predelay: 4.0,
};

/// The compressor of a placed piece's path.
pub const PLACED_MUSIC: CompressorSettings = CompressorSettings {
    gain: 13.2,
    attack: 10.0,
    release: 200.0,
    threshold: -30.0,
    ratio: 3.0,
    predelay: 4.0,
};

/// A compressor running over interleaved frames.
#[derive(Clone, Debug)]
pub struct Compressor {
    s: CompressorSettings,
    width: usize,
    attack: f32,
    release: f32,
    makeup: f32,
    /// The level followed, as a gain.
    level: f32,
    delay: VecDeque<f32>,
}

impl Compressor {
    /// A compressor over frames of `width` channels at `rate`.
    #[must_use]
    pub fn new(s: CompressorSettings, width: usize, rate: u32) -> Self {
        let r = rate.max(1) as f32;
        let coefficient = |ms: f32| {
            if ms <= 0.0 {
                0.0
            } else {
                math::expf(-1.0 / (ms / 1000.0 * r))
            }
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // LINT-OK: a few milliseconds of frames.
        let delay = (s.predelay / 1000.0 * r).round().max(0.0) as usize * width.max(1);
        Self {
            s,
            width: width.max(1),
            attack: coefficient(s.attack),
            release: coefficient(s.release),
            makeup: math::powf(10.0, s.gain / 20.0),
            level: 0.0,
            delay: std::iter::repeat_n(0.0, delay).collect(),
        }
    }

    /// Compress `block` in place.
    pub fn run(&mut self, block: &mut [f32]) {
        for frame in block.chunks_mut(self.width) {
            let peak = frame.iter().fold(0f32, |a, x| a.max(x.abs()));
            let k = if peak > self.level {
                self.attack
            } else {
                self.release
            };
            self.level = peak + (self.level - peak) * k;
            let gain = if self.level > 0.0 {
                #[allow(clippy::cast_possible_truncation)]
                // LINT-OK: a level in decibels.
                let db = (20.0 * math::log10(f64::from(self.level))) as f32;
                let over = db - self.s.threshold;
                if over > 0.0 {
                    math::powf(10.0, -over * (1.0 - 1.0 / self.s.ratio) / 20.0)
                } else {
                    1.0
                }
            } else {
                1.0
            };
            for x in frame.iter_mut() {
                self.delay.push_back(*x);
                let late = self.delay.pop_front().unwrap_or(0.0);
                *x = late * gain * self.makeup;
            }
        }
    }
}
