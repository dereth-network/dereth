//! How a flame's light moves: the one model every flame in the presentation is lit with.
//!
//! The signal is band-limited noise over the scene's clock, a few octaves of gradient noise, so it
//! never repeats and never steps from one frame to the next. A flame's size sets its speed, as a
//! buoyant flame's pulsing does (`f = 1.5 / sqrt(D)` hertz for a flame `D` meters across): a candle
//! dances, a bonfire rolls. Less light is redder light, and the light itself sways, so what it
//! throws moves with it. Each flame has a number of its own, so no two move together, and the same
//! clock gives the same flicker however fast the frames are drawn.

use glam::Vec3;

/// How a light stands at one moment, against how it stands burning steadily.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flicker {
    /// Its brightness, 1 for its usual.
    pub brightness: f32,
    /// What its colour is multiplied by, brightness included.
    pub tint: [f32; 3],
    /// How far it stands from its place, meters, height last.
    pub sway: Vec3,
}

impl Flicker {
    /// A light that does not move.
    pub const STEADY: Self = Self {
        brightness: 1.0,
        tint: [1.0; 3],
        sway: Vec3::ZERO,
    };
}

/// How far brightness moves about its usual when the noise is at its widest, a share of it. The
/// noise is seldom past half of that.
const REACH: f32 = 0.32;
/// How far a gust of air takes it down besides, a share of it.
const GUST: f32 = 0.14;
/// How much green and blue fall further than red as a flame dims, and rise as it brightens.
const COOLING: [f32; 2] = [0.35, 0.8];
/// A flame's slowest pulsing and its fastest, hertz. Past the fastest it would read as a strobe.
const PULSE: (f64, f64) = (0.8, 4.0);

/// The flicker of a flame `size` meters across at `time` seconds, with its own number `seed`.
#[must_use]
pub fn flame(seed: u32, size: f32, time: f64) -> Flicker {
    let across = f64::from(size.max(0.02));
    let pulse = (1.5 / across.sqrt()).clamp(PULSE.0, PULSE.1);
    // Octaves with half the strength at each doubling, the slow motion of a flame carrying most.
    let wander = noise(seed, 0, time * pulse) * 0.57
        + noise(seed, 1, time * pulse * 2.13) * 0.29
        + noise(seed, 2, time * pulse * 4.37) * 0.14;
    // Now and then air catches it, and it ducks and recovers.
    let gust = (noise(seed, 3, time * 0.31) - 0.25).max(0.0) / 0.75;
    let k = 1.0 + REACH * wander - GUST * gust * gust;
    let lean = (0.015 + 0.05 * size).min(0.08);
    let sway = Vec3::new(
        noise(seed, 4, time * pulse * 0.7),
        noise(seed, 5, time * pulse * 0.7),
        noise(seed, 6, time * pulse) * 0.6 + 0.4 * (k - 1.0) / REACH,
    ) * lean;
    Flicker {
        brightness: k,
        tint: tint(k),
        sway,
    }
}

/// The flicker of a bed of embers: a slow, deep breath, with nothing to sway.
#[must_use]
pub fn embers(seed: u32, time: f64) -> Flicker {
    let k = 1.0 + 0.24 * noise(seed, 7, time * 0.35) + 0.08 * noise(seed, 8, time * 0.9);
    Flicker {
        brightness: k,
        tint: tint(k),
        sway: Vec3::ZERO,
    }
}

/// What a flame's colour is multiplied by at brightness `k`: the cooler channels move further.
fn tint(k: f32) -> [f32; 3] {
    let d = k - 1.0;
    [
        k,
        k * (1.0 + COOLING[0] * d).max(0.0),
        k * (1.0 + COOLING[1] * d).max(0.0),
    ]
}

/// About how many meters across a flame is whose light reaches `falloff` meters: the data says how
/// far a flame lights, and a larger flame lights farther.
#[must_use]
pub fn size_of_a_flame_reaching(falloff: f32) -> f32 {
    (falloff * 0.05).clamp(0.03, 2.0)
}

/// A number of a flame's own, from where it stands.
#[must_use]
pub fn seed_at(at: Vec3) -> u32 {
    // LINT-OK: hashing the bits of a position. Not a float conversion.
    at.x.to_bits() ^ at.y.to_bits().rotate_left(11) ^ at.z.to_bits().rotate_left(22)
}

/// Gradient noise at `t`, about -1 to 1, smooth in its slope, for the flame `seed`'s channel
/// `channel`.
fn noise(seed: u32, channel: u32, t: f64) -> f32 {
    let at = t.floor();
    #[allow(clippy::cast_possible_truncation)] // The fraction of a whole number apart.
    let f = (t - at) as f32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // The lattice's place, wrapped: a clock's lifetime stays far inside the type.
    let i = (at as i64) as u32;
    let slope = |k: u32| {
        let mut h = k
            .wrapping_mul(0x9E37_79B1)
            .wrapping_add(seed)
            .wrapping_add(channel.wrapping_mul(0x85EB_CA77));
        h ^= h >> 15;
        h = h.wrapping_mul(0xC2B2_AE3D);
        h ^= h >> 13;
        h = h.wrapping_mul(0x27D4_EB2F);
        h ^= h >> 16;
        #[allow(clippy::cast_precision_loss)] // A slope needs no more than a float's bits.
        let s = h as f32 / u32::MAX as f32;
        s * 4.0 - 2.0
    };
    let (a, b) = (slope(i) * f, slope(i.wrapping_add(1)) * (f - 1.0));
    let s = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    (a + (b - a) * s).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi lighting)
    use super::*;

    fn samples(seed: u32, size: f32, step: f64, n: u32) -> Vec<f32> {
        (0..n)
            .map(|k| flame(seed, size, f64::from(k) * step).brightness)
            .collect()
    }

    #[test]
    fn a_flame_stays_within_what_reads_as_a_flame_and_never_goes_out() {
        for seed in 0..40 {
            for v in samples(seed * 7919, 0.3, 0.013, 4000) {
                assert!((0.6..=1.3).contains(&v), "{v}");
            }
        }
    }

    #[test]
    fn a_flame_moves_smoothly_from_frame_to_frame_with_no_step_or_strobe() {
        for size in [0.02, 0.1, 0.5, 3.0] {
            let v = samples(11, size, 1.0 / 60.0, 3000);
            let worst = v
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0, f32::max);
            assert!(worst < 0.08, "{size} m: {worst} in a frame");
        }
    }

    /// How many times a second it crosses its middle: how fast it reads.
    fn crossings(size: f32) -> f32 {
        let v: Vec<f32> = (0..200)
            .flat_map(|seed| samples(seed * 31 + 5, size, 0.005, 2000))
            .collect();
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        let n = v
            .chunks(2000)
            .flat_map(|c| c.windows(2))
            .filter(|w| (w[0] - mean) * (w[1] - mean) < 0.0)
            .count();
        n as f32 / (200.0 * 10.0)
    }

    #[test]
    fn a_small_flame_dances_faster_than_a_large_one() {
        let (candle, torch, bonfire) = (crossings(0.1), crossings(0.5), crossings(2.5));
        assert!(candle > torch * 1.4, "{candle} and {torch}");
        assert!(torch > bonfire * 1.4, "{torch} and {bonfire}");
    }

    #[test]
    fn two_flames_do_not_move_together_and_one_clock_gives_one_flicker() {
        let (a, b) = (samples(1, 0.3, 0.02, 1500), samples(2, 0.3, 0.02, 1500));
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        let (ma, mb) = (mean(&a), mean(&b));
        let cov: f32 = a.iter().zip(&b).map(|(x, y)| (x - ma) * (y - mb)).sum();
        let var = |v: &[f32], m: f32| v.iter().map(|x| (x - m) * (x - m)).sum::<f32>();
        let r = cov / (var(&a, ma) * var(&b, mb)).sqrt();
        assert!(r.abs() < 0.3, "correlation {r}");
        assert_eq!(flame(9, 0.3, 123.456), flame(9, 0.3, 123.456));
    }

    #[test]
    fn it_does_not_repeat_as_a_sum_of_sines_would() {
        let v = samples(77, 0.3, 0.02, 12000);
        let m = v.iter().sum::<f32>() / v.len() as f32;
        let at = |lag: usize| {
            let n = v.len() - lag;
            let c: f32 = (0..n).map(|i| (v[i] - m) * (v[i + lag] - m)).sum();
            let s: f32 = v.iter().map(|x| (x - m) * (x - m)).sum();
            c / s
        };
        for lag in (200..6000).step_by(37) {
            assert!(at(lag) < 0.45, "lag {lag}: {}", at(lag));
        }
    }

    #[test]
    fn a_dimmer_flame_is_a_redder_flame() {
        let dim = tint(0.8);
        let bright = tint(1.15);
        assert!(dim[2] / dim[0] < dim[1] / dim[0] && dim[1] / dim[0] < 1.0);
        assert!(bright[2] / bright[0] > 1.0);
        assert!((tint(1.0)[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_flame_sways_a_little_and_embers_only_breathe() {
        for k in 0..2000 {
            let s = flame(3, 0.3, f64::from(k) * 0.01).sway;
            assert!(s.length() < 0.12, "{s:?}");
        }
        assert_eq!(embers(3, 5.0).sway, Vec3::ZERO);
        let v: Vec<f32> = (0..100)
            .map(|k| embers(3, f64::from(k) * 0.1).brightness)
            .collect();
        assert!(v.iter().any(|x| (x - 1.0).abs() > 0.02));
    }
}
