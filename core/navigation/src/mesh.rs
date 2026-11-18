//! The search over a region's navigation mesh: its convex polygons, searched a polygon at a time
//! rather than a place at a time. The places of a polygon where something stands (a door, a
//! creature, an object), where the route may end, or where a walk failed are searched one by
//! one, as are the places no polygon holds.
//!
//! A polygon is entered at a place beside the edge it is crossed by and left from another, walked
//! straight between them. What a polygon's way costs is measured from where the way entered it.
//! Each polygon it links to is entered where the edge between them is crossed nearest the
//! straight line on toward the destination; each place searched by itself is stepped to from the
//! polygon's place beside it. The chain of places the search answers walks each polygon along
//! the straight line between where it was entered and left.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use dereth_primitives::Vec3;

use crate::poly::corner_point;
use crate::region::Region;
use crate::search::{Gate, Gates, Steps, STEPS};
use crate::vmath::dist;

/// No node: where a way starts.
const NONE: u32 = u32::MAX;

/// A node's bookkeeping.
#[derive(Debug, Clone, Copy)]
struct Entry {
    /// The cost of the way to `entry`.
    g: f32,
    /// `g` and the straight distance on from `entry` to where the search is going.
    f: f32,
    /// The place the way enters the node at.
    entry: u32,
    /// The node the way came from, and the place it left that node at.
    came: u32,
    exit: u32,
    closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Open {
    f: f32,
    node: u32,
}

impl Eq for Open {}

impl Ord for Open {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.total_cmp(&self.f).then_with(|| o.node.cmp(&self.node))
    }
}

impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// One way out of a node to a neighboring node, the best found so far.
#[derive(Debug, Clone, Copy)]
struct Out {
    to: u32,
    f: f32,
    g: f32,
    entry: u32,
    exit: u32,
}

/// A search over a region's polygons, run a slice at a time: the cheapest chain of places from
/// one place to any of a set of ends, where ending at an end costs its value more. A place
/// settled is one unit of work, and a polygon settled a unit for each way out of it.
///
/// Its nodes are the polygons, numbered as the region numbers them, and the places searched one
/// by one, numbered after them.
#[derive(Debug)]
pub(crate) struct MeshSearch {
    book: HashMap<u32, Entry>,
    open: BinaryHeap<Open>,
    best: Option<(f32, u32)>,
    ends: HashMap<u32, f32>,
    toward: Vec3,
    steps: Steps,
    /// The polygons searched place by place.
    broken: HashSet<u32>,
    /// The steps out of whole polygons into the places of broken ones: from the polygon's place
    /// to the broken one's.
    into_broken: HashMap<u32, Vec<(u32, u32)>>,
    /// The places searched one by one, by place, as nodes.
    loose: HashMap<u32, u32>,
    /// The places searched one by one, by node, from the first after the polygons.
    loose_places: Vec<u32>,
    polys: u32,
}

impl MeshSearch {
    /// A search of `region` from place `from` to any of `ends`, which gather round `toward`: an
    /// end's extra cost is never less than its distance from there. The polygons over a gated
    /// place, an end, or a place taken out after a failed walk are searched place by place.
    pub(crate) fn new(
        region: &Region,
        from: u32,
        ends: HashMap<u32, f32>,
        toward: Vec3,
        steps: Steps,
        gates: &Gates,
    ) -> Self {
        let mut broken: HashSet<u32> = gates.broken().collect();
        for &e in ends.keys() {
            let (ix, iy, _) = region.column_and_z(e);
            broken.extend(region.polys_over(ix, iy));
            if let Some(p) = region.patch_of(e) {
                broken.insert(p);
            }
        }
        // The steps from whole polygons into the broken ones' places.
        let mut into_broken: HashMap<u32, Vec<(u32, u32)>> = HashMap::new();
        // In order, so the same search finds the same way every time.
        let mut order: Vec<u32> = broken.iter().copied().collect();
        order.sort_unstable();
        for &b in &order {
            let Some(poly) = region.polys.get(b as usize) else {
                continue;
            };
            let (x0, y0, x1, y1) = poly.bounds();
            for iy in y0..=y1 {
                for ix in x0..=x1 {
                    let Some(q) = region.patch_place(b, ix, iy) else {
                        continue;
                    };
                    let zq = region.column_and_z(q).2;
                    for (dx, dy) in STEPS {
                        let Some(a) = region.step_into(ix + dx, iy + dy, zq, steps) else {
                            continue;
                        };
                        let Some(p) = region.patch_of(a) else {
                            continue;
                        };
                        if broken.contains(&p) {
                            continue;
                        }
                        into_broken.entry(p).or_default().push((a, q));
                    }
                }
            }
        }
        let mut search = Self {
            book: HashMap::new(),
            open: BinaryHeap::new(),
            best: None,
            ends,
            toward,
            steps,
            broken,
            into_broken,
            loose: HashMap::new(),
            loose_places: Vec::new(),
            polys: u32::try_from(region.polys.len()).unwrap_or(u32::MAX),
        };
        if let Some(start) = search.node_of(region, from) {
            let f = dist(region.node_point(from), toward);
            search.book.insert(
                start,
                Entry {
                    g: 0.0,
                    f,
                    entry: from,
                    came: NONE,
                    exit: NONE,
                    closed: false,
                },
            );
            search.open.push(Open { f, node: start });
        }
        search
    }

    /// The node a clear place is searched as: its polygon, or itself where no polygon holds it or
    /// its polygon is searched place by place.
    fn node_of(&mut self, region: &Region, place: u32) -> Option<u32> {
        if !region.is_clear(place) {
            return None;
        }
        if let Some(p) = region.patch_of(place).filter(|p| !self.broken.contains(p)) {
            return Some(p);
        }
        if let Some(&n) = self.loose.get(&place) {
            return Some(n);
        }
        let n = self
            .polys
            .checked_add(u32::try_from(self.loose_places.len()).ok()?)?;
        self.loose.insert(place, n);
        self.loose_places.push(place);
        Some(n)
    }

    /// A way to node `to`, entering it at `entry`, costing `g` and `f`, from node `came` left at
    /// `exit`: kept when it is the best yet.
    fn relax(&mut self, out: Out, came: u32) {
        if self
            .book
            .get(&out.to)
            .is_some_and(|old| old.closed || old.f <= out.f)
        {
            return;
        }
        self.book.insert(
            out.to,
            Entry {
                g: out.g,
                f: out.f,
                entry: out.entry,
                came,
                exit: out.exit,
                closed: false,
            },
        );
        self.open.push(Open {
            f: out.f,
            node: out.to,
        });
    }

    /// Settle up to `budget` nodes of `region`; once the search is over, the chain of places it
    /// found, or `None` when no end is reached.
    pub(crate) fn step(
        &mut self,
        region: &Region,
        gates: &Gates,
        budget: &mut u64,
    ) -> Option<Option<Vec<u32>>> {
        let gate = |p: u32| gates.of(p);
        loop {
            if *budget == 0 {
                return None;
            }
            let Some(Open { f, node }) = self.open.pop() else {
                break;
            };
            // Nothing still open can end more cheaply than the best end found.
            if self.best.is_some_and(|(b, _)| f >= b) {
                break;
            }
            let Some(e) = self.book.get_mut(&node) else {
                continue;
            };
            if e.closed {
                continue;
            }
            e.closed = true;
            let e = *e;
            if node >= self.polys {
                *budget -= 1;
                let u = self.loose_places[(node - self.polys) as usize];
                if let Some(extra) = self.ends.get(&u) {
                    let total = e.g + extra;
                    if self.best.is_none_or(|(b, _)| total < b) {
                        self.best = Some((total, node));
                    }
                }
                self.ways_from_place(region, &gate, node, u, e.g);
            } else {
                let p = &region.polys[node as usize];
                let n = p.links.len()
                    + region.exits.get(&node).map_or(0, Vec::len)
                    + self.into_broken.get(&node).map_or(0, Vec::len);
                *budget = budget.saturating_sub(u64::try_from(n.max(1)).unwrap_or(u64::MAX));
                self.ways_from_poly(region, &gate, node, e);
            }
        }
        self.open.clear();
        let Some((_, end)) = self.best else {
            return Some(None);
        };
        Some(Some(self.chain(region, end)))
    }

    /// The ways on from a place searched by itself: a step to each neighbouring column, as the
    /// search place by place steps.
    fn ways_from_place(
        &mut self,
        region: &Region,
        gate: &dyn Fn(u32) -> Gate,
        node: u32,
        u: u32,
        g: f32,
    ) {
        let steps = self.steps;
        let (hx, hy, hz) = region.column_and_z(u);
        let p_here = region.node_point(u);
        let passable = |j: u32| gate(j) != Gate::Blocked;
        for (k, (dx, dy)) in STEPS.iter().enumerate() {
            let Some(next) = region.step_into(hx + dx, hy + dy, hz, steps) else {
                continue;
            };
            if !passable(next) {
                continue;
            }
            if k >= 4 {
                // No cutting a corner: both orthogonal columns have to be passable too.
                let a = region.step_into(hx + dx, hy, hz, steps);
                let b = region.step_into(hx, hy + dy, hz, steps);
                if !a.is_some_and(passable) || !b.is_some_and(passable) {
                    continue;
                }
            }
            let p_next = region.node_point(next);
            let step = dist(p_here, p_next);
            let mut cost = g + step;
            if region.is_wading(next) {
                cost += step * region.wade_cost;
            }
            match gate(next) {
                Gate::Door(d, c) if gate(u) != Gate::Door(d, c) => cost += c,
                Gate::Penalty(c) if !matches!(gate(u), Gate::Penalty(_)) => cost += c,
                _ => {}
            }
            let Some(to) = self.node_of(region, next) else {
                continue;
            };
            let f = cost + dist(p_next, self.toward);
            self.relax(
                Out {
                    to,
                    f,
                    g: cost,
                    entry: next,
                    exit: u,
                },
                node,
            );
        }
    }

    /// The ways on from a polygon entered at `e.entry`: into each polygon it links to, across the
    /// edge between them where it is nearest the straight line on, and a step to each place
    /// searched by itself beside it.
    fn ways_from_poly(&mut self, region: &Region, gate: &dyn Fn(u32) -> Gate, node: u32, e: Entry) {
        let poly = &region.polys[node as usize];
        let pe = region.node_point(e.entry);
        let wade_here = if poly.wading { region.wade_cost } else { 0.0 };
        let s = region.spacing;
        let mut outs: Vec<Out> = Vec::new();
        for link in &*poly.links {
            if self.broken.contains(&link.to) {
                continue;
            }
            let (a, b) = (
                (f64::from(link.a.0), f64::from(link.a.1)),
                (f64::from(link.b.0), f64::from(link.b.1)),
            );
            let len = dereth_primitives::num::math::hypot(b.0 - a.0, b.1 - a.1);
            if len <= 0.0 {
                continue;
            }
            // Where the edge is crossed: nearest the way on, kept two and a half columns in from
            // its ends where it is long enough, so a way does not hug the corner a wall makes.
            let at = |t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            let cost_at = |t: f64| {
                let x = corner_point(s, at(t), pe.z);
                dist(pe, x) + dist(x, self.toward)
            };
            let margin = (2.5 / len).min(0.5);
            let (mut lo, mut hi) = (margin, 1.0 - margin);
            for _ in 0..24 {
                let (m1, m2) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
                if cost_at(m1) <= cost_at(m2) {
                    hi = m2;
                } else {
                    lo = m1;
                }
            }
            let (cx, cy) = at((lo + hi) / 2.0);
            // The places either side of that point: in the column a step inside each polygon,
            // toward its middle.
            let (Some(xa), Some(xb)) = (
                region.poly_place_inside(node, (cx, cy)),
                region.poly_place_inside(link.to, (cx, cy)),
            ) else {
                continue;
            };
            if gate(xb) == Gate::Blocked {
                continue;
            }
            let (pa, pb) = (region.node_point(xa), region.node_point(xb));
            let mut g = e.g + dist(pe, pa) * (1.0 + wade_here) + dist(pa, pb);
            if region.polys[link.to as usize].wading {
                g += dist(pa, pb) * region.wade_cost;
            }
            let f = g + dist(pb, self.toward);
            let out = Out {
                to: link.to,
                f,
                g,
                entry: xb,
                exit: xa,
            };
            match outs.iter_mut().find(|o| o.to == link.to) {
                Some(o) if o.f <= f => {}
                Some(o) => *o = out,
                None => outs.push(out),
            }
        }
        let steps_out = region
            .exits
            .get(&node)
            .into_iter()
            .flatten()
            .chain(self.into_broken.get(&node).into_iter().flatten())
            .copied()
            .collect::<Vec<_>>();
        for (a, b) in steps_out {
            if gate(b) == Gate::Blocked {
                continue;
            }
            let (pa, pb) = (region.node_point(a), region.node_point(b));
            let step = dist(pa, pb);
            let mut g = e.g + dist(pe, pa) * (1.0 + wade_here) + step;
            if region.is_wading(b) {
                g += step * region.wade_cost;
            }
            match gate(b) {
                Gate::Door(_, c) | Gate::Penalty(c) => g += c,
                _ => {}
            }
            let Some(to) = self.node_of(region, b) else {
                continue;
            };
            if to == node {
                continue;
            }
            let f = g + dist(pb, self.toward);
            let out = Out {
                to,
                f,
                g,
                entry: b,
                exit: a,
            };
            match outs.iter_mut().find(|o| o.to == to) {
                Some(o) if o.f <= f => {}
                Some(o) => *o = out,
                None => outs.push(out),
            }
        }
        for out in outs {
            self.relax(out, node);
        }
    }

    /// The chain of places from the start to node `end`: each node's places from where the way
    /// entered it to where it left, straight across a polygon.
    fn chain(&self, region: &Region, end: u32) -> Vec<u32> {
        let mut back = Vec::new();
        let mut at = end;
        if let Some(e) = self.book.get(&end) {
            back.push(e.entry);
        }
        while let Some(e) = self.book.get(&at).filter(|e| e.came != NONE) {
            let m = e.came;
            let entry = self.book.get(&m).map_or(e.exit, |x| x.entry);
            if m >= self.polys {
                back.push(e.exit);
            } else {
                let (x0, y0, z0) = region.column_and_z(e.exit);
                let (x1, y1, _) = region.column_and_z(entry);
                let mut z = z0;
                for (ix, iy) in line(x0, y0, x1, y1) {
                    let place = region
                        .patch_place(m, ix, iy)
                        .or_else(|| region.step_into(ix, iy, z, self.steps));
                    if let Some(p) = place {
                        z = region.column_and_z(p).2;
                        back.push(p);
                    }
                }
            }
            at = m;
        }
        back.reverse();
        back.dedup();
        back
    }
}

/// The columns of a straight line from `(x0, y0)` to `(x1, y1)`, both included, each a step from
/// the last, straight or corner to corner.
fn line(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i32, i32)> {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = ((x1 - x0).signum(), (y1 - y0).signum());
    let mut err = dx + dy;
    let (mut x, mut y) = (x0, y0);
    let mut out = Vec::with_capacity(usize::try_from(dx.max(-dy)).unwrap_or(0) + 1);
    loop {
        out.push((x, y));
        if x == x1 && y == y1 {
            return out;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (unit checks of the line across a polygon)
    use super::line;

    #[test]
    fn a_line_across_columns_steps_one_column_at_a_time_from_end_to_end() {
        for (a, b) in [
            ((0, 0), (5, 2)),
            ((3, 7), (-2, 1)),
            ((4, 4), (4, 4)),
            ((0, 0), (0, -6)),
        ] {
            let l = line(a.0, a.1, b.0, b.1);
            assert_eq!(l.first(), Some(&a));
            assert_eq!(l.last(), Some(&b));
            for w in l.windows(2) {
                let (dx, dy) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
                assert!(
                    dx.abs() <= 1 && dy.abs() <= 1 && (dx, dy) != (0, 0),
                    "{l:?}"
                );
            }
        }
    }
}
