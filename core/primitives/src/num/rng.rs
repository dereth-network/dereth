//! The two pseudo-random generators, which must never be merged.
//!
//! The client draws from both *within the same subsystem* — ambient sound timing uses [`Ran2`] while
//! the probability gate on the same sound uses [`CrtRand`]; character generation picks heritage with
//! one and clothing with the other. Draw order is observable.

/// Turbine's `Random`, which is the Numerical Recipes `ran2` generator.
///
/// Seeded exactly once per session, during client start-up immediately after the frame timer,
/// with `time(NULL)` — so a whole session's Turbine randomness derives from the wall-clock
/// second at which the client started.
///
/// All arithmetic is 32-bit signed with truncating division, matching the original.
#[derive(Debug, Clone)]
pub struct Ran2 {
    seed: i32,
    idum2: i32,
    iy: i32,
    iv: [i32; Self::NTAB],
}

impl Ran2 {
    const NTAB: usize = 32;
    const IM1: i32 = 2_147_483_563;
    const IA1: i32 = 40_014;
    const IQ1: i32 = 53_668;
    const IM2: i32 = 2_147_483_399;
    const IA2: i32 = 40_692;
    const IQ2: i32 = 52_774;
    const NDIV: i32 = 67_108_862;
    const IMM1: i32 = 2_147_483_562;
    const AM: f64 = 4.656_613_057_391_769e-10;
    const RNMX: f64 = 0.999_999_88;

    /// Seed the generator.
    ///
    /// Note that a seed of zero becomes one, but negative seeds are **not** normalised, which is a
    /// deviation from textbook `ran2` and is reproduced here deliberately.
    #[must_use]
    pub fn new(seed: i32) -> Self {
        let seed = if seed == 0 { 1 } else { seed };
        let mut idum = seed;
        let mut iv = [0i32; Self::NTAB];
        // 40 iterations: 8 warm-ups, then fill iv[31] down to iv[0].
        for j in (0..(Self::NTAB + 8)).rev() {
            let k = idum / Self::IQ1; // truncating toward zero
            idum = idum
                .wrapping_mul(Self::IA1)
                .wrapping_sub(k.wrapping_mul(Self::IM1));
            if idum < 0 {
                idum += Self::IM1;
            }
            if j < Self::NTAB {
                iv[j] = idum;
            }
        }
        Self {
            seed: idum,
            idum2: seed,
            iy: iv[0],
            iv,
        }
    }

    /// The next draw: an `f64` in `[0, 0.99999988]`.
    pub fn next_f64(&mut self) -> f64 {
        let k = self.seed / Self::IQ1;
        self.seed = self
            .seed
            .wrapping_mul(Self::IA1)
            .wrapping_sub(k.wrapping_mul(Self::IM1));
        if self.seed < 0 {
            self.seed += Self::IM1;
        }

        let k = self.idum2 / Self::IQ2;
        self.idum2 = self
            .idum2
            .wrapping_mul(Self::IA2)
            .wrapping_sub(k.wrapping_mul(Self::IM2));
        if self.idum2 < 0 {
            self.idum2 += Self::IM2;
        }

        #[allow(clippy::cast_sign_loss)] // iy is always >= 1 here, so the quotient is 0..31
        let j = (self.iy / Self::NDIV) as usize;
        self.iy = self.iv[j] - self.idum2;
        self.iv[j] = self.seed;
        if self.iy < 1 {
            self.iy += Self::IMM1;
        }

        let t = f64::from(self.iy) * Self::AM;
        if t > Self::RNMX {
            Self::RNMX
        } else {
            t
        }
    }

    /// An integer roll. Inclusive of both endpoints.
    pub fn roll_i32(&mut self, a: i32, b: i32) -> i32 {
        if a == b {
            return a;
        }
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        #[allow(clippy::cast_sign_loss)] // the original does exactly this reinterpretation
        let range = (hi.wrapping_sub(lo) as u32).wrapping_add(1);
        let d = f64::from(range);
        lo + super::to_i32_f64(d * self.next_f64())
    }

    /// The same roll over floats.
    pub fn roll_f32(&mut self, a: f32, b: f32) -> f32 {
        if a == b {
            return a;
        }
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        #[allow(clippy::cast_possible_truncation)] // the original narrows to f32 here too
        let r = self.next_f64() as f32;
        lo + (hi - lo) * r
    }
}

/// The Microsoft C runtime's `rand`, used by a separate set of call sites: the Perlin noise tables
/// (seeded `srand(0)`), character-generation clothing, and the probability gates on ambient sounds.
///
/// `s = s * 214013 + 2531011; return (s >> 16) & 0x7FFF`.
#[derive(Debug, Clone)]
pub struct CrtRand {
    state: u32,
}

impl CrtRand {
    /// `srand(seed)`. The Perlin tables use `srand(0)`.
    #[must_use]
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// `rand()` — a value in `0..=0x7FFF`.
    pub fn next_u16(&mut self) -> u16 {
        self.state = self.state.wrapping_mul(214_013).wrapping_add(2_531_011);
        #[allow(clippy::cast_possible_truncation)] // masked to 15 bits on the line above
        {
            ((self.state >> 16) & 0x7FFF) as u16
        }
    }
}

impl Default for CrtRand {
    /// The client's Perlin tables are built after `srand(0)`.
    fn default() -> Self {
        Self::new(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Oracle: the recovered random-number behavior, which gives the algorithm and constants read
    // out of the retail binary.
    #[test]
    fn ran2_stays_in_range_and_is_deterministic() {
        let mut a = Ran2::new(12345);
        let mut b = Ran2::new(12345);
        for _ in 0..10_000 {
            let x = a.next_f64();
            assert_eq!(x, b.next_f64(), "same seed must give the same stream");
            assert!((0.0..=0.999_999_88).contains(&x), "out of range: {x}");
        }
    }

    #[test]
    fn ran2_zero_seed_becomes_one_but_negative_is_left_alone() {
        // The zero-to-one fixup is in the original; the absence of a negative fixup also is.
        let mut zero = Ran2::new(0);
        let mut one = Ran2::new(1);
        assert_eq!(zero.next_f64(), one.next_f64());

        let mut neg = Ran2::new(-42);
        let x = neg.next_f64();
        assert!((0.0..=0.999_999_88).contains(&x), "{x}");
    }

    #[test]
    fn roll_i32_is_inclusive_of_both_endpoints() {
        let mut r = Ran2::new(7);
        let mut seen_lo = false;
        let mut seen_hi = false;
        for _ in 0..20_000 {
            let v = r.roll_i32(1, 10);
            assert!((1..=10).contains(&v), "{v}");
            seen_lo |= v == 1;
            seen_hi |= v == 10;
        }
        assert!(
            seen_lo && seen_hi,
            "roll_i32(1, 10) must reach both 1 and 10"
        );
    }

    #[test]
    fn roll_with_equal_bounds_returns_that_bound_without_drawing() {
        let mut r = Ran2::new(99);
        let before = r.clone().next_f64();
        assert_eq!(r.roll_i32(5, 5), 5);
        // the generator must not have advanced
        assert_eq!(r.next_f64(), before);
    }

    // Oracle: the MSVC LCG recurrence recorded in
    // the original runtime's random-number behavior.
    #[test]
    fn crt_rand_matches_the_known_msvc_sequence_for_seed_zero() {
        // These are the first values MSVC's rand() produces after srand(0); they are a well-known
        // fixed sequence and are what the Perlin tables are built from.
        let mut r = CrtRand::new(0);
        assert_eq!(r.next_u16(), 38);
        assert_eq!(r.next_u16(), 7719);
        assert_eq!(r.next_u16(), 21238);
        assert_eq!(r.next_u16(), 2437);
    }

    #[test]
    fn crt_rand_stays_in_range() {
        let mut r = CrtRand::new(1);
        for _ in 0..10_000 {
            assert!(r.next_u16() <= 0x7FFF);
        }
    }
}
