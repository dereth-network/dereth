//! The search over a region's patches: its clear places merged into rectangles a body walks
//! straight across, searched a patch at a time rather than a place at a time. The places of a
//! patch where something stands (a door, a creature, an object), where the route may end, or
//! where a walk failed are searched one by one, as every place once was.
//!
//! A patch is entered at a place on its edge and left from another, walked straight between them.
//! What a patch's way costs is measured from where the way entered it, and each patch it leads to
//! is entered at the place that leaves the least to walk: across the patch, the step over, and the
//! straight distance on to the destination. The chain of places the search answers walks each
//! patch along the straight line between where it was entered and left.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use dereth_primitives::Vec3;

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

/// One way out of a patch to a neighbouring node, the best found so far.
#[derive(Debug, Clone, Copy)]
struct Out {
    to: u32,
    f: f32,
    g: f32,
    entry: u32,
    exit: u32,
}

/// A search over a region's patches, run a slice at a time: the cheapest chain of places from
/// one place to any of a set of ends, where ending at an end costs its value more. A place
/// settled is one unit of work, and a patch settled a unit for each place round its edge, the
/// places the way out of it is looked for from.
///
/// Its nodes are the patches, numbered as the region numbers them, and the places searched one
/// by one, numbered after them.
#[derive(Debug)]
pub(crate) struct MeshSearch {
    book: HashMap<u32, Entry>,
    open: BinaryHeap<Open>,
    best: Option<(f32, u32)>,
    ends: HashMap<u32, f32>,
    toward: Vec3,
    steps: Steps,
    /// The patches searched place by place.
    broken: HashSet<u32>,
    /// The places searched one by one, by place, as nodes.
    loose: HashMap<u32, u32>,
    /// The places searched one by one, by node, from the first after the patches.
    loose_places: Vec<u32>,
    patches: u32,
}

impl MeshSearch {
    /// A search of `region` from place `from` to any of `ends`, which gather round `toward`: an
    /// end's extra cost is never less than its distance from there. The patches holding a gated
    /// place, an end, or a place taken out after a failed walk are searched place by place.
    pub(crate) fn new(
        region: &Region,
        from: u32,
        ends: HashMap<u32, f32>,
        toward: Vec3,
        steps: Steps,
        gates: &Gates,
    ) -> Self {
        let broken: HashSet<u32> = gates
            .broken()
            .chain(ends.keys().filter_map(|&p| region.patch_of(p)))
            .collect();
        let mut search = Self {
            book: HashMap::new(),
            open: BinaryHeap::new(),
            best: None,
            ends,
            toward,
            steps,
            broken,
            loose: HashMap::new(),
            loose_places: Vec::new(),
            patches: u32::try_from(region.patches.len()).unwrap_or(u32::MAX),
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

    /// The node a clear place is searched as: its patch, or itself where its patch is searched
    /// place by place.
    fn node_of(&mut self, region: &Region, place: u32) -> Option<u32> {
        let p = region.patch_of(place)?;
        if !self.broken.contains(&p) {
            return Some(p);
        }
        if let Some(&n) = self.loose.get(&place) {
            return Some(n);
        }
        let n = self
            .patches
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
            if node >= self.patches {
                *budget -= 1;
                let u = self.loose_places[(node - self.patches) as usize];
                if let Some(extra) = self.ends.get(&u) {
                    let total = e.g + extra;
                    if self.best.is_none_or(|(b, _)| total < b) {
                        self.best = Some((total, node));
                    }
                }
                self.ways_from_place(region, &gate, node, u, e.g);
            } else {
                let p = region.patches[node as usize];
                *budget = budget.saturating_sub(2 * (u64::from(p.w) + u64::from(p.h)));
                self.ways_from_patch(region, &gate, node, e);
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

    /// The ways on from a patch entered at `e.entry`: from each place on its edge, a step out to
    /// each neighbouring column outside it (and corner to corner at its corners), the way to
    /// each node it reaches entering at the place that leaves the least to walk.
    fn ways_from_patch(
        &mut self,
        region: &Region,
        gate: &dyn Fn(u32) -> Gate,
        node: u32,
        e: Entry,
    ) {
        let steps = self.steps;
        let patch = region.patches[node as usize];
        let pe = region.node_point(e.entry);
        let wade_here = if patch.wading { region.wade_cost } else { 0.0 };
        let passable = |j: u32| gate(j) != Gate::Blocked;
        let mut outs: Vec<Out> = Vec::new();
        for (ix, iy) in patch.rim() {
            let Some(a) = region.patch_place(node, ix, iy) else {
                continue;
            };
            let pa = region.node_point(a);
            let across = dist(pe, pa) * (1.0 + wade_here);
            for (k, (dx, dy)) in STEPS.iter().enumerate() {
                if patch.holds(ix + dx, iy + dy) {
                    continue;
                }
                if k >= 4 {
                    // Only at a corner: elsewhere a step straight out and along reaches it.
                    if patch.holds(ix + dx, iy) || patch.holds(ix, iy + dy) {
                        continue;
                    }
                }
                let Some(b) = region.step_into(ix + dx, iy + dy, pa.z, steps) else {
                    continue;
                };
                if !passable(b) {
                    continue;
                }
                if k >= 4 {
                    let s1 = region.step_into(ix + dx, iy, pa.z, steps);
                    let s2 = region.step_into(ix, iy + dy, pa.z, steps);
                    if !s1.is_some_and(passable) || !s2.is_some_and(passable) {
                        continue;
                    }
                }
                let pb = region.node_point(b);
                let step = dist(pa, pb);
                let mut g = e.g + across + step;
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
        }
        for out in outs {
            self.relax(out, node);
        }
    }

    /// The chain of places from the start to node `end`: each node's places from where the way
    /// entered it to where it left, straight across a patch.
    fn chain(&self, region: &Region, end: u32) -> Vec<u32> {
        let mut back = Vec::new();
        let mut at = end;
        if let Some(e) = self.book.get(&end) {
            back.push(e.entry);
        }
        while let Some(e) = self.book.get(&at).filter(|e| e.came != NONE) {
            let m = e.came;
            let entry = self.book.get(&m).map_or(e.exit, |x| x.entry);
            if m >= self.patches {
                back.push(e.exit);
            } else {
                let (x0, y0, _) = region.column_and_z(e.exit);
                let (x1, y1, _) = region.column_and_z(entry);
                for (ix, iy) in line(x0, y0, x1, y1) {
                    if let Some(p) = region.patch_place(m, ix, iy) {
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
    //! Behaviour: none (unit checks of the line across a patch)
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
