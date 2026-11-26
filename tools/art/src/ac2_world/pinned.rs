//! The converted ground pinned to AC2's under the placed statics, so each stands as deep in it
//! as AC2 sets it.
//!
//! The converted land samples AC2's ground at its own vertices, 8 m apart where AC2's are 10 m,
//! and its triangles run between them as AC2's never did: under a static narrower than a square,
//! the converted ground can stand a meter or more off AC2's, though both agree at every vertex.
//! A static set into its ground by the median of its points then sinks too far or stands too high
//! for the parts of it the ground meets.
//!
//! So the corners of the squares under each placed static's footprint are fitted to AC2's ground
//! there: AC2's ground is sampled every meter over the footprint, and the corners' heights are
//! the least-squares fit of the converted triangles to those samples, held to where they were
//! by [`HOLD`] ([`fit`]). Every footprint's samples lie on the same ground, AC2's, so statics
//! that share a corner ask the same of it. A corner of a square with water is never moved, and a
//! corner the fit would move more than [`CAP`] is left as it is.

use std::collections::BTreeMap;

use crate::ac2_world::ground::on_square;

/// Samples of AC2's ground over a footprint, meters apart.
pub const SPACING: f64 = 1.0;
/// How strongly a corner holds to where it was against the samples asking it to move: a hundredth
/// of one sample with its whole weight on the corner, enough to choose among fits the samples
/// alone leave open (one sample, three corners) the one that moves the corners least.
pub const HOLD: f64 = 0.01;
/// The most a corner is moved, meters.
pub const CAP: f64 = 3.0;
/// Rounds of the fit.
const ROUNDS: usize = 100;

/// The points AC2's ground is sampled at under `footprints` (boxes on the ground, world meters:
/// west, south, east, north): every whole multiple of [`SPACING`] inside a box, once however many
/// boxes hold it, and the middle of a box too small to hold one.
#[must_use]
pub fn under(footprints: &[(f64, f64, f64, f64)]) -> Vec<(f64, f64)> {
    let mut at = std::collections::BTreeSet::new();
    // LINT-OK: meters of a world a few tens of kilometers wide.
    #[allow(clippy::cast_possible_truncation)]
    let whole = |v: f64| (v / SPACING) as i64;
    for &(w, s, e, n) in footprints {
        let (x0, x1) = (
            whole((w / SPACING).ceil() * SPACING),
            whole((e / SPACING).floor() * SPACING),
        );
        let (y0, y1) = (
            whole((s / SPACING).ceil() * SPACING),
            whole((n / SPACING).floor() * SPACING),
        );
        if x0 > x1 || y0 > y1 {
            // LINT-OK: as above; the middle, rounded to the nearest sample.
            #[allow(clippy::cast_possible_truncation)]
            at.insert((
                (((w + e) / 2.0) / SPACING).round() as i64,
                (((s + n) / 2.0) / SPACING).round() as i64,
            ));
            continue;
        }
        for x in x0..=x1 {
            for y in y0..=y1 {
                at.insert((x, y));
            }
        }
    }
    // LINT-OK: as above.
    #[allow(clippy::cast_precision_loss)]
    at.into_iter()
        .map(|(x, y)| (x as f64 * SPACING, y as f64 * SPACING))
        .collect()
}

/// A sample of AC2's ground on a converted square: the square (its south-west corner's indices),
/// where on it (east and north, 0 to 1), and the converted world's height of AC2's ground there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub square: (usize, usize),
    pub fu: f64,
    pub fv: f64,
    pub target: f64,
}

/// The heights of a square's corners at a point on it, as a sum: each corner's share, in the
/// order south-west, south-east, north-east, north-west.
fn shares(cut: bool, fu: f64, fv: f64) -> [f64; 4] {
    let mut w = [0.0; 4];
    for (c, s) in w.iter_mut().enumerate() {
        let mut e = [0.0; 4];
        e[c] = 1.0;
        *s = on_square(cut, e, fu, fv);
    }
    w
}

/// A square's corners' vertex indices, in a world `side` vertices a side, in [`shares`]'s order.
fn corners((i, j): (usize, usize), side: usize) -> [usize; 4] {
    [
        i * side + j,
        (i + 1) * side + j,
        (i + 1) * side + j + 1,
        i * side + j + 1,
    ]
}

/// The corners' heights that fit the converted triangles to `samples`, each corner held to its
/// height `at` gives by [`HOLD`]: by vertex index, the corners the samples lie on. `cut` says how
/// each square is cut (x major over `(side - 1)²`, as [`on_square`] takes it).
#[must_use]
pub fn fit(
    side: usize,
    cut: &dyn Fn(usize, usize) -> bool,
    at: &dyn Fn(usize) -> f64,
    samples: &[Sample],
) -> BTreeMap<usize, f64> {
    // Each corner's samples and its share of each.
    let mut on: BTreeMap<usize, Vec<(usize, f64)>> = BTreeMap::new();
    let mut weights = Vec::with_capacity(samples.len());
    for (k, s) in samples.iter().enumerate() {
        let w = shares(cut(s.square.0, s.square.1), s.fu, s.fv);
        let c = corners(s.square, side);
        for n in 0..4 {
            if w[n] > 0.0 {
                on.entry(c[n]).or_default().push((k, w[n]));
            }
        }
        weights.push((c, w));
    }
    let start: BTreeMap<usize, f64> = on.keys().map(|&v| (v, at(v))).collect();
    let mut h = start.clone();
    // The converted ground at each sample as the corners stand now.
    let mut now: Vec<f64> = weights
        .iter()
        .map(|(c, w)| {
            (0..4)
                .filter(|&n| w[n] > 0.0)
                .map(|n| w[n] * h[&c[n]])
                .sum()
        })
        .collect();
    for _ in 0..ROUNDS {
        for (&v, list) in &on {
            let old = h[&v];
            let (mut num, mut den) = (HOLD * start[&v], HOLD);
            for &(k, w) in list {
                num += w * (samples[k].target - (now[k] - w * old));
                den += w * w;
            }
            let new = num / den;
            for &(k, w) in list {
                now[k] += w * (new - old);
            }
            h.insert(v, new);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's statics)

    use super::*;

    /// A square's corner shares at a point add up to one, and at a corner the share is that
    /// corner's alone.
    #[test]
    fn a_points_shares_of_its_squares_corners_add_up_to_one() {
        for cut in [false, true] {
            for (fu, fv) in [(0.2, 0.7), (0.9, 0.1), (0.5, 0.5)] {
                let w = shares(cut, fu, fv);
                assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-12);
            }
            assert_eq!(shares(cut, 0.0, 0.0), [1.0, 0.0, 0.0, 0.0]);
        }
    }

    /// A flat pad sunk into a square whose corners stand higher pulls the corners down to it,
    /// and the converted ground over the pad ends near the pad's height.
    #[test]
    fn the_ground_under_a_footprint_comes_to_ac2s_ground_there() {
        // One square, 3 x 3 vertices, every corner at 10 m; AC2's ground a pad at 9 m over the
        // middle of the south-west square.
        let side = 3;
        let samples: Vec<Sample> = (0..5)
            .flat_map(|a| (0..5).map(move |b| (a, b)))
            .map(|(a, b)| Sample {
                square: (0, 0),
                fu: 0.3 + 0.1 * f64::from(a),
                fv: 0.3 + 0.1 * f64::from(b),
                target: 9.0,
            })
            .collect();
        let h = fit(side, &|_, _| true, &|_| 10.0, &samples);
        assert_eq!(h.len(), 4, "only the square's corners move");
        let worst = samples
            .iter()
            .map(|s| {
                let w = shares(true, s.fu, s.fv);
                let c = corners(s.square, side);
                ((0..4).map(|n| w[n] * h[&c[n]]).sum::<f64>() - s.target).abs()
            })
            .fold(0.0, f64::max);
        assert!(worst < 0.1, "the ground stands {worst:.3} m off the pad");
    }

    /// A footprint is sampled every meter inside it, two footprints' shared points once, and one
    /// smaller than a meter at its middle.
    #[test]
    fn a_footprint_is_sampled_every_meter_and_a_shared_point_once() {
        let at = under(&[
            (0.5, 0.5, 2.5, 1.5),
            (2.0, 1.0, 3.0, 1.0),
            (10.2, 10.2, 10.4, 10.4),
        ]);
        assert_eq!(at, vec![(1.0, 1.0), (2.0, 1.0), (3.0, 1.0), (10.0, 10.0)],);
    }

    /// Samples that already lie on the converted ground move no corner.
    #[test]
    fn ground_that_already_agrees_is_left_as_it_is() {
        let samples = [Sample {
            square: (0, 0),
            fu: 0.25,
            fv: 0.5,
            target: 4.0,
        }];
        let h = fit(2, &|_, _| false, &|_| 4.0, &samples);
        assert!(h.values().all(|&v| (v - 4.0).abs() < 1e-12));
    }
}
