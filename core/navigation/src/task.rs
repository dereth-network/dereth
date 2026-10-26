//! Planning a slice at a time: a route, or the region round a place, worked on in units of work
//! (a floor sample taken, a place's clearance decided or merged into a patch, a place settled by
//! the search or looked out from at a patch's edge), so that the work can be spread over frames or
//! run whole on a thread of its own.
//!
//! The units are counted, not timed: the caller decides how many to spend, by its own clock.

use std::collections::HashMap;

use dereth_primitives::{CellId, ObjectId, Position, Quat, Vec3};

use crate::mesh::MeshSearch;
use crate::path::{End, NavPath, NoPath};
use crate::region::{block_xy, Finishing, Region};
use crate::search::{Gate, Gates, Pull, Search};
use crate::source::NavSource;
use crate::vmath::dist;
use crate::world::{build_path, NavWorld, NEAR_RISE, SHORT_COST};

/// What a task is for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Want {
    /// A walking route to the destination.
    Route,
    /// A route ending within this far of the destination, as [`NavWorld::plan_near`] ends one.
    Approach(f32),
    /// Only the region round both ends built, and the destination taken to the nearest place to
    /// stand.
    Place,
}

/// A planning task: a route, or the region round a place, worked on with
/// [`NavWorld::advance`].
#[derive(Debug)]
pub struct Task {
    from: Position,
    to: Position,
    want: Want,
    /// Take the destination to the nearest place to stand within this far first; with `true`,
    /// there has to be one.
    snap: Option<(f32, bool)>,
    /// Plan toward a window's edge when the two ends are too far apart for one window.
    corridor: bool,
    /// Whether the route goes the whole way: false once it aims at a window's edge.
    whole: bool,
    phase: Phase,
}

#[derive(Debug)]
enum Phase {
    /// Finding the region, or what to build.
    Locate,
    /// Building a region.
    Build(Box<Finishing>),
    /// Searching a region's patches, the region known by its serial number.
    Search {
        region: u64,
        search: Box<MeshSearch>,
        gates: Gates,
        door_ids: Vec<ObjectId>,
    },
    /// With no walk there, searching again with the drops off the region's ledges, to name the
    /// first a way there takes.
    Drop {
        region: u64,
        search: Box<Search>,
        gates: Gates,
    },
    /// Straightening the chain the search found.
    Pull {
        region: u64,
        chain: Vec<u32>,
        pull: Pull,
        gates: Gates,
        door_ids: Vec<ObjectId>,
    },
}

/// What a task came to.
#[derive(Debug, Clone, PartialEq)]
pub enum Done {
    /// A route, and whether it goes the whole way (false: to a window's edge on the way, from
    /// where the rest is planned).
    Route(Result<(NavPath, bool), NoPath>),
    /// The place to stand nearest the destination, or the destination itself when none is near.
    Place(Result<Position, NoPath>),
}

impl Task {
    /// A walking route from `from` as far toward `to` as one window holds, as
    /// [`NavWorld::plan_toward`] plans it; with a `snap`, to the nearest place to stand within
    /// that far of `to` when there is one.
    #[must_use]
    pub const fn route(from: Position, to: Position, snap: Option<f32>) -> Self {
        Self::new(from, to, Want::Route, snap, true)
    }

    /// A route from `from` as far toward `to` as one window holds, ending within `reach` of it,
    /// as [`NavWorld::approach`] plans it; with a `snap`, `to` taken to the nearest place to
    /// stand within that far of it first, when there is one.
    #[must_use]
    pub const fn approach(from: Position, to: Position, reach: f32, snap: Option<f32>) -> Self {
        Self::new(from, to, Want::Approach(reach), snap, true)
    }

    /// Where the task goes: the destination asked for, once taken to a place to stand the place
    /// it was taken to, and once aiming at a window's edge that edge.
    #[must_use]
    pub const fn destination(&self) -> Position {
        self.to
    }

    /// The region round `at` built, and the nearest place to stand within `reach` of it.
    #[must_use]
    pub const fn place(at: Position, reach: f32) -> Self {
        Self::new(at, at, Want::Place, Some(reach), false)
    }

    const fn new(
        from: Position,
        to: Position,
        want: Want,
        snap: Option<f32>,
        corridor: bool,
    ) -> Self {
        let snap = match snap {
            Some(r) => Some((r, false)),
            None => None,
        };
        Self {
            from,
            to,
            want,
            snap,
            corridor,
            whole: true,
            phase: Phase::Locate,
        }
    }

    /// Plan exactly from `from` to `to`, no window's edge on the way.
    pub(crate) const fn exact(from: Position, to: Position, want: Want) -> Self {
        Self::new(from, to, want, None, false)
    }
}

/// Where a task's region is.
enum Located {
    /// Built: its index.
    Have(usize),
    /// To build: a dungeon's, from this cell.
    Interior(CellId),
    /// To build: an outdoor window `(x0, y0, x1, y1)` of landblocks.
    Outdoor((i32, i32, i32, i32)),
}

impl<S: NavSource> NavWorld<S> {
    /// Where the region a route from `from` to `to` is planned in is, or what to build.
    ///
    /// A region already built is used when it covers both ends; an outdoor one only when its
    /// window also holds the margin of landblocks round each end that a window built for these
    /// two ends would hold. One built for another route can cover an end with its own edge, where
    /// the way to it may run outside the window: planned in it, such a route ends unreachable.
    fn locate(&self, from: &Position, to: &Position) -> Result<Located, NoPath> {
        let m = self.config.margin_blocks;
        let (ax, ay) = block_xy(from.cell.0 >> 16);
        let (bx, by) = block_xy(to.cell.0 >> 16);
        let wanted = (
            (ax.min(bx) - m).max(0),
            (ay.min(by) - m).max(0),
            (ax.max(bx) + m).min(0xFE),
            (ay.max(by) + m).min(0xFE),
        );
        let holds = |w: (i32, i32, i32, i32)| {
            w.0 <= wanted.0 && w.1 <= wanted.1 && w.2 >= wanted.2 && w.3 >= wanted.3
        };
        if let Some(i) = self
            .regions
            .iter()
            .position(|r| r.covers(from) && r.covers(to) && r.window().is_none_or(holds))
        {
            return Ok(Located::Have(i));
        }
        let interior = from.cell.0 & 0xFFFF >= 0x0100;
        if interior
            && !self
                .opens_outdoors(from.cell)
                .ok_or(NoPath::NoCell(from.cell))?
        {
            if let Some(i) = self.regions.iter().position(|r| r.covers(from)) {
                return Ok(Located::Have(i));
            }
            return Ok(Located::Interior(from.cell));
        }
        // Outdoors: a window over both ends' landblocks and a margin round them.
        let window = wanted;
        if window.2 - window.0 + 1 > self.config.max_blocks
            || window.3 - window.1 + 1 > self.config.max_blocks
        {
            return Err(NoPath::TooFar);
        }
        Ok(Located::Outdoor(window))
    }

    /// Keep a built region, numbered; a new outdoor window replaces the last.
    fn insert(&mut self, mut region: Region) -> usize {
        self.serial += 1;
        region.serial = self.serial;
        if region.window().is_some() {
            self.regions.retain(|r| r.window().is_none());
        }
        self.regions.push(region);
        self.regions.len() - 1
    }

    /// The index of the region a route from `from` to `to` is planned in, built now whole if it
    /// has not been.
    pub(crate) fn region_index(&mut self, from: &Position, to: &Position) -> Result<usize, NoPath> {
        let region = match self.locate(from, to)? {
            Located::Have(i) => return Ok(i),
            Located::Interior(cell) => Region::build_interior(&self.source, cell, &self.config),
            Located::Outdoor(window) => Region::build_outdoor(&self.source, window, &self.config),
        };
        let region = region.ok_or(NoPath::NoCell(from.cell))?;
        Ok(self.insert(region))
    }

    /// The middle of the farthest landblock along the corridor from `from` toward `to` that a
    /// window round both it and `from` still holds, on the ground.
    fn window_edge(&self, from: &Position, to: &Position) -> Result<Position, NoPath> {
        let a = block_xy(from.cell.0 >> 16);
        let b = block_xy(to.cell.0 >> 16);
        let chain = self.corridor(a, b).ok_or(NoPath::Unreachable)?;
        let reach = self.config.max_blocks - 1 - 2 * self.config.margin_blocks;
        let next = chain
            .iter()
            .rev()
            .find(|c| (c.0 - a.0).abs() <= reach && (c.1 - a.1).abs() <= reach)
            .copied()
            .ok_or(NoPath::TooFar)?;
        if next == a {
            return Err(NoPath::TooFar);
        }
        let id = u16::try_from((next.0 << 8) | next.1).map_err(|_| NoPath::TooFar)?;
        let block = CellId((u32::from(id) << 16) | 0x24);
        let land = self
            .source
            .landblock(dereth_primitives::LandblockId(id))
            .ok_or(NoPath::NoCell(block))?;
        let middle = Vec3::new(96.0, 96.0, 0.0);
        let z = land.find_terrain_poly(0x24, middle).map_or(0.0, |p| {
            -(p.plane.d + p.plane.normal.x * 96.0 + p.plane.normal.y * 96.0) / p.plane.normal.z
        });
        Ok(Position::new(
            block,
            dereth_primitives::Frame::new(Vec3::new(96.0, 96.0, z), Quat::IDENTITY),
        ))
    }

    /// The search a task starts over region `i`.
    fn begin_search(&self, i: usize, task: &Task) -> Result<Phase, NoPath> {
        let region = &self.regions[i];
        let (from, to) = (&task.from, &task.to);
        if !region.covers(to) {
            return Err(NoPath::Disconnected {
                from: from.cell,
                to: to.cell,
            });
        }
        let (gates, door_ids) = self.gates(region);
        let gates = Gates::new(region, gates);
        let gate = |i: u32| gates.of(i);
        let steps = self.steps();
        let start = region
            .nearest_clear(region.to_region(from), 1.5, |i| gate(i) != Gate::Blocked)
            .ok_or(NoPath::NoStandingPlace(End::Start))?;
        let (ends, toward) = match task.want {
            Want::Approach(reach) => {
                let target = region.to_region(to);
                let ends: HashMap<u32, f32> = region
                    .clear_near(target, reach, NEAR_RISE)
                    .filter(|&i| gate(i) != Gate::Blocked)
                    .map(|i| (i, SHORT_COST * dist(region.node_point(i), target)))
                    .collect();
                if ends.is_empty() {
                    return Err(NoPath::NoStandingPlace(End::Goal));
                }
                (ends, target)
            }
            Want::Route | Want::Place => {
                let goal = region
                    .nearest_clear(region.to_region(to), 1.5, |i| gate(i) != Gate::Blocked)
                    .ok_or(NoPath::NoStandingPlace(End::Goal))?;
                (HashMap::from([(goal, 0.0)]), region.node_point(goal))
            }
        };
        let search = MeshSearch::new(region, start, ends, toward, steps, &gates);
        Ok(Phase::Search {
            region: region.serial,
            search: Box::new(search),
            gates,
            door_ids,
        })
    }

    /// The search again over region `i` with the drops off its ledges, for a task no walk
    /// answers: `None` outdoors, or with no drop anywhere.
    fn drop_search(
        &self,
        i: usize,
        (from, to, want): (&Position, &Position, Want),
        gates: &mut Gates,
    ) -> Option<Phase> {
        let region = &self.regions[i];
        let solids = region.dungeon_solids(&self.source, &self.config)?;
        let drops = region.ledge_drops(&solids, &self.config);
        if drops.is_empty() {
            return None;
        }
        let gates = std::mem::take(gates);
        let gate = |p: u32| gates.of(p);
        let steps = self.steps();
        let start =
            region.nearest_clear(region.to_region(from), 1.5, |p| gate(p) != Gate::Blocked)?;
        let (ends, toward) = match want {
            Want::Approach(reach) => {
                let target = region.to_region(to);
                let ends: HashMap<u32, f32> = region
                    .clear_near(target, reach, NEAR_RISE)
                    .filter(|&p| gate(p) != Gate::Blocked)
                    .map(|p| (p, SHORT_COST * dist(region.node_point(p), target)))
                    .collect();
                (ends, target)
            }
            Want::Route | Want::Place => {
                let goal = region
                    .nearest_clear(region.to_region(to), 1.5, |p| gate(p) != Gate::Blocked)?;
                (HashMap::from([(goal, 0.0)]), region.node_point(goal))
            }
        };
        let search = Search::new(region, start, ends, toward, steps).with_drops(drops);
        Some(Phase::Drop {
            region: region.serial,
            search: Box::new(search),
            gates,
        })
    }

    /// What a task that stopped short came to.
    const fn failed(task: &Task, e: NoPath) -> Done {
        match task.want {
            Want::Place => Done::Place(Err(e)),
            Want::Route | Want::Approach(_) => Done::Route(Err(e)),
        }
    }

    /// Work on `task`, spending about `budget` units (a floor sample taken, a place's clearance
    /// decided or merged into a patch, a place settled by the search or looked out from at a
    /// patch's edge, each one; ordering a region's places and straightening a route are done
    /// whole); what it came to once it is done.
    pub fn advance(&mut self, task: &mut Task, budget: &mut u64) -> Option<Done> {
        loop {
            match &mut task.phase {
                Phase::Locate => match self.locate(&task.from, &task.to) {
                    Ok(Located::Have(i)) => {
                        if let Some((reach, required)) = task.snap.take() {
                            let region = &self.regions[i];
                            match region.nearest_place(&task.to, reach) {
                                Some(p) => task.to = region.position(p),
                                None if required => {
                                    return Some(Self::failed(
                                        task,
                                        NoPath::NoStandingPlace(End::Goal),
                                    ))
                                }
                                None => {}
                            }
                        }
                        if task.want == Want::Place {
                            return Some(Done::Place(Ok(task.to)));
                        }
                        match self.begin_search(i, task) {
                            Ok(phase) => task.phase = phase,
                            Err(e) => return Some(Self::failed(task, e)),
                        }
                    }
                    Ok(Located::Interior(cell)) => {
                        match Region::start_interior(&self.source, cell, &self.config) {
                            Some(f) => task.phase = Phase::Build(Box::new(f)),
                            None => return Some(Self::failed(task, NoPath::NoCell(cell))),
                        }
                    }
                    Ok(Located::Outdoor(window)) => {
                        match Region::start_outdoor(&self.source, window, &self.config) {
                            Some(f) => task.phase = Phase::Build(Box::new(f)),
                            None => {
                                return Some(Self::failed(task, NoPath::NoCell(task.from.cell)))
                            }
                        }
                    }
                    Err(NoPath::TooFar) if task.corridor && task.whole => {
                        // Too far for one window: as far as a window round the start holds,
                        // along the landblocks between.
                        match self.window_edge(&task.from, &task.to) {
                            Ok(aim) => {
                                task.to = aim;
                                task.snap = Some((48.0, true));
                                task.want = Want::Route;
                                task.whole = false;
                            }
                            Err(e) => return Some(Self::failed(task, e)),
                        }
                    }
                    Err(e) => return Some(Self::failed(task, e)),
                },
                Phase::Build(finishing) => {
                    let region = finishing.step(budget)?;
                    self.insert(region);
                    task.phase = Phase::Locate;
                }
                Phase::Search {
                    region,
                    search,
                    gates,
                    door_ids,
                } => {
                    let Some(i) = self.regions.iter().position(|r| r.serial == *region) else {
                        // The region went (forgotten, or replaced by another window): again.
                        task.phase = Phase::Locate;
                        continue;
                    };
                    let chain = search.step(&self.regions[i], gates, budget)?;
                    let Some(chain) = chain else {
                        // No walk there: is there a way with a drop? A dungeon's only.
                        match self.drop_search(i, (&task.from, &task.to, task.want), gates) {
                            Some(phase) => {
                                task.phase = phase;
                                continue;
                            }
                            None => return Some(Self::failed(task, NoPath::Unreachable)),
                        }
                    };
                    task.phase = Phase::Pull {
                        region: *region,
                        chain,
                        pull: Pull::default(),
                        gates: std::mem::take(gates),
                        door_ids: std::mem::take(door_ids),
                    };
                }
                Phase::Drop {
                    region,
                    search,
                    gates,
                } => {
                    let Some(i) = self.regions.iter().position(|r| r.serial == *region) else {
                        task.phase = Phase::Locate;
                        continue;
                    };
                    let r = &self.regions[i];
                    let chain = search.step(r, gates, budget)?;
                    let steps = self.steps();
                    // The first drop the way takes: two places in a row no step joins.
                    let found = chain.as_deref().and_then(|c| {
                        c.windows(2).find_map(|w| {
                            let (a, b) = (r.node_point(w[0]), r.node_point(w[1]));
                            (a.z - b.z > steps.max_drop).then(|| NoPath::OnlyByDrop {
                                stand: r.position(w[0]),
                                land: r.position(w[1]),
                                height: a.z - b.z,
                            })
                        })
                    });
                    return Some(Self::failed(task, found.unwrap_or(NoPath::Unreachable)));
                }
                Phase::Pull {
                    region,
                    chain,
                    pull,
                    gates,
                    door_ids,
                } => {
                    let Some(i) = self.regions.iter().position(|r| r.serial == *region) else {
                        task.phase = Phase::Locate;
                        continue;
                    };
                    let steps = self.steps();
                    let r = &self.regions[i];
                    if !r.pull_step(chain, pull, steps, gates, budget) {
                        return None;
                    }
                    let path = build_path(
                        r, &task.from, &task.to, chain, &pull.kept, gates, door_ids, steps,
                    );
                    return Some(Done::Route(Ok((path, task.whole))));
                }
            }
        }
    }

    /// Run `task` to its end.
    pub fn run(&mut self, mut task: Task) -> Done {
        loop {
            let mut all = u64::MAX;
            if let Some(done) = self.advance(&mut task, &mut all) {
                return done;
            }
        }
    }

    /// Run a route task to its end.
    pub(crate) fn run_route(&mut self, task: Task) -> Result<(NavPath, bool), NoPath> {
        match self.run(task) {
            Done::Route(r) => r,
            Done::Place(p) => p.and(Err(NoPath::Unreachable)),
        }
    }
}
