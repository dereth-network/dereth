//! The world's cells for the shared transition: each 20 m land cell's ground (eight triangles, two
//! to each 10 m square) and the solid parts of the objects placed in it, handed to the core as cells
//! this world builds itself.
//!
//! Blocks are built from the world's records when they are added ([`Land::add_block`]): the
//! terrain from the landblock's heights, and every drawn entity of the landblock info that has a
//! physics tree, registered in each land cell its bounding sphere reaches.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use auberean_world::math::over_polygon;
use auberean_world::records::{RegionSet, WorldRecords};
use dereth_physics::cell::{Cell, CellArray, CellInfo, HostCell};
use dereth_physics::geom::{Plane, Polygon, Sphere};
use dereth_physics::globals::LAND_HEIGHT_TABLE_LEN;
use dereth_physics::source::{EnvCellGeometry, LandSource, PhysicsPart};
use dereth_physics::transition::{collide, Transition, TransitionCtx, TransitionState, TreeScale};
use dereth_physics::{LandblockCollision, V3};
use dereth_primitives::{CellId, Frame, LandblockId, Position, Quat, Vec3};
use dereth_terrain::land::mesh::{land_geometry, LandGeometry};

use crate::geom::{BBox, BoxOps};
use crate::mesh::Trees;
use crate::object::Shape;
use crate::rules::BLOCK_LENGTH;

/// Land cells to a block side.
pub const CELLS_PER_SIDE: usize = 8;
/// The side of a land cell, in meters.
pub const CELL_LENGTH: f32 = BLOCK_LENGTH / 8.0;
/// The terrain kind that opens a hole in the ground.
const HOLE_TERRAIN: u32 = 0x1E;

/// A land cell's id: block `(x, y)`, cell `(cx, cy)` (x major, numbered from 1).
#[must_use]
pub fn land_cell_id(x: u8, y: u8, cx: usize, cy: usize) -> CellId {
    let index = u32::try_from(cx * CELLS_PER_SIDE + cy + 1).unwrap_or(0);
    CellId(u32::from(x) << 24 | u32::from(y) << 16 | index)
}

/// The land cell of a point in a block's frame, as `(cx, cy)`.
#[must_use]
pub fn cell_of(local: Vec3) -> Option<(usize, usize)> {
    if !(0.0..BLOCK_LENGTH).contains(&local.x) || !(0.0..BLOCK_LENGTH).contains(&local.y) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((
        (local.x / CELL_LENGTH) as usize,
        (local.y / CELL_LENGTH) as usize,
    ))
}

/// A solid part placed in a block: its collision geometry and where it stands, the sphere
/// around it in the block's frame, the box around what its object draws, in the object's frame
/// and scaled, and the property collections of the entities its object is made from, innermost
/// first.
#[derive(Debug, Clone)]
pub struct Solid {
    pub part: PhysicsPart,
    /// The object's whole scale along each axis, which its tree is tested at.
    pub scale: Vec3,
    pub bounds: Sphere,
    pub render_box: BBox,
    pub properties: Vec<Arc<auberean_assets::property::PropertyCollection>>,
}

/// Placed entities that collide through cylinders or spheres, each where it stands.
pub type Placed = Vec<(Position, Arc<Shape>)>;

/// One block's collision: its ground and its solids, by land cell.
#[derive(Debug)]
pub struct Block {
    pub x: u8,
    pub y: u8,
    pub geometry: LandGeometry,
    pub solids: Vec<Solid>,
    /// Per land cell (x major), the solids its area reaches.
    pub cell_solids: Vec<Vec<u32>>,
    /// The height of the block's water surface, if it has water.
    pub water: Option<f32>,
    /// The placed entities that collide through their setups' cylinders or spheres, each where it
    /// stands ([`crate::object::placed_entity`]).
    pub objects: Placed,
    /// Whether a body may walk on it: the world's passability map.
    pub passable: bool,
}

impl Block {
    /// A block of level ground at height `z` with nothing on it.
    #[must_use]
    pub fn flat(x: u8, y: u8, z: f32) -> Self {
        let side = CELLS_PER_SIDE * 2;
        let n = side + 1;
        #[allow(clippy::cast_precision_loss)]
        let step = BLOCK_LENGTH / side as f32;
        let mut vertices = Vec::with_capacity(n * n);
        for i in 0..n {
            for j in 0..n {
                #[allow(clippy::cast_precision_loss)]
                vertices.push(Vec3::new(i as f32 * step, j as f32 * step, z));
            }
        }
        let plane = Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -z,
        };
        let mut triangles = Vec::with_capacity(2 * side * side);
        for i in 0..side {
            for j in 0..side {
                let v = |a: usize, b: usize| u32::try_from(a * n + b).unwrap_or(0);
                let (sw, se, nw, ne) = (v(i, j), v(i + 1, j), v(i, j + 1), v(i + 1, j + 1));
                triangles.push(dereth_terrain::land::mesh::LandTriangle {
                    v: [sw, se, nw],
                    plane,
                });
                triangles.push(dereth_terrain::land::mesh::LandTriangle {
                    v: [se, ne, nw],
                    plane,
                });
            }
        }
        let geometry = LandGeometry {
            squares_per_side: side,
            vertices_per_side: n,
            vertices,
            sw_to_ne_cut: vec![false; side * side],
            triangles,
            holes: vec![false; side * side],
        };
        Self {
            x,
            y,
            geometry,
            solids: Vec::new(),
            cell_solids: vec![Vec::new(); CELLS_PER_SIDE * CELLS_PER_SIDE],
            water: None,
            objects: Vec::new(),
            passable: true,
        }
    }

    /// The ground triangle under a point in the block's frame, with its plane: of the triangles
    /// of the land cell holding the point, its squares taken x-major and each square's two in
    /// order, the first the point lies over (within the band); none over a hole.
    // D2-03822
    #[must_use]
    pub fn ground_under(&self, local: Vec3) -> Option<(Polygon, Plane)> {
        let side = self.geometry.squares_per_side;
        let per = side / CELLS_PER_SIDE;
        let (cx, cy) = cell_of(local)?;
        let p = [local.x, local.y, local.z];
        for i in cx * per..(cx + 1) * per {
            for j in cy * per..(cy + 1) * per {
                let n = i * side + j;
                if self.geometry.holes.get(n).copied().unwrap_or(false) {
                    continue;
                }
                for t in &self.geometry.triangles[2 * n..2 * n + 2] {
                    let v = t.v.map(|k| self.geometry.vertices[k as usize]);
                    if over_polygon(&v.map(|c| [c.x, c.y, c.z]), p, true) {
                        return Some((Polygon::new(v.to_vec()), t.plane));
                    }
                }
            }
        }
        None
    }
}

/// One land cell, answering for its ground and its solids.
#[derive(Debug)]
pub struct LandCell {
    pub id: CellId,
    pub block: Arc<Block>,
    pub cx: usize,
    pub cy: usize,
}

impl HostCell for LandCell {
    fn pos(&self, id: CellId) -> Position {
        Position::new(id, Frame::new(Vec3::ZERO, Quat::IDENTITY))
    }

    // D2-03824
    fn point_in_cell(&self, local: Vec3) -> bool {
        // The square a point lies over, numbered on from the block's own squares.
        #[allow(clippy::cast_possible_truncation)]
        let square = |v: f32| {
            let k = (f64::from(v) / f64::from(CELL_LENGTH)).floor() as i64;
            usize::try_from(k.rem_euclid(CELLS_PER_SIDE as i64)).unwrap_or(0)
        };
        (square(local.x), square(local.y)) == (self.cx, self.cy)
    }

    // D2-03835
    fn find_env_collisions(&self, ctx: &TransitionCtx<'_>, t: &mut Transition) -> TransitionState {
        if !self.block.passable {
            self.turn_back(t);
            return TransitionState::Adjusted;
        }
        let off = t.rules.block_offset(t.sphere_path.check_pos.cell, self.id);
        // The ground: the triangle under the sphere's lowest point, when it is in this cell.
        let low = t.sphere_path.global_low_point.sub(off);
        if self.point_in_cell(low) {
            if let Some((_, plane)) = self.block.ground_under(low) {
                let mut s = t.sphere_path.global_sphere[0];
                s.center = s.center.sub(off);
                let r = collide::validate_walkable(t, &s, &plane, false, 0.0, self.id);
                if r != TransitionState::Ok {
                    return r;
                }
            }
        }
        // The solids standing in the cell.
        let k = self.cx * CELLS_PER_SIDE + self.cy;
        for &i in &self.block.cell_solids[k] {
            let solid = &self.block.solids[i as usize];
            let r = collide::object_part_find_obj_collisions_scaled(
                ctx,
                t,
                &solid.part,
                TreeScale::axes(solid.scale),
            );
            if r != TransitionState::Ok {
                if !t
                    .object_info
                    .has(dereth_physics::transition::ObjectInfoState::CONTACT)
                {
                    t.collision_info.collided_with_environment = true;
                }
                return r;
            }
        }
        TransitionState::Ok
    }
}

impl LandCell {
    /// Turns a move into this cell back at the land cell's edge: by where the body stood against
    /// the cell (north or south of it, else east or west), the move's part along that axis is
    /// taken again the other way from where it got to, and that axis is what it collided along.
    // D2-03833
    fn turn_back(&self, t: &mut Transition) {
        #[allow(clippy::cast_precision_loss)]
        let center = |k: usize| CELL_LENGTH * (k as f32 + 0.5);
        let half = CELL_LENGTH * 0.5;
        let cell = Position::new(
            self.id,
            Frame::new(
                Vec3::new(center(self.cx), center(self.cy), 0.0),
                Quat::IDENTITY,
            ),
        );
        let path = &t.sphere_path;
        let from = t.rules.get_offset(&cell, &path.curr_pos);
        let mut d = t.rules.get_offset(&path.curr_pos, &path.check_pos);
        let normal = if from.y < -half {
            d.y = -d.y;
            Vec3::new(0.0, -1.0, 0.0)
        } else if from.y > half {
            d.y = -d.y;
            Vec3::new(0.0, 1.0, 0.0)
        } else {
            d.x = -d.x;
            Vec3::new(if from.x >= -half { 1.0 } else { -1.0 }, 0.0, 0.0)
        };
        t.sphere_path.cell_array_valid = false;
        t.sphere_path.check_pos.frame.origin = t.sphere_path.check_pos.frame.origin.add(d);
        t.sphere_path.cache_global_sphere(Some(d));
        t.collision_info.set_collision_normal(normal);
    }
}

/// The world's land, as the shared core asks for it.
#[derive(Debug)]
pub struct Land {
    blocks: RwLock<HashMap<(u8, u8), Arc<Block>>>,
    /// Interior cells by full id ([`crate::interior`]).
    cells: RwLock<HashMap<u32, Arc<EnvCellGeometry>>>,
    /// The solid parts of the entities placed in interior cells, by full id.
    cell_solids: RwLock<HashMap<u32, Arc<Vec<Solid>>>>,
    /// The entities placed in interior cells that collide through their setups' cylinders or
    /// spheres, by full id.
    cell_objects: RwLock<HashMap<u32, Arc<Placed>>>,
    table: Box<[f32; LAND_HEIGHT_TABLE_LEN]>,
}

impl Default for Land {
    fn default() -> Self {
        Self::new()
    }
}

impl Land {
    #[must_use]
    pub fn new() -> Self {
        Self {
            blocks: RwLock::new(HashMap::new()),
            cells: RwLock::new(HashMap::new()),
            cell_solids: RwLock::new(HashMap::new()),
            cell_objects: RwLock::new(HashMap::new()),
            table: Box::new([0.0; LAND_HEIGHT_TABLE_LEN]),
        }
    }

    /// Add an interior cell.
    pub fn insert_cell(&self, cell: EnvCellGeometry) {
        if let Ok(mut c) = self.cells.write() {
            c.insert(cell.id.0, Arc::new(cell));
        }
    }

    /// Add the solid parts of the entities placed in an interior cell.
    pub fn insert_cell_solids(&self, id: CellId, solids: Vec<Solid>) {
        if let Ok(mut c) = self.cell_solids.write() {
            c.insert(id.0, Arc::new(solids));
        }
    }

    /// Add the entities placed in an interior cell that collide through cylinders or spheres.
    pub fn insert_cell_objects(&self, id: CellId, objects: Placed) {
        if let Ok(mut c) = self.cell_objects.write() {
            c.insert(id.0, Arc::new(objects));
        }
    }

    /// The entities placed in an interior cell that collide through cylinders or spheres.
    #[must_use]
    pub fn cell_objects(&self, id: CellId) -> Option<Arc<Placed>> {
        self.cell_objects.read().ok()?.get(&id.0).cloned()
    }

    /// The solid parts of the entities placed in an interior cell.
    #[must_use]
    pub fn cell_solids(&self, id: CellId) -> Option<Arc<Vec<Solid>>> {
        self.cell_solids.read().ok()?.get(&id.0).cloned()
    }

    /// The interior cells of block `block` (the high word of their ids) that were added, in id
    /// order.
    #[must_use]
    pub fn interior_cells_of(&self, block: u32) -> Vec<CellId> {
        let Ok(c) = self.cells.read() else {
            return Vec::new();
        };
        let mut ids: Vec<CellId> = c
            .keys()
            .filter(|&&id| id & 0xFFFF_0000 == block & 0xFFFF_0000)
            .map(|&id| CellId(id))
            .collect();
        ids.sort_by_key(|c| c.0);
        ids
    }

    /// The interior cell with this id, when it was added.
    #[must_use]
    pub fn cell(&self, id: CellId) -> Option<Arc<EnvCellGeometry>> {
        self.cells.read().ok()?.get(&id.0).cloned()
    }

    /// The block at `(x, y)`, when it was added.
    // D2-03686, D2-10786, D2-03684
    #[must_use]
    pub fn block(&self, x: u8, y: u8) -> Option<Arc<Block>> {
        self.blocks.read().ok()?.get(&(x, y)).cloned()
    }

    /// Add an already built block.
    // D2-03694
    pub fn insert(&self, block: Block) {
        if let Ok(mut b) = self.blocks.write() {
            b.insert((block.x, block.y), Arc::new(block));
        }
    }

    /// Take block `(x, y)` out of the land (its ground no longer found), keeping it to be put back
    /// as it was; its interior cells stay.
    // D2-03690
    pub fn release_block(&self, x: u8, y: u8) -> Option<Arc<Block>> {
        self.blocks.write().ok()?.remove(&(x, y))
    }

    /// Put back a block taken out.
    pub fn recover_block(&self, block: Arc<Block>) {
        if let Ok(mut b) = self.blocks.write() {
            b.insert((block.x, block.y), block);
        }
    }

    /// The water plane of a land cell: its block's water surface, level; none in a block without
    /// water or in an interior cell.
    // D2-03821, D2-03754, D2-03751, D2-03752, D2-03753
    #[must_use]
    pub fn water_plane(&self, cell: CellId) -> Option<Plane> {
        let land = self.land_cell(cell)?;
        let h = land.block.water?;
        Some(Plane {
            normal: Vec3::new(0.0, 0.0, 1.0),
            d: -h,
        })
    }

    /// The ground's height under a point of `block`'s frame, and the ground's plane there.
    #[must_use]
    pub fn ground(&self, block: LandblockId, p: Vec3) -> Option<(f32, Plane)> {
        let (x, y, local) = normalize(block, p)?;
        let (_, plane) = self.block(x, y)?.ground_under(local)?;
        let n = plane.normal;
        if n.z.abs() < 1e-6 {
            return None;
        }
        Some((-(n.x * local.x + n.y * local.y + plane.d) / n.z, plane))
    }

    /// Where the line from `from` to `to` (in `block`'s frame) first meets the ground, or with
    /// `solids` also a solid object: the point and the surface's normal.
    #[must_use]
    pub fn trace(
        &self,
        block: LandblockId,
        from: Vec3,
        to: Vec3,
        solids: bool,
    ) -> Option<(Vec3, Vec3)> {
        let d = to.sub(from);
        let len = d.mag2().sqrt();
        if len < 1e-6 {
            return None;
        }
        let at = |t: f32| from.add(d.mul(t));
        let below = |t: f32| self.ground(block, at(t)).is_some_and(|(z, _)| at(t).z < z);
        // The ground: march in half-meter steps, then halve the step that crossed.
        let mut best: Option<(f32, Vec3)> = None;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let steps = ((len / 0.5).ceil() as usize).max(1);
        let mut prev = 0.0f32;
        for k in 1..=steps {
            #[allow(clippy::cast_precision_loss)]
            let t = k as f32 / steps as f32;
            if below(t) {
                let (mut lo, mut hi) = (prev, t);
                for _ in 0..16 {
                    let mid = 0.5 * (lo + hi);
                    if below(mid) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                let n = self
                    .ground(block, at(lo))
                    .map_or(Vec3::new(0.0, 0.0, 1.0), |(_, p)| p.normal);
                best = Some((lo, n));
                break;
            }
            prev = t;
        }
        if solids {
            for (t, n) in self.solid_hits(block, from, to) {
                if best.is_none_or(|(b, _)| t < b) {
                    best = Some((t, n));
                }
            }
        }
        best.map(|(t, n)| (at(t), n))
    }

    /// Every solid the line from `from` to `to` passes through, as the fraction of the way it
    /// meets it and the surface's normal.
    fn solid_hits(&self, block: LandblockId, from: Vec3, to: Vec3) -> Vec<(f32, Vec3)> {
        let r = crate::rules::rules();
        let mut out = Vec::new();
        let Ok(blocks) = self.blocks.read() else {
            return out;
        };
        for b in blocks.values() {
            let off = r.block_offset(
                land_cell_id(b.x, b.y, 0, 0),
                land_cell_id(block.x(), block.y(), 0, 0),
            );
            // The line in this block's frame.
            let (f, t) = (from.add(off), to.add(off));
            for s in &b.solids {
                if segment_distance(s.bounds.center, f, t) > s.bounds.radius {
                    continue;
                }
                let Some(tree) = s.part.physics_bsp.as_ref() else {
                    continue;
                };
                let frame = &s.part.pos.frame;
                let inv = if s.part.gfxobj_scale > 0.0 {
                    1.0 / s.part.gfxobj_scale
                } else {
                    1.0
                };
                let lf = dereth_physics::math::globaltolocal(frame, f).mul(inv);
                let lt = dereth_physics::math::globaltolocal(frame, t).mul(inv);
                for poly in &tree.polygons {
                    if let Some(u) = segment_polygon(lf, lt, &poly.vertices) {
                        let n = dereth_physics::math::localtoglobalvec(
                            dereth_physics::math::l2g(frame.rotation),
                            poly.plane.normal,
                        );
                        out.push((u, n));
                    }
                }
            }
        }
        out
    }

    /// Build block `(x, y)` from the world's records and add it: its ground, and the solid parts of
    /// its placed entities. Returns whether the world has the block.
    pub fn add_block(
        &self,
        records: &WorldRecords,
        region: &RegionSet,
        trees: &mut Trees,
        x: u8,
        y: u8,
    ) -> bool {
        let Some(block) = build(records, region, trees, x, y) else {
            return false;
        };
        self.insert(block);
        true
    }

    // D2-03632, D2-03839, D2-03838
    fn land_cell(&self, id: CellId) -> Option<LandCell> {
        let index = usize::from(id.index());
        if !(1..=CELLS_PER_SIDE * CELLS_PER_SIDE).contains(&index) {
            return None;
        }
        let lb = id.landblock();
        let block = self.block(lb.x(), lb.y())?;
        Some(LandCell {
            id,
            block,
            cx: (index - 1) / CELLS_PER_SIDE,
            cy: (index - 1) % CELLS_PER_SIDE,
        })
    }
}

/// Build block `(x, y)`.
// D2-03759, D2-03687, D2-03635, D2-03764
pub fn build(
    records: &WorldRecords,
    region: &RegionSet,
    trees: &mut Trees,
    x: u8,
    y: u8,
) -> Option<Block> {
    let (bx, by) = (i32::from(x), i32::from(y));
    let lb = records.landblock(bx, by)?;
    let land = &region.land;
    let holes: Vec<bool> = lb
        .vertices
        .iter()
        .map(|&w| land.word.terrain.of(w) == HOLE_TERRAIN)
        .collect();
    let grid = land.grid();
    let geometry = land_geometry(
        &land.spec,
        &lb.heights,
        &land.heights,
        bx,
        by,
        grid.cells_per_side,
        Some(&holes),
    );
    let mut solids = Vec::new();
    let mut cell_solids = vec![Vec::new(); CELLS_PER_SIDE * CELLS_PER_SIDE];
    let mut objects = Vec::new();
    if let Some(info) = records.landblock_info(bx, by) {
        // Any cell of the block will do: a part's cell only says which block's frame it is in.
        let any = land_cell_id(x, y, 0, 0);
        for e in &info.entities.entities {
            let at = e.offset.as_ref().map_or(Vec3::ZERO, |f| {
                Vec3::new(f.frame.origin.x, f.frame.origin.y, f.frame.origin.z)
            });
            let cell = cell_of(at).map_or(any, |(cx, cy)| land_cell_id(x, y, cx, cy));
            if let Some((pos, shape)) = crate::object::placed_entity(records, trees, e, cell) {
                objects.push((pos, Arc::new(shape)));
            }
            for s in crate::object::placed_solids(records, trees, e, any) {
                let i = u32::try_from(solids.len()).unwrap_or(u32::MAX);
                register(&mut cell_solids, &s.bounds, i);
                solids.push(s);
            }
        }
    }
    let lowest = geometry
        .vertices
        .iter()
        .map(|v| v.z)
        .fold(f32::INFINITY, f32::min);
    let water = region
        .water
        .as_ref()
        .and_then(|w| w.block(land, bx, by, lowest))
        .map(|w| w.height);
    Some(Block {
        x,
        y,
        geometry,
        solids,
        cell_solids,
        water,
        objects,
        passable: region.passable(u32::from(x), u32::from(y)),
    })
}

/// Register solid `i` in every land cell its bounds reach.
fn register(cell_solids: &mut [Vec<u32>], bounds: &Sphere, i: u32) {
    let lo = |v: f32| ((v - bounds.radius) / CELL_LENGTH).floor();
    let hi = |v: f32| ((v + bounds.radius) / CELL_LENGTH).floor();
    #[allow(clippy::cast_possible_truncation)]
    let clamp = |v: f32| (v.max(0.0) as usize).min(CELLS_PER_SIDE - 1);
    let (x0, x1) = (lo(bounds.center.x), hi(bounds.center.x));
    let (y0, y1) = (lo(bounds.center.y), hi(bounds.center.y));
    if x1 < 0.0 || y1 < 0.0 || x0 >= CELLS_PER_SIDE as f32 || y0 >= CELLS_PER_SIDE as f32 {
        return;
    }
    for cx in clamp(x0)..=clamp(x1) {
        for cy in clamp(y0)..=clamp(y1) {
            cell_solids[cx * CELLS_PER_SIDE + cy].push(i);
        }
    }
}

/// The distance from `c` to the segment from `a` to `b`.
pub(crate) fn segment_distance(c: Vec3, a: Vec3, b: Vec3) -> f32 {
    let ab = b.sub(a);
    let l2 = ab.mag2();
    let t = if l2 > 0.0 {
        (c.sub(a).dot(ab) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    c.sub(a.add(ab.mul(t))).mag2().sqrt()
}

/// Where the segment from `a` to `b` crosses polygon `v` (a fan from its first vertex), as the
/// fraction of the way.
pub(crate) fn segment_polygon(a: Vec3, b: Vec3, v: &[Vec3]) -> Option<f32> {
    let d = b.sub(a);
    let mut best: Option<f32> = None;
    for k in 1..v.len().saturating_sub(1) {
        let (p0, p1, p2) = (v[0], v[k], v[k + 1]);
        let (e1, e2) = (p1.sub(p0), p2.sub(p0));
        let h = d.cross(e2);
        let det = e1.dot(h);
        if det.abs() < 1e-9 {
            continue;
        }
        let f = 1.0 / det;
        let s = a.sub(p0);
        let u = f * s.dot(h);
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = s.cross(e1);
        let w = f * d.dot(q);
        if w < 0.0 || u + w > 1.0 {
            continue;
        }
        let t = f * e2.dot(q);
        if (0.0..=1.0).contains(&t) && best.is_none_or(|b| t < b) {
            best = Some(t);
        }
    }
    best
}

/// The block and point a point outside its block's square walks into.
fn normalize(block: LandblockId, p: Vec3) -> Option<(u8, u8, Vec3)> {
    let (dx, dy) = (
        dereth_primitives::num::floor_to_i32(p.x / BLOCK_LENGTH),
        dereth_primitives::num::floor_to_i32(p.y / BLOCK_LENGTH),
    );
    let x = u8::try_from(i32::from(block.x()) + dx).ok()?;
    let y = u8::try_from(i32::from(block.y()) + dy).ok()?;
    #[allow(clippy::cast_precision_loss)]
    let local = Vec3::new(
        p.x - dx as f32 * BLOCK_LENGTH,
        p.y - dy as f32 * BLOCK_LENGTH,
        p.z,
    );
    Some((x, y, local))
}

impl LandSource for Land {
    fn landblock(&self, _id: LandblockId) -> Option<Arc<LandblockCollision>> {
        None
    }

    fn height_table(&self) -> &[f32; LAND_HEIGHT_TABLE_LEN] {
        &self.table
    }

    fn env_cell(&self, cell: CellId) -> Option<Arc<EnvCellGeometry>> {
        self.cell(cell)
    }

    fn host_cell(&self, id: CellId) -> Option<Arc<dyn HostCell>> {
        self.land_cell(id).map(|c| Arc::new(c) as Arc<dyn HostCell>)
    }

    /// A transition that has left every cell it knew falls out into this world's 20 m land cells.
    // D2-05791
    fn adjust_to_outside(&self, id: &mut CellId, origin: &mut Vec3) -> bool {
        crate::cells::adjust_to_outside(id, origin)
    }

    /// A search that reaches the outdoors from inside, through a doorway out, takes in this world's
    /// 20 m land cells under the box around the spheres, each with its ground when its block is in.
    // D2-03834, D2-03832
    fn add_outside_cells(&self, pos: &Position, spheres: &[Sphere], arr: &mut CellArray) -> bool {
        for id in crate::cells::outside_cells(pos.cell, &BBox::of_spheres(spheres)) {
            let cell = self.land_cell(id).map(|c| Cell::Host {
                id,
                cell: Arc::new(c),
            });
            arr.add_cell(id, cell);
        }
        true
    }

    // D2-03850, D2-03834
    fn host_cell_list(
        &self,
        pos: &Position,
        spheres: &[Sphere],
        arr: &mut CellArray,
        want_container: bool,
        _hits_interior_cell: &mut bool,
    ) -> Option<Option<Cell>> {
        // Inside, the shared search walks the interior cells' doorways.
        if crate::interior::is_interior(pos.cell) {
            return None;
        }
        arr.cells.clear();
        arr.added_outside = true;
        let block = pos.cell.landblock();
        let mut add = |p: Vec3| {
            let Some((x, y, local)) = normalize(block, p) else {
                return;
            };
            let Some((cx, cy)) = cell_of(local) else {
                return;
            };
            let id = land_cell_id(x, y, cx, cy);
            if arr.contains(id) {
                return;
            }
            let cell = self.land_cell(id).map(|c| Cell::Host {
                id,
                cell: Arc::new(c),
            });
            arr.cells.push(CellInfo { cell_id: id, cell });
        };
        for s in spheres {
            for (dx, dy) in [
                (0.0, 0.0),
                (-1.0, -1.0),
                (1.0, -1.0),
                (-1.0, 1.0),
                (1.0, 1.0),
            ] {
                add(Vec3::new(
                    s.center.x + dx * s.radius,
                    s.center.y + dy * s.radius,
                    s.center.z,
                ));
            }
        }
        if !want_container {
            return Some(None);
        }
        let container = spheres.first().and_then(|s| {
            let (x, y, local) = normalize(block, s.center)?;
            let (cx, cy) = cell_of(local)?;
            let id = land_cell_id(x, y, cx, cy);
            self.land_cell(id).map(|cell| Cell::Host {
                id,
                cell: Arc::new(cell),
            })
        });
        Some(container)
    }
}
