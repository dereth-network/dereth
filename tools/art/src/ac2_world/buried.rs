//! AC2's statics that AC2 sets into its ground (a bridge's ends in its banks, a wall's foot in a
//! slope, a rock half sunk), kept set into the converted ground.
//!
//! **How high a static stands:** as far above or below its AC2 height as the converted ground
//! stands above or below AC2's where the static is set into it: the median, over its points under
//! AC2's ground on converted squares with no water corner, of the converted ground less AC2's. A
//! static with no such point on dry squares takes the median over all its points under the
//! ground; one with none at all is placed by its origin, as before. Taking it at the origin alone
//! raised a bridge by its river's depth, since the converted ground over water is the water's
//! surface, not AC2's bed. Statics that stand against each other take this over all their points
//! together ([`crate::ac2_world::joined`]).
//!
//! **Covering what still shows:** the converted ground on 8 m squares sags where AC2's 10 m
//! squares had a crest or a bank, so a point AC2 buries can still stand above it. Each corner of a
//! dry square under such a point is raised to the first height of the table at least as high as
//! the corner plus what shows, which covers the point on either of the square's triangles. Less
//! than [`TOLERANCE`] is left as it is, and more than [`CAP`] is left and listed: raising the land
//! that far would make a spike. Water corners are never raised. Nor is a corner whose next height
//! up the table overshoots by more than what shows: where the table's heights stand far apart,
//! covering a sliver of a static would sink it deeper than the sliver ever showed.

use std::collections::BTreeMap;

use crate::ac2_world::table;

/// A point is under AC2's ground when it lies this far below it, meters.
pub const BURIED: f64 = 0.05;
/// What shows of a buried point is covered only past this much, meters.
pub const TOLERANCE: f64 = 0.25;
/// The most the land is raised to cover a point, meters.
pub const CAP: f64 = 3.0;

/// One point of a placed static.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    /// Its height in AC2 meters.
    pub z: f64,
    /// AC2's ground under it, AC2 meters.
    pub ac2: f64,
    /// The converted ground under it, the converted world's meters (rounded heights).
    pub converted: f64,
    /// Whether the converted square under it has a water corner.
    pub wet: bool,
}

impl Point {
    /// Whether AC2 sets it into its ground.
    #[must_use]
    pub fn buried(&self) -> bool {
        self.ac2 - self.z > BURIED
    }
}

/// How far a static moves from its AC2 height (AC2 meters to the converted world's), taken from
/// its points under AC2's ground; `None` when it has none.
#[must_use]
pub fn shift(points: &[Point]) -> Option<f64> {
    let buried: Vec<&Point> = points.iter().filter(|p| p.buried()).collect();
    let dry: Vec<f64> = buried
        .iter()
        .filter(|p| !p.wet)
        .map(|p| p.converted - p.ac2)
        .collect();
    let mut d = if dry.is_empty() {
        buried.iter().map(|p| p.converted - p.ac2).collect()
    } else {
        dry
    };
    if d.is_empty() {
        return None;
    }
    d.sort_by(f64::total_cmp);
    Some(d[d.len() / 2])
}

/// How far `p` stands above the converted ground once moved by `shift` (negative: under it).
#[must_use]
pub fn shows(p: &Point, shift: f64) -> f64 {
    p.z + shift - p.converted
}

/// What covering a static's buried points asks of the land.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cover {
    /// Points to cover on dry squares: the square (x, y index of its south-west corner) and how
    /// much of the point shows.
    pub dry: Vec<((usize, usize), f64)>,
    /// The most that shows of a point on a square with a water corner (left as it is).
    pub wet: f64,
    /// The most that shows of a dry point past [`CAP`] (left as it is).
    pub past_cap: f64,
}

/// What covering asks for a static moved by `shift`, of its points with the converted squares
/// they lie on.
#[must_use]
pub fn cover(points: &[(Point, (usize, usize))], shift: f64) -> Cover {
    let mut c = Cover::default();
    for (p, square) in points {
        if !p.buried() {
            continue;
        }
        let s = shows(p, shift);
        if s <= TOLERANCE {
            continue;
        }
        if p.wet {
            c.wet = c.wet.max(s);
        } else if s > CAP {
            c.past_cap = c.past_cap.max(s);
        } else {
            c.dry.push((*square, s));
        }
    }
    c
}

/// The heights each raised vertex needs, by its index in a world `side` vertices a side whose
/// vertices stand at `meters`, rounded into `table`: each corner of a covered square at least its
/// height plus what shows, the greatest of every static's asks; but for a corner the table's next
/// height up would overshoot by more than what shows.
#[must_use]
pub fn wants(
    covers: &[Cover],
    side: usize,
    meters: &dyn Fn(usize) -> f64,
    table: &[f32; table::LEN],
) -> BTreeMap<usize, f64> {
    let mut out: BTreeMap<usize, f64> = BTreeMap::new();
    for c in covers {
        for &((i, j), s) in &c.dry {
            for (a, b) in [(i, j), (i + 1, j), (i, j + 1), (i + 1, j + 1)] {
                let k = a * side + b;
                let want = meters(k) + s;
                if f64::from(table[usize::from(at_least(table, want))]) - want > s {
                    continue;
                }
                let e = out.entry(k).or_insert(want);
                *e = e.max(want);
            }
        }
    }
    out
}

/// The index of the first height of `table` at least `h`; the last when none is.
#[must_use]
pub fn at_least(table: &[f32; table::LEN], h: f64) -> u8 {
    let i = table
        .partition_point(|&t| f64::from(t) < h)
        .min(table::LEN - 1);
    // LINT-OK: an index into a 256-entry table.
    #[allow(clippy::cast_possible_truncation)]
    let i = i as u8;
    i
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's buried statics)
    use super::*;

    const SEA: f64 = table::SEA as f64;

    fn point(z: f64, ac2: f64, converted: f64, wet: bool) -> Point {
        Point {
            z,
            ac2,
            converted,
            wet,
        }
    }

    /// The bridge by Cragstone, as measured: its ends sunk 11.45 m into banks at 77.75 m (AC2),
    /// which the converted ground has at 3.75 to 5.18 m; its middle over a river whose bed is at
    /// 63.24 m under water the converted ground has at its surface, 0 m.
    fn bridge() -> Vec<Point> {
        let mut v = Vec::new();
        for converted in [3.75, 4.17, 4.6, 5.0, 5.18] {
            v.push(point(66.3, 77.75, converted, false));
        }
        // An end's corner on a square with a water corner.
        v.push(point(66.3, 77.75, 2.0, true));
        // The deck over the river, not under AC2's ground.
        v.push(point(74.56, 63.24, 0.0, true));
        v
    }

    #[test]
    fn a_bridge_over_water_stands_by_its_banks_not_by_its_river() {
        let at_origin = 0.0 - 63.24;
        let s = shift(&bridge()).expect("its ends are buried");
        // The banks' median: 4.6 m converted against 77.75 m.
        assert!((s - (4.6 - 77.75)).abs() < 1e-9, "{s}");
        // Taken at the origin it stood 9.5 m higher; the ends' lowest point now sits under the
        // banks it is set into.
        assert!(at_origin - s > 9.0);
        let lowest = 66.3 + s;
        assert!(lowest < 3.75, "{lowest}");
        // By the origin, the ends stood above the converted banks.
        assert!(66.3 + at_origin > 3.06 - 0.01);
    }

    #[test]
    fn a_static_with_no_buried_point_on_dry_ground_uses_its_wet_ones() {
        let pts = [point(10.0, 12.0, 1.0, true), point(10.0, 12.0, 2.0, true)];
        assert_eq!(shift(&pts), Some(2.0 - 12.0));
        // And one with none at all is left to its origin.
        assert_eq!(shift(&[point(20.0, 12.0, 1.0, false)]), None);
    }

    #[test]
    fn covering_asks_only_for_what_shows_on_dry_squares_within_the_cap() {
        let s = -70.0;
        let pts = [
            // Shows 0.1 m: within the tolerance.
            (point(80.0, 81.0, 9.9, false), (1, 1)),
            // Shows 1 m on a dry square: covered.
            (point(80.0, 81.0, 9.0, false), (2, 1)),
            // Shows 2 m on a wet square: left, and noted.
            (point(80.0, 81.0, 8.0, true), (3, 1)),
            // Shows 5 m: past the cap, left, and noted.
            (point(80.0, 81.0, 5.0, false), (4, 1)),
            // Not under AC2's ground: never covered.
            (point(90.0, 81.0, 0.0, false), (5, 1)),
        ];
        let c = cover(&pts, s);
        assert_eq!(c.dry.len(), 1);
        assert_eq!(c.dry[0].0, (2, 1));
        assert!((c.dry[0].1 - 1.0).abs() < 1e-9);
        assert!((c.wet - 2.0).abs() < 1e-9);
        assert!((c.past_cap - 5.0).abs() < 1e-9);
    }

    #[test]
    fn a_covered_squares_corners_rise_to_a_table_height_that_covers_the_point() {
        let mut t = [0f32; table::LEN];
        for (k, v) in t.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let h = k as f32 * 0.7;
            *v = h;
        }
        // A 3 x 3 world, every vertex at 7 m (index 10).
        let height = [10u8; 9];
        let c = Cover {
            dry: vec![((0, 0), 1.0)],
            ..Cover::default()
        };
        let w = wants(&[c], 3, &|k| f64::from(t[usize::from(height[k])]), &t);
        assert_eq!(w.len(), 4);
        for (&k, &h) in &w {
            assert!([0, 1, 3, 4].contains(&k), "{k}");
            assert!((h - 8.0).abs() < 1e-6);
            // 8 m lies between 7.7 (11) and 8.4 (12): the first at least 8 m is 12.
            assert_eq!(at_least(&t, h), 12);
        }
        // A sliver that shows less than the next height up overshoots is left showing.
        let sliver = Cover {
            dry: vec![((0, 0), 0.3)],
            ..Cover::default()
        };
        assert!(wants(&[sliver], 3, &|k| f64::from(t[usize::from(height[k])]), &t).is_empty());
        // Nothing asked, nothing raised.
        assert!(wants(
            &[Cover::default()],
            3,
            &|k| f64::from(t[usize::from(height[k])]),
            &t
        )
        .is_empty());
        // Past the table's top, its last height.
        assert_eq!(at_least(&t, 1.0e6), 255);
    }

    #[test]
    fn the_sea_lowering_is_the_shift_of_a_static_on_unchanged_ground() {
        // Ground the conversion only lowers: the shift is the lowering itself.
        let pts = [point(99.0, 100.0, 100.0 - SEA, false)];
        assert!((shift(&pts).unwrap() + SEA).abs() < 1e-9);
    }
}
