//! The planner: regions built on first use and kept, the objects that come and go, and the plan.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

use dereth_physics::globals::FLOOR_Z;
use dereth_physics::source::SetupGeometry;
use dereth_primitives::num::floor_to_i32;
use dereth_primitives::{CellId, ObjectId, Position, Quat, Vec3};

use crate::path::{global, global_distance, NavPath, NoPath, Waypoint, WaypointAction};
use crate::region::{block_xy, Region};
use crate::search::{Gate, Gates, Steps};
use crate::solids::{object_shapes, Shape};
use crate::source::{NavConfig, NavSource};
use crate::task::{Task, Want};
use crate::vmath::{dist, sub};
use crate::walk::{CheckReport, SegmentCheck};

/// A portal to the outdoors.
const OUTDOORS: u32 = 0xFFFF_FFFF;

/// What an approach pays for each metre its end stands short of the destination, in metres of
/// walking: the route goes to the destination itself whenever it can, unless only by a long way
/// round, and otherwise ends as near it as the body gets.
pub(crate) const SHORT_COST: f32 = 20.0;

/// How far above or below the destination an approach may end: about a body's height.
pub(crate) const NEAR_RISE: f32 = 2.0;

/// What an interior cell's portals join it to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Joined {
    /// The land, somewhere: the cell is inside a building.
    Outdoors,
    /// Only interior cells: one part of a dungeon, named by its lowest cell.
    Part(u32),
}

/// Where a door stands, as far as the world model knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorState {
    /// Open: the route walks through.
    Open,
    /// Closed: the route opens it on the way, at a cost.
    Closed,
    /// Locked: the route goes another way or not at all.
    Locked,
    /// Not known: planned as closed.
    Unknown,
}

/// What an object in the way is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObstacleKind {
    /// Something the route goes around.
    Solid,
    /// A door.
    Door(DoorState),
    /// A creature standing or ambling about: the route goes round it where it can, and through
    /// where it cannot, since the creature will move.
    Creature,
}

/// An object that is not baked into the world: a door, a chest, a creature, anything the server
/// places.
#[derive(Debug, Clone)]
pub struct Obstacle {
    /// The object.
    pub id: ObjectId,
    /// Where it stands.
    pub position: Position,
    /// Its collision half.
    pub setup: Arc<SetupGeometry>,
    /// Its scale.
    pub scale: f32,
    /// What it is.
    pub kind: ObstacleKind,
}

/// The planner over one source.
#[derive(Debug)]
pub struct NavWorld<S> {
    pub(crate) source: S,
    pub(crate) config: NavConfig,
    pub(crate) regions: Vec<Region>,
    obstacles: BTreeMap<u32, Obstacle>,
    /// The serial number the last region built was given.
    pub(crate) serial: u64,
}

impl<S: NavSource> NavWorld<S> {
    /// A planner over `source`, with nothing built yet.
    pub fn new(source: S, config: NavConfig) -> Self {
        Self {
            source,
            config,
            regions: Vec::new(),
            obstacles: BTreeMap::new(),
            serial: 0,
        }
    }

    /// The configuration.
    pub const fn config(&self) -> &NavConfig {
        &self.config
    }

    /// The source.
    pub const fn source(&self) -> &S {
        &self.source
    }

    /// The region a route starting in `cell` is planned in, built now if it has not been: the
    /// dungeon holding an interior cell, or the outdoors (the cell's landblock and its margin) for
    /// a land cell or a building's interior.
    ///
    /// # Errors
    /// [`NoPath::NoCell`] for a cell the source cannot supply.
    pub fn region(&mut self, cell: CellId) -> Result<&Region, NoPath> {
        let p = Position::new(cell, dereth_primitives::Frame::default());
        let i = self.region_index(&p, &p)?;
        Ok(&self.regions[i])
    }

    /// The region a route from `from` to `to` is planned in, built now if it has not been.
    ///
    /// # Errors
    /// [`NoPath`] says why there is none.
    pub fn region_between(&mut self, from: &Position, to: &Position) -> Result<&Region, NoPath> {
        let i = self.region_index(from, to)?;
        Ok(&self.regions[i])
    }

    /// What an interior cell's portals join it to, inside its landblock: the land somewhere (a
    /// building's inside), or only other interior cells (one part of a dungeon). `None` for a cell
    /// the source cannot supply.
    fn joined(&self, cell: CellId) -> Option<Joined> {
        let mut seen = std::collections::BTreeSet::from([cell.0]);
        let mut queue = VecDeque::from([cell.0]);
        while let Some(id) = queue.pop_front() {
            let g = self.source.env_cell(CellId(id))?;
            for p in &g.portals {
                if p.other_cell_id == OUTDOORS {
                    return Some(Joined::Outdoors);
                }
                if p.other_cell_id & 0xFFFF_0000 == id & 0xFFFF_0000 && seen.insert(p.other_cell_id)
                {
                    queue.push_back(p.other_cell_id);
                }
            }
        }
        Some(Joined::Part(seen.first().copied().unwrap_or(cell.0)))
    }

    /// Whether an interior cell is reached from the outdoors: its cells open onto the land
    /// somewhere, so it is a building's inside rather than a dungeon.
    pub(crate) fn opens_outdoors(&self, cell: CellId) -> Option<bool> {
        self.joined(cell).map(|j| j == Joined::Outdoors)
    }

    /// Where a body standing at `p` can walk: the outdoors (land and the buildings it opens onto),
    /// or the part of a dungeon its cells join, which is not always the whole dungeon. `None` for
    /// a cell the source cannot supply.
    pub fn realm(&self, p: &Position) -> Option<crate::journey::Realm> {
        if p.cell.0 & 0xFFFF < 0x0100 {
            return Some(crate::journey::Realm::Outdoors);
        }
        Some(match self.joined(p.cell)? {
            Joined::Outdoors => crate::journey::Realm::Outdoors,
            Joined::Part(lowest) => crate::journey::Realm::Part(CellId(lowest)),
        })
    }

    /// The landblocks a walk from landblock `from` to landblock `to` crosses, both included: the
    /// cheapest chain over the land, round landblocks that are all water and dearer through
    /// steep ones. `None` when the land does not join them.
    #[must_use]
    pub fn corridor(&self, from: (i32, i32), to: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        use std::collections::BinaryHeap;
        let passable = |b: (i32, i32)| -> Option<f32> {
            let id = dereth_primitives::LandblockId(
                u16::try_from((b.0 << 8) | b.1)
                    .ok()
                    .filter(|_| (0..=0xFE).contains(&b.0) && (0..=0xFE).contains(&b.1))?,
            );
            let land = self.source.landblock(id)?;
            if land.water_type == dereth_physics::land::WaterType::EntirelyWater {
                return None;
            }
            #[allow(clippy::cast_precision_loss)]
            let steep = land
                .polygons
                .iter()
                .filter(|p| p.plane.normal.z < FLOOR_Z)
                .count() as f32
                / land.polygons.len().max(1) as f32;
            Some(1.0 + 4.0 * steep)
        };
        #[derive(PartialEq)]
        struct Open(f32, (i32, i32));
        impl Eq for Open {}
        impl PartialOrd for Open {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Open {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                o.0.total_cmp(&self.0)
            }
        }
        let h = |b: (i32, i32)| {
            #[allow(clippy::cast_precision_loss)]
            let (dx, dy) = ((b.0 - to.0) as f32, (b.1 - to.1) as f32);
            (dx * dx + dy * dy).sqrt()
        };
        let mut cost: HashMap<(i32, i32), f32> = HashMap::from([(from, 0.0)]);
        let mut came: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
        let mut open = BinaryHeap::from([Open(h(from), from)]);
        let mut seen = std::collections::HashSet::new();
        while let Some(Open(_, b)) = open.pop() {
            if !seen.insert(b) {
                continue;
            }
            if b == to {
                let mut chain = vec![to];
                let mut at = to;
                while let Some(&p) = came.get(&at) {
                    chain.push(p);
                    at = p;
                }
                chain.reverse();
                return Some(chain);
            }
            for (dx, dy) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let n = (b.0 + dx, b.1 + dy);
                let Some(c) = passable(n) else { continue };
                let step = if dx != 0 && dy != 0 { 1.414 * c } else { c };
                let g = cost[&b] + step;
                if cost.get(&n).is_none_or(|&old| g < old) {
                    cost.insert(n, g);
                    came.insert(n, b);
                    open.push(Open(g + h(n), n));
                }
            }
        }
        None
    }

    /// Plan as far toward `to` as one region reaches: the whole way when both ends fit one window,
    /// else along the corridor of landblocks to the farthest place a window round `from` holds.
    /// The `bool` says whether the route ends at the goal; walk it and plan again from where it
    /// ends.
    ///
    /// # Errors
    /// [`NoPath`] says why there is none.
    pub fn plan_toward(
        &mut self,
        from: &Position,
        to: &Position,
    ) -> Result<(NavPath, bool), NoPath> {
        self.toward(from, to, None)
    }

    /// [`Self::plan_toward`] for something to be reached rather than stood on: the last window's
    /// route ends as [`Self::plan_near`] ends it, within `reach` of `to`.
    ///
    /// # Errors
    /// [`NoPath`] says why there is none.
    pub fn approach(
        &mut self,
        from: &Position,
        to: &Position,
        reach: f32,
    ) -> Result<(NavPath, bool), NoPath> {
        self.toward(from, to, Some(reach))
    }

    fn toward(
        &mut self,
        from: &Position,
        to: &Position,
        reach: Option<f32>,
    ) -> Result<(NavPath, bool), NoPath> {
        self.run_route(match reach {
            Some(r) => Task::approach(*from, *to, r, None),
            None => Task::route(*from, *to, None),
        })
    }

    /// Forget every region in a landblock: its cells were released or changed.
    pub fn forget_landblock(&mut self, landblock: u16) {
        let (x, y) = block_xy(u32::from(landblock));
        self.regions.retain(|r| {
            let inside = r.window().map_or(r.origin == (x, y), |(x0, y0, x1, y1)| {
                (x0..=x1).contains(&x) && (y0..=y1).contains(&y)
            });
            !inside
        });
    }

    /// Take out the places just ahead of a body that stopped at `at` on its way to `toward`, as
    /// far round as the body is wide: walking through there failed, whatever the floor and the
    /// walls said. Routes planned from then on go another way. Answers how many places went out.
    pub fn block_ahead(&mut self, at: &Position, toward: &Position) -> usize {
        let Ok(ri) = self.region_index(at, at) else {
            return 0;
        };
        let radius = self.config.agent.radius;
        let stop = forward_of(at, toward, 0.4);
        let region = &mut self.regions[ri];
        let p = region.to_region(&stop);
        region.repair_around(p, radius)
    }

    /// Put back every place [`Self::block_ahead`] took out: with them out, no route remains.
    pub fn unblock(&mut self) -> usize {
        self.regions.iter_mut().map(Region::restore_repaired).sum()
    }

    /// Add or replace an object in the way.
    pub fn set_obstacle(&mut self, o: Obstacle) {
        self.obstacles.insert(o.id.0, o);
    }

    /// Take an object away.
    pub fn remove_obstacle(&mut self, id: ObjectId) {
        self.obstacles.remove(&id.0);
    }

    /// Record a door's state, if the door is known.
    pub fn set_door_state(&mut self, id: ObjectId, state: DoorState) {
        if let Some(o) = self.obstacles.get_mut(&id.0) {
            o.kind = ObstacleKind::Door(state);
        }
    }

    pub(crate) fn steps(&self) -> Steps {
        Steps {
            max_rise: self.config.max_rise,
            max_drop: self.config.max_drop,
        }
    }

    /// The places each object stands on, with what passing it costs.
    pub(crate) fn gates(&self, region: &Region) -> (HashMap<u32, Gate>, Vec<ObjectId>) {
        let mut gates = HashMap::new();
        let mut ids = Vec::new();
        let agent = &self.config.agent;
        let clear = agent.radius + self.config.margin;
        let heights = agent.test_heights(self.config.step_allowance);
        let s = region.spacing;
        for o in self.obstacles.values() {
            if !region.covers(&o.position) {
                continue;
            }
            let gate = match o.kind {
                ObstacleKind::Door(DoorState::Open) => continue,
                ObstacleKind::Solid | ObstacleKind::Door(DoorState::Locked) => Gate::Blocked,
                ObstacleKind::Creature => Gate::Penalty(self.config.creature_cost),
                ObstacleKind::Door(DoorState::Closed | DoorState::Unknown) => {
                    let idx = u16::try_from(ids.len()).unwrap_or(u16::MAX);
                    ids.push(o.id);
                    Gate::Door(idx, self.config.door_cost)
                }
            };
            let mut frame = o.position.frame;
            frame.origin = region.to_region(&o.position);
            let shapes: Vec<Shape> = object_shapes(&o.setup, &frame, o.scale, FLOOR_Z, |_| {});
            for shape in &shapes {
                let (min, max) = shape.bounds();
                let x0 = floor_to_i32((min.x - clear) / s);
                let x1 = floor_to_i32((max.x + clear) / s) + 1;
                let y0 = floor_to_i32((min.y - clear) / s);
                let y1 = floor_to_i32((max.y + clear) / s) + 1;
                for ix in x0..=x1 {
                    for iy in y0..=y1 {
                        for i in region.column(ix, iy) {
                            let p = region.node_point(i);
                            let hits = heights.iter().any(|h| {
                                shape.dist_sq(Vec3::new(p.x, p.y, p.z + h)) < clear * clear
                            });
                            if hits && gates.get(&i).is_none_or(|g| *g != Gate::Blocked) {
                                gates.insert(i, gate);
                            }
                        }
                    }
                }
            }
        }
        (gates, ids)
    }

    /// Plan a route from `from` to `to`: inside one dungeon, or outdoors across a window of
    /// landblocks and into and out of the buildings in it.
    ///
    /// # Errors
    /// [`NoPath`] says why there is none.
    pub fn plan(&mut self, from: &Position, to: &Position) -> Result<NavPath, NoPath> {
        self.run_route(Task::exact(*from, *to, Want::Route))
            .map(|(p, _)| p)
    }

    /// Plan a route from `from` to `to`, or, where the body cannot stand at `to` or walk there
    /// (a shopkeeper behind a counter, someone in a booth with a window to the street), to the
    /// place nearest it that the body can walk to, within `reach` across and a body's height up
    /// or down. The route's [`NavPath::goal_offset`] says how far short it ends.
    ///
    /// # Errors
    /// [`NoPath`] says why there is none: [`NoPath::NoStandingPlace`] for the goal when no place
    /// within `reach` is clear, [`NoPath::Unreachable`] when none of them is walked to.
    pub fn plan_near(
        &mut self,
        from: &Position,
        to: &Position,
        reach: f32,
    ) -> Result<NavPath, NoPath> {
        self.run_route(Task::exact(*from, *to, Want::Approach(reach)))
            .map(|(p, _)| p)
    }

    /// Plan a route and walk it with `check`, the body's own movement. A leg the body cannot
    /// walk straight is walked through the places the search went through instead; where even
    /// that fails, the places around where the body stopped are taken out and the route is
    /// planned again, up to `attempts` times.
    ///
    /// # Errors
    /// [`NoPath`] when no route remains; the report says what was tried.
    pub fn plan_checked(
        &mut self,
        from: &Position,
        to: &Position,
        check: &mut dyn SegmentCheck,
        attempts: usize,
    ) -> (Result<NavPath, NoPath>, CheckReport) {
        let mut report = CheckReport::default();
        for _ in 0..attempts.max(1) {
            report.plans += 1;
            let mut path = match self.plan(from, to) {
                Ok(p) => p,
                Err(e) => return (Err(e), report),
            };
            match walk_path(&mut path, check, &mut report) {
                Ok(()) => {
                    report.walked = true;
                    return (Ok(path), report);
                }
                Err(stopped) => {
                    let Ok(ri) = self.region_index(from, to) else {
                        return (Err(NoPath::Unreachable), report);
                    };
                    let radius = self.config.agent.radius;
                    let region = &mut self.regions[ri];
                    let at = region.to_region(&stopped);
                    report.repaired_places += region.repair_around(at, radius);
                }
            }
        }
        (Err(NoPath::Unreachable), report)
    }
}

/// Walk every leg; a failed leg is walked again through its places. `Err` carries the place
/// past which the body could not go.
fn walk_path(
    path: &mut NavPath,
    check: &mut dyn SegmentCheck,
    report: &mut CheckReport,
) -> Result<(), Position> {
    let Some(first) = path.waypoints.first().map(|w| w.position) else {
        return Ok(());
    };
    if !check.place(&first) {
        return Err(first);
    }
    let mut i = 0;
    while i + 1 < path.waypoints.len() {
        let target = path.waypoints[i + 1].position;
        let out = check.walk(&target);
        report.legs += 1;
        report.distance += out.distance;
        report.seconds += out.seconds;
        if out.reached {
            i += 1;
            continue;
        }
        report.failed_legs += 1;
        if path.legs[i].len() < 2 {
            // Already one place apart: there is nothing finer to walk.
            return Err(toward(&out.at, &target, 0.4));
        }
        // Walk the leg again through its places, from where it started.
        report.refined_legs += 1;
        if !check.place(&path.waypoints[i].position) {
            return Err(path.waypoints[i].position);
        }
        path.densify_leg(i);
    }
    Ok(())
}

/// The position `d` across from `a` toward `b`, at `a`'s height, in `a`'s cell.
fn forward_of(a: &Position, b: &Position, d: f32) -> Position {
    let (ga, gb) = (global(a), global(b));
    let (dx, dy) = (gb.x - ga.x, gb.y - ga.y);
    let l = (dx * dx + dy * dy).sqrt();
    let mut out = *a;
    if l > 1e-4 {
        out.frame.origin.x += dx * d / l;
        out.frame.origin.y += dy * d / l;
    }
    out
}

/// The position `d` along the line from `a` to `b` (in `a`'s cell), or `b` if it is nearer.
fn toward(a: &Position, b: &Position, d: f32) -> Position {
    let (ga, gb) = (global(a), global(b));
    let l = dist(ga, gb);
    if l <= d || l <= 0.0 {
        return *b;
    }
    let v = sub(gb, ga);
    let mut out = *a;
    out.frame.origin = Vec3::new(
        a.frame.origin.x + v.x * d / l,
        a.frame.origin.y + v.y * d / l,
        a.frame.origin.z + v.z * d / l,
    );
    out
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_path(
    region: &Region,
    from: &Position,
    to: &Position,
    chain: &[u32],
    kept: &[u32],
    gates: &Gates,
    door_ids: &[ObjectId],
    steps: Steps,
) -> NavPath {
    let gate = |p: u32| gates.of(p);
    let mut waypoints = Vec::with_capacity(kept.len() + 1);
    let mut legs: Vec<Vec<Position>> = Vec::with_capacity(kept.len());
    let start = region.position(kept[0]);
    let mut from_body = *from;
    from_body.frame.rotation = Quat::IDENTITY;
    let start_apart = global_distance(from, &start) > region.spacing;
    waypoints.push(Waypoint {
        position: if start_apart { from_body } else { start },
        action: None,
    });
    if start_apart {
        waypoints.push(Waypoint {
            position: start,
            action: None,
        });
        legs.push(vec![start]);
    }
    let mut c = 0;
    for &k in &kept[1..] {
        let mut leg = Vec::new();
        while c + 1 < chain.len() {
            c += 1;
            leg.push(region.position(chain[c]));
            if chain[c] == k {
                break;
            }
        }
        // A waypoint followed by a door place: open the door here.
        if let (Gate::Free, Gate::Door(d, _)) = (gate(*chain_before(chain, c, &leg)), gate(k)) {
            if let Some(last) = waypoints.last_mut() {
                last.action = door_ids
                    .get(usize::from(d))
                    .map(|id| WaypointAction::OpenDoor(*id));
            }
        }
        waypoints.push(Waypoint {
            position: region.position(k),
            action: None,
        });
        legs.push(leg);
    }
    let goal_offset = waypoints
        .last()
        .map_or(0.0, |w| global_distance(&w.position, to));
    // How far each corner may be cut across: none at a door, nor through one.
    let points: Vec<Vec3> = waypoints
        .iter()
        .map(|w| region.to_region(&w.position))
        .collect();
    let mut cuts = vec![0.0; waypoints.len()];
    for i in 1..waypoints.len().saturating_sub(1) {
        if waypoints[i].action.is_none() {
            cuts[i] = region.corner_cut(points[i - 1], points[i], points[i + 1], steps, gates);
        }
    }
    NavPath {
        waypoints,
        legs,
        goal_offset,
        cuts,
    }
}

/// The place the chain stood on just before the leg that ends at index `c`.
fn chain_before<'a>(chain: &'a [u32], c: usize, leg: &[Position]) -> &'a u32 {
    let back = leg.len();
    &chain[c.saturating_sub(back)]
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks the approach and a dungeon's parts over made-up dungeon rooms).
    use super::*;
    use crate::journey::{plan_journey, JourneyConfig, Leg, Realm};
    use crate::path::End;
    use crate::portals::{EdgeSource, PortalEdge};
    use dereth_physics::source::{CellPortal, EnvCellGeometry};
    use dereth_physics::Polygon;
    use dereth_primitives::{DataId, Frame};

    /// Rooms, their floors and walls.
    struct Rooms(Vec<Arc<EnvCellGeometry>>);

    impl NavSource for Rooms {
        fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>> {
            self.0.iter().find(|c| c.id == cell).cloned()
        }
        fn setup(&self, _id: DataId) -> Option<Arc<SetupGeometry>> {
            None
        }
    }

    const CELL: CellId = CellId(0x0001_0100);

    /// A floor from `(x0, y0)` to `(x1, y1)`.
    fn floor(x0: f32, y0: f32, x1: f32, y1: f32) -> Polygon {
        Polygon::new(vec![
            Vec3::new(x0, y0, 0.0),
            Vec3::new(x1, y0, 0.0),
            Vec3::new(x1, y1, 0.0),
            Vec3::new(x0, y1, 0.0),
        ])
    }

    /// A wall from `(x0, y0)` to `(x1, y1)`, 2.5 m high.
    fn wall(x0: f32, y0: f32, x1: f32, y1: f32) -> Polygon {
        Polygon::new(vec![
            Vec3::new(x0, y0, 0.0),
            Vec3::new(x1, y1, 0.0),
            Vec3::new(x1, y1, 2.5),
            Vec3::new(x0, y0, 2.5),
        ])
    }

    /// A room 12 m by 6 m with a booth in its corner, `x` 8 to 12 and `y` 0 to 3, walled on its two
    /// inner sides; the wall along `y` 3 runs from `x` 8 to `gap`, open from there to the room's
    /// edge.
    fn room_with_booth(gap: f32) -> NavWorld<Rooms> {
        let cell = EnvCellGeometry {
            id: CELL,
            frame: Frame::default(),
            physics_polygons: vec![
                floor(0.0, 0.0, 12.0, 6.0),
                wall(8.0, 0.0, 8.0, 3.0),
                wall(8.0, 3.0, gap, 3.0),
            ],
            ..EnvCellGeometry::default()
        };
        NavWorld::new(Rooms(vec![Arc::new(cell)]), NavConfig::default())
    }

    fn at(x: f32, y: f32) -> Position {
        in_cell(CELL, x, y)
    }

    fn in_cell(cell: CellId, x: f32, y: f32) -> Position {
        Position::new(cell, Frame::new(Vec3::new(x, y, 0.0), Quat::IDENTITY))
    }

    fn end(p: &NavPath) -> Vec3 {
        p.waypoints
            .last()
            .expect("a route has an end")
            .position
            .frame
            .origin
    }

    #[test]
    fn an_approach_to_a_walled_in_place_ends_at_the_nearest_place_walked_to_within_reach() {
        let mut nav = room_with_booth(12.0);
        let (from, inside) = (at(2.0, 2.0), at(10.0, 1.5));
        assert_eq!(
            nav.plan(&from, &inside),
            Err(NoPath::Unreachable),
            "premise: the booth is walled in"
        );
        let route = nav.plan_near(&from, &inside, 2.5).expect("a route near it");
        let e = end(&route);
        assert!(e.y > 3.0, "the route ends outside the booth's wall: {e:?}");
        assert!(
            (e.x - 10.0).abs() < 0.3,
            "right across the wall from it: {e:?}"
        );
        assert!(
            route.goal_offset > 2.0 && route.goal_offset <= 2.5,
            "{}",
            route.goal_offset
        );
        // With nothing outside the booth within reach, there is no route.
        assert_eq!(nav.plan_near(&from, &inside, 1.0), Err(NoPath::Unreachable));
    }

    #[test]
    fn an_approach_goes_to_the_place_itself_through_a_way_in_even_round_the_far_side() {
        // The booth opens at its far corner: the place inside is reached round the wall, a longer
        // walk than to the wall's near side.
        let mut nav = room_with_booth(10.5);
        let (from, inside) = (at(2.0, 2.0), at(9.0, 1.5));
        let route = nav.plan_near(&from, &inside, 2.5).expect("a route in");
        let e = end(&route);
        assert!(
            e.y < 3.0 && e.x > 8.0,
            "the route ends inside the booth: {e:?}"
        );
        assert!(route.goal_offset < 0.3, "{}", route.goal_offset);
        assert!(
            route
                .waypoints
                .iter()
                .any(|w| w.position.frame.origin.x > 10.5),
            "through the opening"
        );
    }

    #[test]
    fn an_approach_with_no_clear_place_within_reach_has_no_standing_place() {
        let mut nav = room_with_booth(12.0);
        let off_the_floor =
            Position::new(CELL, Frame::new(Vec3::new(30.0, 30.0, 0.0), Quat::IDENTITY));
        assert_eq!(
            nav.plan_near(&at(2.0, 2.0), &off_the_floor, 2.5),
            Err(NoPath::NoStandingPlace(End::Goal))
        );
    }

    /// A dungeon of three rooms: the first and second joined by a doorway, the third apart from
    /// both, though only a wall's width from the first.
    fn dungeon_of_two_parts() -> NavWorld<Rooms> {
        let (a, b, c) = (CELL, CellId(0x0001_0101), CellId(0x0001_0102));
        let doorway = |to: CellId| CellPortal {
            other_cell_id: to.0,
            portal: wall(12.0, 0.0, 12.0, 6.0),
            portal_side: true,
            other_portal_id: 0,
            exact_match: false,
        };
        let room = |id: CellId, floor: Polygon, portals: Vec<CellPortal>| {
            Arc::new(EnvCellGeometry {
                id,
                frame: Frame::default(),
                physics_polygons: vec![floor],
                portals,
                ..EnvCellGeometry::default()
            })
        };
        NavWorld::new(
            Rooms(vec![
                room(a, floor(0.0, 0.0, 12.0, 6.0), vec![doorway(b)]),
                room(b, floor(12.0, 0.0, 24.0, 6.0), vec![doorway(a)]),
                room(c, floor(0.0, 7.0, 12.0, 13.0), Vec::new()),
            ]),
            NavConfig::default(),
        )
    }

    #[test]
    fn a_dungeon_is_as_many_realms_as_parts_its_cells_join_and_a_journey_between_them_takes_its_portal(
    ) {
        let nav = dungeon_of_two_parts();
        let (a, b, c) = (CELL, CellId(0x0001_0101), CellId(0x0001_0102));
        let realm = |p: &Position| nav.realm(p);
        assert_eq!(realm(&in_cell(a, 2.0, 2.0)), Some(Realm::Part(a)));
        assert_eq!(
            realm(&in_cell(b, 20.0, 2.0)),
            Some(Realm::Part(a)),
            "joined"
        );
        assert_eq!(realm(&in_cell(c, 2.0, 10.0)), Some(Realm::Part(c)), "apart");
        // A portal in the first room sends to the third: the journey there walks to it and takes
        // it, though the third room is only a few metres from the start as the crow flies.
        let portal = PortalEdge {
            source: EdgeSource::Learned,
            wcid: Some(1),
            name: String::from("Portal"),
            from: in_cell(b, 22.0, 3.0),
            to: Some(in_cell(c, 10.0, 10.0)),
            verified: 0,
            uses: 0,
            failures: 0,
            levels: (None, None),
            retired: None,
        };
        let j = plan_journey(
            &in_cell(a, 2.0, 5.0),
            &in_cell(c, 2.0, 8.0),
            &[&portal],
            &[],
            &realm,
            &JourneyConfig::default(),
        )
        .expect("a journey");
        assert!(
            matches!(
                j.legs.as_slice(),
                [Leg::Walk { .. }, Leg::Portal { .. }, Leg::Walk { .. }]
            ),
            "{:?}",
            j.legs
        );
    }

    /// A floor from `(x0, y0)` to `(x1, y1)`, rising from `z0` along `x0` to `z1` along `x1`.
    fn slope(x0: f32, y0: f32, x1: f32, y1: f32, z0: f32, z1: f32) -> Polygon {
        Polygon::new(vec![
            Vec3::new(x0, y0, z0),
            Vec3::new(x1, y0, z1),
            Vec3::new(x1, y1, z1),
            Vec3::new(x0, y1, z0),
        ])
    }

    /// A low floor 12 m by 10 m and beside it a high one a metre up, 8 m by 6 m; with `ramp`, a
    /// ramp beside the high floor rising from the low floor's level to the high floor's.
    fn ledge(ramp: bool) -> NavWorld<Rooms> {
        let mut floors = vec![
            slope(0.0, 0.0, 12.0, 10.0, 0.0, 0.0),
            slope(12.0, 0.0, 20.0, 6.0, 1.0, 1.0),
        ];
        if ramp {
            floors.push(slope(12.0, 6.0, 20.0, 10.0, 0.0, 1.0));
        }
        let cell = EnvCellGeometry {
            id: CELL,
            frame: Frame::default(),
            physics_polygons: floors,
            ..EnvCellGeometry::default()
        };
        NavWorld::new(Rooms(vec![Arc::new(cell)]), NavConfig::default())
    }

    #[test]
    fn a_ledge_higher_than_a_step_parts_the_patches_and_the_route_climbs_the_ramp_beside_it() {
        let low = at(2.0, 5.0);
        let high = Position::new(CELL, Frame::new(Vec3::new(18.0, 3.0, 1.0), Quat::IDENTITY));
        let mut nav = ledge(false);
        assert_eq!(
            nav.plan(&low, &high),
            Err(NoPath::Unreachable),
            "no way up the ledge"
        );
        let mut nav = ledge(true);
        let route = nav.plan(&low, &high).expect("a route up the ramp");
        assert!(
            route.legs.iter().flatten().any(|p| p.frame.origin.y > 6.0),
            "by the ramp: {:?}",
            route.waypoints
        );
        let region = nav.region(CELL).expect("built");
        let steps = nav_steps();
        let mut places = 0;
        for i in region.places().filter(|&i| region.is_clear(i)) {
            places += 1;
            let p = region.patch_of(i).expect("every clear place is in a patch");
            let (ix, iy, z) = region.column_and_z(i);
            assert_eq!(region.patch_place(p, ix, iy), Some(i));
            // Its neighbours in the patch are a step from it, up or down.
            for (dx, dy) in [(1, 0), (0, 1)] {
                if let Some(j) = region.patch_place(p, ix + dx, iy + dy) {
                    let dz = region.column_and_z(j).2 - z;
                    assert!(dz.abs() <= steps, "{dz} between neighbours of a patch");
                }
            }
        }
        assert!(
            region.patches.len() * 50 <= places,
            "{} patches for {places} places",
            region.patches.len()
        );
    }

    /// The step a patch's neighbouring places may differ by.
    fn nav_steps() -> f32 {
        let c = NavConfig::default();
        c.max_rise.min(c.max_drop)
    }
}
