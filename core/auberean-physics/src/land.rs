//! The world's cells for the shared transition: each 20 m land cell's ground (eight triangles, two
//! to each 10 m square) and the solid parts of the objects placed in it, handed to the core as cells
//! this world builds itself.
//!
//! Blocks are built from the world's records when they are added ([`Land::add_block`]): the
//! terrain from the landblock's heights, and every drawn entity of the landblock info that has a
//! physics tree, registered in each land cell its bounding sphere reaches.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use auberean_assets::landblock::Landblock;
use auberean_assets::property::{PropertyCollection, PropertyList, PropertyValue};
use auberean_world::land::LandConstants;
use auberean_world::math::over_polygon;
use auberean_world::property::BlockProperties;
use auberean_world::records::{RegionSet, WorldRecords};
use auberean_world::scenery::on_road;
use auberean_world::terrain::{vertex_materials, LandSource as TerrainSource, VertexMaterials};
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
/// The group of an interior cell's own properties, and the one that names its physics material.
const CELL_GROUP: u32 = 0x4000_0010;
const CELL_MATERIAL: u32 = 0x4000_005F;
/// The property of that group that names the kind of ambient sound of the cell, and the kind of a
/// cell that names none.
const CELL_AMBIENT_KIND: u32 = 0x4000_0060;
pub const INTERIOR_KIND: u32 = 2;

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
/// first. One whose properties make it something some mover passes through ([`Solid::marked`])
/// is met as a placed object of the world, which asks; every other is solid to all, and its land
/// cells answer for it.
#[derive(Debug, Clone)]
pub struct Solid {
    pub part: PhysicsPart,
    /// The object's whole scale along each axis, which its tree is tested at.
    pub scale: Vec3,
    pub bounds: Sphere,
    pub render_box: BBox,
    pub properties: Vec<Arc<auberean_assets::property::PropertyCollection>>,
    /// Its properties make it something some mover passes through.
    pub marked: bool,
}

/// Placed entities that collide through cylinders or spheres, each where it stands.
pub type Placed = Vec<(Position, Arc<Shape>)>;

/// A solid of a block reaching a land cell of a block next to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overhang {
    /// The other block, blocks east and north of this one.
    pub dx: i8,
    pub dy: i8,
    /// The land cell it reaches there.
    pub cx: u8,
    pub cy: u8,
    /// The solid, by its index in this block.
    pub solid: u32,
}

/// One block's collision: its ground and its solids, by land cell.
#[derive(Debug)]
pub struct Block {
    pub x: u8,
    pub y: u8,
    pub geometry: LandGeometry,
    pub solids: Vec<Solid>,
    /// Per land cell (x major), the solids its area reaches.
    pub cell_solids: Vec<Vec<u32>>,
    /// The solids that reach over the block's edge, and into which cells of the blocks around it.
    pub overhangs: Vec<Overhang>,
    /// The height of the block's water surface, if it has water.
    pub water: Option<f32>,
    /// The placed entities that collide through their setups' cylinders or spheres, each where it
    /// stands ([`crate::object::placed_entity`]).
    pub objects: Placed,
    /// Whether a body may walk on it: the world's passability map.
    pub passable: bool,
    /// The ground's and the road's physics materials at its vertices, and whether each vertex is
    /// road.
    pub materials: Option<GroundMaterials>,
    /// What a body in its water touches.
    pub water_material: u32,
}

/// A block's ground for what a body stands on.
#[derive(Clone, Debug)]
pub struct GroundMaterials {
    pub vertices: VertexMaterials,
    /// Its landscape words, and the constants that read them.
    pub words: Vec<u32>,
    pub land: Arc<LandConstants>,
}

impl Block {
    /// Stands a solid in the block: in the lists of the land cells it reaches, and among the
    /// overhangs for those of the blocks around.
    pub fn add_solid(&mut self, solid: Solid) {
        let i = u32::try_from(self.solids.len()).unwrap_or(u32::MAX);
        register(&mut self.cell_solids, &mut self.overhangs, &solid.bounds, i);
        self.solids.push(solid);
    }

    /// The physics material under point `at` of the block: of the vertex nearest the point, the
    /// road's when the point is on the road, else the ground's.
    // D2-03662, D2-03636, D2-13217, D2-03829
    #[must_use]
    pub fn physics_material(&self, at: Vec3) -> u32 {
        let Some(m) = &self.materials else {
            return 0;
        };
        let step = m.land.grid().vertex_spacing();
        let side = self.geometry.vertices_per_side;
        let nearest = |v: f32| -> Option<usize> {
            #[allow(clippy::cast_possible_truncation)]
            let k = (f64::from(v) / f64::from(step)).floor() as i64;
            #[allow(clippy::cast_precision_loss)]
            let past = v - k as f32 * step;
            usize::try_from(if past <= step * 0.5 { k } else { k + 1 }).ok()
        };
        let (Some(i), Some(j)) = (nearest(at.x), nearest(at.y)) else {
            return 0;
        };
        let k = side * i + j;
        let list = if on_road(&m.land, side, &m.words, at.x, at.y) {
            &m.vertices.road
        } else {
            &m.vertices.ground
        };
        list.get(k).copied().unwrap_or(0)
    }
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
            overhangs: Vec::new(),
            water: None,
            objects: Vec::new(),
            passable: true,
            materials: None,
            water_material: 0,
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

    /// The ground triangles of land cell `(cx, cy)`: its squares taken x-major, two to a square,
    /// none over a hole.
    // D2-03820, D2-03760
    pub(crate) fn cell_triangles(
        &self,
        cx: usize,
        cy: usize,
    ) -> impl Iterator<Item = Polygon> + '_ {
        let side = self.geometry.squares_per_side;
        let per = side / CELLS_PER_SIDE;
        (cx * per..(cx + 1) * per)
            .flat_map(move |i| (cy * per..(cy + 1) * per).map(move |j| i * side + j))
            .filter(|&n| !self.geometry.holes.get(n).copied().unwrap_or(false))
            .flat_map(|n| self.geometry.triangles[2 * n..2 * n + 2].iter())
            .map(|t| Polygon {
                vertices: t
                    .v
                    .iter()
                    .map(|&k| self.geometry.vertices[k as usize])
                    .collect(),
                plane: t.plane,
                pos_surface: 1,
            })
    }
}

/// One land cell, answering for its ground and its solids.
#[derive(Debug)]
pub struct LandCell {
    pub id: CellId,
    pub block: Arc<Block>,
    pub cx: usize,
    pub cy: usize,
    /// The solids of the blocks around that reach this cell, each with its block.
    overhanging: Vec<(Arc<Block>, u32)>,
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
        // The solids standing in the cell, but for those a mover walks through.
        let k = self.cx * CELLS_PER_SIDE + self.cy;
        let own = self.block.cell_solids[k]
            .iter()
            .map(|&i| &self.block.solids[i as usize]);
        let over = self.overhanging.iter().map(|(b, i)| &b.solids[*i as usize]);
        for solid in own
            .chain(over)
            .filter(|s| !s.marked && !crate::object::uneven(s.scale))
        {
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

/// What an interior cell adds to the material of a body in it.
#[derive(Clone, Debug, Default)]
pub struct CellMaterial {
    /// Its landblock's properties.
    pub block: Arc<PropertyCollection>,
    /// Its own.
    pub own: Option<PropertyCollection>,
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
    /// The properties laid over the world.
    properties: OnceLock<Option<Arc<BlockProperties>>>,
    /// What interior cells add to the material of a body in them, by full id: their landblock's
    /// properties, then their own.
    cell_materials: RwLock<HashMap<u32, Arc<CellMaterial>>>,
    /// The interior cells with doorways out that touch each land cell, by the land cell's full id.
    touching: RwLock<HashMap<u32, Vec<CellId>>>,
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
            properties: OnceLock::new(),
            cell_materials: RwLock::new(HashMap::new()),
            touching: RwLock::new(HashMap::new()),
            table: Box::new([0.0; LAND_HEIGHT_TABLE_LEN]),
        }
    }

    /// Add an interior cell.
    pub fn insert_cell(&self, cell: EnvCellGeometry) {
        if let Ok(mut c) = self.cells.write() {
            c.insert(cell.id.0, Arc::new(cell));
        }
    }

    /// Tells each land cell of block `(x, y)` the interior cells that touch it, from `cells`, the low
    /// words of the interior cells the block sees from outside: each with a doorway out and a physics
    /// tree touches the land cells under the box around its tree's sphere, reckoned from the land
    /// cell its middle lies over. A land cell past the block's edge counts as the cell at the same
    /// place in this block.
    // D2-03669, D2-03877, D2-04114
    pub fn connect_cells(&self, x: u8, y: u8, cells: &[u32]) {
        let block = u32::from(x) << 24 | u32::from(y) << 16;
        for &low in cells {
            let id = CellId(block | low);
            let Some(g) = self.cell(id) else {
                continue;
            };
            if !crate::camera::touches_outside(&g) {
                continue;
            }
            let Some(sphere) = g
                .physics_bsp
                .as_ref()
                .and_then(|t| t.root())
                .map(|n| n.sphere)
            else {
                continue;
            };
            let (mut over, mut origin) = (id, g.frame.origin);
            let _ = crate::cells::adjust_to_outside(&mut over, &mut origin);
            let at = Position::new(over, Frame::new(origin, g.frame.rotation));
            let found = crate::cells::find_cells_of_spheres(self, &at, &[sphere]);
            let Ok(mut touching) = self.touching.write() else {
                return;
            };
            for c in found {
                if crate::interior::is_interior(c) || c.index() == 0 {
                    continue;
                }
                let k = usize::from(c.index()) - 1;
                let here = land_cell_id(x, y, k / CELLS_PER_SIDE, k % CELLS_PER_SIDE);
                touching.entry(here.0).or_default().push(id);
            }
        }
    }

    /// The interior cells that touch land cell `id`.
    #[must_use]
    pub fn touching_cells(&self, id: CellId) -> Vec<CellId> {
        self.touching
            .read()
            .ok()
            .and_then(|t| t.get(&id.0).cloned())
            .unwrap_or_default()
    }

    /// The properties laid over the world, taken once.
    pub fn properties_or(
        &self,
        read: impl FnOnce() -> Option<Arc<BlockProperties>>,
    ) -> Option<&Arc<BlockProperties>> {
        self.properties.get_or_init(read).as_ref()
    }

    /// What a body in cell `id`'s water touches: of the landblock, when it is here.
    // D2-03663
    #[must_use]
    pub fn water_material(&self, id: CellId) -> u32 {
        let lb = id.landblock();
        self.block(lb.x(), lb.y()).map_or(0, |b| b.water_material)
    }

    /// What a body standing at `at` in cell `id` touches: a land cell's ground; in an interior cell
    /// the material its own properties name, the last of them, none if they name none.
    // D2-03497, D2-03528, D2-03606
    #[must_use]
    pub fn ground_material(&self, id: CellId, at: Vec3) -> u32 {
        if crate::interior::is_interior(id) {
            let Some(c) = self
                .cell_materials
                .read()
                .ok()
                .and_then(|c| c.get(&id.0).cloned())
            else {
                return 0;
            };
            let properties = c.own.iter().flat_map(|p| p.groups.iter());
            return properties
                .filter(|g| g.group == CELL_GROUP)
                .flat_map(|g| g.properties.iter())
                .filter(|p| p.id == CELL_MATERIAL)
                .fold(0, |kept, p| match p.value {
                    PropertyValue::DataFile(d) => d.0,
                    _ => kept,
                });
        }
        let lb = id.landblock();
        self.block(lb.x(), lb.y())
            .map_or(0, |b| b.physics_material(at))
    }

    /// The kind of ambient sound an interior cell makes: what its own properties name, else the
    /// plain interior's.
    // D2-03606, D2-03545
    #[must_use]
    pub fn cell_ambient_kind(&self, id: CellId) -> u32 {
        let Some(c) = self
            .cell_materials
            .read()
            .ok()
            .and_then(|c| c.get(&id.0).cloned())
        else {
            return INTERIOR_KIND;
        };
        c.own
            .iter()
            .flat_map(|p| p.groups.iter())
            .filter(|g| g.group == CELL_GROUP)
            .flat_map(|g| g.properties.iter())
            .filter(|p| p.id == CELL_AMBIENT_KIND)
            .fold(INTERIOR_KIND, |kept, p| match p.value {
                PropertyValue::Enum(k) => k,
                _ => kept,
            })
    }

    /// Add what an interior cell adds to the material of a body in it.
    pub fn insert_cell_material(&self, id: CellId, material: CellMaterial) {
        if let Ok(mut c) = self.cell_materials.write() {
            c.insert(id.0, Arc::new(material));
        }
    }

    /// Updates `material` with what holds at `at` in cell `id`: what the region lays over the block,
    /// and in an interior cell its landblock's properties and then its own.
    // D2-03828, D2-03560, D2-13692
    pub fn update_cell_material(
        &self,
        id: CellId,
        at: &Position,
        material: &mut PropertyCollection,
        list: &PropertyList,
    ) {
        let lb = at.cell.landblock();
        if let Some(Some(p)) = self.properties.get() {
            crate::material::update_block_material(
                p,
                material,
                list,
                u32::from(lb.x()),
                u32::from(lb.y()),
                at.frame.origin,
            );
        }
        let cell = self
            .cell_materials
            .read()
            .ok()
            .and_then(|c| c.get(&id.0).cloned());
        if let Some(c) = cell {
            material.update_collection(&c.block);
            if let Some(own) = &c.own {
                material.update_collection(own);
            }
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

    /// The interior cell, of those added, that point `at` of its landblock's frame lies in: the
    /// lowest id of any that hold it.
    // D2-03568
    #[must_use]
    pub fn find_in_visible_cells(&self, at: Vec3) -> Option<CellId> {
        let cells = self.cells.read().ok()?;
        let mut ids: Vec<u32> = cells.keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter().map(CellId).find(|id| {
            cells.get(&id.0).is_some_and(|g| {
                let local = dereth_physics::math::globaltolocal(&g.frame, at);
                g.cell_bsp
                    .as_ref()
                    .is_none_or(|t| t.point_inside_cell_bsp(local))
            })
        })
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
        self.solids_near(block, from, to)
            .iter()
            .flat_map(|s| s.hits(from, to))
            .collect()
    }

    /// The solids of every block whose spheres the line from `from` to `to` (in `block`'s frame)
    /// reaches.
    pub(crate) fn solids_near(&self, block: LandblockId, from: Vec3, to: Vec3) -> Vec<SolidNear> {
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
            for (index, s) in b.solids.iter().enumerate() {
                if segment_distance(s.bounds.center, f, t) <= s.bounds.radius {
                    out.push(SolidNear {
                        block: Arc::clone(b),
                        index,
                        off,
                    });
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
        self.add_block_near(records, region, trees, &mut NearBlocks::default(), x, y)
    }

    /// As [`Land::add_block`], for one of many blocks read in turn along the world's columns:
    /// a block's landblock is read once for it and the blocks around it.
    pub fn add_block_near(
        &self,
        records: &WorldRecords,
        region: &RegionSet,
        trees: &mut Trees,
        near: &mut NearBlocks,
        x: u8,
        y: u8,
    ) -> bool {
        self.properties_or(|| region.properties.clone());
        let Some(block) = build_near(records, region, trees, near, x, y) else {
            return false;
        };
        self.insert(block);
        true
    }

    // D2-03632, D2-03839, D2-03838
    /// The solids of the blocks around block `(x, y)` that reach its land cell `(cx, cy)`: only a
    /// cell on the block's edge has any.
    #[must_use]
    pub fn overhanging(&self, x: u8, y: u8, cx: usize, cy: usize) -> Vec<(Arc<Block>, u32)> {
        let last = CELLS_PER_SIDE - 1;
        let mut out = Vec::new();
        if cx != 0 && cy != 0 && cx != last && cy != last {
            return out;
        }
        for dx in -1i16..=1 {
            for dy in -1i16..=1 {
                if (dx, dy) == (0, 0)
                    || (dx < 0 && cx != 0)
                    || (dx > 0 && cx != last)
                    || (dy < 0 && cy != 0)
                    || (dy > 0 && cy != last)
                {
                    continue;
                }
                let (Ok(nx), Ok(ny)) = (
                    u8::try_from(i16::from(x) + dx),
                    u8::try_from(i16::from(y) + dy),
                ) else {
                    continue;
                };
                let Some(b) = self.block(nx, ny) else {
                    continue;
                };
                for o in &b.overhangs {
                    #[allow(clippy::cast_possible_truncation)]
                    if i16::from(o.dx) == -dx
                        && i16::from(o.dy) == -dy
                        && usize::from(o.cx) == cx
                        && usize::from(o.cy) == cy
                    {
                        out.push((Arc::clone(&b), o.solid));
                    }
                }
            }
        }
        out
    }

    fn land_cell(&self, id: CellId) -> Option<LandCell> {
        let index = usize::from(id.index());
        if !(1..=CELLS_PER_SIDE * CELLS_PER_SIDE).contains(&index) {
            return None;
        }
        let lb = id.landblock();
        let block = self.block(lb.x(), lb.y())?;
        let (cx, cy) = ((index - 1) / CELLS_PER_SIDE, (index - 1) % CELLS_PER_SIDE);
        Some(LandCell {
            id,
            block,
            cx,
            cy,
            overhanging: self.overhanging(lb.x(), lb.y(), cx, cy),
        })
    }
}

/// A solid of some block near a line, and how to bring the line into that block's frame.
#[derive(Debug)]
pub struct SolidNear {
    block: Arc<Block>,
    index: usize,
    off: Vec3,
}

impl SolidNear {
    /// Where the line from `from` to `to` passes through this solid, as the fraction of the way it
    /// meets each polygon and the polygon's facing, turned into the world.
    fn hits(&self, from: Vec3, to: Vec3) -> Vec<(f32, Vec3)> {
        let mut out = Vec::new();
        let s = &self.block.solids[self.index];
        let Some(tree) = s.part.physics_bsp.as_ref() else {
            return out;
        };
        let (f, t) = (from.add(self.off), to.add(self.off));
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
        out
    }
}

/// What of `solids` the line from `from` to `to` (in its block's frame) meets over land cell
/// `(cx, cy)` of that block: a solid reaching over from another block is met where it stands over
/// this one.
pub(crate) fn solids_hit_in_cell(
    solids: &[SolidNear],
    (cx, cy): (usize, usize),
    from: Vec3,
    to: Vec3,
) -> Vec<(f32, Vec3)> {
    let index = |v: f32| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let i = (v / CELL_LENGTH).floor().max(0.0) as usize;
        i.min(CELLS_PER_SIDE - 1)
    };
    solids
        .iter()
        .flat_map(|s| s.hits(from, to))
        .filter(|(t, _)| {
            let at = from.add(to.sub(from).mul(*t));
            (index(at.x), index(at.y)) == (cx, cy)
        })
        .collect()
}

/// The landblocks read for the blocks being built: those of the column being built and the
/// columns beside it. A block needs its own and the eight around it.
#[derive(Debug, Default)]
pub struct NearBlocks {
    kept: HashMap<(i32, i32), Option<Landblock>>,
    column: Option<i32>,
}

impl NearBlocks {
    fn read(&mut self, records: &WorldRecords, x: i32, y: i32) {
        self.kept
            .entry((x, y))
            .or_insert_with(|| records.landblock(x, y));
    }

    /// How many blocks' landblocks it holds or knows the world lacks.
    #[must_use]
    pub fn kept(&self) -> usize {
        self.kept.len()
    }

    /// Now building in column `x`: forget what is further from it than the columns beside it.
    fn at_column(&mut self, x: i32) {
        if self.column != Some(x) {
            self.kept.retain(|k, _| (k.0 - x).abs() <= 1);
            self.column = Some(x);
        }
    }
}

impl TerrainSource for NearBlocks {
    fn landblock(&self, x: i32, y: i32) -> Option<&Landblock> {
        self.kept.get(&(x, y))?.as_ref()
    }
}

/// Build block `(x, y)`.
pub fn build(
    records: &WorldRecords,
    region: &RegionSet,
    trees: &mut Trees,
    x: u8,
    y: u8,
) -> Option<Block> {
    build_near(records, region, trees, &mut NearBlocks::default(), x, y)
}

/// Build block `(x, y)`, its landblock and those around it read through `near`.
// D2-03759, D2-03687, D2-03635, D2-03764
pub fn build_near(
    records: &WorldRecords,
    region: &RegionSet,
    trees: &mut Trees,
    near: &mut NearBlocks,
    x: u8,
    y: u8,
) -> Option<Block> {
    let (bx, by) = (i32::from(x), i32::from(y));
    near.at_column(bx);
    near.read(records, bx, by);
    TerrainSource::landblock(near, bx, by)?;
    for dx in -1..=1 {
        for dy in -1..=1 {
            near.read(records, bx + dx, by + dy);
        }
    }
    let lb = TerrainSource::landblock(near, bx, by)?;
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
    let mut overhangs = Vec::new();
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
                register(&mut cell_solids, &mut overhangs, &s.bounds, i);
                solids.push(s);
            }
        }
    }
    let lowest = geometry
        .vertices
        .iter()
        .map(|v| v.z)
        .fold(f32::INFINITY, f32::min);
    let water_block = region
        .water
        .as_ref()
        .and_then(|w| w.block(land, bx, by, lowest));
    let water_material = water_block.as_ref().map_or(0, |w| w.physics_material.0);
    let water = water_block.map(|w| w.height);
    let materials =
        vertex_materials(land, region.surfaces(), &*near, bx, by).map(|vertices| GroundMaterials {
            vertices,
            words: lb.vertices.clone(),
            land: Arc::new(land.clone()),
        });
    Some(Block {
        x,
        y,
        geometry,
        solids,
        cell_solids,
        overhangs,
        water,
        objects,
        passable: region.passable(u32::from(x), u32::from(y)),
        materials,
        water_material,
    })
}

/// Register solid `i` in every land cell its bounds reach: those of its own block in the cells'
/// lists, those of the blocks around it as overhangs.
fn register(cell_solids: &mut [Vec<u32>], overhangs: &mut Vec<Overhang>, bounds: &Sphere, i: u32) {
    let cell = |v: f32| dereth_primitives::num::floor_to_i32(v / CELL_LENGTH);
    let side = i32::try_from(CELLS_PER_SIDE).unwrap_or(i32::MAX);
    let (x0, x1) = (
        cell(bounds.center.x - bounds.radius),
        cell(bounds.center.x + bounds.radius),
    );
    let (y0, y1) = (
        cell(bounds.center.y - bounds.radius),
        cell(bounds.center.y + bounds.radius),
    );
    for cx in x0..=x1 {
        for cy in y0..=y1 {
            let (dx, dy) = (cx.div_euclid(side), cy.div_euclid(side));
            let (lx, ly) = (cx.rem_euclid(side), cy.rem_euclid(side));
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            if dx == 0 && dy == 0 {
                cell_solids[lx as usize * CELLS_PER_SIDE + ly as usize].push(i);
            } else if dx.abs() <= 1 && dy.abs() <= 1 {
                overhangs.push(Overhang {
                    dx: dx as i8,
                    dy: dy as i8,
                    cx: lx as u8,
                    cy: ly as u8,
                    solid: i,
                });
            }
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

    // D2-03842, D2-03843
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
