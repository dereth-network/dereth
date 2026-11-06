//! Source distribution arithmetic; callers can supply deterministic draws.
// Rules ported from ac1999 (Adam Plumb), AGPL-3.0: Server/Source/PhatSDK/Support/RandomRange.cpp.

use empyrean_common::dotnet::CsCast;

use empyrean_common::thread_safe_random::ThreadSafeRandom;

pub trait LootRng {
    fn unit(&mut self) -> f64;
    fn integer(&mut self, lo: i32, hi: i32) -> i32 {
        let offset: i32 = (self.unit() * (f64::from(hi) - f64::from(lo) + 1.0))
            .floor()
            .cs_cast();
        lo + offset
    }
}
#[derive(Debug)]
pub struct ThreadRng;
impl LootRng for ThreadRng {
    fn unit(&mut self) -> f64 {
        ThreadSafeRandom::next_interval(0.0)
    }
    fn integer(&mut self, lo: i32, hi: i32) -> i32 {
        ThreadSafeRandom::next(lo, hi)
    }
}
#[derive(Debug, Clone, Copy)]
pub enum Favor {
    Low,
    Mid,
    High,
    Specific(f64),
}

pub fn favored(r: &mut impl LootRng, lo: i32, hi: i32, f: Favor, strength: f64, q: f64) -> i32 {
    if lo == hi {
        return r.integer(lo, hi);
    }
    let width = f64::from(hi - lo);
    let point = match f {
        Favor::Low => f64::from(lo) + width * q,
        Favor::Mid => f64::from(lo) + (width / 2.0).round() + width * q,
        Favor::High => f64::from(hi) - width * q,
        Favor::Specific(v) => v + width * q,
    }
    .clamp(f64::from(lo), f64::from(hi));
    let n = f64::from(hi - lo + 1);
    let weight = |v: i32| {
        let max: f32 = (n * 1000.0).cs_cast();
        let denominator: f32 = dereth_primitives::num::math::pow(
            1.0 + strength * strength / n,
            (point - f64::from(v)).abs(),
        )
        .cs_cast();
        max / denominator
    };
    let total = (lo..=hi).fold(0.0f32, |a, v| a + weight(v));
    let draw: f32 = r.unit().cs_cast();
    let mut cumulative = 0.0f32;
    for v in lo..=hi {
        cumulative += weight(v) / total;
        if cumulative >= draw {
            return v;
        }
    }
    hi
}
pub fn real(
    r: &mut impl LootRng,
    lo: f64,
    hi: f64,
    f: Option<Favor>,
    strength: f64,
    q: f64,
) -> f64 {
    let a = (lo * 1000.0).round().cs_cast();
    let b = (hi * 1000.0).round().cs_cast();
    let value = if let Some(f) = f {
        favored(r, a, b, f, strength, q)
    } else {
        r.integer(a, b)
    };
    (f64::from(value) / 1000.0).clamp(lo, hi)
}
/// Source percentages multiply the stored float by 100 before promotion for quality.
pub fn chance(r: &mut impl LootRng, probability: f64, quality: f64) -> bool {
    let base: f32 = probability.cs_cast();
    let threshold = f64::from(base * 100.0) * (1.0 + 2.0 * quality);
    f64::from(r.integer(0, 99)) < threshold
}
pub fn index(r: &mut impl LootRng, n: usize) -> usize {
    r.integer(0, i32::try_from(n).expect("validated loot pool length") - 1) as usize
}
pub fn weighted(r: &mut impl LootRng, weights: &[i32]) -> usize {
    let total: i32 = weights.iter().map(|n| (*n).max(0)).sum();
    if total == 0 {
        return index(r, weights.len());
    }
    let mut roll = r.integer(0, total - 1);
    for (i, w) in weights.iter().enumerate() {
        roll -= (*w).max(0);
        if roll < 0 {
            return i;
        }
    }
    weights.len() - 1
}
/// The source helper's places=2 actually retains three decimal places.
pub fn modifier_round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}
