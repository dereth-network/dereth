//! The client's one-dimensional gradient noise and its fractal sum: the camera's shake, portal
//! space's tunnel, and the waveforms that flicker by it.

use dereth_primitives::num::math;

/// The client's one-dimensional gradient noise: a table of 256 gradients and a shuffle, from the
/// C library's generator seeded with 0.
#[derive(Clone, Debug)]
pub struct Noise {
    perm: [u8; 512],
    grad: [f32; 512],
}

impl Default for Noise {
    fn default() -> Self {
        Self::new()
    }
}

impl Noise {
    // D2-13290
    #[must_use]
    pub fn new() -> Self {
        let mut state: u32 = 0;
        let mut rand = || {
            state = state.wrapping_mul(214_013).wrapping_add(2_531_011);
            (state >> 16) & 0x7FFF
        };
        let mut perm = [0u8; 512];
        let mut grad = [0.0f32; 512];
        for i in 0..256 {
            perm[i] = u8::try_from(i).unwrap_or(0);
            #[allow(clippy::cast_precision_loss)]
            let g = ((rand() % 512) as f32 - 256.0) / 256.0;
            grad[i] = g;
        }
        for i in (1..256).rev() {
            let j = (rand() % 256) as usize;
            perm.swap(i, j);
        }
        for k in 0..256 {
            perm[256 + k] = perm[k];
            grad[256 + k] = grad[k];
        }
        Self { perm, grad }
    }

    /// The noise at a point: the point and 10000 added, its whole part (toward zero) picking two
    /// neighboring gradients, the curve between their slopes eased by `3f^2 - 2f^3`.
    // D2-13291
    #[must_use]
    pub fn at(&self, x: f32) -> f32 {
        #[allow(clippy::cast_possible_truncation)]
        let v = self.at_wide(f64::from(x)) as f32;
        v
    }

    /// [`Self::at`] for a point given in double precision, worked in it: the point is taken to
    /// single precision first, the first gradient's product kept to single precision, and the
    /// result left in double precision.
    #[must_use]
    pub fn at_wide(&self, x: f64) -> f64 {
        #[allow(clippy::cast_possible_truncation)]
        let x = x as f32;
        let t = f64::from(x) + 10_000.0;
        let i = dereth_primitives::num::to_i32_f64(t);
        let r = t - f64::from(i);
        #[allow(clippy::cast_sign_loss)]
        let k = (i & 255) as usize;
        let g = |k: usize| f64::from(self.grad[usize::from(self.perm[k & 255])]);
        #[allow(clippy::cast_possible_truncation)]
        let a = f64::from((r * g(k)) as f32);
        let d = (r - 1.0) * g(k + 1) - a;
        r * r * (3.0 - (r + r)) * d + a
    }

    /// Fractional Brownian motion over the noise: `octaves` layers, each `lacunarity` times the
    /// frequency of the last and weighted by its frequency to the power `-h`; a fractional last
    /// octave adds that share of one more layer.
    // D2-13292
    #[must_use]
    pub fn fbm1(&self, point: f64, h: f64, lacunarity: f64, octaves: f64) -> f32 {
        let mut weights = Vec::new();
        if 0.0 < octaves {
            let mut frequency = 1.0f64;
            let mut i = 0u32;
            loop {
                weights.push(math::pow(frequency, -h));
                frequency *= lacunarity;
                i += 1;
                if f64::from(i) > octaves {
                    break;
                }
            }
        }
        let mut value = 0.0f64;
        let mut point = point;
        let mut i = 0usize;
        if 0.0 < octaves {
            loop {
                value += self.at_wide(point) * weights[i];
                i += 1;
                point *= lacunarity;
                #[allow(clippy::cast_precision_loss)]
                let done = (i as f64) >= octaves;
                if done {
                    break;
                }
            }
        }
        let rest = octaves - f64::from(dereth_primitives::num::to_i32_f64(octaves));
        if rest != 0.0 {
            let w = weights.get(i).copied().unwrap_or(0.0);
            value += self.at_wide(point) * w * rest;
        }
        #[allow(clippy::cast_possible_truncation)]
        let v = value as f32;
        v
    }
}
