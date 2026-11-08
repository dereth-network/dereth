//! The ripples' picture: a small surface of wind ripples that repeats seamlessly, made once, its
//! slopes held as a normal picture with its smaller levels. The water lays it several times over
//! itself, at different sizes, turned and drifting with the scene's clock.

use dereth_primitives::num::math::{atan2f, cosf, expf, powf, sin_cos, sinf};

/// A number from `seed`, the same every time: all of its bits well mixed.
fn pcg(seed: u32) -> u32 {
    let state = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    let word = ((state >> ((state >> 28) + 4)) ^ state).wrapping_mul(277_803_737);
    (word >> 22) ^ word
}

/// The picture's side, texels.
pub const SIDE: u32 = 256;
/// How many waves make it.
const WAVES: u32 = 72;
/// The wind's heading the waves mostly run along, as east and north.
const WIND: (f32, f32) = (0.82, 0.57);

/// One wave of the picture: its whole number of crests across the picture east and north, its
/// height and its phase.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Wave {
    k: (i32, i32),
    height: f32,
    phase: f32,
}

fn unit(seed: u32) -> f32 {
    // LINT-OK: sixteen bits, exact in f32.
    #[allow(clippy::cast_precision_loss)]
    let v = (pcg(seed) >> 16) as f32 / 65_536.0;
    v
}

/// The waves, the same every time: each with a whole number of crests across the picture, so it
/// repeats seamlessly; shorter ones lower, most running near the wind's heading.
fn waves() -> Vec<Wave> {
    let tau = std::f32::consts::TAU;
    let wind = atan2f(WIND.1, WIND.0);
    (0..WAVES)
        .filter_map(|i| {
            let a = unit(i * 4 + 1);
            let b = unit(i * 4 + 2);
            let c = unit(i * 4 + 3);
            // Between three and twenty-two crests across, more of the short.
            let crests = 3.0 + 19.0 * a * a;
            // Within seventy degrees of the wind, most nearer it; one in six from anywhere.
            let spread = if i % 6 == 0 {
                tau
            } else {
                2.4 * (b - 0.5) * (0.5 + b)
            };
            let heading = wind + spread;
            // LINT-OK: a few dozen crests.
            #[allow(clippy::cast_possible_truncation)]
            let k = (
                (crests * cosf(heading)).round() as i32,
                (crests * sinf(heading)).round() as i32,
            );
            if k == (0, 0) {
                return None;
            }
            // LINT-OK: small whole numbers.
            #[allow(clippy::cast_precision_loss)]
            let len = ((k.0 * k.0 + k.1 * k.1) as f32).sqrt();
            Some(Wave {
                k,
                height: powf(len, -1.6),
                phase: c * tau,
            })
        })
        .collect()
}

/// The surface's slope east and north at `(u, v)` (0..1 across the picture), unscaled.
fn slope(waves: &[Wave], u: f32, v: f32) -> (f32, f32) {
    let tau = std::f32::consts::TAU;
    let mut s = (0.0, 0.0);
    for w in waves {
        // LINT-OK: a few dozen crests.
        #[allow(clippy::cast_precision_loss)]
        let (kx, ky) = (w.k.0 as f32, w.k.1 as f32);
        let ph = tau * (kx * u + ky * v) + w.phase;
        // Sharper crests than a sine: the slope of exp(sin), as wind ripples have.
        let e = expf(sinf(ph) - 1.0);
        let d = w.height * e * cosf(ph) * tau;
        s.0 += d * kx;
        s.1 += d * ky;
    }
    s
}

/// The picture, `rgba8unorm`: its levels from the full size down to one texel, each the mean of
/// the one before. Red and green hold the slope east and north as a share of the steepest slope
/// the water's shader takes it to hold (0.5 level), blue nought and alpha whole. Rows run north to south, as pictures do.
#[must_use]
pub fn picture() -> Vec<Vec<u8>> {
    let waves = waves();
    let n = SIDE as usize;
    let mut slopes = vec![(0.0f32, 0.0f32); n * n];
    let mut most = 0.0f32;
    for (row, line) in slopes.chunks_mut(n).enumerate() {
        for (col, s) in line.iter_mut().enumerate() {
            // LINT-OK: texel indices.
            #[allow(clippy::cast_precision_loss)]
            let (u, v) = (
                (col as f32 + 0.5) / n as f32,
                1.0 - (row as f32 + 0.5) / n as f32,
            );
            *s = slope(&waves, u, v);
            most = most.max(s.0.abs()).max(s.1.abs());
        }
    }
    // The steepest texel a little short of the picture's steepest slope.
    let scale = 0.95 / most.max(1e-6);
    let mut level: Vec<(f32, f32)> = slopes.iter().map(|(x, y)| (x * scale, y * scale)).collect();
    let mut size = n;
    let mut levels = Vec::new();
    loop {
        levels.push(
            level
                .iter()
                .flat_map(|(x, y)| {
                    // LINT-OK: 0..255.
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let q = |v: f32| ((v * 0.5 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
                    [q(*x), q(*y), 0, 255]
                })
                .collect(),
        );
        if size == 1 {
            break;
        }
        let half = size / 2;
        let mut next = vec![(0.0, 0.0); half * half];
        for r in 0..half {
            for c in 0..half {
                let at = |rr: usize, cc: usize| level[(2 * r + rr) * size + 2 * c + cc];
                let (a, b, d, e) = (at(0, 0), at(0, 1), at(1, 0), at(1, 1));
                next[r * half + c] = (
                    (a.0 + b.0 + d.0 + e.0) * 0.25,
                    (a.1 + b.1 + d.1 + e.1) * 0.25,
                );
            }
        }
        level = next;
        size = half;
    }
    levels
}

/// One of the layers the water lays the picture in: how many meters one repeat covers, how much
/// it bends the surface, its turn (radians, counterclockwise from east), and how fast it drifts
/// along its own east, meters a second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layer {
    /// Meters one repeat covers.
    pub repeat: f32,
    /// How much it bends the surface.
    pub strength: f32,
    /// Its turn, radians.
    pub turn: f32,
    /// How fast it drifts, meters a second.
    pub drift: f32,
}

/// The layers.
pub const LAYERS: [Layer; 4] = [
    Layer {
        repeat: 2.6,
        strength: 1.0,
        turn: 0.0,
        drift: 0.16,
    },
    Layer {
        repeat: 1.45,
        strength: 0.7,
        turn: 2.04,
        drift: 0.11,
    },
    Layer {
        repeat: 6.3,
        strength: 0.55,
        turn: 4.21,
        drift: 0.07,
    },
    Layer {
        repeat: 0.61,
        strength: 0.35,
        turn: 1.1,
        drift: 0.05,
    },
];

/// Where `layer` lies at `clock` seconds for a frame whose space's origin stands `global` meters
/// east and north in the world: the share of a repeat its picture is moved, across and down, for
/// a point of the frame's space. Worked out in double precision and wrapped, so the ripples stay
/// put in the world wherever the frame's origin is, hold still while the clock does, and never
/// jump however long the clock has run.
#[must_use]
pub fn moved(layer: &Layer, global: [f64; 2], clock: f64) -> [f32; 2] {
    let (sin, cos) = sin_cos(f64::from(layer.turn));
    let repeat = f64::from(layer.repeat);
    let across =
        (cos * global[0] + sin * global[1]) / repeat - clock * f64::from(layer.drift) / repeat;
    let down = (sin * global[0] - cos * global[1]) / repeat;
    // LINT-OK: shares of a repeat, 0..1.
    #[allow(clippy::cast_possible_truncation)]
    [across.rem_euclid(1.0) as f32, down.rem_euclid(1.0) as f32]
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    fn slope_of(level: &[u8], at: usize) -> (i32, i32) {
        (i32::from(level[at * 4]), i32::from(level[at * 4 + 1]))
    }

    /// The picture repeats without a seam: the step across its edge is no larger than the largest
    /// step inside it, each way. And it holds every level down to one texel.
    #[test]
    fn the_ripples_picture_repeats_without_a_seam_and_holds_every_level() {
        let levels = picture();
        assert_eq!(levels.len(), 9);
        let n = SIDE as usize;
        assert_eq!(levels[0].len(), n * n * 4);
        assert_eq!(levels[8].len(), 4);
        let step = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs().max((a.1 - b.1).abs());
        let (mut inside, mut seam) = (0, 0);
        for r in 0..n {
            for c in 0..n {
                let here = slope_of(&levels[0], r * n + c);
                let across = slope_of(&levels[0], r * n + (c + 1) % n);
                let down = slope_of(&levels[0], ((r + 1) % n) * n + c);
                for (next, on_seam) in [(across, c + 1 == n), (down, r + 1 == n)] {
                    if on_seam {
                        seam = seam.max(step(here, next));
                    } else {
                        inside = inside.max(step(here, next));
                    }
                }
            }
        }
        assert!(inside > 0, "the picture holds ripples");
        assert!(seam <= inside, "seam {seam}, inside {inside}");
    }

    /// A held clock moves nothing, a running clock drifts each layer along its own east; and the
    /// frame's origin moving by a whole number of a layer's repeats along its own east leaves it
    /// where it was.
    #[test]
    fn the_layers_hold_still_with_the_clock_and_stay_put_in_the_world() {
        let l = LAYERS[0];
        assert_eq!(
            moved(&l, [192.0, 384.0], 40.0),
            moved(&l, [192.0, 384.0], 40.0)
        );
        let a = moved(&l, [0.0, 0.0], 0.0);
        let b = moved(&l, [0.0, 0.0], 1.0);
        assert!((a[0] - b[0]).abs() > 0.01);
        assert!((a[1] - b[1]).abs() < 1e-6);
        // The first layer has no turn: its east is the world's.
        let c = moved(&l, [f64::from(l.repeat) * 7.0, 0.0], 0.0);
        assert!((c[0] - a[0]).abs() < 1e-3 || (c[0] - a[0]).abs() > 0.999);
    }
}
