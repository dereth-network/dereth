//! The world's dice: a small seeded generator, so a run is the same each time for the same seed
//! and the same events (the tests' seed is fixed; the server takes one from the system's random source at start).

/// A seeded generator (xorshift64*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let v = (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32;
        v
    }

    /// Whether a chance in `[0, 1]` comes up.
    pub fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    /// A whole number in `[0, n)` (0 when `n` is 0).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation)]
        let i = (self.next_u64() % n as u64) as usize;
        i
    }

    /// A number in `[lo, hi]`.
    pub fn between(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }

    /// One of several weighted choices: the index of the one picked (none when every weight is 0).
    pub fn weighted(&mut self, weights: impl IntoIterator<Item = f32> + Clone) -> Option<usize> {
        let total: f32 = weights.clone().into_iter().filter(|w| *w > 0.0).sum();
        if total <= 0.0 {
            return None;
        }
        let mut r = self.unit() * total;
        let mut last = None;
        for (i, w) in weights.into_iter().enumerate() {
            if w <= 0.0 {
                continue;
            }
            last = Some(i);
            if r < w {
                return Some(i);
            }
            r -= w;
        }
        last
    }
}
