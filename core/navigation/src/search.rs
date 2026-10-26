//! The search over a region's standing places, and the pull that straightens its result.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use dereth_primitives::num::floor_to_i32;
use dereth_primitives::Vec3;

use crate::region::Region;
use crate::vmath::{dist, dist_xy};

/// What stands on a place besides the cells and their statics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Gate {
    /// Nothing.
    Free,
    /// A door the route has to open: its index among the region's doors, and the extra cost.
    Door(u16, f32),
    /// Something the route would rather go round, at this extra cost: a creature standing there.
    Penalty(f32),
    /// Something the route cannot pass.
    Blocked,
}

/// What stands on a region's places for one search: each place's gate, and the patches holding a
/// gated place or a place taken out, which a straight line crosses place by place.
#[derive(Debug, Default)]
pub(crate) struct Gates {
    map: HashMap<u32, Gate>,
    patches: HashSet<u32>,
}

impl Gates {
    /// The gates `map` names, over `region`.
    pub(crate) fn new(region: &Region, map: HashMap<u32, Gate>) -> Self {
        let patches = map
            .keys()
            .filter_map(|&p| region.patch_of(p))
            .chain(region.dirty.iter().copied())
            .collect();
        Self { map, patches }
    }

    /// The gate on place `p`.
    pub(crate) fn of(&self, p: u32) -> Gate {
        self.map.get(&p).copied().unwrap_or(Gate::Free)
    }

    /// Whether every place of patch `p` is free and clear: a straight line crosses it whole.
    pub(crate) fn whole(&self, p: u32) -> bool {
        !self.patches.contains(&p)
    }

    /// The patches that are not whole.
    pub(crate) fn broken(&self) -> impl Iterator<Item = u32> + '_ {
        self.patches.iter().copied()
    }
}

/// The eight neighbouring columns, orthogonal first.
pub(crate) const STEPS: [(i32, i32); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (1, 1),
    (1, -1),
    (-1, 1),
    (-1, -1),
];

/// How many failed lines past the last good one the pull tries before it settles.
const PULL_MISSES: u32 = 12;

/// The longest straight leg the pull makes, in metres.
const PULL_REACH: f32 = 40.0;

/// The lengths, longest first, a corner of a route is tried cut across by.
const CUTS: [f32; 4] = [3.0, 2.0, 1.25, 0.75];

/// Past this many places a search keeps its costs in a map rather than in arrays over every place.
const DENSE_LIMIT: usize = 2_000_000;

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

/// The search's limits on stepping between places.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Steps {
    pub max_rise: f32,
    pub max_drop: f32,
}

/// A search's bookkeeping per place: the best cost found, where it came from, and whether it is
/// settled. Arrays over every place for a dungeon; a map for a window of terrain, of which a search
/// touches a small part.
#[derive(Debug)]
enum Book {
    Dense {
        cost: Vec<f32>,
        came: Vec<u32>,
        closed: Vec<bool>,
    },
    Sparse(HashMap<u32, (f32, u32, bool)>),
}

impl Book {
    fn new(region: &Region) -> Self {
        let n = region.place_count();
        if region.ground.is_none() && n <= DENSE_LIMIT {
            Self::Dense {
                cost: vec![f32::INFINITY; n],
                came: vec![u32::MAX; n],
                closed: vec![false; n],
            }
        } else {
            Self::Sparse(HashMap::new())
        }
    }

    fn cost(&self, i: u32) -> f32 {
        match self {
            Self::Dense { cost, .. } => cost[i as usize],
            Self::Sparse(m) => m.get(&i).map_or(f32::INFINITY, |e| e.0),
        }
    }

    fn came(&self, i: u32) -> u32 {
        match self {
            Self::Dense { came, .. } => came[i as usize],
            Self::Sparse(m) => m.get(&i).map_or(u32::MAX, |e| e.1),
        }
    }

    fn set(&mut self, i: u32, c: f32, from: u32) {
        match self {
            Self::Dense { cost, came, .. } => {
                cost[i as usize] = c;
                came[i as usize] = from;
            }
            Self::Sparse(m) => {
                let e = m.entry(i).or_insert((f32::INFINITY, u32::MAX, false));
                e.0 = c;
                e.1 = from;
            }
        }
    }

    /// Settle `i`; false when it already was.
    fn close(&mut self, i: u32) -> bool {
        match self {
            Self::Dense { closed, .. } => !std::mem::replace(&mut closed[i as usize], true),
            Self::Sparse(m) => {
                let e = m.entry(i).or_insert((f32::INFINITY, u32::MAX, false));
                !std::mem::replace(&mut e.2, true)
            }
        }
    }
}

/// A chain of places being straightened a slice at a time: the places kept so far, and the one
/// the next line is tried from.
#[derive(Debug, Default)]
pub(crate) struct Pull {
    pub kept: Vec<u32>,
    i: usize,
}

/// A search over a region's places, run a slice at a time: the cheapest chain of places from
/// one place to any of a set of ends, where ending at an end costs its value more. Each place
/// settled is one unit of work.
#[derive(Debug)]
pub(crate) struct Search {
    book: Book,
    open: BinaryHeap<Open>,
    best: Option<(f32, u32)>,
    ends: HashMap<u32, f32>,
    toward: Vec3,
    steps: Steps,
    /// One-way drops off ledges the search may take: from a place to the places it lands on.
    drops: HashMap<u32, Vec<u32>>,
}

impl Search {
    /// A search of `region` from place `from` to any of `ends`, which gather round `toward`: an
    /// end's extra cost is never less than its distance from there, so the straight distance to
    /// `toward` never overestimates what is left of a chain.
    pub(crate) fn new(
        region: &Region,
        from: u32,
        ends: HashMap<u32, f32>,
        toward: Vec3,
        steps: Steps,
    ) -> Self {
        let mut book = Book::new(region);
        book.set(from, 0.0, u32::MAX);
        let open = BinaryHeap::from([Open {
            f: dist(region.node_point(from), toward),
            node: from,
        }]);
        Self {
            book,
            open,
            best: None,
            ends,
            toward,
            steps,
            drops: HashMap::new(),
        }
    }

    /// The same search, allowed these drops off ledges besides walking.
    pub(crate) fn with_drops(mut self, drops: HashMap<u32, Vec<u32>>) -> Self {
        self.drops = drops;
        self
    }

    /// Settle up to `budget` places of `region`; once the search is over, the chain it found, or
    /// `None` when no end is reached.
    pub(crate) fn step(
        &mut self,
        region: &Region,
        gates: &Gates,
        budget: &mut u64,
    ) -> Option<Option<Vec<u32>>> {
        let gate = |p: u32| gates.of(p);
        let steps = self.steps;
        let toward = self.toward;
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
            if !self.book.close(node) {
                continue;
            }
            *budget -= 1;
            if let Some(extra) = self.ends.get(&node) {
                let total = self.book.cost(node) + extra;
                if self.best.is_none_or(|(b, _)| total < b) {
                    self.best = Some((total, node));
                }
            }
            let (hx, hy, hz) = region.column_and_z(node);
            let g_here = self.book.cost(node);
            let p_here = region.node_point(node);
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
                let mut g = g_here + step;
                if region.is_wading(next) {
                    g += step * region.wade_cost;
                }
                match gate(next) {
                    Gate::Door(d, c) if gate(node) != Gate::Door(d, c) => g += c,
                    Gate::Penalty(c) if !matches!(gate(node), Gate::Penalty(_)) => g += c,
                    _ => {}
                }
                if g < self.book.cost(next) {
                    self.book.set(next, g, node);
                    self.open.push(Open {
                        f: g + dist(p_next, toward),
                        node: next,
                    });
                }
            }
            for &next in self.drops.get(&node).map_or(&[][..], Vec::as_slice) {
                if gate(next) == Gate::Blocked {
                    continue;
                }
                let p_next = region.node_point(next);
                let g = g_here + dist(p_here, p_next);
                if g < self.book.cost(next) {
                    self.book.set(next, g, node);
                    self.open.push(Open {
                        f: g + dist(p_next, toward),
                        node: next,
                    });
                }
            }
        }
        // Over: a chain back from the best end, if any.
        self.open.clear();
        let Some((_, end)) = self.best else {
            return Some(None);
        };
        let mut chain = vec![end];
        let mut at = end;
        while self.book.came(at) != u32::MAX {
            at = self.book.came(at);
            chain.push(at);
        }
        chain.reverse();
        Some(Some(chain))
    }
}

impl Region {
    /// The clear place in column `(ix, iy)` a body standing at height `z` steps onto, if any:
    /// the nearest in height within a step.
    pub(crate) fn step_into(&self, ix: i32, iy: i32, z: f32, steps: Steps) -> Option<u32> {
        let mut best: Option<(f32, u32)> = None;
        for j in self.column(ix, iy) {
            if !self.is_clear(j) {
                continue;
            }
            let dz = self.column_and_z(j).2 - z;
            if dz > steps.max_rise || -dz > steps.max_drop {
                continue;
            }
            if best.is_none_or(|(b, _)| dz.abs() < b) {
                best = Some((dz.abs(), j));
            }
        }
        best.map(|(_, j)| j)
    }

    /// Whether a body can walk straight from place `a` to place `b`: the grid column nearest each
    /// point of the line, every quarter of a sample along it, holds a clear, free place within a
    /// step of the floor the line is crossing. The line keeps within half a sample of clear
    /// places, on whichever side of it a wall stands.
    pub(crate) fn line_walkable(&self, a: u32, b: u32, steps: Steps, gates: &Gates) -> bool {
        let wet = self.is_wading(a) && self.is_wading(b);
        self.line_walkable_between(self.node_point(a), self.node_point(b), wet, steps, gates)
    }

    /// [`Self::line_walkable`] between two points of region space standing on floors; through
    /// water only when `wet`. Where the line enters a whole patch, it is walkable as far as it
    /// stays in the patch, and the samples there are passed over.
    pub(crate) fn line_walkable_between(
        &self,
        pa: Vec3,
        pb: Vec3,
        wet: bool,
        steps: Steps,
        gates: &Gates,
    ) -> bool {
        let s = self.spacing;
        let length = dist_xy(pa, pb);
        let n = floor_to_i32(length / (s * 0.25)).max(1);
        #[allow(clippy::cast_precision_loss)]
        let nf = n as f32;
        let mut z = pa.z;
        let mut k = 0;
        while k <= n {
            #[allow(clippy::cast_precision_loss)]
            let t = k as f32 / nf;
            let p = Vec3::new(pa.x + (pb.x - pa.x) * t, pa.y + (pb.y - pa.y) * t, z);
            let ix = floor_to_i32(p.x / s + 0.5);
            let iy = floor_to_i32(p.y / s + 0.5);
            let Some(j) = self.step_into(ix, iy, z, steps) else {
                return false;
            };
            // A straight line keeps out of the water unless it starts and ends in it.
            if gates.of(j) != Gate::Free || (self.is_wading(j) && !wet) {
                return false;
            }
            z = self.node_point(j).z;
            if let Some(patch) = self.patch_of(j).filter(|&q| gates.whole(q)) {
                if let Some((last, place)) = self.last_sample_in(patch, pa, pb, n, k) {
                    k = last;
                    z = self.node_point(place).z;
                }
            }
            k += 1;
        }
        (z - pb.z).abs() <= steps.max_rise.max(steps.max_drop)
    }

    /// Of the `n + 1` samples of the line from `pa` to `pb`, the last from sample `k` on whose
    /// column is still inside patch `p`, with the patch's place there.
    fn last_sample_in(&self, p: u32, pa: Vec3, pb: Vec3, n: i32, k: i32) -> Option<(i32, u32)> {
        let patch = self.patches.get(p as usize)?;
        let s = self.spacing;
        // The patch's columns cover x from (x0 - 1/2) s up to (x0 + w - 1/2) s, and y alike.
        let leave = |a: f32, d: f32, lo: i32, len: u8| -> f32 {
            #[allow(clippy::cast_precision_loss)]
            let (lo, hi) = (
                (lo as f32 - 0.5) * s,
                (lo as f32 + f32::from(len) - 0.5) * s,
            );
            if d > 0.0 {
                (hi - a) / d
            } else if d < 0.0 {
                (lo - a) / d
            } else {
                f32::INFINITY
            }
        };
        let t = leave(pa.x, pb.x - pa.x, i32::from(patch.x0), patch.w).min(leave(
            pa.y,
            pb.y - pa.y,
            i32::from(patch.y0),
            patch.h,
        ));
        #[allow(clippy::cast_precision_loss)]
        let mut last = if t.is_finite() {
            floor_to_i32(t * n as f32 - 1e-3).min(n)
        } else {
            n
        };
        while last > k {
            #[allow(clippy::cast_precision_loss)]
            let u = last as f32 / n as f32;
            let ix = floor_to_i32((pa.x + (pb.x - pa.x) * u) / s + 0.5);
            let iy = floor_to_i32((pa.y + (pb.y - pa.y) * u) / s + 0.5);
            if patch.holds(ix, iy) {
                return self.patch_place(p, ix, iy).map(|q| (last, q));
            }
            last -= 1;
        }
        None
    }

    /// How far before and after the corner at `v`, between the legs from `a` and to `b`, a body
    /// may cut straight across it: the longest of a few lengths, no longer than either leg, whose
    /// line across is walkable; zero when none is.
    pub(crate) fn corner_cut(&self, a: Vec3, v: Vec3, b: Vec3, steps: Steps, gates: &Gates) -> f32 {
        let (la, lb) = (dist_xy(a, v), dist_xy(v, b));
        let along = |to: Vec3, l: f32, e: f32| {
            let f = e / l;
            Vec3::new(
                v.x + (to.x - v.x) * f,
                v.y + (to.y - v.y) * f,
                v.z + (to.z - v.z) * f,
            )
        };
        CUTS.iter()
            .copied()
            .filter(|&e| e <= la && e <= lb)
            .find(|&e| {
                self.line_walkable_between(along(a, la, e), along(b, lb, e), false, steps, gates)
            })
            .unwrap_or(0.0)
    }

    /// Straighten a chain of places a slice at a time: from each kept place, jump to the farthest
    /// later place in a straight walkable line; places where a gate changes are always kept. Each
    /// line tried costs a unit for every sample along it. True once the chain is straightened,
    /// the places kept in `pull`.
    pub(crate) fn pull_step(
        &self,
        chain: &[u32],
        pull: &mut Pull,
        steps: Steps,
        gates: &Gates,
        budget: &mut u64,
    ) -> bool {
        let gate = |p: u32| gates.of(p);
        let Some(&first) = chain.first() else {
            return true;
        };
        if pull.kept.is_empty() {
            pull.kept.push(first);
        }
        while pull.i + 1 < chain.len() {
            if *budget == 0 {
                return false;
            }
            let i = pull.i;
            let mut best = i + 1;
            let mut j = i + 2;
            // A line that fails a few places running past the last one that held will not hold
            // again soon; stopping there keeps the pull linear in practice.
            let mut misses = 0;
            let origin = self.node_point(chain[i]);
            while j < chain.len() && misses < PULL_MISSES {
                if gate(chain[j - 1]) != Gate::Free || gate(chain[j]) != Gate::Free {
                    break;
                }
                let across = dist_xy(origin, self.node_point(chain[j]));
                if across > PULL_REACH {
                    break;
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let samples = (across / self.spacing) as u64 + 1;
                *budget = budget.saturating_sub(samples);
                if self.line_walkable(chain[i], chain[j], steps, gates) {
                    best = j;
                    misses = 0;
                } else {
                    misses += 1;
                }
                j += 1;
            }
            pull.kept.push(chain[best]);
            pull.i = best;
        }
        true
    }
}
