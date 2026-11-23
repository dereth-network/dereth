//! The converted world's height table: 256 ascending heights, entry 0 at 0 m.

/// Entries in a height table.
pub const LEN: usize = 256;

/// AC2's sea: its water plane over the open sea, in AC2 meters. The converted world lowers
/// everything by this, so its sea is at 0 m as AC1's is.
pub const SEA: f32 = 70.95;

/// The highest height AC1's table accepts.
pub const MAX: f32 = 800.0;

/// AC2 meters lowered to the converted world's, floored at 0.
#[must_use]
pub fn lower(h: f64) -> f64 {
    (h - f64::from(SEA)).max(0.0)
}

/// AC2's own table lowered and floored, sorted ascending, entry 0 at 0 m.
#[must_use]
pub fn lowered(ac2: &[f32; LEN]) -> [f32; LEN] {
    let mut t = [0f32; LEN];
    for (o, &h) in t.iter_mut().zip(ac2) {
        // LINT-OK: a height of a few hundred meters.
        #[allow(clippy::cast_possible_truncation)]
        let v = lower(f64::from(h)) as f32;
        *o = v;
    }
    t.sort_by(f32::total_cmp);
    t[0] = 0.0;
    t
}

/// The index of the entry nearest `h`; on a tie, the lower one. `table` is ascending.
#[must_use]
pub fn quantize(table: &[f32; LEN], h: f64) -> u8 {
    let i = table.partition_point(|&t| f64::from(t) < h);
    let best = if i == 0 {
        0
    } else if i >= LEN {
        LEN - 1
    } else {
        let (lo, hi) = (f64::from(table[i - 1]), f64::from(table[i]));
        if h - lo <= hi - h {
            i - 1
        } else {
            i
        }
    };
    u8::try_from(best).unwrap_or(u8::MAX)
}

/// How a table rounds a set of heights: the mean and the greatest error, in meters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rounding {
    pub mean: f64,
    pub max: f64,
}

/// A table fitted to `heights` (meters, at least 0): entry 0 at 0 m, the other 255 spaced by the
/// cumulative of the heights' density's cube root (the spacing that keeps the mean square error
/// least for many entries) plus a uniform share that bounds the widest step. Of a fixed ladder of
/// uniform shares, the one with the least mean error whose greatest error is at most `max_err`
/// wins (else the one with the least greatest error). Worked over a histogram of 1 cm bins, so
/// the same heights in any order give the same table. Answers the table and how it rounds.
#[must_use]
pub fn fitted(heights: &[f32], max_err: f64) -> ([f32; LEN], Rounding) {
    const BIN: f64 = 0.01;
    const SHARES: [f64; 12] = [
        0.0, 0.02, 0.05, 0.1, 0.2, 0.35, 0.5, 1.0, 2.0, 5.0, 10.0, 1e6,
    ];
    let top = heights.iter().copied().fold(0f32, f32::max).min(MAX);
    // LINT-OK: at most 80,000 bins of 1 cm under 800 m.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let bins = (f64::from(top) / BIN) as usize + 1;
    let mut hist = vec![0u64; bins];
    for &h in heights {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let b = ((f64::from(h.clamp(0.0, top)) / BIN).round() as usize).min(bins - 1);
        hist[b] += 1;
    }
    let at = |b: usize| b as f64 * BIN;
    let cube: Vec<f64> = hist
        .iter()
        .map(|&c| dereth_primitives::num::math::cbrt(c as f64))
        .collect();
    let mean_cube = cube.iter().skip(1).sum::<f64>() / (bins.max(2) - 1) as f64;
    let place = |share: f64| -> [f32; LEN] {
        let w = |b: usize| cube[b] + share * mean_cube.max(1e-9);
        let total: f64 = (1..bins).map(w).sum();
        let mut t = [0f64; LEN];
        let mut k = 1;
        if total > 0.0 {
            let mut acc = 0.0;
            for b in 1..bins {
                acc += w(b);
                while k < LEN && acc >= total * (k as f64 - 0.5) / (LEN - 1) as f64 {
                    t[k] = at(b);
                    k += 1;
                }
            }
        }
        while k < LEN {
            t[k] = at(bins - 1);
            k += 1;
        }
        let mut out = [0f32; LEN];
        let mut last = 0f32;
        for k in 1..LEN {
            // LINT-OK: heights under 800 m.
            #[allow(clippy::cast_possible_truncation)]
            let v = (t[k] as f32).max(last + 0.01);
            out[k] = v;
            last = v;
        }
        out
    };
    let rate = |t: &[f32; LEN]| -> Rounding {
        let (mut sum, mut n, mut max) = (0.0, 0.0, 0.0f64);
        for (b, &c) in hist.iter().enumerate() {
            if c == 0 {
                continue;
            }
            let h = at(b);
            let e = (f64::from(t[usize::from(quantize(t, h))]) - h).abs();
            sum += e * c as f64;
            n += c as f64;
            max = max.max(e);
        }
        Rounding {
            mean: if n > 0.0 { sum / n } else { 0.0 },
            max,
        }
    };
    let mut best: Option<([f32; LEN], Rounding)> = None;
    for share in SHARES {
        let t = place(share);
        let r = rate(&t);
        let better = match &best {
            None => true,
            Some((_, b)) => {
                let (ok, bok) = (r.max <= max_err, b.max <= max_err);
                match (ok, bok) {
                    (true, false) => true,
                    (false, true) => false,
                    (true, true) => r.mean < b.mean,
                    (false, false) => r.max < b.max,
                }
            }
        };
        if better {
            best = Some((t, r));
        }
    }
    best.expect("a ladder of shares")
}

/// Whether `t` is a table AC1 takes: ascending, entry 0 at 0 m, nothing above [`MAX`].
#[must_use]
pub fn is_valid(t: &[f32; LEN]) -> bool {
    t[0] == 0.0 && t.windows(2).all(|w| w[0] < w[1]) && t[LEN - 1] <= MAX
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's height table)
    use super::*;

    #[test]
    fn lowering_puts_ac2s_sea_at_zero_and_floors_what_lies_below_it() {
        assert_eq!(lower(f64::from(SEA)), 0.0);
        assert_eq!(lower(40.0), 0.0);
        assert!((lower(100.0) - 29.05).abs() < 1e-5);
    }

    #[test]
    fn the_lowered_table_starts_at_zero_and_ascends() {
        let mut ac2 = [0f32; LEN];
        for (i, h) in ac2.iter_mut().enumerate() {
            *h = 60.0 + i as f32;
        }
        // One entry out of order, as AC2's own table has.
        ac2.swap(30, 31);
        let t = lowered(&ac2);
        assert_eq!(t[0], 0.0);
        assert!(t.windows(2).all(|w| w[0] <= w[1]));
        #[allow(clippy::cast_possible_truncation)]
        let top = lower(315.0) as f32;
        assert_eq!(t[LEN - 1], top);
    }

    #[test]
    fn a_height_takes_the_nearest_entry_and_the_lower_on_a_tie() {
        let mut t = [0f32; LEN];
        for (i, h) in t.iter_mut().enumerate() {
            *h = 2.0 * i as f32;
        }
        assert_eq!(quantize(&t, 0.0), 0);
        assert_eq!(quantize(&t, 0.9), 0);
        assert_eq!(quantize(&t, 1.0), 0);
        assert_eq!(quantize(&t, 1.1), 1);
        assert_eq!(quantize(&t, 509.0), 254);
        assert_eq!(quantize(&t, 9999.0), 255);
        assert_eq!(quantize(&t, -3.0), 0);
    }

    #[test]
    fn a_fitted_table_is_valid_bounds_its_error_and_rounds_closer_than_even_steps() {
        // Most heights low, a few high: the fit spends its entries low.
        let mut hs = Vec::new();
        for i in 0..20_000 {
            hs.push((i % 2000) as f32 * 0.025); // 0..50 m, dense
        }
        for i in 0..200 {
            hs.push(50.0 + i as f32 * 2.0); // 50..450 m, sparse
        }
        let (t, r) = fitted(&hs, 1.0);
        assert!(is_valid(&t));
        assert!(r.max <= 1.0);
        let even: [f32; LEN] = std::array::from_fn(|i| i as f32 * 450.0 / 255.0);
        let err = |tab: &[f32; LEN]| -> f64 {
            hs.iter()
                .map(|&h| {
                    (f64::from(tab[quantize(tab, f64::from(h)) as usize]) - f64::from(h)).abs()
                })
                .sum::<f64>()
                / hs.len() as f64
        };
        assert!(err(&t) < err(&even), "{} vs {}", err(&t), err(&even));
        assert!((err(&t) - r.mean).abs() < 0.01);
    }

    #[test]
    fn a_fitted_table_does_not_depend_on_the_heights_order() {
        let hs: Vec<f32> = (0..5000)
            .map(|i| ((i * 7919) % 4000) as f32 * 0.1)
            .collect();
        let mut rev = hs.clone();
        rev.reverse();
        assert_eq!(fitted(&hs, 1.0).0, fitted(&rev, 1.0).0);
    }
}
