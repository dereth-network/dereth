//! The server's random numbers: [`DotNetRandom`], a faithful model of seeded `System.Random`, and
//! the thread-local generator behind [`ThreadSafeRandom`](crate::thread_safe_random::ThreadSafeRandom).
//!
//! # Algorithm
//! `new Random(seed)` in .NET 6+ selects `Net5CompatSeedImpl`, whose `CompatPrng` is Knuth's
//! subtractive generator (TAOCP vol. 2 §3.6, "ran3") as .NET has always shipped it. Source:
//! dotnet/runtime `src/libraries/System.Private.CoreLib/src/System/Random.Net5CompatImpl.cs`.
//!
//! * Seeding fills a 56-entry table: `seedArray[55] = 161803398 - |seed|` (`int.MinValue` maps to
//!   `int.MaxValue`), the other 54 entries by stepping the index by 21 modulo 55, then four
//!   mixing passes of `seedArray[i] -= seedArray[1 + (i + 30) % 55]`. `inext = 0`,
//!   `inextp = 21`.
//! * `InternalSample` advances both indices (wrapping 56 to 1), returns
//!   `seedArray[inext] - seedArray[inextp]` folded into `[0, int.MaxValue)`, and stores it.
//! * `Sample()` is `InternalSample() * (1.0 / int.MaxValue)`; `NextDouble()` is `Sample()`.
//! * `Next(min, max)` is `(int)(Sample() * range) + min` when `range = max - min` fits in `int`,
//!   else the large-range sample built from two draws.
//!
//! The tests pin the first outputs for several seeds; those values were produced by the .NET
//! 8.0.22 runtime (the algorithm is frozen for compatibility, so every runtime agrees).
//!
//! # Seeding policy
//! Each thread owns its generator. Nothing seeds it implicitly from entropy:
//! * a thread that draws before anything seeded it uses [`DEFAULT_SEED`], so an unseeded test is
//!   deterministic;
//! * [`seed`] reseeds the calling thread (tests);
//! * [`seed_from_entropy`] reseeds the calling thread from OS entropy; the server binary calls it
//!   at the start of every thread it spawns that draws (world, network, database).
//!
//! A second per-thread generator ([`with_port_rng`]) serves draws the port makes that ACE does
//! not (retail timings ACE fixed), so they never shift the sequence of ACE's own draws. It follows
//! the same policy: [`DEFAULT_SEED`] until seeded, and [`seed`] and [`seed_from_entropy`] reseed
//! it too, from a seed derived from the main one.
//!
//! ACE itself uses `new Random()` (unseeded), which .NET 6+ implements with xoshiro256**, not this
//! algorithm; draw-for-draw alignment with a live ACE therefore needs a seeded ACE build (a
//! recorded divergence on `ThreadSafeRandom`).

use std::cell::RefCell;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

const MSEED: i32 = 161_803_398;

/// The seed a thread's generator starts from when nothing seeded it.
pub const DEFAULT_SEED: i32 = 0;

/// Seeded `System.Random` (`Net5CompatSeedImpl`). See the module documentation.
#[derive(Clone)]
pub struct DotNetRandom {
    seed_array: [i32; 56],
    inext: usize,
    inextp: usize,
}

impl std::fmt::Debug for DotNetRandom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DotNetRandom")
            .field("inext", &self.inext)
            .field("inextp", &self.inextp)
            .finish_non_exhaustive()
    }
}

impl DotNetRandom {
    /// `new Random(seed)`.
    #[must_use]
    pub fn new(seed: i32) -> Self {
        let mut seed_array = [0i32; 56];
        let subtraction = if seed == i32::MIN {
            i32::MAX
        } else {
            seed.abs()
        };
        let mut mj = MSEED - subtraction;
        seed_array[55] = mj;
        let mut mk = 1i32;
        let mut ii = 0usize;
        for _ in 1..55 {
            ii += 21;
            if ii >= 55 {
                ii -= 55;
            }
            seed_array[ii] = mk;
            mk = mj.wrapping_sub(mk);
            if mk < 0 {
                mk = mk.wrapping_add(i32::MAX);
            }
            mj = seed_array[ii];
        }
        for _ in 1..5 {
            for i in 1..56 {
                let mut n = i + 30;
                if n >= 55 {
                    n -= 55;
                }
                seed_array[i] = seed_array[i].wrapping_sub(seed_array[1 + n]);
                if seed_array[i] < 0 {
                    seed_array[i] = seed_array[i].wrapping_add(i32::MAX);
                }
            }
        }
        Self {
            seed_array,
            inext: 0,
            inextp: 21,
        }
    }

    /// `InternalSample()`: a value in `[0, int.MaxValue)`.
    fn internal_sample(&mut self) -> i32 {
        let mut loc_inext = self.inext + 1;
        if loc_inext >= 56 {
            loc_inext = 1;
        }
        let mut loc_inextp = self.inextp + 1;
        if loc_inextp >= 56 {
            loc_inextp = 1;
        }
        let mut ret_val = self.seed_array[loc_inext].wrapping_sub(self.seed_array[loc_inextp]);
        if ret_val == i32::MAX {
            ret_val -= 1;
        }
        if ret_val < 0 {
            ret_val = ret_val.wrapping_add(i32::MAX);
        }
        self.seed_array[loc_inext] = ret_val;
        self.inext = loc_inext;
        self.inextp = loc_inextp;
        ret_val
    }

    /// `Sample()`: `[0.0, 1.0)`.
    fn sample(&mut self) -> f64 {
        f64::from(self.internal_sample()) * (1.0 / f64::from(i32::MAX))
    }

    /// `GetSampleForLargeRange()`: a sample with a full 32 bits of resolution.
    fn get_sample_for_large_range(&mut self) -> f64 {
        let mut result = self.internal_sample();
        if self.internal_sample() % 2 == 0 {
            result = -result;
        }
        let mut d = f64::from(result);
        d += f64::from(i32::MAX - 1);
        d /= 2.0 * f64::from(i32::MAX) - 1.0;
        d
    }

    /// `Next()`: `[0, int.MaxValue)`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> i32 {
        self.internal_sample()
    }

    /// `Next(maxValue)`: `[0, maxValue)`.
    ///
    /// # Panics
    /// When `max_value < 0` (`ArgumentOutOfRangeException`).
    pub fn next_max(&mut self, max_value: i32) -> i32 {
        assert!(
            max_value >= 0,
            "ArgumentOutOfRangeException: 'maxValue' must be greater than or equal to 0"
        );
        #[allow(clippy::cast_possible_truncation)]
        let v = (self.sample() * f64::from(max_value)) as i32;
        v
    }

    /// `Next(minValue, maxValue)`: `[minValue, maxValue)`, or `minValue` when they are equal.
    ///
    /// # Panics
    /// When `min_value > max_value` (`ArgumentOutOfRangeException`).
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub fn next_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        assert!(
            min_value <= max_value,
            "ArgumentOutOfRangeException: 'minValue' cannot be greater than maxValue ({min_value} > {max_value})"
        );
        let range = i64::from(max_value) - i64::from(min_value);
        if range <= i64::from(i32::MAX) {
            ((self.sample() * range as f64) as i32).wrapping_add(min_value)
        } else {
            ((self.get_sample_for_large_range() * range as f64) as i64 + i64::from(min_value))
                as i32
        }
    }

    /// `NextDouble()`: `[0.0, 1.0)`.
    pub fn next_double(&mut self) -> f64 {
        self.sample()
    }
}

thread_local! {
    static RNG: RefCell<Option<DotNetRandom>> = const { RefCell::new(None) };
    static PORT_RNG: RefCell<Option<DotNetRandom>> = const { RefCell::new(None) };
}

/// The port stream's seed for a main seed: a fixed mix, so the two streams differ.
fn port_seed(s: i32) -> i32 {
    s ^ 0x5A5A_5A5A
}

fn entropy_seed() -> i32 {
    let mut h = RandomState::new().build_hasher();
    h.write_u64(0x5EED);
    i32::from_le_bytes(h.finish().to_le_bytes()[..4].try_into().unwrap_or([0; 4]))
}

/// Runs `f` on the calling thread's generator, creating it by the seeding policy on first use.
pub fn with_thread_rng<R>(f: impl FnOnce(&mut DotNetRandom) -> R) -> R {
    RNG.with(|cell| {
        let mut slot = cell.borrow_mut();
        let rng = slot.get_or_insert_with(|| DotNetRandom::new(DEFAULT_SEED));
        f(rng)
    })
}

/// Runs `f` on the calling thread's port generator: the stream for draws ACE does not make (see
/// the module documentation), created by the seeding policy on first use.
pub fn with_port_rng<R>(f: impl FnOnce(&mut DotNetRandom) -> R) -> R {
    PORT_RNG.with(|cell| {
        let mut slot = cell.borrow_mut();
        let rng = slot.get_or_insert_with(|| DotNetRandom::new(port_seed(DEFAULT_SEED)));
        f(rng)
    })
}

/// Reseeds the calling thread's generator. `System.Random` takes an `int` seed; the low 32 bits of
/// `seed` are used as that `int`.
pub fn seed(seed: u64) {
    let s = i32::from_le_bytes(seed.to_le_bytes()[..4].try_into().unwrap_or([0; 4]));
    RNG.with(|cell| *cell.borrow_mut() = Some(DotNetRandom::new(s)));
    PORT_RNG.with(|cell| *cell.borrow_mut() = Some(DotNetRandom::new(port_seed(s))));
}

/// Reseeds the calling thread from OS entropy. Only the server binary calls this, once per
/// thread; it is never implicit.
pub fn seed_from_entropy() {
    let s = entropy_seed();
    RNG.with(|cell| *cell.borrow_mut() = Some(DotNetRandom::new(s)));
    PORT_RNG.with(|cell| *cell.borrow_mut() = Some(DotNetRandom::new(port_seed(s))));
}
