//! One navigable region, sampled into standing places: a dungeon's connected interior cells, or a
//! window of landblocks with their terrain, buildings, scenery and the interiors their buildings
//! open onto.
//!
//! A region is built in five steps:
//!
//! 1. **Cells.** For a dungeon, every cell reachable from the first one through the portals the
//!    cells record, inside the landblock. Outdoors, every interior cell a building in the window
//!    opens onto, and the cells those reach in turn.
//! 2. **Floors and solids.** Each cell's collision polygons, placed by the cell's frame, split by
//!    their slope: a polygon a body can stand on is a floor, anything else is solid. Static
//!    objects, building shells and scenery add their collision shapes, and their walkable polygons
//!    add floors.
//! 3. **Standing places.** The floors are sampled on a square grid. A grid column can hold several
//!    places, one per floor level above it. Outdoors, the terrain adds one more place to every
//!    column of the window. Its height is read off the terrain when asked for, so a landblock's
//!    ground costs two bits a place rather than a stored record.
//! 4. **Clearance.** A place is clear when the body standing there, raised by the height it
//!    steps over, is at least its radius plus a margin from every solid, and no other floor
//!    crowds the space it occupies. A building's shell counts only for places outdoors: it stops
//!    a body standing outside the building, and none in its rooms, which their own walls bound
//!    (a room sunk below the ground the building stands on is inside the shell's footprint and
//!    under its floor). Terrain too steep to stand on is never clear; terrain under
//!    wading-deep water is clear but costs more to cross, unless the route may not wade.
//! 5. **Patches.** The clear places are merged into rectangles of columns, a place to a column,
//!    each place a step up or down from its neighbours and alone on its floor in its column: from
//!    each place not yet in one, as far east as places join, then north a row at a time while the
//!    whole row joins. A patch of stored places is at most eight metres a side; the terrain's are
//!    merged a tile of columns at a time, as each tile's clearance is decided, and stay inside it.
//!    A body walks straight between any two places of a patch, so a route is searched a patch at a
//!    time.
//!
//! All of it is in **region space**: the landblock space of the region's first landblock, so a
//! window of several landblocks is one continuous plane.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use dereth_physics::globals::FLOOR_Z;
use dereth_physics::land::{LandblockCollision, WaterType};
use dereth_physics::source::{BuildingGeometry, EnvCellGeometry};
use dereth_primitives::frame::localtoglobal;
use dereth_primitives::num::floor_to_i32;
use dereth_primitives::{CellId, Frame, LandblockId, Position, Quat, Vec3};

use crate::solids::{bsp_shapes, object_shapes, Owner, Poly, Shape, SolidIndex};
use crate::source::{NavConfig, NavSource};
use crate::vmath::add;

/// A portal to the outdoors.
const OUTDOORS: u32 = 0xFFFF_FFFF;

/// A landblock's side, in metres.
pub(crate) const BLOCK_LENGTH: f32 = 192.0;

/// The bit that marks a place id as a terrain place: the rest is its index in the ground grid.
pub(crate) const GROUND: u32 = 1 << 31;

/// Two floor samples in one column closer than this are the same place.
const SAME_LEVEL: f32 = 0.1;

/// The widest gap between two floors a step apart, in grid columns, that a body steps across: two
/// columns at the usual spacing are under 0.75 m, which a human body, 0.96 m across, spans.
const GAP_COLUMNS: i32 = 2;

/// Water deeper than this (the depth over a water vertex of a part-water cell, and anything
/// deeper) is wading.
const WADING_DEPTH: f32 = 0.4;

/// An interior floor this close above or below the terrain takes the terrain's place: the body is
/// indoors there.
const INDOORS_OVER_TERRAIN: f32 = 3.0;

/// A place is clear of solids.
pub(crate) const CLEAR: u8 = 1;
/// A place is blocked by a cell's own geometry or a floor above it.
pub(crate) const BLOCKED_BY_CELL: u8 = 2;
/// A place is blocked only by a static object.
pub(crate) const BLOCKED_BY_STATIC: u8 = 4;
/// A place was taken out after a walk through it failed.
pub(crate) const REPAIRED: u8 = 8;

/// The side of a tile of grid columns, in columns.
const TILE: i32 = 16;

/// The columns of a tile, as the first place of each and one past the last, from the tile's base.
#[derive(Debug)]
struct Tile {
    base: u32,
    starts: Box<[u16; (TILE * TILE) as usize + 1]>,
}

/// The grid columns of the stored places: where in [`Region::nodes`] each column's places are.
///
/// Columns are grouped in tiles of 16 by 16 and the places stored tile by tile, so a column costs
/// two bytes where its tile holds any place and nothing where it holds none.
#[derive(Debug)]
pub(crate) struct Columns {
    tx0: i32,
    ty0: i32,
    tw: i32,
    th: i32,
    /// Per tile of the bounds, its index in `tiles`, or `u32::MAX` for an empty tile.
    index: Vec<u32>,
    tiles: Vec<Tile>,
}

/// The tile of a column.
const fn tile_of(ix: i32, iy: i32) -> (i32, i32) {
    (ix.div_euclid(TILE), iy.div_euclid(TILE))
}

/// The order places are stored in: tile by tile, row by row inside a tile.
pub(crate) fn storage_order(&(x, y): &(i32, i32)) -> (i32, i32, i32, i32) {
    let (tx, ty) = tile_of(x, y);
    (ty, tx, y, x)
}

impl Columns {
    /// Lay out `counts`, keyed by column and sorted in [`storage_order`], the order the places
    /// were stored in.
    fn new(counts: &[((i32, i32), u32)]) -> Self {
        let Some(&((fx, fy), _)) = counts.first() else {
            return Self {
                tx0: 0,
                ty0: 0,
                tw: 0,
                th: 0,
                index: Vec::new(),
                tiles: Vec::new(),
            };
        };
        let (t0x, t0y) = tile_of(fx, fy);
        let (mut tx0, mut ty0, mut tx1, mut ty1) = (t0x, t0y, t0x, t0y);
        for &((x, y), _) in counts {
            let (tx, ty) = tile_of(x, y);
            tx0 = tx0.min(tx);
            ty0 = ty0.min(ty);
            tx1 = tx1.max(tx);
            ty1 = ty1.max(ty);
        }
        let (tw, th) = (tx1 - tx0 + 1, ty1 - ty0 + 1);
        let area = usize::try_from(tw * th).unwrap_or(0);
        let mut index = vec![u32::MAX; area];
        let mut tiles: Vec<Tile> = Vec::new();
        let mut per = [0u32; (TILE * TILE) as usize];
        let mut current: Option<(i32, i32)> = None;
        let mut base = 0u32;
        let mut next = 0u32;
        let close = |tiles: &mut Vec<Tile>, per: &mut [u32; (TILE * TILE) as usize], base: u32| {
            let mut starts = Box::new([0u16; (TILE * TILE) as usize + 1]);
            let mut acc = 0u32;
            for (i, n) in per.iter().enumerate() {
                starts[i] = u16::try_from(acc).unwrap_or(u16::MAX);
                acc += n;
            }
            starts[(TILE * TILE) as usize] = u16::try_from(acc).unwrap_or(u16::MAX);
            tiles.push(Tile { base, starts });
            *per = [0; (TILE * TILE) as usize];
        };
        for &((x, y), n) in counts {
            let t = tile_of(x, y);
            if current != Some(t) {
                if current.is_some() {
                    close(&mut tiles, &mut per, base);
                }
                current = Some(t);
                base = next;
                if let Ok(i) = usize::try_from((t.1 - ty0) * tw + (t.0 - tx0)) {
                    index[i] = u32::try_from(tiles.len()).unwrap_or(u32::MAX);
                }
            }
            let local = y.rem_euclid(TILE) * TILE + x.rem_euclid(TILE);
            per[usize::try_from(local).unwrap_or(0)] = n;
            next += n;
        }
        close(&mut tiles, &mut per, base);
        Self {
            tx0,
            ty0,
            tw,
            th,
            index,
            tiles,
        }
    }

    pub(crate) fn get(&self, ix: i32, iy: i32) -> std::ops::Range<u32> {
        let (tx, ty) = tile_of(ix, iy);
        let (dx, dy) = (tx - self.tx0, ty - self.ty0);
        if dx < 0 || dy < 0 || dx >= self.tw || dy >= self.th {
            return 0..0;
        }
        let Some(&t) = usize::try_from(dy * self.tw + dx)
            .ok()
            .and_then(|i| self.index.get(i))
        else {
            return 0..0;
        };
        let Some(tile) = self.tiles.get(t as usize) else {
            return 0..0;
        };
        let local = usize::try_from(iy.rem_euclid(TILE) * TILE + ix.rem_euclid(TILE)).unwrap_or(0);
        tile.base + u32::from(tile.starts[local])..tile.base + u32::from(tile.starts[local + 1])
    }

    fn bytes(&self) -> usize {
        self.index.capacity() * 4
            + self.tiles.capacity()
                * (std::mem::size_of::<Tile>() + ((TILE * TILE) as usize + 1) * 2)
    }
}

/// One stored place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Node {
    pub ix: i16,
    pub iy: i16,
    pub z: f32,
    /// Index into [`Region::cell_ids`].
    pub cell: u16,
    pub flags: u8,
    /// The patch the place belongs to, or [`NO_PATCH`] for a place that is not clear.
    pub patch: u32,
}

/// No patch: a place that is not clear.
pub(crate) const NO_PATCH: u32 = u32::MAX;

/// The longest side of a patch of stored places, in columns: eight metres.
const PATCH_SIDE: i32 = 32;

/// A patch: a rectangle of grid columns holding one clear place each, all on one floor, so that a
/// body walks straight between any two of them. Each pair of neighbouring places steps up or down
/// no more than a step, and neither column holds another clear place a step or two above or
/// below, so a step into the patch's column lands on the patch's own place. The places of a
/// region are merged into patches when it is built; a route is searched over the patches, and
/// within one it walks straight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Patch {
    pub x0: i16,
    pub y0: i16,
    pub w: u8,
    pub h: u8,
    /// The terrain's places, rather than stored ones.
    pub ground: bool,
    /// Terrain the body wades at.
    pub wading: bool,
}

impl Patch {
    /// Whether column `(ix, iy)` is inside the patch.
    pub(crate) fn holds(&self, ix: i32, iy: i32) -> bool {
        let (x0, y0) = (i32::from(self.x0), i32::from(self.y0));
        ix >= x0 && iy >= y0 && ix < x0 + i32::from(self.w) && iy < y0 + i32::from(self.h)
    }

    /// The columns on its edge, each once, as `(ix, iy)`.
    pub(crate) fn rim(&self) -> impl Iterator<Item = (i32, i32)> {
        let (x0, y0) = (i32::from(self.x0), i32::from(self.y0));
        let (x1, y1) = (x0 + i32::from(self.w) - 1, y0 + i32::from(self.h) - 1);
        (y0..=y1).flat_map(move |iy| {
            let whole = iy == y0 || iy == y1;
            let xs: Box<dyn Iterator<Item = i32>> = if whole {
                Box::new(x0..=x1)
            } else if x1 > x0 {
                Box::new([x0, x1].into_iter())
            } else {
                Box::new(std::iter::once(x0))
            };
            xs.map(move |ix| (ix, iy))
        })
    }
}

impl Node {
    const fn is_clear(&self) -> bool {
        self.flags & CLEAR != 0 && self.flags & REPAIRED == 0
    }
}

/// One standing place, as the region reports it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Place {
    /// The grid column.
    pub ix: i32,
    /// The grid column.
    pub iy: i32,
    /// The floor height.
    pub z: f32,
    /// The cell whose floor this is: an interior cell, or for terrain and outdoor objects the land
    /// cell the place stands in.
    pub cell: CellId,
    /// Whether the body can stand here.
    pub clear: bool,
    /// Whether only a static object keeps the body from standing here.
    pub blocked_by_static: bool,
    /// Whether this is the terrain.
    pub ground: bool,
    /// Terrain too steep to stand on.
    pub steep: bool,
    /// Terrain under water deeper than wading depth, or in a landblock that is all water.
    pub water: bool,
}

impl Place {
    /// Whether the body can stand here.
    #[must_use]
    pub const fn is_clear(&self) -> bool {
        self.clear
    }
}

/// What building a region found. Counted so that a region that silently lost its geometry reads
/// differently from one that has none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BuildReport {
    /// Landblocks of terrain in the region (none for a dungeon).
    pub landblocks: usize,
    /// Interior cells in the region.
    pub cells: usize,
    /// Cells a portal named that the source could not supply.
    pub missing_cells: Vec<CellId>,
    /// Portals to the outdoors.
    pub outdoor_portals: usize,
    /// Buildings whose shells are solids.
    pub buildings: usize,
    /// Floor polygons sampled.
    pub floors: usize,
    /// Solid shapes indexed.
    pub solids: usize,
    /// Static placements, scenery included, whose setup gave shapes.
    pub statics: usize,
    /// Static placements the source had no setup for.
    pub statics_unresolved: usize,
    /// Stored places: floors of cells and objects.
    pub nodes: usize,
    /// Of those, the clear ones.
    pub clear_nodes: usize,
    /// Of the rest, those blocked only by a static object.
    pub static_blocked_nodes: usize,
    /// Terrain places.
    pub ground: usize,
    /// Of those, the clear ones.
    pub clear_ground: usize,
    /// Terrain places too steep to stand on.
    pub steep_ground: usize,
    /// Terrain places under water the route leaves out: an all-water landblock, or wading
    /// water when the route may not wade.
    pub water_ground: usize,
    /// Clear terrain places the body wades at.
    pub wading_ground: usize,
    /// Patches the clear places were merged into, terrain included.
    pub patches: usize,
}

/// The terrain of an outdoor region: a clear bit per place, the rest read off the landblocks.
#[derive(Debug)]
pub(crate) struct Ground {
    pub x0: i32,
    pub y0: i32,
    pub w: i32,
    pub h: i32,
    clear: Vec<u64>,
    static_blocked: Vec<u64>,
    /// Places the body stands in water at: clear, but wading.
    water: Vec<u64>,
    repaired: HashSet<u32>,
    lands: HashMap<(i32, i32), Arc<LandblockCollision>>,
}

/// What the terrain is at one place.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Terrain {
    pub z: f32,
    pub normal_z: f32,
    pub depth: f32,
    pub all_water_block: bool,
    pub cell: CellId,
}

fn get_bit(v: &[u64], i: u32) -> bool {
    v.get((i >> 6) as usize)
        .is_some_and(|w| w >> (i & 63) & 1 != 0)
}

fn set_bit(v: &mut [u64], i: u32) {
    if let Some(w) = v.get_mut((i >> 6) as usize) {
        *w |= 1 << (i & 63);
    }
}

impl Ground {
    pub(crate) fn index(&self, ix: i32, iy: i32) -> Option<u32> {
        let (dx, dy) = (ix - self.x0, iy - self.y0);
        if dx < 0 || dy < 0 || dx >= self.w || dy >= self.h {
            return None;
        }
        u32::try_from(dy * self.w + dx).ok()
    }

    pub(crate) fn column_of(&self, index: u32) -> (i32, i32) {
        let i = i32::try_from(index).unwrap_or(0);
        (self.x0 + i % self.w, self.y0 + i / self.w)
    }

    fn len(&self) -> u32 {
        u32::try_from(self.w * self.h).unwrap_or(0)
    }

    pub(crate) fn is_clear(&self, index: u32) -> bool {
        get_bit(&self.clear, index) && !self.repaired.contains(&index)
    }
}

/// A dungeon's connected interior cells, or a window of landblocks, sampled.
#[derive(Debug)]
pub struct Region {
    /// The landblock whose space is region space, as block coordinates.
    pub(crate) origin: (i32, i32),
    /// The window of landblocks an outdoor region covers, `(x0, y0, x1, y1)` inclusive.
    pub(crate) window: Option<(i32, i32, i32, i32)>,
    pub(crate) cells: BTreeSet<u32>,
    pub(crate) cell_ids: Vec<CellId>,
    pub(crate) spacing: f32,
    pub(crate) columns: Columns,
    pub(crate) nodes: Vec<Node>,
    pub(crate) ground: Option<Ground>,
    pub(crate) steps: crate::search::Steps,
    /// The extra cost of wading, per metre.
    pub(crate) wade_cost: f32,
    pub(crate) report: BuildReport,
    /// The number its planner gave it when it was built.
    pub(crate) serial: u64,
    /// The patches the clear places are merged into: the terrain's first, tile by tile, then the
    /// stored places'.
    pub(crate) patches: Vec<Patch>,
    /// Per tile of the terrain, the first of its patches; one more at the end.
    pub(crate) ground_tiles: Vec<u32>,
    /// Patches one of whose places was taken out after a walk through it failed: searched place by
    /// place until the places are put back.
    pub(crate) dirty: HashSet<u32>,
}

/// A landblock id's block coordinates.
pub(crate) fn block_xy(block: u32) -> (i32, i32) {
    let b = block & 0xFFFF;
    (
        i32::try_from(b >> 8).unwrap_or(0),
        i32::try_from(b & 0xFF).unwrap_or(0),
    )
}

/// Block coordinates as a landblock id, when inside the world.
fn block_id(bx: i32, by: i32) -> Option<LandblockId> {
    let x = u16::try_from(bx).ok().filter(|&x| x <= 0xFE)?;
    let y = u16::try_from(by).ok().filter(|&y| y <= 0xFE)?;
    Some(LandblockId((x << 8) | y))
}

/// The pieces a region is assembled from, in region space.
struct Builder<'s, S: ?Sized> {
    source: &'s S,
    config: &'s NavConfig,
    origin: (i32, i32),
    solids: SolidIndex,
    floors: Vec<(Poly, u16)>,
    cell_ids: Vec<CellId>,
    cell_slot: HashMap<u32, u16>,
    cells: BTreeSet<u32>,
    report: BuildReport,
    placement: u32,
}

impl<'s, S: NavSource + ?Sized> Builder<'s, S> {
    fn new(source: &'s S, config: &'s NavConfig, origin: (i32, i32)) -> Self {
        let clear = config.agent.radius + config.margin;
        Self {
            source,
            config,
            origin,
            solids: SolidIndex::new(clear + 0.05),
            floors: Vec::new(),
            cell_ids: Vec::new(),
            cell_slot: HashMap::new(),
            cells: BTreeSet::new(),
            report: BuildReport::default(),
            placement: 0,
        }
    }

    /// The offset from a landblock's space to region space.
    fn offset(&self, block: u32) -> Vec3 {
        let (bx, by) = block_xy(block >> 16);
        #[allow(clippy::cast_precision_loss)]
        Vec3::new(
            (bx - self.origin.0) as f32 * BLOCK_LENGTH,
            (by - self.origin.1) as f32 * BLOCK_LENGTH,
            0.0,
        )
    }

    fn slot(&mut self, cell: CellId) -> u16 {
        if let Some(s) = self.cell_slot.get(&cell.0) {
            return *s;
        }
        let s = u16::try_from(self.cell_ids.len()).unwrap_or(u16::MAX);
        self.cell_ids.push(cell);
        self.cell_slot.insert(cell.0, s);
        s
    }

    /// Every cell reachable from `starts` through the cells' portals inside their landblock.
    fn flood(&mut self, starts: &[CellId]) -> Vec<Arc<EnvCellGeometry>> {
        let mut out = Vec::new();
        let mut queue: VecDeque<u32> = VecDeque::new();
        for s in starts {
            if self.cells.insert(s.0) {
                queue.push_back(s.0);
            }
        }
        while let Some(id) = queue.pop_front() {
            let Some(g) = self.source.env_cell(CellId(id)) else {
                self.report.missing_cells.push(CellId(id));
                self.cells.remove(&id);
                continue;
            };
            for p in &g.portals {
                if p.other_cell_id == OUTDOORS {
                    self.report.outdoor_portals += 1;
                    continue;
                }
                if p.other_cell_id & 0xFFFF_0000 == id & 0xFFFF_0000
                    && self.cells.insert(p.other_cell_id)
                {
                    queue.push_back(p.other_cell_id);
                }
            }
            out.push(g);
        }
        out
    }

    fn add_cell(&mut self, g: &EnvCellGeometry) {
        let off = self.offset(g.id.0);
        let slot = self.slot(g.id);
        for poly in &g.physics_polygons {
            let verts: Vec<Vec3> = poly
                .vertices
                .iter()
                .map(|v| add(localtoglobal(&g.frame, *v), off))
                .collect();
            let Some(p) = Poly::new(verts) else { continue };
            if p.normal.z >= FLOOR_Z {
                self.floors.push((p, slot));
            } else {
                self.solids.push(Shape::Poly(p), Owner::Cell);
            }
        }
        for (id, frame) in &g.static_objects {
            if id.0 == 0 {
                continue;
            }
            self.add_object(*id, frame, 1.0, g.id);
        }
    }

    /// One placed object, its frame in the space of `cell`'s landblock.
    fn add_object(
        &mut self,
        id: dereth_primitives::DataId,
        frame: &Frame,
        scale: f32,
        cell: CellId,
    ) {
        self.placement += 1;
        let Some(setup) = self.source.setup(id) else {
            self.report.statics_unresolved += 1;
            return;
        };
        self.report.statics += 1;
        if !self.config.statics {
            return;
        }
        let off = self.offset(cell.0);
        let placed = Frame::new(add(frame.origin, off), frame.rotation);
        let slot = self.slot(cell);
        let mut part_floors = Vec::new();
        let shapes = object_shapes(&setup, &placed, scale, FLOOR_Z, |p| part_floors.push(p));
        for p in part_floors {
            self.floors.push((p, slot));
        }
        let owner = Owner::Static(self.placement);
        for s in shapes {
            self.solids.push(s, owner);
        }
    }

    /// A building's shell: part 0's tree, the only part building collision tests.
    fn add_building(&mut self, b: &BuildingGeometry, cell: CellId) {
        let Some(part) = b.parts.first() else { return };
        let Some(bsp) = part.physics_bsp.as_ref() else {
            return;
        };
        self.report.buildings += 1;
        let off = self.offset(cell.0);
        let frame = Frame::new(add(part.pos.frame.origin, off), part.pos.frame.rotation);
        let slot = self.slot(cell);
        let mut shell_floors = Vec::new();
        let shapes = bsp_shapes(bsp, &frame, part.gfxobj_scale, FLOOR_Z, |p| {
            shell_floors.push(p);
        });
        for p in shell_floors {
            self.floors.push((p, slot));
        }
        for s in shapes {
            self.solids.push(s, Owner::Shell);
        }
    }

    /// The region's pieces, gathered: what is left is sampling the floors, adding the terrain
    /// and deciding what is clear, a slice at a time.
    fn finishing(
        mut self,
        window: Option<(i32, i32, i32, i32)>,
        lands: HashMap<(i32, i32), Arc<LandblockCollision>>,
    ) -> Finishing {
        self.report.cells = self.cells.len();
        self.report.floors = self.floors.len();
        self.report.solids = self.solids.shapes.len();
        Finishing {
            config: self.config.clone(),
            origin: self.origin,
            window,
            lands: Some(lands),
            solids: self.solids,
            floors: self.floors,
            cell_ids: self.cell_ids,
            cells: self.cells,
            report: self.report,
            stage: Stage::Sample,
            raw: HashMap::new(),
            nodes: Vec::new(),
            columns: None,
            ground: None,
            patches: Vec::new(),
            ground_tiles: Vec::new(),
            next: 0,
        }
    }
}

/// How far a region being finished has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Sampling the floors into standing places, floor by floor.
    Sample,
    /// Ordering the places by column.
    Index,
    /// Deciding which stored places are clear, place by place.
    Places,
    /// Deciding which terrain places are clear, a tile at a time, and merging each tile's clear
    /// places into patches.
    Terrain,
    /// Merging the stored places into patches, place by place.
    Merge,
}

/// A region being finished a slice at a time: its floors sampled into places, the places
/// ordered by column, and each place's clearance decided. Each floor sample and each place
/// decided is one unit of work.
#[derive(Debug)]
pub(crate) struct Finishing {
    config: NavConfig,
    origin: (i32, i32),
    window: Option<(i32, i32, i32, i32)>,
    lands: Option<HashMap<(i32, i32), Arc<LandblockCollision>>>,
    solids: SolidIndex,
    floors: Vec<(Poly, u16)>,
    cell_ids: Vec<CellId>,
    cells: BTreeSet<u32>,
    report: BuildReport,
    stage: Stage,
    raw: HashMap<(i32, i32), Vec<(f32, u16)>>,
    nodes: Vec<Node>,
    columns: Option<Columns>,
    ground: Option<Ground>,
    patches: Vec<Patch>,
    ground_tiles: Vec<u32>,
    /// The next floor, place, terrain tile or place to merge to work on.
    next: usize,
}

/// The places a narrow gap between two floors leaves out. Where two floors a step apart in height
/// lie no more than [`GAP_COLUMNS`] columns apart along a row or a column of the grid, with no
/// floor near their height between (a crack between a dais and its step, a riser between treads
/// wider than the spacing), a body far wider than the gap steps across it standing on the higher
/// edge: each column between gets a place at that height. Whether the body fits there is decided
/// with every other place's clearance, so a wall in the gap still stops it.
fn narrow_gaps(
    raw: &HashMap<(i32, i32), Vec<(f32, u16)>>,
    config: &NavConfig,
) -> Vec<((i32, i32), f32, u16)> {
    // The body's own step: the floors are sampled the same whatever the route may climb.
    let rise = config.agent.step_up;
    let near = |key: (i32, i32), top: f32| {
        raw.get(&key).is_some_and(|levels| {
            levels
                .iter()
                .any(|&(z, _)| z > top - rise && z < top + SAME_LEVEL)
        })
    };
    let mut out = Vec::new();
    let mut keys: Vec<&(i32, i32)> = raw.keys().collect();
    keys.sort_unstable();
    for &(ix, iy) in keys {
        for (dx, dy) in [(1, 0), (0, 1)] {
            for gap in 1..=GAP_COLUMNS {
                let far = (ix + dx * (gap + 1), iy + dy * (gap + 1));
                let Some(beyond) = raw.get(&far) else {
                    continue;
                };
                for &(za, ca) in &raw[&(ix, iy)] {
                    for &(zb, cb) in beyond {
                        if (za - zb).abs() > rise {
                            continue;
                        }
                        let (top, cell) = if za >= zb { (za, ca) } else { (zb, cb) };
                        let between: Vec<(i32, i32)> =
                            (1..=gap).map(|k| (ix + dx * k, iy + dy * k)).collect();
                        if between.iter().all(|&k| !near(k, top)) {
                            out.extend(between.into_iter().map(|k| (k, top, cell)));
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    out.dedup_by(|a, b| a.0 == b.0 && (a.1 - b.1).abs() < SAME_LEVEL);
    out
}

impl Finishing {
    /// Work on, spending about `budget` units (ordering the places is done whole); the region
    /// once it is done.
    pub(crate) fn step(&mut self, budget: &mut u64) -> Option<Region> {
        loop {
            if *budget == 0 {
                return None;
            }
            match self.stage {
                Stage::Sample => self.sample(budget),
                Stage::Index => self.index(budget),
                Stage::Places => self.clear_places(budget),
                Stage::Terrain => {
                    if self.clear_terrain(budget) {
                        self.stage = Stage::Merge;
                        self.next = 0;
                    }
                }
                Stage::Merge => {
                    if self.merge_places(budget) {
                        return Some(self.region());
                    }
                }
            }
        }
    }

    /// Standing places on the floors.
    fn sample(&mut self, budget: &mut u64) {
        let s = self.config.spacing;
        while *budget > 0 {
            let Some((p, cell)) = self.floors.get(self.next) else {
                self.stage = Stage::Index;
                self.next = 0;
                return;
            };
            self.next += 1;
            let x0 = floor_to_i32(p.min.x / s);
            let x1 = floor_to_i32(p.max.x / s) + 1;
            let y0 = floor_to_i32(p.min.y / s);
            let y1 = floor_to_i32(p.max.y / s) + 1;
            for ix in x0..=x1 {
                #[allow(clippy::cast_precision_loss)]
                let x = ix as f32 * s;
                if x < p.min.x - 1e-4 || x > p.max.x + 1e-4 {
                    continue;
                }
                for iy in y0..=y1 {
                    *budget = budget.saturating_sub(1);
                    #[allow(clippy::cast_precision_loss)]
                    let y = iy as f32 * s;
                    if y < p.min.y - 1e-4 || y > p.max.y + 1e-4 || !p.contains_xy(x, y) {
                        continue;
                    }
                    self.raw
                        .entry((ix, iy))
                        .or_default()
                        .push((p.z_at(x, y), *cell));
                }
            }
        }
    }

    /// The places ordered column by column, and the terrain's extent.
    fn index(&mut self, budget: &mut u64) {
        let s = self.config.spacing;
        let mut raw = std::mem::take(&mut self.raw);
        for (key, z, cell) in narrow_gaps(&raw, &self.config) {
            raw.entry(key).or_default().push((z, cell));
        }
        let mut keys: Vec<(i32, i32)> = raw.keys().copied().collect();
        *budget = budget.saturating_sub(keys.len() as u64);
        keys.sort_unstable_by_key(storage_order);
        let mut nodes: Vec<Node> = Vec::new();
        let mut counts: Vec<((i32, i32), u32)> = Vec::with_capacity(keys.len());
        for key in keys {
            let mut levels = raw.remove(&key).unwrap_or_default();
            levels.sort_by(|a, b| a.0.total_cmp(&b.0));
            let before = nodes.len();
            let mut last: Option<usize> = None;
            for (z, cell) in levels {
                if let Some(i) = last {
                    if z - nodes[i].z < SAME_LEVEL {
                        // The same floor met twice (two cells or two polygons sharing an edge):
                        // stand on the higher.
                        nodes[i].z = z;
                        continue;
                    }
                }
                nodes.push(Node {
                    ix: i16::try_from(key.0).unwrap_or(i16::MAX),
                    iy: i16::try_from(key.1).unwrap_or(i16::MAX),
                    z,
                    cell,
                    flags: 0,
                    patch: NO_PATCH,
                });
                last = Some(nodes.len() - 1);
            }
            counts.push((key, u32::try_from(nodes.len() - before).unwrap_or(0)));
        }
        nodes.shrink_to_fit();
        self.columns = Some(Columns::new(&counts));
        self.nodes = nodes;
        // The terrain's extent.
        let origin = self.origin;
        let lands = self.lands.take().unwrap_or_default();
        self.ground = self.window.map(|(bx0, by0, bx1, by1)| {
            let per = floor_to_i32(BLOCK_LENGTH / s);
            let x0 = (bx0 - origin.0) * per;
            let y0 = (by0 - origin.1) * per;
            let w = (bx1 - bx0 + 1) * per;
            let h = (by1 - by0 + 1) * per;
            let words = usize::try_from(w * h).unwrap_or(0).div_ceil(64);
            Ground {
                x0,
                y0,
                w,
                h,
                clear: vec![0; words],
                static_blocked: vec![0; words],
                water: vec![0; words],
                repaired: HashSet::new(),
                lands,
            }
        });
        self.stage = Stage::Places;
        self.next = 0;
    }

    /// Whether the cell of slot `c` is an interior cell.
    fn interior(&self, c: u16) -> bool {
        self.cell_ids
            .get(usize::from(c))
            .is_some_and(|id| id.0 & 0xFFFF >= 0x0100)
    }

    /// Clearance of the stored places.
    fn clear_places(&mut self, budget: &mut u64) {
        let config = &self.config;
        let agent = &config.agent;
        let clear_dist = agent.radius + config.margin;
        let s = config.spacing;
        let heights = agent.test_heights(config.step_allowance);
        let headroom = agent.top_center + agent.radius + config.margin;
        let mut points = Vec::with_capacity(heights.len());
        let Some(columns) = self.columns.as_ref() else {
            self.stage = Stage::Terrain;
            return;
        };
        while *budget > 0 {
            let i = self.next;
            let Some(&n) = self.nodes.get(i) else {
                self.stage = Stage::Terrain;
                self.next = 0;
                return;
            };
            self.next += 1;
            *budget -= 1;
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (f32::from(n.ix) * s, f32::from(n.iy) * s);
            let crowded = columns.get(i32::from(n.ix), i32::from(n.iy)).any(|j| {
                let dz = self.nodes[j as usize].z - n.z;
                dz > SAME_LEVEL && dz < headroom
            });
            if crowded {
                self.nodes[i].flags = BLOCKED_BY_CELL;
                continue;
            }
            points.clear();
            points.extend(heights.iter().map(|h| Vec3::new(x, y, n.z + h)));
            // A building's shell stops a body standing outdoors, never one in its rooms.
            let indoors = self
                .cell_ids
                .get(usize::from(n.cell))
                .is_some_and(|id| id.0 & 0xFFFF >= 0x0100);
            self.nodes[i].flags = if !self.solids.clear_of(&points, clear_dist, |o| match o {
                Owner::Cell => false,
                Owner::Shell => indoors,
                Owner::Static(_) => true,
            }) {
                BLOCKED_BY_CELL
            } else if !self
                .solids
                .clear_of(&points, clear_dist, |o| !matches!(o, Owner::Static(_)))
            {
                BLOCKED_BY_STATIC
            } else {
                CLEAR
            };
        }
    }

    /// Clearance of the terrain, a tile of columns at a time, each tile's clear places merged
    /// into patches once it is decided; true once every terrain place is.
    fn clear_terrain(&mut self, budget: &mut u64) -> bool {
        let Some(mut g) = self.ground.take() else {
            return true;
        };
        let config = &self.config;
        let agent = &config.agent;
        let clear_dist = agent.radius + config.margin;
        let s = config.spacing;
        let heights = agent.test_heights(config.step_allowance);
        let headroom = agent.top_center + agent.radius + config.margin;
        let (join, apart) = joins(config);
        let mut points = Vec::with_capacity(heights.len());
        let origin = self.origin;
        let (tw, th) = ((g.w + TILE - 1) / TILE, (g.h + TILE - 1) / TILE);
        let tiles = usize::try_from(tw * th).unwrap_or(0);
        let mut floor = TileFloor::default();
        let done = loop {
            let t = self.next;
            if t >= tiles {
                break true;
            }
            if *budget == 0 {
                break false;
            }
            *budget = budget.saturating_sub(u64::from(TILE.unsigned_abs().pow(2)));
            self.next += 1;
            self.ground_tiles
                .push(u32::try_from(self.patches.len()).unwrap_or(u32::MAX));
            let t = i32::try_from(t).unwrap_or(0);
            let (bx, by) = (g.x0 + (t % tw) * TILE, g.y0 + (t / tw) * TILE);
            floor.clear();
            for ly in 0..TILE {
                for lx in 0..TILE {
                    let (ix, iy) = (bx + lx, by + ly);
                    let Some(index) = g.index(ix, iy) else {
                        continue;
                    };
                    self.report.ground += 1;
                    let Some(t) = terrain_at(&g.lands, origin, s, ix, iy) else {
                        continue;
                    };
                    if t.all_water_block || (!config.wade && t.depth > WADING_DEPTH) {
                        self.report.water_ground += 1;
                        continue;
                    }
                    if t.normal_z < FLOOR_Z {
                        self.report.steep_ground += 1;
                        continue;
                    }
                    // A floor over the terrain: indoors (the interior floor is the place), or an
                    // object's top or a porch at the terrain's level or crowding the body above
                    // it.
                    let covered = self.columns.as_ref().is_some_and(|columns| {
                        columns.get(ix, iy).any(|j| {
                            let n = &self.nodes[j as usize];
                            let dz = n.z - t.z;
                            if self.interior(n.cell) {
                                dz.abs() < INDOORS_OVER_TERRAIN
                            } else {
                                dz > -SAME_LEVEL && dz < headroom
                            }
                        })
                    });
                    if covered {
                        continue;
                    }
                    #[allow(clippy::cast_precision_loss)]
                    let (x, y) = (ix as f32 * s, iy as f32 * s);
                    points.clear();
                    points.extend(heights.iter().map(|h| Vec3::new(x, y, t.z + h)));
                    if !self
                        .solids
                        .clear_of(&points, clear_dist, |o| matches!(o, Owner::Static(_)))
                    {
                        continue;
                    }
                    if !self
                        .solids
                        .clear_of(&points, clear_dist, |o| !matches!(o, Owner::Static(_)))
                    {
                        set_bit(&mut g.static_blocked, index);
                        continue;
                    }
                    set_bit(&mut g.clear, index);
                    let wading = t.depth > WADING_DEPTH;
                    if wading {
                        set_bit(&mut g.water, index);
                        self.report.wading_ground += 1;
                    }
                    self.report.clear_ground += 1;
                    // Alone in its column: no clear stored place a step or two above or below.
                    let alone = !self.columns.as_ref().is_some_and(|columns| {
                        columns.get(ix, iy).any(|j| {
                            let n = &self.nodes[j as usize];
                            n.is_clear() && (n.z - t.z).abs() <= apart
                        })
                    });
                    floor.set(lx, ly, t.z, wading, alone);
                }
            }
            for (lx, ly, w, h) in floor.merge(join) {
                let l = usize::try_from(ly * TILE + lx).unwrap_or(0);
                self.patches.push(Patch {
                    x0: i16::try_from(bx + lx).unwrap_or(i16::MAX),
                    y0: i16::try_from(by + ly).unwrap_or(i16::MAX),
                    w: u8::try_from(w).unwrap_or(1),
                    h: u8::try_from(h).unwrap_or(1),
                    ground: true,
                    wading: floor.wet[l],
                });
            }
        };
        if done {
            self.ground_tiles
                .push(u32::try_from(self.patches.len()).unwrap_or(u32::MAX));
        }
        self.ground = Some(g);
        done
    }

    /// Whether no other clear place shares stored place `i`'s column within `apart` of its
    /// height: a step into the column from a place a step from it lands on it.
    fn alone(&self, i: usize, apart: f32) -> bool {
        let n = self.nodes[i];
        let (ix, iy) = (i32::from(n.ix), i32::from(n.iy));
        let crowded = self.columns.as_ref().is_some_and(|columns| {
            columns.get(ix, iy).any(|j| {
                let m = &self.nodes[j as usize];
                j as usize != i && m.is_clear() && (m.z - n.z).abs() <= apart
            })
        });
        if crowded {
            return false;
        }
        let Some(g) = self.ground.as_ref() else {
            return true;
        };
        if !g.index(ix, iy).is_some_and(|k| g.is_clear(k)) {
            return true;
        }
        terrain_at(&g.lands, self.origin, self.config.spacing, ix, iy)
            .is_none_or(|t| (t.z - n.z).abs() > apart)
    }

    /// The clear stored place of column `(ix, iy)` not yet in a patch, alone in its column, and
    /// within `join` of height `z`.
    fn joinable(&self, ix: i32, iy: i32, z: f32, join: f32, apart: f32) -> Option<u32> {
        let columns = self.columns.as_ref()?;
        columns.get(ix, iy).find(|&j| {
            let m = &self.nodes[j as usize];
            m.is_clear()
                && m.patch == NO_PATCH
                && (m.z - z).abs() <= join
                && self.alone(j as usize, apart)
        })
    }

    /// Merge the stored places into patches, a place at a time: from each clear place not yet in
    /// one, as far east as places join, then row by row north while a whole row joins the one
    /// below. True once every place is in one.
    fn merge_places(&mut self, budget: &mut u64) -> bool {
        let (join, apart) = joins(&self.config);
        loop {
            let i = self.next;
            let Some(&n) = self.nodes.get(i) else {
                return true;
            };
            if *budget == 0 {
                return false;
            }
            *budget -= 1;
            self.next += 1;
            if !n.is_clear() || n.patch != NO_PATCH {
                continue;
            }
            let (x0, y0) = (i32::from(n.ix), i32::from(n.iy));
            let first = u32::try_from(i).unwrap_or(u32::MAX);
            let mut rows: Vec<Vec<u32>> = vec![vec![first]];
            if self.alone(i, apart) {
                let mut x = x0 + 1;
                while x - x0 < PATCH_SIDE {
                    let last = rows[0][rows[0].len() - 1];
                    let Some(j) = self.joinable(x, y0, self.nodes[last as usize].z, join, apart)
                    else {
                        break;
                    };
                    rows[0].push(j);
                    x += 1;
                }
                let mut y = y0 + 1;
                'rows: while y - y0 < PATCH_SIDE {
                    let below = &rows[rows.len() - 1];
                    let mut row: Vec<u32> = Vec::with_capacity(below.len());
                    for (k, &b) in below.iter().enumerate() {
                        let x = x0 + i32::try_from(k).unwrap_or(0);
                        let Some(j) = self.joinable(x, y, self.nodes[b as usize].z, join, apart)
                        else {
                            break 'rows;
                        };
                        if let Some(&west) = row.last() {
                            let dz = self.nodes[west as usize].z - self.nodes[j as usize].z;
                            if dz.abs() > join {
                                break 'rows;
                            }
                        }
                        row.push(j);
                    }
                    rows.push(row);
                    y += 1;
                }
            }
            let id = u32::try_from(self.patches.len()).unwrap_or(NO_PATCH);
            for &j in rows.iter().flatten() {
                self.nodes[j as usize].patch = id;
            }
            self.patches.push(Patch {
                x0: n.ix,
                y0: n.iy,
                w: u8::try_from(rows[0].len()).unwrap_or(1),
                h: u8::try_from(rows.len()).unwrap_or(1),
                ground: false,
                wading: false,
            });
        }
    }

    /// The finished region.
    fn region(&mut self) -> Region {
        let nodes = std::mem::take(&mut self.nodes);
        let ground = self.ground.take();
        let mut report = std::mem::take(&mut self.report);
        report.nodes = nodes.len();
        report.clear_nodes = nodes.iter().filter(|n| n.is_clear()).count();
        report.static_blocked_nodes = nodes
            .iter()
            .filter(|n| n.flags & BLOCKED_BY_STATIC != 0)
            .count();
        report.landblocks = ground.as_ref().map_or(0, |g| g.lands.len());
        report.patches = self.patches.len();
        Region {
            origin: self.origin,
            window: self.window,
            cells: std::mem::take(&mut self.cells),
            cell_ids: std::mem::take(&mut self.cell_ids),
            spacing: self.config.spacing,
            columns: self.columns.take().unwrap_or_else(|| Columns::new(&[])),
            nodes,
            ground,
            wade_cost: self.config.wade_cost,
            steps: crate::search::Steps {
                max_rise: self.config.max_rise,
                max_drop: self.config.max_drop,
            },
            report,
            serial: 0,
            patches: std::mem::take(&mut self.patches),
            ground_tiles: std::mem::take(&mut self.ground_tiles),
            dirty: HashSet::new(),
        }
    }
}

/// How far apart two neighbouring places of a patch may be in height (a step, either way), and
/// how far another clear place in a column has to be from a patch's own (a step or two).
fn joins(config: &NavConfig) -> (f32, f32) {
    (
        config.max_rise.min(config.max_drop),
        2.0 * config.max_rise.max(config.max_drop),
    )
}

/// The places a tile holds side by side.
const TILE_PLACES: usize = (TILE * TILE) as usize;

/// One tile of terrain columns, as its clearance was decided: the floor's height, whether the
/// body stands there, wades there, and is alone in its column.
#[derive(Debug)]
struct TileFloor {
    z: [f32; TILE_PLACES],
    ok: [bool; TILE_PLACES],
    wet: [bool; TILE_PLACES],
    alone: [bool; TILE_PLACES],
}

impl Default for TileFloor {
    fn default() -> Self {
        Self {
            z: [0.0; TILE_PLACES],
            ok: [false; TILE_PLACES],
            wet: [false; TILE_PLACES],
            alone: [false; TILE_PLACES],
        }
    }
}

impl TileFloor {
    fn clear(&mut self) {
        self.ok = [false; TILE_PLACES];
    }

    fn set(&mut self, lx: i32, ly: i32, z: f32, wet: bool, alone: bool) {
        let i = usize::try_from(ly * TILE + lx).unwrap_or(0);
        self.z[i] = z;
        self.ok[i] = true;
        self.wet[i] = wet;
        self.alone[i] = alone;
    }

    /// The tile's clear places as patches `(lx, ly, w, h)`: from each place not yet in one, as
    /// far east as places join, then row by row north while a whole row joins the one below. A
    /// place not alone in its column is a patch of its own.
    fn merge(&self, join: f32) -> Vec<(i32, i32, i32, i32)> {
        let at = |x: i32, y: i32| usize::try_from(y * TILE + x).unwrap_or(0);
        let joined = |a: usize, b: usize| (self.z[a] - self.z[b]).abs() <= join;
        let mut taken = [false; TILE_PLACES];
        let mut out = Vec::new();
        for ly in 0..TILE {
            for lx in 0..TILE {
                let i = at(lx, ly);
                if !self.ok[i] || taken[i] {
                    continue;
                }
                let (mut w, mut h) = (1, 1);
                if self.alone[i] {
                    let fits = |j: usize, taken: &[bool]| {
                        self.ok[j] && !taken[j] && self.alone[j] && self.wet[j] == self.wet[i]
                    };
                    while lx + w < TILE
                        && fits(at(lx + w, ly), &taken)
                        && joined(at(lx + w - 1, ly), at(lx + w, ly))
                    {
                        w += 1;
                    }
                    'rows: while ly + h < TILE {
                        for x in lx..lx + w {
                            let j = at(x, ly + h);
                            if !fits(j, &taken)
                                || !joined(at(x, ly + h - 1), j)
                                || (x > lx && !joined(at(x - 1, ly + h), j))
                            {
                                break 'rows;
                            }
                        }
                        h += 1;
                    }
                }
                for y in ly..ly + h {
                    for x in lx..lx + w {
                        taken[at(x, y)] = true;
                    }
                }
                out.push((lx, ly, w, h));
            }
        }
        out
    }
}

/// The terrain at a grid column of region space.
pub(crate) fn terrain_at(
    lands: &HashMap<(i32, i32), Arc<LandblockCollision>>,
    origin: (i32, i32),
    s: f32,
    ix: i32,
    iy: i32,
) -> Option<Terrain> {
    #[allow(clippy::cast_precision_loss)]
    let (gx, gy) = (
        ix as f32 * s + origin.0 as f32 * BLOCK_LENGTH,
        iy as f32 * s + origin.1 as f32 * BLOCK_LENGTH,
    );
    let bx = floor_to_i32(gx / BLOCK_LENGTH);
    let by = floor_to_i32(gy / BLOCK_LENGTH);
    let land = lands.get(&(bx, by))?;
    if land.side_cell_count != 8 {
        return None;
    }
    #[allow(clippy::cast_precision_loss)]
    let local = Vec3::new(
        gx - bx as f32 * BLOCK_LENGTH,
        gy - by as f32 * BLOCK_LENGTH,
        0.0,
    );
    let cx = floor_to_i32(local.x / 24.0).clamp(0, 7);
    let cy = floor_to_i32(local.y / 24.0).clamp(0, 7);
    let index = u16::try_from(cx * 8 + cy + 1).ok()?;
    let poly = land
        .find_terrain_poly(index, local)
        .or_else(|| land.cell_polygons(index).map(|(a, _)| a))?;
    let n = poly.plane.normal;
    if n.z.abs() < 1e-6 {
        return None;
    }
    let z = -(poly.plane.d + n.x * local.x + n.y * local.y) / n.z;
    let cell = CellId((u32::from(land.id.0) << 16) | u32::from(index));
    Some(Terrain {
        z,
        normal_z: n.z,
        depth: land.get_water_depth(cell, local),
        all_water_block: land.water_type == WaterType::EntirelyWater,
        cell,
    })
}

impl Region {
    /// Build the dungeon region that holds `start`. `None` when the source has no such cell.
    pub(crate) fn build_interior<S: NavSource + ?Sized>(
        source: &S,
        start: CellId,
        config: &NavConfig,
    ) -> Option<Self> {
        let mut all = u64::MAX;
        Self::start_interior(source, start, config)?.step(&mut all)
    }

    /// Start building the dungeon region that holds `start`: its cells gathered, the rest to do
    /// a slice at a time. `None` when the source has no such cell.
    pub(crate) fn start_interior<S: NavSource + ?Sized>(
        source: &S,
        start: CellId,
        config: &NavConfig,
    ) -> Option<Finishing> {
        source.env_cell(start)?;
        let origin = block_xy(start.0 >> 16);
        let mut b = Builder::new(source, config, origin);
        let geometry = b.flood(&[start]);
        for g in &geometry {
            b.add_cell(g);
        }
        Some(b.finishing(None, HashMap::new()))
    }

    /// Build the outdoor region over the window of landblocks `(x0, y0, x1, y1)`: their terrain,
    /// buildings, outdoor objects, and every interior their buildings open onto.
    pub(crate) fn build_outdoor<S: NavSource + ?Sized>(
        source: &S,
        window: (i32, i32, i32, i32),
        config: &NavConfig,
    ) -> Option<Self> {
        let mut all = u64::MAX;
        Self::start_outdoor(source, window, config)?.step(&mut all)
    }

    /// Start building the outdoor region over the window of landblocks `(x0, y0, x1, y1)`: its
    /// pieces gathered, the rest to do a slice at a time.
    pub(crate) fn start_outdoor<S: NavSource + ?Sized>(
        source: &S,
        window: (i32, i32, i32, i32),
        config: &NavConfig,
    ) -> Option<Finishing> {
        let (x0, y0, x1, y1) = window;
        let mut b = Builder::new(source, config, (x0, y0));
        let mut lands = HashMap::new();
        let mut doors = Vec::new();
        for bx in x0..=x1 {
            for by in y0..=y1 {
                let Some(id) = block_id(bx, by) else { continue };
                let Some(land) = source.landblock(id) else {
                    continue;
                };
                lands.insert((bx, by), land);
                for index in 1..=0x40_u32 {
                    let cell = CellId((u32::from(id.0) << 16) | index);
                    if let Some(shell) = source.building(cell) {
                        b.add_building(&shell, cell);
                    }
                    doors.extend(source.building_cells(cell));
                }
                for p in source.land_statics(id) {
                    let mut cell = CellId((u32::from(id.0) << 16) | 1);
                    let mut o = p.frame.origin;
                    if !dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut o) {
                        continue;
                    }
                    // The placement keeps its own block's space whatever cell it fell in.
                    let home = CellId((u32::from(id.0) << 16) | (cell.0 & 0xFFFF));
                    b.add_object(p.id, &p.frame, p.scale, home);
                }
            }
        }
        if lands.is_empty() {
            return None;
        }
        let geometry = b.flood(&doors);
        for g in &geometry {
            b.add_cell(g);
        }
        Some(b.finishing(Some(window), lands))
    }

    /// What building the region found.
    #[must_use]
    pub fn report(&self) -> &BuildReport {
        &self.report
    }

    /// Whether this region covers `p`: one of its interior cells, or terrain inside its window.
    #[must_use]
    pub fn covers(&self, p: &Position) -> bool {
        if p.cell.0 & 0xFFFF >= 0x0100 {
            return self.cells.contains(&p.cell.0);
        }
        let (bx, by) = block_xy(p.cell.0 >> 16);
        self.window
            .is_some_and(|(x0, y0, x1, y1)| (x0..=x1).contains(&bx) && (y0..=y1).contains(&by))
    }

    /// Whether `cell` is one of the region's interior cells.
    #[must_use]
    pub fn contains(&self, cell: CellId) -> bool {
        self.cells.contains(&cell.0)
    }

    /// The region's interior cells, in id order.
    pub fn cells(&self) -> impl Iterator<Item = CellId> + '_ {
        self.cells.iter().map(|c| CellId(*c))
    }

    /// The window of landblocks an outdoor region covers, as block coordinates
    /// `(x0, y0, x1, y1)`, inclusive.
    #[must_use]
    pub const fn window(&self) -> Option<(i32, i32, i32, i32)> {
        self.window
    }

    /// The sample spacing.
    #[must_use]
    pub const fn spacing(&self) -> f32 {
        self.spacing
    }

    /// How many places the region has, terrain included.
    #[must_use]
    pub fn place_count(&self) -> usize {
        self.nodes.len() + self.ground.as_ref().map_or(0, |g| g.len() as usize)
    }

    /// Every place's id: the stored places, then the terrain's.
    pub fn places(&self) -> impl Iterator<Item = u32> + '_ {
        let n = u32::try_from(self.nodes.len()).unwrap_or(0);
        let g = self.ground.as_ref().map_or(0, Ground::len);
        (0..n).chain((0..g).map(|i| i | GROUND))
    }

    /// One place.
    #[must_use]
    pub fn place(&self, id: u32) -> Place {
        if id & GROUND != 0 {
            let g = self.ground.as_ref().expect("a terrain place has terrain");
            let index = id & !GROUND;
            let (ix, iy) = g.column_of(index);
            let t = terrain_at(&g.lands, self.origin, self.spacing, ix, iy);
            return Place {
                ix,
                iy,
                z: t.map_or(0.0, |t| t.z),
                cell: t.map_or(CellId(0), |t| t.cell),
                clear: g.is_clear(index),
                blocked_by_static: get_bit(&g.static_blocked, index),
                ground: true,
                steep: t.is_some_and(|t| t.normal_z < FLOOR_Z),
                water: t.is_some_and(|t| t.all_water_block || t.depth > WADING_DEPTH),
            };
        }
        let n = self.nodes[id as usize];
        Place {
            ix: i32::from(n.ix),
            iy: i32::from(n.iy),
            z: n.z,
            cell: self.cell_ids[usize::from(n.cell)],
            clear: n.is_clear(),
            blocked_by_static: n.flags & BLOCKED_BY_STATIC != 0,
            ground: false,
            steep: false,
            water: false,
        }
    }

    /// Whether the body can stand at place `id`.
    #[must_use]
    pub fn is_clear(&self, id: u32) -> bool {
        if id & GROUND != 0 {
            return self
                .ground
                .as_ref()
                .is_some_and(|g| g.is_clear(id & !GROUND));
        }
        self.nodes.get(id as usize).is_some_and(Node::is_clear)
    }

    /// Whether the body wades at place `id`.
    pub(crate) fn is_wading(&self, id: u32) -> bool {
        id & GROUND != 0
            && self
                .ground
                .as_ref()
                .is_some_and(|g| get_bit(&g.water, id & !GROUND))
    }

    /// A place's column and floor height.
    pub(crate) fn column_and_z(&self, id: u32) -> (i32, i32, f32) {
        if id & GROUND != 0 {
            let g = self.ground.as_ref().expect("a terrain place has terrain");
            let (ix, iy) = g.column_of(id & !GROUND);
            let z = terrain_at(&g.lands, self.origin, self.spacing, ix, iy).map_or(0.0, |t| t.z);
            return (ix, iy, z);
        }
        let n = &self.nodes[id as usize];
        (i32::from(n.ix), i32::from(n.iy), n.z)
    }

    /// A place's point in region space.
    #[must_use]
    pub fn node_point(&self, id: u32) -> Vec3 {
        let (ix, iy, z) = self.column_and_z(id);
        #[allow(clippy::cast_precision_loss)]
        Vec3::new(ix as f32 * self.spacing, iy as f32 * self.spacing, z)
    }

    /// The places of one grid column: the stored ones and, outdoors, the terrain's.
    pub fn column(&self, ix: i32, iy: i32) -> impl Iterator<Item = u32> + '_ {
        let ground = self
            .ground
            .as_ref()
            .and_then(|g| g.index(ix, iy))
            .map(|i| i | GROUND);
        self.columns.get(ix, iy).chain(ground)
    }

    /// The patch a clear place belongs to; `None` for a place that is not clear.
    pub(crate) fn patch_of(&self, id: u32) -> Option<u32> {
        if id & GROUND == 0 {
            let p = self.nodes.get(id as usize)?.patch;
            return (p != NO_PATCH).then_some(p);
        }
        let g = self.ground.as_ref()?;
        let (ix, iy) = g.column_of(id & !GROUND);
        let tw = (g.w + TILE - 1) / TILE;
        let t = usize::try_from(((iy - g.y0) / TILE) * tw + (ix - g.x0) / TILE).ok()?;
        let (&a, &b) = (self.ground_tiles.get(t)?, self.ground_tiles.get(t + 1)?);
        (a..b).find(|&p| self.patches[p as usize].holds(ix, iy))
    }

    /// Patch `p`'s place in column `(ix, iy)`, a column inside it.
    pub(crate) fn patch_place(&self, p: u32, ix: i32, iy: i32) -> Option<u32> {
        let patch = self.patches.get(p as usize)?;
        if patch.ground {
            return self.ground.as_ref()?.index(ix, iy).map(|i| i | GROUND);
        }
        self.columns
            .get(ix, iy)
            .find(|&j| self.nodes[j as usize].patch == p)
    }

    /// The offset from a cell's landblock space to region space.
    fn offset_of(&self, cell: CellId) -> Vec3 {
        let (bx, by) = block_xy(cell.0 >> 16);
        #[allow(clippy::cast_precision_loss)]
        Vec3::new(
            (bx - self.origin.0) as f32 * BLOCK_LENGTH,
            (by - self.origin.1) as f32 * BLOCK_LENGTH,
            0.0,
        )
    }

    /// A position in region space.
    #[must_use]
    pub fn to_region(&self, p: &Position) -> Vec3 {
        add(p.frame.origin, self.offset_of(p.cell))
    }

    /// A point of region space as a position: in the interior cell `cell` when it is one, else in
    /// the land cell the point stands in.
    pub(crate) fn position_of(&self, point: Vec3, cell: CellId) -> Position {
        if cell.0 & 0xFFFF >= 0x0100 {
            let off = self.offset_of(cell);
            let local = Vec3::new(point.x - off.x, point.y - off.y, point.z);
            return Position::new(cell, Frame::new(local, Quat::IDENTITY));
        }
        let (ox, oy) = self.origin;
        let base = block_id(ox, oy).map_or(CellId(1), |b| CellId((u32::from(b.0) << 16) | 1));
        let mut c = base;
        let mut o = point;
        if dereth_physics::landdefs::adjust_to_outside(&mut c, &mut o) {
            Position::new(c, Frame::new(o, Quat::IDENTITY))
        } else {
            Position::new(cell, Frame::new(point, Quat::IDENTITY))
        }
    }

    /// A place as a position.
    #[must_use]
    pub fn position(&self, id: u32) -> Position {
        let cell = if id & GROUND != 0 {
            CellId(0)
        } else {
            self.cell_ids[usize::from(self.nodes[id as usize].cell)]
        };
        self.position_of(self.node_point(id), cell)
    }

    /// The connected sets of clear places, as a label per clear place and the size of each set,
    /// indexed by label.
    #[must_use]
    pub fn components(&self) -> (HashMap<u32, u32>, Vec<usize>) {
        let mut label: HashMap<u32, u32> = HashMap::new();
        let mut sizes = Vec::new();
        let mut stack = Vec::new();
        for seed in self.places() {
            if label.contains_key(&seed) || !self.is_clear(seed) {
                continue;
            }
            let id = u32::try_from(sizes.len()).unwrap_or(u32::MAX);
            let mut size = 0;
            label.insert(seed, id);
            stack.push(seed);
            while let Some(i) = stack.pop() {
                size += 1;
                for j in self.neighbours(i) {
                    if let std::collections::hash_map::Entry::Vacant(e) = label.entry(j) {
                        e.insert(id);
                        stack.push(j);
                    }
                }
            }
            sizes.push(size);
        }
        (label, sizes)
    }

    /// The clear places a body at place `i` can step to.
    #[must_use]
    pub fn neighbours(&self, i: u32) -> Vec<u32> {
        let (hx, hy, hz) = self.column_and_z(i);
        let mut out = Vec::new();
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
            let Some(j) = self.step_into(hx + dx, hy + dy, hz, self.steps) else {
                continue;
            };
            if dx != 0 && dy != 0 {
                let a = self.step_into(hx + dx, hy, hz, self.steps);
                let b = self.step_into(hx, hy + dy, hz, self.steps);
                if a.is_none() || b.is_none() {
                    continue;
                }
            }
            out.push(j);
        }
        out
    }

    /// The bytes the region holds, roughly: places, columns and terrain bits.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.patches.capacity() * std::mem::size_of::<Patch>()
            + self.ground_tiles.capacity() * 4
            + self.columns.bytes()
            + self.cell_ids.capacity() * 4
            + self.ground.as_ref().map_or(0, |g| {
                (g.clear.capacity() + g.static_blocked.capacity() + g.water.capacity()) * 8
                    + g.repaired.capacity() * 8
            })
    }

    /// The clear place nearest to `p`, within `reach` across and a body height up or down.
    pub(crate) fn nearest_clear(
        &self,
        p: Vec3,
        reach: f32,
        usable: impl Fn(u32) -> bool,
    ) -> Option<u32> {
        let s = self.spacing;
        let r = floor_to_i32(reach / s) + 1;
        let cx = floor_to_i32(p.x / s + 0.5);
        let cy = floor_to_i32(p.y / s + 0.5);
        let mut best: Option<(f32, u32)> = None;
        for ix in cx - r..=cx + r {
            for iy in cy - r..=cy + r {
                for i in self.column(ix, iy) {
                    if !self.is_clear(i) || !usable(i) {
                        continue;
                    }
                    let q = self.node_point(i);
                    let dz = q.z - p.z;
                    if !(-1.0..=1.0).contains(&dz) {
                        continue;
                    }
                    let across = crate::vmath::dist_xy(p, q);
                    let d = across + dz.abs() * 2.0;
                    if across <= reach && best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, i));
                    }
                }
            }
        }
        best.map(|(_, i)| i)
    }

    /// The clear places within `reach` across of `p` and `rise` up or down.
    pub(crate) fn clear_near(
        &self,
        p: Vec3,
        reach: f32,
        rise: f32,
    ) -> impl Iterator<Item = u32> + '_ {
        let s = self.spacing;
        let r = floor_to_i32(reach / s) + 1;
        let cx = floor_to_i32(p.x / s + 0.5);
        let cy = floor_to_i32(p.y / s + 0.5);
        (cx - r..=cx + r)
            .flat_map(move |ix| (cy - r..=cy + r).map(move |iy| (ix, iy)))
            .flat_map(move |(ix, iy)| self.column(ix, iy))
            .filter(move |&i| {
                let q = self.node_point(i);
                self.is_clear(i)
                    && (q.z - p.z).abs() <= rise
                    && crate::vmath::dist_xy(p, q) <= reach
            })
    }

    /// The clear place nearest to `p`, within `reach` across, measured across and then up or
    /// down; `None` when there is none.
    #[must_use]
    pub fn nearest_place(&self, p: &Position, reach: f32) -> Option<u32> {
        let q = self.to_region(p);
        let s = self.spacing;
        let r = floor_to_i32(reach / s) + 1;
        let cx = floor_to_i32(q.x / s + 0.5);
        let cy = floor_to_i32(q.y / s + 0.5);
        let mut best: Option<(f32, u32)> = None;
        for ix in cx - r..=cx + r {
            for iy in cy - r..=cy + r {
                for i in self.column(ix, iy) {
                    if !self.is_clear(i) {
                        continue;
                    }
                    let n = self.node_point(i);
                    let across = crate::vmath::dist_xy(q, n);
                    let d = across + (n.z - q.z).abs();
                    if across <= reach && best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, i));
                    }
                }
            }
        }
        best.map(|(_, i)| i)
    }

    /// Take out every place within `radius` of `p` (across) and a body height of it (up and
    /// down): a walk through there failed.
    pub(crate) fn repair_around(&mut self, p: Vec3, radius: f32) -> usize {
        let s = self.spacing;
        let r = floor_to_i32(radius / s) + 1;
        let cx = floor_to_i32(p.x / s + 0.5);
        let cy = floor_to_i32(p.y / s + 0.5);
        let mut hits = Vec::new();
        for ix in cx - r..=cx + r {
            for iy in cy - r..=cy + r {
                for i in self.column(ix, iy) {
                    let q = self.node_point(i);
                    if crate::vmath::dist_xy(p, q) <= radius
                        && (q.z - p.z).abs() < 1.0
                        && self.is_clear(i)
                    {
                        hits.push(i);
                    }
                }
            }
        }
        for &i in &hits {
            if let Some(p) = self.patch_of(i) {
                self.dirty.insert(p);
            }
            if i & GROUND != 0 {
                if let Some(g) = self.ground.as_mut() {
                    g.repaired.insert(i & !GROUND);
                }
            } else {
                self.nodes[i as usize].flags |= REPAIRED;
            }
        }
        hits.len()
    }

    /// The walls, floors' edges and objects of a dungeon region, gathered again from `source`:
    /// what the region's places were decided against, which it does not keep. `None` outdoors.
    pub(crate) fn dungeon_solids<S: NavSource + ?Sized>(
        &self,
        source: &S,
        config: &NavConfig,
    ) -> Option<SolidIndex> {
        if self.window.is_some() {
            return None;
        }
        let mut b = Builder::new(source, config, self.origin);
        for &id in &self.cells {
            if let Some(g) = source.env_cell(CellId(id)) {
                b.add_cell(&g);
            }
        }
        Some(b.solids)
    }

    /// The drops off the region's ledges: from each clear stored place at a ledge's edge, the
    /// clear place a body jumping off it lands on, more than a step below, a metre out. A
    /// ledge is a place a body could step off: a metre of air beyond it at its height (no floor
    /// within a step of it, nothing solid where the body would be), so a crack between two
    /// floors, or a gap a stride crosses, is none. Between the ledge's height and the landing
    /// there is no floor to catch the body first.
    pub(crate) fn ledge_drops(
        &self,
        solids: &SolidIndex,
        config: &NavConfig,
    ) -> HashMap<u32, Vec<u32>> {
        /// Columns of air beyond a ledge: a metre.
        const AIR: i32 = 4;
        /// The column the body lands in, out from the ledge: clear of the edge.
        const OUT: i32 = AIR;
        let agent = &config.agent;
        let clear = agent.radius + config.margin;
        let heights = agent.test_heights(config.step_allowance);
        let s = self.spacing;
        let mut out: HashMap<u32, Vec<u32>> = HashMap::new();
        let mut points = Vec::with_capacity(heights.len());
        for i in 0..u32::try_from(self.nodes.len()).unwrap_or(0) {
            if !self.is_clear(i) {
                continue;
            }
            let (ix, iy, z) = self.column_and_z(i);
            'way: for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                // A metre of air beyond the edge at the ledge's height.
                for k in 1..=AIR {
                    let (nx, ny) = (ix + dx * k, iy + dy * k);
                    let near = self.column(nx, ny).any(|j| {
                        let zj = self.column_and_z(j).2;
                        zj >= z - self.steps.max_drop && zj <= z + agent.top_center
                    });
                    if near {
                        continue 'way;
                    }
                    #[allow(clippy::cast_precision_loss)]
                    let (x, y) = (nx as f32 * s, ny as f32 * s);
                    points.clear();
                    points.extend(heights.iter().map(|h| Vec3::new(x, y, z + h)));
                    if !solids.clear_of(&points, clear, |o| o == Owner::Shell) {
                        continue 'way;
                    }
                }
                // The highest place below, a metre out, and it clear.
                let (nx, ny) = (ix + dx * OUT, iy + dy * OUT);
                let landing = self
                    .column(nx, ny)
                    .map(|j| (self.column_and_z(j).2, j))
                    .filter(|&(zj, _)| zj < z - self.steps.max_drop)
                    .max_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((_, j)) = landing.filter(|&(_, j)| self.is_clear(j)) {
                    out.entry(i).or_default().push(j);
                }
            }
        }
        out
    }

    /// Put back every place [`Self::repair_around`] took out. Answers how many.
    pub(crate) fn restore_repaired(&mut self) -> usize {
        self.dirty.clear();
        let mut n = 0;
        if let Some(g) = self.ground.as_mut() {
            n += g.repaired.len();
            g.repaired.clear();
        }
        for node in &mut self.nodes {
            if node.flags & REPAIRED != 0 {
                node.flags &= !REPAIRED;
                n += 1;
            }
        }
        n
    }

    /// The landblock this region's space belongs to.
    #[must_use]
    pub fn origin_block(&self) -> LandblockId {
        block_id(self.origin.0, self.origin.1).unwrap_or(LandblockId(0))
    }
}
