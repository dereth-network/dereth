//! The land's regions: the stretches of the outdoors a body can walk between.
//!
//! The outdoors is not one place a walk reaches everywhere: a plateau whose every side is a cliff,
//! an island in the deep sea, a valley only a dungeon leads into. A journey across the land that
//! takes it for one walks into a route that does not exist. So the land is split once, from the
//! player's own cell file, into regions: each land cell's two triangles are walkable when their
//! slope is one a body stands on (the physics' walkable rule) and their landblock is not entirely
//! deep water (which no body enters); walkable triangles that share an edge are one region. A
//! journey plans between regions by the portals that join them.
//!
//! What the land alone says is all it says: a bridge built over a gorge, or a stair of objects up a
//! cliff, joins what it shows apart, and a drop off a cliff goes one way; a walk that crosses
//! between regions tells the regions so ([`LandRegions::join`]).

use std::collections::HashMap;
use std::sync::Arc;

use dereth_physics::land::{LandblockCollision, WaterType};
use dereth_primitives::num::floor_to_i32;
use dereth_primitives::{LandblockId, Position};

/// The steepest a surface a body stands on may be: the z of its normal.
pub const FLOOR_Z: f32 = 0.664_174;

/// The side of a land cell, metres.
const CELL: f32 = 24.0;

/// A region no walk reaches: an unwalkable triangle's.
const NONE: u32 = u32::MAX;

/// The regions of one landblock's triangles.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Block {
    /// Every walkable triangle in one region; the triangles that are not walkable, none.
    Uniform { region: u32, walkable: u128 },
    /// Each triangle's region, `NONE` for one no body stands on.
    Mixed(Box<[u32; 128]>),
}

impl Block {
    fn region(&self, triangle: usize) -> Option<u32> {
        match self {
            Self::Uniform { region, walkable } => {
                (walkable >> triangle & 1 == 1).then_some(*region)
            }
            Self::Mixed(ids) => Some(ids[triangle]).filter(|&r| r != NONE),
        }
    }
}

/// The land's regions, by landblock.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LandRegions {
    blocks: HashMap<u16, Block>,
    /// Regions a walk was seen to cross between, each to the one it is part of.
    joined: HashMap<u32, u32>,
}

/// What the build keeps of a landblock: each triangle walkable or not, and which of a cell's two
/// triangles owns each of its edges (south, east, north, west).
struct Summary {
    walkable: u128,
    owner: [[u8; 4]; 64],
}

/// Which of a cell's four edges a triangle's corners (as `(i, j)` vertex steps from the cell's
/// south-west corner) take in: south, east, north, west.
fn edges_of(corners: &[(u8, u8)]) -> [bool; 4] {
    let has = |c: (u8, u8)| corners.contains(&c);
    [
        has((0, 0)) && has((1, 0)),
        has((1, 0)) && has((1, 1)),
        has((0, 1)) && has((1, 1)),
        has((0, 0)) && has((0, 1)),
    ]
}

fn summary(b: &LandblockCollision) -> Option<Summary> {
    if b.polygons.len() != 128 {
        return None;
    }
    let deep = b.water_type == WaterType::EntirelyWater;
    let mut walkable = 0u128;
    let mut owner = [[0u8; 4]; 64];
    for (k, cell_owner) in owner.iter_mut().enumerate() {
        let (i, j) = (
            u8::try_from(k / 8).unwrap_or(0),
            u8::try_from(k % 8).unwrap_or(0),
        );
        for t in 0..2 {
            let p = &b.polygons[2 * k + t];
            if !deep && p.plane.normal.z >= FLOOR_Z {
                walkable |= 1u128 << (2 * k + t);
            }
            let corners: Vec<(u8, u8)> = p
                .vertices
                .iter()
                .map(|v| {
                    let (vi, vj) = (
                        floor_to_i32(v.x / CELL + 0.5),
                        floor_to_i32(v.y / CELL + 0.5),
                    );
                    (
                        u8::try_from(vi - i32::from(i)).unwrap_or(9),
                        u8::try_from(vj - i32::from(j)).unwrap_or(9),
                    )
                })
                .collect();
            for (e, owns) in edges_of(&corners).into_iter().enumerate() {
                if owns {
                    cell_owner[e] = u8::try_from(t).unwrap_or(0);
                }
            }
        }
    }
    Some(Summary { walkable, owner })
}

/// A triangle's place in the build: its landblock's, then its own.
fn slot(block: u16, triangle: usize) -> u32 {
    u32::from(block) * 128 + u32::try_from(triangle).unwrap_or(0)
}

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        let up = parent[parent[x as usize] as usize];
        parent[x as usize] = up;
        x = up;
    }
    x
}

fn union(parent: &mut [u32], a: u32, b: u32) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra.max(rb) as usize] = ra.min(rb);
    }
}

/// The world's landblocks, 255 by 255.
const BLOCKS: u32 = 255 * 255;

/// The land's regions as they are made: a slice of the landblocks read at a time, so a frame that
/// shares its time with them is not held up for the whole (about a second's reading).
#[derive(Default)]
pub struct RegionBuilder {
    next: u32,
    summaries: HashMap<u16, Summary>,
}

impl std::fmt::Debug for RegionBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegionBuilder")
            .field("next", &self.next)
            .finish_non_exhaustive()
    }
}

impl RegionBuilder {
    /// Read up to `count` more landblocks from `land`: whether every one is read.
    pub fn step(
        &mut self,
        land: &dyn Fn(LandblockId) -> Option<Arc<LandblockCollision>>,
        count: u32,
    ) -> bool {
        let end = self.next.saturating_add(count).min(BLOCKS);
        for n in self.next..end {
            let (x, y) = (n / 255, n % 255);
            let id = u16::try_from((x << 8) | y).unwrap_or(0);
            if let Some(s) = land(LandblockId(id)).as_deref().and_then(summary) {
                self.summaries.insert(id, s);
            }
        }
        self.next = end;
        self.next >= BLOCKS
    }

    /// The regions of what was read.
    #[must_use]
    pub fn finish(self) -> LandRegions {
        LandRegions::of_summaries(&self.summaries)
    }
}

impl LandRegions {
    /// The regions of the land `land` gives each landblock of, the world's 255 by 255.
    #[must_use]
    pub fn build(land: &dyn Fn(LandblockId) -> Option<Arc<LandblockCollision>>) -> Self {
        let mut b = RegionBuilder::default();
        b.step(land, BLOCKS);
        b.finish()
    }

    fn of_summaries(summaries: &HashMap<u16, Summary>) -> Self {
        let mut parent: Vec<u32> = (0..=u32::from(u16::MAX) * 128 + 127).collect();
        let walk = |s: &Summary, t: usize| s.walkable >> t & 1 == 1;
        for (&id, s) in summaries {
            let (x, y) = (id >> 8, id & 0xFF);
            for k in 0..64 {
                let (i, j) = (k / 8, k % 8);
                // The diagonal.
                if walk(s, 2 * k) && walk(s, 2 * k + 1) {
                    union(&mut parent, slot(id, 2 * k), slot(id, 2 * k + 1));
                }
                // East, then north: the cell beside it, in this landblock or the next.
                for (edge, across) in [(1usize, 3usize), (2, 0)] {
                    let (ni, nj, nid) = if edge == 1 {
                        if i < 7 {
                            (i + 1, j, Some(id))
                        } else {
                            (0, j, (x < 0xFE).then(|| ((x + 1) << 8) | y))
                        }
                    } else if j < 7 {
                        (i, j + 1, Some(id))
                    } else {
                        (i, 0, (y < 0xFE).then(|| (x << 8) | (y + 1)))
                    };
                    let Some(nid) = nid else { continue };
                    let Some(n) = summaries.get(&nid) else {
                        continue;
                    };
                    let nk = ni * 8 + nj;
                    let a = 2 * k + usize::from(s.owner[k][edge]);
                    let b = 2 * nk + usize::from(n.owner[nk][across]);
                    if walk(s, a) && walk(n, b) {
                        union(&mut parent, slot(id, a), slot(nid, b));
                    }
                }
            }
        }
        // Each region named by a number of its own, small and in the order the blocks are.
        let mut names: HashMap<u32, u32> = HashMap::new();
        let mut ids: Vec<u16> = summaries.keys().copied().collect();
        ids.sort_unstable();
        let mut blocks = HashMap::new();
        for id in ids {
            let s = &summaries[&id];
            let mut each = [NONE; 128];
            for (t, r) in each.iter_mut().enumerate() {
                if walk(s, t) {
                    let root = find(&mut parent, slot(id, t));
                    let next = u32::try_from(names.len()).unwrap_or(NONE - 1);
                    *r = *names.entry(root).or_insert(next);
                }
            }
            let first = each.iter().copied().find(|&r| r != NONE);
            let block = match first {
                Some(region) if each.iter().all(|&r| r == NONE || r == region) => Block::Uniform {
                    region,
                    walkable: s.walkable,
                },
                None => continue,
                Some(_) => Block::Mixed(Box::new(each)),
            };
            blocks.insert(id, block);
        }
        Self {
            blocks,
            joined: HashMap::new(),
        }
    }

    /// How many landblocks have a region.
    #[must_use]
    pub fn blocks(&self) -> usize {
        self.blocks.len()
    }

    /// The region `r` is part of: itself, unless a walk joined it to another.
    fn root(&self, mut r: u32) -> u32 {
        while let Some(&up) = self.joined.get(&r) {
            r = up;
        }
        r
    }

    /// Regions `a` and `b` are one: a walk went from one to the other, which the land alone did
    /// not show.
    pub fn join(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.root(a), self.root(b));
        if ra != rb {
            self.joined.insert(ra.max(rb), ra.min(rb));
        }
    }

    /// The region of the land at `p`, outdoors or in a building (whose cells share the landblock's
    /// frame): the walkable triangle under it, else the nearest walkable triangle in the cells
    /// round it. `None` where no walkable land is near.
    #[must_use]
    pub fn region_at(&self, p: &Position) -> Option<u32> {
        let id = u16::try_from(p.cell.0 >> 16).ok()?;
        let block = self.blocks.get(&id)?;
        let o = p.frame.origin;
        let cell = |v: f32| dereth_primitives::num::floor_to_i32(v / CELL).clamp(0, 7);
        let (ci, cj) = (cell(o.x), cell(o.y));
        let mut best: Option<(i32, u32)> = None;
        for di in -1..=1 {
            for dj in -1..=1 {
                let (i, j) = (ci + di, cj + dj);
                if !(0..8).contains(&i) || !(0..8).contains(&j) {
                    continue;
                }
                let k = usize::try_from(i * 8 + j).ok()?;
                let far = di.abs() + dj.abs();
                for t in 0..2 {
                    if let Some(r) = block.region(2 * k + t) {
                        if best.is_none_or(|(d, _)| far < d) {
                            best = Some((far, r));
                        }
                    }
                }
            }
        }
        best.map(|(_, r)| self.root(r))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the land's regions over hand-made landblocks).
    use super::*;

    /// A landblock summary: every triangle walkable but those `steep` says, and each cell's
    /// triangles split south-west to north-east (the first owning south and east).
    fn block(steep: impl Fn(usize, usize) -> bool) -> Summary {
        let mut walkable = 0u128;
        for k in 0..64 {
            if !steep(k / 8, k % 8) {
                walkable |= 0b11 << (2 * k);
            }
        }
        Summary {
            walkable,
            owner: [[0, 0, 1, 1]; 64],
        }
    }

    #[test]
    fn a_cliff_all_round_makes_a_region_of_its_own_and_a_gap_in_it_joins_them() {
        let at = |id: u16, i: usize, j: usize| {
            Position::new(
                dereth_primitives::CellId((u32::from(id) << 16) | 1),
                dereth_primitives::Frame::new(
                    dereth_primitives::Vec3::new(i as f32 * CELL + 6.0, j as f32 * CELL + 6.0, 0.0),
                    dereth_primitives::Quat::IDENTITY,
                ),
            )
        };
        // Two landblocks side by side; in the first, a ring of steep cells round its middle.
        let ring = |i: usize, j: usize| {
            (2..=5).contains(&i)
                && (2..=5).contains(&j)
                && !((3..=4).contains(&i) && (3..=4).contains(&j))
        };
        let mut summaries = HashMap::from([(0x1010, block(ring)), (0x1110, block(|_, _| false))]);
        let lands = LandRegions::of_summaries(&summaries);
        let inside = lands.region_at(&at(0x1010, 3, 3)).expect("land");
        let outside = lands.region_at(&at(0x1010, 0, 0)).expect("land");
        let next = lands.region_at(&at(0x1110, 4, 4)).expect("land");
        assert_ne!(inside, outside, "the ring's inside is cut off");
        assert_eq!(outside, next, "a landblock's edge is walked across");
        // A cliff cell's own place: the nearest walkable land, outside the ring.
        assert_eq!(lands.region_at(&at(0x1010, 2, 2)), Some(outside));
        // A gap in the ring: one region.
        summaries.insert(0x1010, block(|i, j| ring(i, j) && !(i == 3 && j == 2)));
        let gapped = LandRegions::of_summaries(&summaries);
        assert_eq!(
            gapped.region_at(&at(0x1010, 3, 3)),
            gapped.region_at(&at(0x1010, 0, 0))
        );
        // A walk seen crossing joins them, the land notwithstanding.
        let mut walked = lands.clone();
        walked.join(inside, outside);
        assert_eq!(
            walked.region_at(&at(0x1010, 3, 3)),
            walked.region_at(&at(0x1010, 0, 0))
        );
    }

    #[test]
    fn deep_sea_and_land_not_there_have_no_region() {
        let mut deep = block(|_, _| false);
        deep.walkable = 0;
        let lands = LandRegions::of_summaries(&HashMap::from([(0x2020, deep)]));
        assert_eq!(lands.blocks(), 0);
        let p = Position::new(
            dereth_primitives::CellId(0x2020_0001),
            dereth_primitives::Frame::new(
                dereth_primitives::Vec3::new(10.0, 10.0, 0.0),
                dereth_primitives::Quat::IDENTITY,
            ),
        );
        assert_eq!(lands.region_at(&p), None);
        assert_eq!(
            lands.region_at(&Position {
                cell: dereth_primitives::CellId(0x3030_0001),
                ..p
            }),
            None
        );
    }
}
