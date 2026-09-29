// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/ThreadSafeRandom.cs
//! `ACE.Common.ThreadSafeRandom`: a per-thread `System.Random`. The generator and its seeding
//! policy are in [`crate::random`].

use crate::dotnet::math;
use crate::random::{self, with_thread_rng};

/// `ThreadSafeRandom` (a static class in ACE).
#[derive(Debug)]
pub struct ThreadSafeRandom;

// ACE: ThreadSafeRandom.maxExclusive
/// The largest double below 1.0.
const MAX_EXCLUSIVE: f64 = 0.999_999_999_999_999_9;

impl ThreadSafeRandom {
    // ACE: ThreadSafeRandom.Next(float, float)
    /// A double in `[min, max)`: `NextDouble() * (max - min) + min`. `max - min` is computed in
    /// `float`, as in ACE, before widening. (C# overload `Next(float, float)`.)
    #[must_use]
    pub fn next_float(min: f32, max: f32) -> f64 {
        with_thread_rng(|r| r.next_double() * f64::from(max - min) + f64::from(min))
    }

    // ACE: ThreadSafeRandom.Next(int, int)
    /// An integer in `[min, max]`, **inclusive** of `max`: `Random.Next(min, max + 1)`.
    ///
    /// # Panics
    /// When `min > max`, and when `max == int.MaxValue` with `min > int.MinValue`: `max + 1`
    /// wraps to `int.MinValue` in C# and `Random.Next` throws `ArgumentOutOfRangeException`.
    #[must_use]
    pub fn next(min: i32, max: i32) -> i32 {
        // ACE-BUG: `max + 1` overflows for int.MaxValue (unchecked C# wraps), so the call throws.
        with_thread_rng(|r| r.next_range(min, max.wrapping_add(1)))
    }

    // ACE: ThreadSafeRandom.NextInterval
    /// `Math.Max(0.0, NextDouble() - qualityMod)`.
    #[must_use]
    pub fn next_interval(quality_mod: f32) -> f64 {
        with_thread_rng(|r| math::max(0.0, r.next_double() - f64::from(quality_mod)))
    }

    // ACE: ThreadSafeRandom.NextIntervalMax
    /// `Math.Min(maxExclusive, NextDouble() + qualityMod)`.
    #[must_use]
    pub fn next_interval_max(quality_mod: f32) -> f64 {
        with_thread_rng(|r| math::min(MAX_EXCLUSIVE, r.next_double() + f64::from(quality_mod)))
    }

    /// Reseeds the calling thread's generator ([`random::seed`]). Tests only.
    pub fn seed(seed: u64) {
        random::seed(seed);
    }

    /// Reseeds the calling thread from OS entropy ([`random::seed_from_entropy`]). The server
    /// binary only.
    pub fn seed_from_entropy() {
        random::seed_from_entropy();
    }
}
