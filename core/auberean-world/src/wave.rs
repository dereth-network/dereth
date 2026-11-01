//! A waveform's value at a moment: the one evaluation every animated value of the client goes
//! through (a particle's motion, a material's moving picture, a light's flicker).
//!
//! The moment is the client's running time in seconds, in double precision. Each of the
//! waveform's base, amplitude, phase, frequency and roughness moves on at its own rate; the shapes
//! are steady, speed, noise (a roll of the client's one generator), sine, square, bounce, the
//! gradient noise, its fractal sum, and a frame loop.

use auberean_assets::common::Waveform;
use auberean_primitives::random::Random;
use dereth_primitives::num::{math, to_i32_f64};

use crate::noise::Noise;

/// The client's gradient noise table, made once.
fn noise() -> &'static Noise {
    static NOISE: std::sync::OnceLock<Noise> = std::sync::OnceLock::new();
    NOISE.get_or_init(Noise::new)
}

/// `w`'s value at `t` seconds: [`value_for_phase`] with no phase of its own.
// D2-13338
#[must_use]
pub fn value(w: &Waveform, t: f64, rng: &Random) -> f32 {
    value_for_phase(w, 0.0, t, rng)
}

/// `w`'s value at `t` seconds with `phase` (in half turns) added to its own. Each moving term is
/// its rate times the moment plus its start, kept in single precision (the base also unrounded);
/// the phase in half turns. Steady: the base. Speed: the frequency times the moment plus the phase
/// and the base. Noise: one roll of the client's generator between -1 and 1 times the amplitude,
/// plus the base. Sine, square, bounce: the sine of the frequency times the moment plus the phase
/// (its sign, its size) times the amplitude, plus the base (a sine that is no number counts as
/// below zero). Gradient noise and its fractal sum (six octaves, each twice the last's
/// frequency, weighted by one less the roughness) at the frequency times the phase plus the
/// moment. Frame loop: the amplitude's frames at the frequency's rate, each frame stepping the
/// base on by the phase. Any other shape: the base.
// D2-13337
#[must_use]
pub fn value_for_phase(w: &Waveform, phase: f32, t: f64, rng: &Random) -> f32 {
    let moved = |rate: f32, at: f32| f64::from(rate) * t + f64::from(at);
    #[allow(clippy::cast_possible_truncation)] // the client keeps each term in single precision
    let single = |x: f64| x as f32;
    let base = moved(w.base_rate, w.base);
    if w.kind == 1 {
        return single(base);
    }
    let amplitude = f64::from(single(moved(w.amplitude_rate, w.amplitude)));
    let turn = single(
        (f64::from(phase) + f64::from(w.phase)) * std::f64::consts::PI
            + f64::from(w.phase_rate) * t,
    );
    let frequency = single(moved(w.frequency_rate, w.frequency));
    let roughness = single(moved(w.roughness_rate, w.roughness));
    let base32 = f64::from(single(base));
    let angle = f64::from(frequency) * t + f64::from(turn);
    let out = match w.kind {
        2 => angle + base,
        3 => (2.0 * rng.draw() - 1.0) * amplitude + base32,
        4 => math::sin(angle) * amplitude + base,
        5 => {
            let s = math::sin(angle);
            // a sine that is no number is not at or above zero
            if s >= 0.0 {
                amplitude + base
            } else {
                -amplitude + base
            }
        }
        6 => math::sin(angle).abs() * amplitude + base,
        7 => {
            let at = f64::from(frequency) * f64::from(turn) + t;
            noise().at_wide(at) * amplitude + base32
        }
        8 => {
            let at = f64::from(frequency) * f64::from(turn) + t;
            let h = f64::from(1.0 - roughness);
            f64::from(noise().fbm1(at, h, 2.0, 6.0)) * amplitude + base32
        }
        9 => frame_loop(single(amplitude), frequency, turn, t, base),
        _ => base,
    };
    single(out)
}

/// The frame loop: `frames` frames (cut toward zero) shown `rate` (cut toward zero) times a
/// second each round; at `t` the base stepped on by `step` for each frame shown so far this round.
fn frame_loop(frames: f32, rate: f32, step: f32, t: f64, base: f64) -> f64 {
    let a = to_i32_f64(f64::from(frames));
    let f = to_i32_f64(f64::from(rate));
    if a <= 0 || f <= 0 {
        return base;
    }
    let period = (1.0 / f64::from(f)) * f64::from(a);
    let rounds = to_i32_f64(t / period);
    let k = to_i32_f64((t - f64::from(rounds) * period) / period * f64::from(a));
    base + f64::from(k) * f64::from(step)
}
