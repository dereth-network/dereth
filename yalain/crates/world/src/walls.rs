//! The world's walls, floors and solid things as creatures meet them: the shared physics' land and
//! interior cells, read a landblock at a time the first time a creature moves in it, and the
//! transition the client moves bodies with.
//!
//! - **A step** moves a creature's body (its setup's spheres and step heights, at its scale) along
//!   the ground through the transition, a short leg at a time as a physics tick would: it slides
//!   along walls and what stands in the world, climbs and goes down what its step heights allow,
//!   and takes the cell it ends in. It stops at the leg that gets nowhere or would leave the ground,
//!   so a creature held up stands or slides and is never put back. Only a body that can move no
//!   way at all, which stands in something, is put where it fits nearest.
//! - **A way** to a goal is searched over a meter's lattice of the ground, each link a step the
//!   transition lets the body take (A*, the straight line its estimate). Inside, a goal in no cell
//!   joined to the creature's by doorways is out of reach at once.
//! - **Searches share a budget.** A pass of the world spends at most [`PASS_MOVES`] moves on
//!   searches, however many creatures are held up. A search the pass cannot finish goes on where it
//!   stopped at a later one: the searches waiting take turns, and a new one goes behind them, so
//!   each is answered in the end.
//!
//! - **What a world places** stands among the walls as it stands in a client's world: its look's
//!   shape at its own scale, solid or not as its description says ([`Standing`]). What moves, comes
//!   and goes or is seen by some players only is left out: a door is met where it stands
//!   ([`crate::usable::doors`]).
//!
//! The host reads the blocks ([`Blocks`]); the walls hold what was read for as long as the world
//! lives.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use auberean_physics::body::heading_rotation;
use auberean_physics::collision::manager::BodyKey;
use auberean_physics::interior::add_block_cells;
use auberean_physics::land::Land;
use auberean_physics::mesh::Trees;
use auberean_physics::object::Shape;
use auberean_physics::solid::Ethereality;
use auberean_physics::world::{World, WorldObject};
use auberean_physics::{Body, BodyConfig};
use auberean_primitives::DataId;
use auberean_world::look::LookRecords as _;
use auberean_world::place::PlaceRecords as _;
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

    /// Put what a world places in `world`, where the land is read; none when it has no shape.
    fn stand(&mut self, land: &Land, world: &mut World, row: &Standing) -> Option<BodyKey> {
        let _ = (land, world, row);
        None
    }
}

/// A thing a world places, as creatures meet it.
#[derive(Debug, Clone, PartialEq)]
pub struct Standing {
    /// Its id in the world.
    pub id: u64,
    /// The entity it is made from, whose properties it takes.
    pub template: u32,
    /// Its look, and its own scale.
    pub look: u32,
    pub scale: f32,
    pub at: StoredPosition,
    /// The numbers its description gives.
    pub ethereality: Ethereality,
}

impl Standing {
    fn position(&self) -> Position {
        let [w, x, y, z] = self.at.rotation;
        let [ox, oy, oz] = self.at.origin;
        Position::new(
            CellId(self.at.cell),
            Frame::new(Vec3::new(ox, oy, oz), Quat { w, x, y, z }),
        )
    }

    /// The block whose reading puts it in the world.
    fn block(&self) -> (u8, u8) {
        block_of(self.at.cell)
    }
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

    fn stand(&mut self, land: &Land, world: &mut World, row: &Standing) -> Option<BodyKey> {
        let shape = Shape::of(&self.records, DataId(row.look), &mut self.trees)?;
        let whole = Vec3::new(
            shape.scale.x * row.scale,
            shape.scale.y * row.scale,
            shape.scale.z * row.scale,
        );
        let at = row.position();
        let key = world.insert(
            land,
            row.id,
            &WorldObject {
                shape: Arc::new(shape),
                position: at,
                is_static: true,
                passable: false,
            },
        );
        world.set_scale(key, whole);
        let lookup = |id: u32| self.records.entity(DataId(id));
        world.apply_entity_properties(key, row.template, &lookup);
        // The description has the last word on what it is, whatever its entities' properties said.
        world.set_ethereality(key, row.ethereality);
        // Filed again by the box of its whole size.
        world.place(land, key, at);
        Some(key)
    }
}

/// What a thing's description says it is and passes through.
#[must_use]
pub fn ethereality_of(description: &yalain_content::Description) -> Ethereality {
    let w = crate::player::weenie_with_numbers(
        auberean_protocol::types::WeenieDesc::default(),
        description.numbers,
    );
    Ethereality::of_numbers(|bit| {
        auberean_protocol::types::WeenieDesc::NUMBER_BITS
            .iter()
            .position(|b| *b == bit)
            .filter(|_| w.mask & bit != 0)
            .map(|i| w.numbers[i])
    })
}

/// A body as the walls meet it: its setup's collision spheres and step heights, at its look's
/// scale, and what it is and passes through.
#[derive(Debug, Clone, PartialEq)]
pub struct Walker {
    /// Center (x, y, z) and radius of each sphere, unscaled.
    pub spheres: Vec<[f32; 4]>,
    pub step_up: f32,
    pub step_down: f32,
    pub scale: f32,
    /// What its description says it is and passes through; none for a body known by its size
    /// alone, which passes through what any new mover does.
    pub ethereality: Option<Ethereality>,
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
            ethereality: None,
        }
    }
}

/// The body a description gives: its look's setup's first two collision spheres and its step
/// heights, at the look's scale, passing through what the description says; none for a look with no
/// setup or a setup with no spheres.
#[must_use]
pub fn walker(records: &WorldRecords, description: &yalain_content::Description) -> Option<Walker> {
    let visual = |did: u32| records.visual(DataId(did)).map(|v| (*v).clone());
    let (setup, scale) = yalain_content::look_setup(description.look, &visual)?;
    let s = records.setup(DataId(setup))?;
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
        ethereality: Some(ethereality_of(description)),
    })
}

/// The body every creature with no walker of its own is given: a human's size.
const ANY_WALKER: (f32, f32) = (0.4, 1.8);

/// The longest leg of a step through the transition, in meters: about what a creature running at 4
/// m/s moves in a physics tick of a thirtieth of a second. A longer leg sinks the body so far into
/// what it meets that the transition can send it back.
const LEG: f32 = 0.125;

/// The longest leg of a step a search tries, in meters. A search only asks where a link ends, and
/// takes none that ends off its lattice point, so it needs fewer moves than a walk.
const SEARCH_LEG: f32 = 0.5;

/// The most moves a pass of the world spends on way searches.
pub const PASS_MOVES: u64 = 2_000;

/// The most moves a waiting search takes in one turn, and the most a new one takes when it is asked
/// for, which is enough to answer most at once.
const TURN_MOVES: u64 = 256;

/// The most moves a search for a walk at rest takes; a walk that needs more is not worth waiting for.
pub const REST_WALK_MOVES: u64 = 512;

/// How many passes a search stays when nobody asks about it.
const FORGOTTEN_AFTER: u64 = 8;

/// How far a goal may move from what a search under way was asked before it is dropped, in meters.
const GOAL_KEPT: f32 = 20.0;

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

/// What a world places, with its entry once it stands.
type Kept = (Standing, Option<BodyKey>);

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
    /// What the world places, by the block whose reading stands it, each with its entry once it
    /// stands.
    standing: BTreeMap<(u8, u8), Vec<Kept>>,
    /// The searches under way, in turn.
    searching: VecDeque<Search>,
    /// How many passes there have been, and the moves left of this one's budget.
    pass: u64,
    moves_left: u64,
    /// The work done so far: moves through the transition, those of them searches', and searches.
    pub moves: u64,
    pub search_moves: u64,
    pub searches: u64,
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
            standing: BTreeMap::new(),
            searching: VecDeque::new(),
            pass: 0,
            moves_left: PASS_MOVES,
            moves: 0,
            search_moves: 0,
            searches: 0,
        }
    }

    /// What a world places becomes `rows`, in place of what it placed before.
    pub fn set_standing(&mut self, rows: Vec<Standing>) {
        for (_, kept) in std::mem::take(&mut self.standing) {
            for (_, key) in kept {
                if let Some(key) = key {
                    self.world.remove(key);
                }
            }
        }
        for row in rows {
            self.standing
                .entry(row.block())
                .or_default()
                .push((row, None));
        }
        let read: Vec<(u8, u8)> = self.read.iter().copied().collect();
        for (x, y) in read {
            self.stand_in(x, y);
        }
    }

    /// Stand what is placed in block `(x, y)`, now it is read, and file what stands beside it
    /// again, since it may reach into this block.
    fn stand_in(&mut self, x: u8, y: u8) {
        if let Some(kept) = self.standing.get_mut(&(x, y)) {
            for (row, key) in kept.iter_mut().filter(|(_, key)| key.is_none()) {
                *key = self.blocks.stand(&self.land, &mut self.world, row);
            }
        }
        for dx in -1i16..=1 {
            for dy in -1i16..=1 {
                if (dx, dy) == (0, 0) {
                    continue;
                }
                let (Ok(a), Ok(b)) = (
                    u8::try_from(i16::from(x) + dx),
                    u8::try_from(i16::from(y) + dy),
                ) else {
                    continue;
                };
                let Some(kept) = self.standing.get(&(a, b)) else {
                    continue;
                };
                for (row, key) in kept {
                    if let Some(key) = key {
                        self.world.place(&self.land, *key, row.position());
                    }
                }
            }
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
        self.stand_in(x, y);
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
    /// never read). It goes as far as it gets on the ground; a body that can move no way at all is
    /// first put where it fits nearest.
    pub fn step(
        &mut self,
        template: u32,
        scale: f32,
        from: &StoredPosition,
        d: [f32; 2],
    ) -> Option<Stepped> {
        self.step_as(template, scale, from, d, None)
    }

    /// A step: a walk's, or a search's in legs of the length given, which never puts the body where
    /// it fits.
    fn step_as(
        &mut self,
        template: u32,
        scale: f32,
        from: &StoredPosition,
        d: [f32; 2],
        searching: Option<f32>,
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
        let (land, world) = (&self.land, &mut self.world);
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
            let entry = world.insert_body(land, 0, &mut b);
            if let Some(e) = w.ethereality {
                world.set_ethereality(entry, e);
            }
            b
        });
        let start = position(from, heading);
        let length = (d[0] * d[0] + d[1] * d[1]).sqrt();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let legs = (length / searching.unwrap_or(LEG)).ceil().max(1.0) as u32;
        #[allow(clippy::cast_precision_loss)]
        let share = 1.0 / legs as f32;
        let leg = Vec3::new(d[0] * share, d[1] * share, 0.0);
        let mut end = start;
        for i in 0..legs {
            self.moves += 1;
            self.search_moves += u64::from(searching.is_some());
            if let Some(next) = take_step(body, &self.land, &mut self.world, end, leg, heading) {
                end = next;
                continue;
            }
            if i == 0
                && searching.is_none()
                && !can_set_out(body, &self.land, &mut self.world, start, heading)
            {
                // It stands in something: it is put where it fits nearest, and steps from there.
                body.place(start, heading);
                let ok = body.set_position_simple(&self.land, &mut self.world, start, true);
                if ok == auberean_physics::body::placing::error::OK {
                    end = body.position;
                    if let Some(next) =
                        take_step(body, &self.land, &mut self.world, end, leg, heading)
                    {
                        end = next;
                    }
                }
            }
            break;
        }
        // A body stands in the world only for its step: it is no wall to the next creature's.
        if let Some(entry) = body.key() {
            let nowhere = Position::new(CellId(0), Frame::new(Vec3::ZERO, Quat::IDENTITY));
            self.world.place(&self.land, entry, nowhere);
        }
        let moved = flat(&start, &end);
        let mut at = stored(&end);
        at.rotation = stored(&start).rotation;
        Some(Stepped { at, moved })
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

/// The way's own last point, reached by a step straight to the goal.
const AT_GOAL: Point = (i32::MIN, i32::MIN, i32::MIN);

/// How far a search has got.
enum Stage {
    /// Walking straight at the goal: where the body is, and how many links it has taken.
    Straight { at: StoredPosition, links: u32 },
    /// Over the lattice.
    Lattice(Box<Lattice>),
    /// Finished, with its way if it found one.
    Done(Option<Vec<StoredPosition>>),
}

/// A search over the lattice, between turns.
struct Lattice {
    reached: HashMap<Point, Reached>,
    /// The open points by cost plus estimate (in millimeters, for ordering).
    open: std::collections::BinaryHeap<std::cmp::Reverse<(u64, Point)>>,
    closed: BTreeSet<Point>,
    longest: f32,
}

/// A way being searched for, by whom and from where to where.
struct Search {
    id: u64,
    template: u32,
    scale: f32,
    from: StoredPosition,
    goal: StoredPosition,
    close: f32,
    /// The length of its legs.
    leg: f32,
    /// The goal in the frame of `from`'s block.
    g: [f32; 3],
    stage: Stage,
    /// The pass somebody last asked about it.
    asked: u64,
}

/// A place's point in the frame of `from`'s block.
fn local_to(from: &StoredPosition, p: &StoredPosition) -> [f32; 3] {
    let o = auberean_physics::geom::offset(&position(from, 0.0), &position(p, 0.0));
    [
        from.origin[0] + o.x,
        from.origin[1] + o.y,
        from.origin[2] + o.z,
    ]
}

fn point_of(p: [f32; 3]) -> Point {
    (
        dereth_primitives::num::floor_to_i32(p[0] / LATTICE + 0.5),
        dereth_primitives::num::floor_to_i32(p[1] / LATTICE + 0.5),
        dereth_primitives::num::floor_to_i32(p[2] / 2.0 + 0.5),
    )
}

fn millimeters(v: f32) -> u64 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let m = (v.max(0.0) * 1000.0) as u64;
    m
}

impl Search {
    fn new(
        id: u64,
        template: u32,
        scale: f32,
        from: &StoredPosition,
        goal: &StoredPosition,
        close: f32,
    ) -> Self {
        Self {
            id,
            template,
            scale,
            from: *from,
            goal: *goal,
            close,
            leg: SEARCH_LEG,
            g: local_to(from, goal),
            stage: Stage::Straight {
                at: *from,
                links: 0,
            },
            asked: 0,
        }
    }

    /// The ground distance from a point to the goal.
    fn gap(&self, p: [f32; 3]) -> f32 {
        ((p[0] - self.g[0]).powi(2) + (p[1] - self.g[1]).powi(2)).sqrt()
    }

    /// The search over the lattice, once walking straight has failed.
    fn lattice(&self) -> Lattice {
        let start = point_of(self.from.origin);
        let mut reached = HashMap::new();
        reached.insert(
            start,
            Reached {
                at: self.from,
                cost: 0.0,
                from: None,
            },
        );
        let mut open = std::collections::BinaryHeap::new();
        open.push(std::cmp::Reverse((
            millimeters(self.gap(self.from.origin)),
            start,
        )));
        Lattice {
            reached,
            open,
            closed: BTreeSet::new(),
            longest: LONGEST_WAY.0 * self.gap(self.from.origin) + LONGEST_WAY.1,
        }
    }
}

/// What asking for a way gives.
#[derive(Debug, Clone, PartialEq)]
pub enum Asked {
    /// The places to walk through in turn, the last within reach of the goal.
    Way(Vec<StoredPosition>),
    /// None leads there.
    NoWay,
    /// No answer yet: ask again.
    Waiting,
}

impl Walls {
    /// A pass of the world begins: its budget is whole, and the searches waiting take their turns
    /// from it. Those nobody has asked about lately are dropped.
    pub fn new_pass(&mut self) {
        self.pass += 1;
        self.moves_left = PASS_MOVES;
        let pass = self.pass;
        self.searching
            .retain(|s| pass.saturating_sub(s.asked) <= FORGOTTEN_AFTER);
        let mut finished = 0;
        while self.moves_left > 0 && finished < self.searching.len() {
            let Some(mut s) = self.searching.pop_front() else {
                break;
            };
            if matches!(s.stage, Stage::Done(_)) {
                finished += 1;
            } else {
                finished = 0;
                self.take_turn(&mut s, TURN_MOVES);
            }
            self.searching.push_back(s);
        }
    }

    /// Ask for a way for the creature `id`, of `template` and `scale`, from `from` to within `close`
    /// meters of `goal` on the ground; `careful` when the last way found could not be walked, which
    /// tries a walk's short legs. A search it has under way goes on, whatever has moved since, unless
    /// the goal has gone far from what it asked.
    #[allow(clippy::too_many_arguments)]
    pub fn ask(
        &mut self,
        id: u64,
        template: u32,
        scale: f32,
        from: &StoredPosition,
        goal: &StoredPosition,
        close: f32,
        careful: bool,
    ) -> Asked {
        if let Some(i) = self.searching.iter().position(|s| s.id == id) {
            let kept = flat(
                &position(&self.searching[i].goal, 0.0),
                &position(goal, 0.0),
            ) <= GOAL_KEPT
                && self.searching[i].template == template;
            if kept {
                self.searching[i].asked = self.pass;
                if !matches!(self.searching[i].stage, Stage::Done(_)) {
                    return Asked::Waiting;
                }
                return match self.searching.remove(i).map(|s| s.stage) {
                    Some(Stage::Done(Some(way))) => Asked::Way(way),
                    _ => Asked::NoWay,
                };
            }
            self.searching.remove(i);
        }
        if !self.joined(from, goal) {
            return Asked::NoWay;
        }
        self.searches += 1;
        let mut s = Search::new(id, template, scale, from, goal, close);
        s.asked = self.pass;
        if careful {
            s.leg = LEG;
        }
        self.take_turn(&mut s, TURN_MOVES);
        if let Stage::Done(way) = s.stage {
            return way.map_or(Asked::NoWay, Asked::Way);
        }
        self.searching.push_back(s);
        Asked::Waiting
    }

    /// A way like [`Walls::way`], for what can do without one: it gives up beyond `most` moves or
    /// what is left of the pass's budget.
    pub fn way_within(
        &mut self,
        template: u32,
        scale: f32,
        from: &StoredPosition,
        goal: &StoredPosition,
        close: f32,
        most: u64,
    ) -> Option<Vec<StoredPosition>> {
        if !self.joined(from, goal) {
            return None;
        }
        self.searches += 1;
        let mut s = Search::new(0, template, scale, from, goal, close);
        self.take_turn(&mut s, most);
        match s.stage {
            Stage::Done(way) => way,
            _ => None,
        }
    }

    /// A way for a creature of `template` and `scale` from `from` to within `close` meters of
    /// `goal` (on the ground), searched to the end at once: the places to walk through in turn, the
    /// last within `close` of the goal; none when the cells are not joined or the search finds no
    /// way within [`MOST_POINTS`] lattice points. Each link of the way is a step the walls let the
    /// body take.
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
        self.searches += 1;
        let mut s = Search::new(0, template, scale, from, goal, close);
        self.run_search(&mut s, u64::MAX);
        match s.stage {
            Stage::Done(way) => way,
            _ => None,
        }
    }

    /// A search goes on for up to `most` moves, or what is left of the pass's budget, and the budget
    /// is charged what it took.
    fn take_turn(&mut self, s: &mut Search, most: u64) {
        let spent = self.run_search(s, most.min(self.moves_left));
        self.moves_left = self.moves_left.saturating_sub(spent);
    }

    /// A search goes on until it is done or has taken `most` moves, stopping only between lattice
    /// points; the moves it took.
    fn run_search(&mut self, s: &mut Search, most: u64) -> u64 {
        let began = self.search_moves;
        while self.search_moves - began < most {
            match std::mem::replace(&mut s.stage, Stage::Done(None)) {
                Stage::Straight { at, links } => s.stage = self.walk_straight(s, at, links),
                Stage::Lattice(mut l) => {
                    s.stage = match self.open_a_point(s, &mut l) {
                        Some(end) => Stage::Done(end),
                        None => Stage::Lattice(l),
                    };
                }
                Stage::Done(way) => {
                    s.stage = Stage::Done(way);
                    break;
                }
            }
        }
        self.search_moves - began
    }

    /// One lattice spacing straight at the goal: how far the search has got after it.
    fn walk_straight(&mut self, s: &Search, at: StoredPosition, links: u32) -> Stage {
        let g = position(&s.goal, 0.0);
        let o = auberean_physics::geom::offset(&position(&at, 0.0), &g);
        let left = (o.x * o.x + o.y * o.y).sqrt();
        if left <= s.close {
            return Stage::Done(Some(vec![at]));
        }
        let go = left.min(LATTICE);
        if links < 64 {
            let d = [o.x / left * go, o.y / left * go];
            if let Some(step) = self.step_as(s.template, s.scale, &at, d, Some(s.leg)) {
                if step.moved >= go * 0.9 {
                    return Stage::Straight {
                        at: step.at,
                        links: links + 1,
                    };
                }
            }
        }
        Stage::Lattice(Box::new(s.lattice()))
    }

    /// Open the search's best point: its way (or that there is none) when that ends the search.
    #[allow(clippy::option_option)]
    fn open_a_point(&mut self, s: &Search, l: &mut Lattice) -> Option<Option<Vec<StoredPosition>>> {
        let Some(std::cmp::Reverse((_, k))) = l.open.pop() else {
            return Some(None);
        };
        if !l.closed.insert(k) {
            return None;
        }
        if l.closed.len() > MOST_POINTS {
            return Some(None);
        }
        let (here, cost) = {
            let r = &l.reached[&k];
            (r.at, r.cost)
        };
        let p = local_to(&s.from, &here);
        let left = s.gap(p);
        if left <= s.close {
            return Some(Some(way_to(&l.reached, k)));
        }
        // Near the goal, a step straight to it.
        if left <= LATTICE * 1.5 {
            let d = [s.g[0] - p[0], s.g[1] - p[1]];
            if let Some(step) = self.step_as(s.template, s.scale, &here, d, Some(s.leg)) {
                if s.gap(local_to(&s.from, &step.at)) <= s.close {
                    l.reached.insert(
                        AT_GOAL,
                        Reached {
                            at: step.at,
                            cost: cost + left,
                            from: Some(k),
                        },
                    );
                    return Some(Some(way_to(&l.reached, AT_GOAL)));
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
            // A point already opened at this height needs no link to it tried.
            if l.closed.contains(&(k.0 + dx, k.1 + dy, k.2)) {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let target = [(k.0 + dx) as f32 * LATTICE, (k.1 + dy) as f32 * LATTICE];
            let d = [target[0] - p[0], target[1] - p[1]];
            let length = (d[0] * d[0] + d[1] * d[1]).sqrt();
            let Some(step) = self.step_as(s.template, s.scale, &here, d, Some(s.leg)) else {
                continue;
            };
            let q = local_to(&s.from, &step.at);
            if ((q[0] - target[0]).powi(2) + (q[1] - target[1]).powi(2)).sqrt() > ON_POINT {
                continue;
            }
            let nk = point_of(q);
            if l.closed.contains(&nk) {
                continue;
            }
            let c = cost + length;
            if c + s.gap(q) > l.longest || l.reached.get(&nk).is_some_and(|r| r.cost <= c) {
                continue;
            }
            l.reached.insert(
                nk,
                Reached {
                    at: step.at,
                    cost: c,
                    from: Some(k),
                },
            );
            l.open
                .push(std::cmp::Reverse((millimeters(c + s.gap(q)), nk)));
        }
        None
    }
}

/// The places from a search's start to a point it reached, in order.
fn way_to(reached: &HashMap<Point, Reached>, end: Point) -> Vec<StoredPosition> {
    let mut k = end;
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
    way
}

/// The ground distance between two places of one block, or of neighbors.
fn flat(a: &Position, b: &Position) -> f32 {
    let o = auberean_physics::geom::offset(a, b);
    (o.x * o.x + o.y * o.y).sqrt()
}

/// Whether a body at `start` can move any way at all: one held up by what is ahead can go back.
fn can_set_out(
    body: &mut Body,
    land: &Land,
    world: &mut World,
    start: Position,
    heading: f32,
) -> bool {
    (0u8..8).any(|k| {
        let a = f32::from(k) * core::f32::consts::FRAC_PI_4;
        let d = Vec3::new(math::sinf(a) * LEG, math::cosf(a) * LEG, 0.0);
        take_step(body, land, world, start, d, heading).is_some()
    })
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
