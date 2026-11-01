//! The world's walls, floors and solid things as creatures meet them: the shared physics' land and
//! interior cells, read a landblock at a time the first time a creature moves in it, and the
//! transition the client moves bodies with.
//!
//! - **A step** moves a creature's body (its setup's spheres and step heights, at its scale) along
//!   the ground through the transition: it slides along walls and what stands in the world, climbs
//!   and goes down what its step heights allow, and takes the cell it ends in. A step that would
//!   leave the ground is not taken.
//! - **A way** to a goal is searched over a meter's lattice of the ground, each link a step the
//!   transition lets the body take (A*, the straight line its estimate). Inside, a goal in no cell
//!   joined to the creature's by doorways is out of reach at once.
//!
//! The host reads the blocks ([`Blocks`]); the walls hold what was read for as long as the world
//! lives.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use auberean_physics::body::heading_rotation;
use auberean_physics::interior::add_block_cells;
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;
use auberean_physics::world::World;
use auberean_physics::{Body, BodyConfig};
use auberean_world::records::{RegionSet, WorldRecords};
use dereth_physics::geom::Sphere;
use dereth_primitives::num::math;
use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};
use yalain_store::StoredPosition;

/// Reads a landblock into the land: its ground and the solid objects placed on it, and its
/// interior cells with what stands in them.
pub trait Blocks {
    /// Read block `(x, y)` into `land`; false when the world has no such block.
    fn load(&mut self, land: &Land, x: u8, y: u8) -> bool;
}

/// The blocks as a world's records hold them: the ground and the solid objects placed on it, and
/// the interior cells (each mesh's tree read once).
pub struct RecordBlocks {
    records: WorldRecords,
    region: RegionSet,
    trees: Trees,
    /// The blocks the world had, and the interior cells read.
    pub blocks: usize,
    pub cells: usize,
}

impl std::fmt::Debug for RecordBlocks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RecordBlocks({} blocks, {} cells)",
            self.blocks, self.cells
        )
    }
}

impl RecordBlocks {
    #[must_use]
    pub fn new(records: WorldRecords, region: RegionSet) -> Self {
        Self {
            records,
            region,
            trees: Trees::default(),
            blocks: 0,
            cells: 0,
        }
    }

    /// The records, for what else the host reads of them.
    #[must_use]
    pub fn records(&self) -> &WorldRecords {
        &self.records
    }
}

impl Blocks for RecordBlocks {
    fn load(&mut self, land: &Land, x: u8, y: u8) -> bool {
        let ground = land.block(x, y).is_some()
            || land.add_block(&self.records, &self.region, &mut self.trees, x, y);
        let cells = add_block_cells(land, &self.records, &mut self.trees, x, y);
        self.blocks += usize::from(ground);
        self.cells += cells;
        ground || cells > 0
    }
}

/// A body as the walls meet it: its setup's collision spheres and step heights, at its look's
/// scale.
#[derive(Debug, Clone, PartialEq)]
pub struct Walker {
    /// Center (x, y, z) and radius of each sphere, unscaled.
    pub spheres: Vec<[f32; 4]>,
    pub step_up: f32,
    pub step_down: f32,
    pub scale: f32,
}

impl Walker {
    /// A body of a radius and a height (already scaled) when its setup is not known: a sphere at
    /// its feet and one at its head.
    #[must_use]
    pub fn of_size(radius: f32, height: f32) -> Self {
        let r = radius.max(0.1);
        let top = (height - r).max(r);
        Self {
            spheres: vec![[0.0, 0.0, r, r], [0.0, 0.0, top, r]],
            step_up: (height * 0.25).max(0.2),
            step_down: (height * 0.25).max(0.2),
            scale: 1.0,
        }
    }
}

/// The body a look gives: its setup's first two collision spheres and its step heights, at the
/// look's scale; none for a look with no setup or a setup with no spheres.
#[must_use]
pub fn walker(records: &WorldRecords, look: u32) -> Option<Walker> {
    let visual = |did: u32| records.get::<auberean_assets::visual::VisualDesc>(did);
    let (setup, scale) = yalain_content::look_setup(look, &visual)?;
    let s: auberean_assets::setup::Setup = records.get(setup)?;
    let spheres: Vec<[f32; 4]> = s
        .spheres
        .iter()
        .take(2)
        .map(|p| [p.center.x, p.center.y, p.center.z, p.radius])
        .collect();
    (!spheres.is_empty()).then_some(Walker {
        spheres,
        step_up: s.step_up_height,
        step_down: s.step_down_height,
        scale,
    })
}

/// The body every creature with no walker of its own is given: a human's size.
const ANY_WALKER: (f32, f32) = (0.4, 1.8);

/// The spacing of the lattice a way is searched over, in meters.
const LATTICE: f32 = 1.0;

/// The most lattice points a search of a way takes before it gives up, and the longest way it
/// takes, as a multiple of the straight way and meters more.
pub const MOST_POINTS: usize = 400;
pub const LONGEST_WAY: (f32, f32) = (3.0, 5.0);

/// How near a lattice point a step must end to reach it, in meters.
const ON_POINT: f32 = 0.3;

/// A node of a block's graph of cells: an interior cell's low word, or the outdoors.
const OUTSIDE: u16 = 0;

/// A block's cells and their doorways: for each cell (and the outdoors), the cells beside it.
#[derive(Debug, Default)]
struct Graph {
    edges: BTreeMap<u16, Vec<u16>>,
}

/// What a step did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stepped {
    /// Where the body ended (its cell, its point; the rotation is the step's heading).
    pub at: StoredPosition,
    /// How far it got across the ground, in meters.
    pub moved: f32,
}

/// The walls the creatures meet.
pub struct Walls {
    land: Arc<Land>,
    world: World,
    blocks: Box<dyn Blocks>,
    /// The blocks read (whether or not the world had them).
    read: BTreeSet<(u8, u8)>,
    graphs: HashMap<u32, Graph>,
    /// Each template's body, and the bodies stepped (by template and scale).
    pub walkers: HashMap<u32, Walker>,
    bodies: HashMap<(u32, u32), Body>,
}

impl std::fmt::Debug for Walls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Walls({} blocks read, {} walkers, {} bodies)",
            self.read.len(),
            self.walkers.len(),
            self.bodies.len()
        )
    }
}

fn block_of(cell: u32) -> (u8, u8) {
    let [x, y, _, _] = cell.to_be_bytes();
    (x, y)
}

fn indoors(cell: u32) -> bool {
    cell & 0xFFFF >= 0x100
}

fn position(at: &StoredPosition, heading: f32) -> Position {
    Position::new(
        CellId(at.cell),
        Frame::new(
            Vec3::new(at.origin[0], at.origin[1], at.origin[2]),
            heading_rotation(heading),
        ),
    )
}

fn stored(p: &Position) -> StoredPosition {
    let q: Quat = p.frame.rotation;
    StoredPosition {
        cell: p.cell.0,
        origin: [p.frame.origin.x, p.frame.origin.y, p.frame.origin.z],
        rotation: [q.w, q.x, q.y, q.z],
    }
}

impl Walls {
    /// Walls over `land`, its objects in `world` (with the install's physics types), each block
    /// read by `blocks` when first needed.
    #[must_use]
    pub fn new(land: Arc<Land>, world: World, blocks: Box<dyn Blocks>) -> Self {
        Self {
            land,
            world,
            blocks,
            read: BTreeSet::new(),
            graphs: HashMap::new(),
            walkers: HashMap::new(),
            bodies: HashMap::new(),
        }
    }

    /// How many blocks have been read.
    #[must_use]
    pub fn blocks_read(&self) -> usize {
        self.read.len()
    }

    /// The land the walls stand on.
    #[must_use]
    pub fn land(&self) -> &Land {
        &self.land
    }

    fn read_block(&mut self, x: u8, y: u8) {
        if !self.read.insert((x, y)) {
            return;
        }
        self.blocks.load(&self.land, x, y);
        let block = u32::from(x) << 24 | u32::from(y) << 16;
        let graph = self.graph_of(block);
        self.graphs.insert(block, graph);
    }

    /// Read what a place needs: its block, and around it for a place outdoors or a cell seen from
    /// outside.
    fn need(&mut self, cell: u32) {
        let (x, y) = block_of(cell);
        self.read_block(x, y);
        let outside =
            !indoors(cell) || self.land.cell(CellId(cell)).is_some_and(|g| g.seen_outside);
        if outside {
            for dx in -1i16..=1 {
                for dy in -1i16..=1 {
                    if let (Ok(bx), Ok(by)) = (
                        u8::try_from(i16::from(x) + dx),
                        u8::try_from(i16::from(y) + dy),
                    ) {
                        self.read_block(bx, by);
                    }
                }
            }
        }
    }

    /// The doorways of a block's interior cells.
    fn graph_of(&self, block: u32) -> Graph {
        let mut g = Graph::default();
        for id in self.land.interior_cells_of(block) {
            let Some(cell) = self.land.cell(id) else {
                continue;
            };
            let low = u16::try_from(id.0 & 0xFFFF).unwrap_or(OUTSIDE);
            for p in &cell.portals {
                let other = if p.other_cell_id == auberean_physics::cells::OUTDOORS {
                    OUTSIDE
                } else {
                    u16::try_from(p.other_cell_id & 0xFFFF).unwrap_or(OUTSIDE)
                };
                g.edges.entry(low).or_default().push(other);
                g.edges.entry(other).or_default().push(low);
            }
        }
        g
    }

    /// Whether doorways join the cells of two places (the outdoors counting as one cell): none
    /// join an interior cell of one block to another block's, and two places outdoors are joined.
    pub fn joined(&mut self, from: &StoredPosition, goal: &StoredPosition) -> bool {
        self.need(from.cell);
        let block = from.cell & 0xFFFF_0000;
        let node = |cell: u32| -> Option<u16> {
            if !indoors(cell) {
                Some(OUTSIDE)
            } else if cell & 0xFFFF_0000 == block {
                u16::try_from(cell & 0xFFFF).ok()
            } else {
                None
            }
        };
        let (Some(start), Some(end)) = (node(from.cell), node(goal.cell)) else {
            return false;
        };
        if start == end {
            return true;
        }
        let Some(graph) = self.graphs.get(&block) else {
            return true;
        };
        let mut queue = VecDeque::from([start]);
        let mut seen = BTreeSet::from([start]);
        while let Some(n) = queue.pop_front() {
            if n == end {
                return true;
            }
            for next in graph.edges.get(&n).into_iter().flatten() {
                if seen.insert(*next) {
                    queue.push_back(*next);
                }
            }
        }
        false
    }

    /// Move the body of a creature of `template` and `scale` from `from` by `(dx, dy)` along the
    /// ground through the transition; none when the walls cannot place it there (its cell was
    /// never read). A step that would leave the ground stays where it was.
    pub fn step(
        &mut self,
        template: u32,
        scale: f32,
        from: &StoredPosition,
        d: [f32; 2],
    ) -> Option<Stepped> {
        self.need(from.cell);
        let (x, y) = block_of(from.cell);
        let known = if indoors(from.cell) {
            self.land.cell(CellId(from.cell)).is_some()
        } else {
            self.land.block(x, y).is_some()
        };
        if !known {
            return None;
        }
        let heading = math::atan2f(d[0], d[1]).to_degrees().rem_euclid(360.0);
        let key = (template, scale.to_bits());
        let walker = self.walkers.get(&template).cloned();
        let body = self.bodies.entry(key).or_insert_with(|| {
            let w = walker.unwrap_or_else(|| Walker::of_size(ANY_WALKER.0, ANY_WALKER.1));
            let config = BodyConfig {
                spheres: w
                    .spheres
                    .iter()
                    .map(|s| Sphere::new(Vec3::new(s[0], s[1], s[2]), s[3]))
                    .collect(),
                step_up: w.step_up,
                step_down: w.step_down,
                scale: w.scale * scale,
                ground: auberean_assets::animation::MotionParams::default(),
                air: auberean_assets::animation::MotionParams::default(),
            };
            let mut b = Body::new(config, position(from, heading), heading);
            b.is_player = false;
            b
        });
        let start = position(from, heading);
        let go = Vec3::new(d[0], d[1], 0.0);
        let mut took = take_step(body, &self.land, &mut self.world, start, go, heading);
        if took.is_none() {
            // A body that cannot set out from where it is (it stands a little into the floor or
            // what stands on it) is first put where it fits nearest.
            body.place(start, heading);
            let ok = body.set_position_simple(&self.land, &mut self.world, start, true);
            if ok == auberean_physics::body::placing::error::OK {
                let fitted = body.position;
                took = take_step(body, &self.land, &mut self.world, fitted, go, heading);
            }
        }
        let end = took.unwrap_or(start);
        let moved = flat(&start, &end);
        let mut at = stored(&end);
        at.rotation = stored(&start).rotation;
        Some(Stepped { at, moved })
    }
}

impl Walls {
    /// Where a body walking straight from `from` toward `goal` a lattice spacing at a time ends,
    /// when it gets within `close` of it on the ground without being held up.
    fn straight(
        &mut self,
        template: u32,
        scale: f32,
        from: &StoredPosition,
        goal: &StoredPosition,
        close: f32,
    ) -> Option<StoredPosition> {
        let g = position(goal, 0.0);
        let mut at = *from;
        for _ in 0..64 {
            let o = auberean_physics::geom::offset(&position(&at, 0.0), &g);
            let left = (o.x * o.x + o.y * o.y).sqrt();
            if left <= close {
                return Some(at);
            }
            let go = left.min(LATTICE);
            let s = self.step(template, scale, &at, [o.x / left * go, o.y / left * go])?;
            if s.moved < go * 0.9 {
                return None;
            }
            at = s.at;
        }
        None
    }
}

/// A lattice point: its x and y in lattice spacings, its height in two-meter bands.
type Point = (i32, i32, i32);

/// A lattice point a search reached: where the body stood there, the way's length so far, and the
/// point it came from.
struct Reached {
    at: StoredPosition,
    cost: f32,
    from: Option<Point>,
}

impl Walls {
    /// A way for a creature of `template` and `scale` from `from` to within `close` meters of
    /// `goal` (on the ground): the places to walk through in turn, the last within `close` of the
    /// goal; none when the cells are not joined or the search finds no way within
    /// [`MOST_POINTS`] lattice points. Each link of the way is a step the walls let the body take.
    pub fn way(
        &mut self,
        template: u32,
        scale: f32,
        from: &StoredPosition,
        goal: &StoredPosition,
        close: f32,
    ) -> Option<Vec<StoredPosition>> {
        if !self.joined(from, goal) {
            return None;
        }
        if let Some(end) = self.straight(template, scale, from, goal, close) {
            return Some(vec![end]);
        }
        let origin = position(from, 0.0);
        // A place's point in the frame of `from`'s block.
        let local = |p: &StoredPosition| {
            let o = auberean_physics::geom::offset(&origin, &position(p, 0.0));
            [
                from.origin[0] + o.x,
                from.origin[1] + o.y,
                from.origin[2] + o.z,
            ]
        };
        let g = local(goal);
        let gap = |p: [f32; 3]| ((p[0] - g[0]).powi(2) + (p[1] - g[1]).powi(2)).sqrt();
        let key = |p: [f32; 3]| {
            (
                dereth_primitives::num::floor_to_i32(p[0] / LATTICE + 0.5),
                dereth_primitives::num::floor_to_i32(p[1] / LATTICE + 0.5),
                dereth_primitives::num::floor_to_i32(p[2] / 2.0 + 0.5),
            )
        };
        let start = key(from.origin);
        let longest = LONGEST_WAY.0 * gap(from.origin) + LONGEST_WAY.1;
        let mut reached: HashMap<Point, Reached> = HashMap::new();
        reached.insert(
            start,
            Reached {
                at: *from,
                cost: 0.0,
                from: None,
            },
        );
        // The open points by cost plus estimate (in millimeters, for ordering).
        let mut open: std::collections::BinaryHeap<std::cmp::Reverse<(u64, Point)>> =
            std::collections::BinaryHeap::new();
        let mm = |v: f32| -> u64 {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let m = (v.max(0.0) * 1000.0) as u64;
            m
        };
        open.push(std::cmp::Reverse((mm(gap(from.origin)), start)));
        let mut closed: BTreeSet<Point> = BTreeSet::new();
        let mut end = None;
        while let Some(std::cmp::Reverse((_, k))) = open.pop() {
            if !closed.insert(k) {
                continue;
            }
            if closed.len() > MOST_POINTS {
                break;
            }
            let (here, cost) = {
                let r = &reached[&k];
                (r.at, r.cost)
            };
            let p = local(&here);
            let left = gap(p);
            if left <= close {
                end = Some(k);
                break;
            }
            // Near the goal, a step straight to it.
            if left <= LATTICE * 1.5 {
                let d = [g[0] - p[0], g[1] - p[1]];
                if let Some(s) = self.step(template, scale, &here, d) {
                    if gap(local(&s.at)) <= close {
                        let gk = (i32::MIN, i32::MIN, i32::MIN);
                        reached.insert(
                            gk,
                            Reached {
                                at: s.at,
                                cost: cost + left,
                                from: Some(k),
                            },
                        );
                        end = Some(gk);
                        break;
                    }
                }
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
                #[allow(clippy::cast_precision_loss)]
                let target = [(k.0 + dx) as f32 * LATTICE, (k.1 + dy) as f32 * LATTICE];
                let d = [target[0] - p[0], target[1] - p[1]];
                let length = (d[0] * d[0] + d[1] * d[1]).sqrt();
                let Some(s) = self.step(template, scale, &here, d) else {
                    continue;
                };
                let q = local(&s.at);
                if ((q[0] - target[0]).powi(2) + (q[1] - target[1]).powi(2)).sqrt() > ON_POINT {
                    continue;
                }
                let nk = key(q);
                if closed.contains(&nk) {
                    continue;
                }
                let c = cost + length;
                if c + gap(q) > longest || reached.get(&nk).is_some_and(|r| r.cost <= c) {
                    continue;
                }
                reached.insert(
                    nk,
                    Reached {
                        at: s.at,
                        cost: c,
                        from: Some(k),
                    },
                );
                open.push(std::cmp::Reverse((mm(c + gap(q)), nk)));
            }
        }
        let mut k = end?;
        let mut way = Vec::new();
        while let Some(r) = reached.get(&k) {
            way.push(r.at);
            match r.from {
                Some(f) => k = f,
                None => break,
            }
        }
        way.pop();
        way.reverse();
        Some(way)
    }
}

/// The ground distance between two places of one block, or of neighbors.
fn flat(a: &Position, b: &Position) -> f32 {
    let o = auberean_physics::geom::offset(a, b);
    (o.x * o.x + o.y * o.y).sqrt()
}

/// One move of `body` from `start` by `d`; none when it went nowhere or left the ground.
fn take_step(
    body: &mut Body,
    land: &Land,
    world: &mut World,
    start: Position,
    d: Vec3,
    heading: f32,
) -> Option<Position> {
    body.place(start, heading);
    body.velocity = Vec3::ZERO;
    body.contact = true;
    body.on_walkable = true;
    body.contact_plane = None;
    body.sliding_normal = None;
    body.move_through(land, world, d, heading);
    let end = body.position;
    if !body.contact || flat(&start, &end) < 1e-4 {
        return None;
    }
    Some(end)
}
