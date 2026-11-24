//! AC2's statics that stand against each other (a bridge's spans on its pillar and its walls, a
//! gatehouse's towers and arch, a fort's walls), moved up or down as one.
//!
//! **Why:** each static stands as far above or below its AC2 height as the converted ground
//! differs from AC2's where it is set into the ground ([`crate::ac2_world::buried`]). Two pieces
//! that touch in AC2 but are set into different ground (a pillar in a road, the spans resting on it
//! set into nothing, the walls they reach in a bank) would each move by their own amount, and stand
//! apart: a span's end in a wall's top, a pillar short of the spans it holds up.
//!
//! **Which stand together:** two pieces touch when a vertex of one lies within the other's own box
//! (turned and scaled as it is placed), or a point piece's origin does, give or take [`MARGIN`].
//! Every piece joined to another through a chain of such touches is in its group. Trees and
//! plants at rest join none: their crowns reach over whatever stands beside them. Nor does a piece
//! wider than [`WIDEST`]: the land's own shapes AC2 places as statics (cliffs, ridges, a whole
//! mountainside) hold everything standing on them in their box, and what stands on them is set
//! into its own ground.
//!
//! **How far a group moves:** as one static would, over every buried point of every piece
//! ([`crate::ac2_world::buried::shift`]); with none buried, by the ground under the origins of its
//! pieces on dry squares (the median); with none on dry ground, by the sea's lowering alone, as
//! the converted water's surface is AC2's lowered.

use std::collections::BTreeMap;

use crate::ac2_world::buried::{self, Point};

/// How near two pieces stand to touch, meters.
pub const MARGIN: f64 = 0.25;
/// The widest a piece can be, east to west or south to north, and join others, meters.
pub const WIDEST: f64 = 100.0;
/// The side of the squares pieces are sorted into to find the ones near each other, meters.
const CELL: f64 = 16.0;

/// A placed piece as the grouping sees it: its box in AC2's world, and whether it joins others.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    pub joins: bool,
}

impl Piece {
    fn near(&self, o: &Self) -> bool {
        (0..3).all(|k| self.lo[k] <= o.hi[k] + MARGIN && o.lo[k] <= self.hi[k] + MARGIN)
    }
}

fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

/// Each piece's group: the lowest index among the pieces it stands with (itself when alone).
/// `touches(a, b)` says whether two pieces whose boxes meet touch.
#[must_use]
pub fn groups(pieces: &[Piece], touches: &mut dyn FnMut(usize, usize) -> bool) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..pieces.len()).collect();
    // The joining pieces by the squares their boxes reach.
    #[allow(clippy::cast_possible_truncation)]
    let cell = |v: f64| (v / CELL).floor() as i64;
    let mut squares: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (i, p) in pieces.iter().enumerate().filter(|(_, p)| p.joins) {
        for x in cell(p.lo[0] - MARGIN)..=cell(p.hi[0] + MARGIN) {
            for y in cell(p.lo[1] - MARGIN)..=cell(p.hi[1] + MARGIN) {
                squares.entry((x, y)).or_default().push(i);
            }
        }
    }
    let mut asked = std::collections::BTreeSet::new();
    for list in squares.values() {
        for (k, &a) in list.iter().enumerate() {
            for &b in &list[k + 1..] {
                if !pieces[a].near(&pieces[b]) || !asked.insert((a, b)) {
                    continue;
                }
                let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                if ra != rb && touches(a, b) {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
    }
    (0..pieces.len()).map(|i| root(&mut parent, i)).collect()
}

/// What one piece offers to its group's height: its points, and the converted ground less AC2's
/// at its origin when that lies on a dry square.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Member {
    pub points: Vec<Point>,
    pub origin_dry: Option<f64>,
}

/// How far a group of pieces moves from its AC2 height (AC2 meters to the converted world's);
/// `None` when nothing of it stands on dry ground or is set into any, and it is lowered as the sea
/// is.
#[must_use]
pub fn shift(members: &[&Member]) -> Option<f64> {
    let all: Vec<Point> = members
        .iter()
        .flat_map(|m| m.points.iter().copied())
        .collect();
    if let Some(s) = buried::shift(&all) {
        return Some(s);
    }
    let mut d: Vec<f64> = members.iter().filter_map(|m| m.origin_dry).collect();
    if d.is_empty() {
        return None;
    }
    d.sort_by(f64::total_cmp);
    Some(d[d.len() / 2])
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's statics that stand together)
    use super::*;

    fn piece(lo: [f64; 3], hi: [f64; 3]) -> Piece {
        Piece {
            lo,
            hi,
            joins: true,
        }
    }

    fn point(z: f64, ac2: f64, converted: f64, wet: bool) -> Point {
        Point {
            z,
            ac2,
            converted,
            wet,
        }
    }

    #[test]
    fn pieces_that_touch_through_a_chain_are_one_group_and_the_rest_stand_alone() {
        // Two walls, a span between them over a pillar, and a hut far off.
        let p = [
            piece([0.0, 0.0, 0.0], [4.0, 4.0, 10.0]),
            piece([3.9, 1.0, 9.0], [12.0, 3.0, 11.0]),
            piece([7.0, 1.0, 0.0], [9.0, 3.0, 9.1]),
            piece([11.9, 0.0, 0.0], [16.0, 4.0, 10.0]),
            piece([60.0, 60.0, 0.0], [64.0, 64.0, 5.0]),
        ];
        let g = groups(&p, &mut |_, _| true);
        assert_eq!(g, vec![0, 0, 0, 0, 4]);
    }

    #[test]
    fn boxes_that_meet_without_touching_join_nothing_and_trees_join_nothing() {
        let mut p = [
            piece([0.0, 0.0, 0.0], [4.0, 4.0, 4.0]),
            piece([2.0, 2.0, 0.0], [6.0, 6.0, 4.0]),
            piece([5.0, 5.0, 0.0], [9.0, 9.0, 4.0]),
        ];
        // The first two meet as boxes but their own shapes do not touch.
        let g = groups(&p, &mut |a, b| (a, b) != (0, 1));
        assert_eq!(g, vec![0, 1, 1]);
        // A tree at rest between them joins neither.
        p[1].joins = false;
        let mut asked = Vec::new();
        let g = groups(&p, &mut |a, b| {
            asked.push((a, b));
            true
        });
        assert_eq!(g, vec![0, 1, 2]);
        assert!(asked.is_empty(), "{asked:?}");
    }

    #[test]
    fn pieces_beyond_the_margin_stand_apart_and_within_it_together() {
        let a = piece([0.0, 0.0, 0.0], [4.0, 4.0, 4.0]);
        let near = piece([4.0 + MARGIN * 0.5, 0.0, 0.0], [8.0, 4.0, 4.0]);
        let far = piece([4.0 + MARGIN * 2.0, 0.0, 0.0], [8.0, 4.0, 4.0]);
        assert_eq!(groups(&[a, near], &mut |_, _| true), vec![0, 0]);
        assert_eq!(groups(&[a, far], &mut |_, _| true), vec![0, 1]);
    }

    /// The gatehouse's middle: a pillar set 2 m into a road the converted ground has 1.5 m low,
    /// and a span resting on it, set into nothing, its origin over ground the converted world has
    /// 3 m low.
    #[test]
    fn a_span_resting_on_a_pillar_moves_with_the_pillar() {
        let pillar = Member {
            points: (0..5)
                .map(|_| point(98.0, 100.0, 100.0 - 70.95 - 1.5, false))
                .collect(),
            origin_dry: Some(-70.95 - 1.5),
        };
        let span = Member {
            points: vec![point(115.0, 100.0, 100.0 - 70.95, false)],
            origin_dry: Some(-70.95 - 3.0),
        };
        let s = shift(&[&pillar, &span]).expect("the pillar is set in");
        assert!((s - (-70.95 - 1.5)).abs() < 1e-9, "{s}");
        // Alone, the span would have stood 1.5 m lower than the pillar it rests on.
        assert!((shift(&[&span]).unwrap() - s).abs() > 1.4);
    }

    #[test]
    fn a_group_set_into_nothing_moves_by_its_dry_origins_and_one_over_water_is_lowered() {
        let dry = Member {
            points: Vec::new(),
            origin_dry: Some(-72.0),
        };
        let wet = Member::default();
        assert_eq!(shift(&[&dry, &wet]), Some(-72.0));
        assert_eq!(shift(&[&wet]), None);
    }
}
