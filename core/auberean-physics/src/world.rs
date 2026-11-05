//! The world every body moves against: one table of objects and, for every cell, the list of the
//! objects that reach into it, which the transition walks for each cell a moving body touches.
//!
//! The table holds the bodies the host puts in ([`World::insert_body`]), the objects of the world
//! it puts in ([`World::insert`]: signs, statues, anything with a look), and the scenery of the
//! world's records, which comes in by itself the first time a body moves in a block: the solid
//! parts placed in its interior cells, and the placed entities that collide through their setups'
//! cylinders or spheres. Each entry has a key the table gives it, counted up and never reused
//! while the table lives, and the object id the host gave it (none for scenery).
//!
//! An entry stands in the collision list of every cell its shape reaches ([`Shape::cross_cells`]),
//! worked out again whenever it is put somewhere else.
//!
//! Each entry also has a physics type, and an object that moves has collision settings and keeps
//! its collisions ([`Collisions`]): what is solid to what follows from those ([`crate::solid`]),
//! and the transition asks this world, through [`View`], to test each object a mover meets.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use auberean_assets::property::{PhysicsMaterial, PropertyGroup, PropertyList};
use dereth_physics::arena::{Arena, PhysHandle};
use dereth_physics::source::SetupPart;
use dereth_physics::transition::collide::{ObjGeometry, ObjectCollisions};
use dereth_physics::transition::TreeScale;
use dereth_physics::transition::{Transition, TransitionCtx, TransitionState};
use dereth_physics::{CellRuntime, LandSource, PhysicsObj, SetupGeometry, V3};
use dereth_primitives::shape::Sphere;
use dereth_primitives::{CellId, LandblockId, ObjectId, Position, Vec3};

use crate::cells;
use crate::collision::manager::{BodyKey, CollisionHost, Collisions};
use crate::collision::{settings, CollisionReport, CollisionSettings, ContactRecord};
use crate::ethereal::{flag_bit, PhysicsTypes};
use crate::geom::BBox;
use crate::land::Land;
use crate::object::Shape;
use crate::solid::{self, Ethereality, Party};

mod contact;
mod detect;
mod detection;
mod props;

pub use detect::GameObjects;
pub(crate) use detection::PHYSICS_SHAPES;
pub use detection::{split_ray, Detection, NoGameObjects};
pub(crate) use props::group;
pub use props::{entity_graphics_state, graphics, render};

/// The physics type the client gives the entities placed outside.
pub const STATIC_ETHEREAL: u32 = 0x4000_0038;
/// The physics type a new object that moves passes through, moving and being placed.
pub const MOVER_ETHEREAL: u32 = 0x4000_0024;

/// An object of the world as the bodies meet it.
#[derive(Clone, Debug)]
pub struct WorldObject {
    pub shape: Arc<Shape>,
    pub position: Position,
    /// Never moves.
    pub is_static: bool,
    /// Bodies pass through it whatever its physics type.
    pub passable: bool,
}

#[derive(Debug)]
struct Entry {
    handle: PhysHandle,
    id: u64,
    shape: Arc<Shape>,
    cells: Vec<CellId>,
    is_static: bool,
    /// The host says bodies pass through it.
    passable: bool,
    /// The physics types it is.
    physics_type: PhysicsTypes,
    /// The terrain type its collisions last showed, not yet taken by its body.
    terrain: Option<u32>,
    /// Its collisions, once it has collision settings.
    collisions: Option<Box<Collisions>>,
    /// Bodies that pass through it do not test it at all.
    // D2-06294
    ignore_collisions: bool,
    /// The radius within which it can be met, taken with the mover's and the step height.
    radius: f32,
    /// What it reported of its collisions, taken by the host.
    reports: Vec<CollisionReport>,
    /// Its own properties (its description's physics, gameplay and pusher properties), once it
    /// has any, and the physics material its description names (0: none).
    properties: Option<Box<PhysicsMaterial>>,
    material: u32,
    /// How it is drawn, as its drawing properties set it: the bits of
    /// [`graphics`].
    graphics_state: u32,
    /// Whether it is drawn in the game, once a drawing property has said.
    game_draw: Option<bool>,
    /// The scale along each axis its physics trees are tested at.
    tree_scale: Vec3,
    /// Placed by its cell or landblock: it stands in the cells the box around what it draws
    /// reaches, whatever it collides with.
    placed: bool,
}

/// The world's objects and the cells' collision lists.
#[derive(Debug, Default)]
pub struct World {
    objects: Arena<PhysicsObj>,
    cells: BTreeMap<u32, CellRuntime>,
    entries: BTreeMap<BodyKey, Entry>,
    by_id: BTreeMap<u64, BodyKey>,
    by_handle: BTreeMap<PhysHandle, BodyKey>,
    last_key: BodyKey,
    /// The blocks (high words) whose scenery is in: interior solids, placed entities outside and
    /// inside.
    interior_blocks: BTreeSet<u32>,
    placed_outside: BTreeSet<u32>,
    placed_inside: BTreeSet<u32>,
    /// The install's property list (the physics types' bits), when the host has given it.
    properties: PropertyList,
    materials: BTreeMap<u32, PhysicsMaterial>,
    /// The entries put in an interior cell not yet read, whose collision lists wait for it.
    waiting: BTreeSet<BodyKey>,
    /// The clock, seconds.
    now: f64,
    /// The entries whose other updates something woke ([`World::take_woken`]).
    woken: BTreeSet<BodyKey>,
    /// Who watches each cell, the objects standing in each, and what the watchers were told.
    watching: detection::Watching,
    /// The world's one stamp, counted on by each insert attempt of every move and each search:
    /// what a move or a search takes once is taken once for that count.
    physics_stamp: u64,
}

/// The ids the world's placed entities take among the objects' ids for the shared core: a bit
/// apart from the host's.
const SCENERY_ID: u32 = 0x8000_0000;

/// The containment radius of a shape standing at `scale`: the length of the scale times the
/// setup's radius.
// D2-06006, D2-06467, D2-06623
fn containment(shape: &Shape) -> f32 {
    let s = shape.scale;
    (s.x * s.x + s.y * s.y + s.z * s.z).sqrt() * shape.containment_radius
}

impl World {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The install's property list, for the physics types' bits and the properties of what is
    /// touched.
    pub fn set_property_list(&mut self, list: PropertyList) {
        self.properties = list;
    }

    /// A physics material the collisions may ask for.
    pub fn add_material(&mut self, id: u32, material: PhysicsMaterial) {
        self.materials.insert(id, material);
    }

    /// The clock the collisions keep time by, seconds.
    pub fn set_now(&mut self, now: f64) {
        self.now = now;
    }

    #[must_use]
    pub fn now(&self) -> f64 {
        self.now
    }

    /// The world's stamp, counted on by each insert attempt of every move and each search.
    #[must_use]
    pub fn physics_stamp(&self) -> u64 {
        self.physics_stamp
    }

    /// Counts the world's stamp on once.
    // D2-06978
    pub fn increment_physics_stamp(&mut self) {
        self.physics_stamp += 1;
    }

    /// A move's transition takes its insert attempts from the world's stamp.
    pub(crate) fn begin_attempts(&self, t: &mut Transition) {
        t.attempt = self.physics_stamp;
    }

    /// The world's stamp after a move's transition counted it on by its insert attempts.
    pub(crate) fn end_attempts(&mut self, t: &Transition) {
        self.physics_stamp = self.physics_stamp.max(t.attempt);
    }

    /// How many entries the table holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry with object id `id`.
    // D2-05766, D2-05965, D2-06977
    #[must_use]
    pub fn find(&self, id: u64) -> Option<BodyKey> {
        self.by_id.get(&id).copied()
    }

    /// An entry's object id; 0 for scenery or a key not in the table.
    #[must_use]
    pub fn object_id(&self, key: BodyKey) -> u64 {
        self.entries.get(&key).map_or(0, |e| e.id)
    }

    /// The cells whose collision lists hold entry `key`.
    #[must_use]
    pub fn cells_of(&self, key: BodyKey) -> Option<&[CellId]> {
        self.entries.get(&key).map(|e| e.cells.as_slice())
    }

    /// How many entries stand in the collision list of `cell`.
    #[must_use]
    pub fn in_cell(&self, cell: CellId) -> usize {
        self.cells
            .get(&cell.0)
            .map_or(0, |c| c.shadow_object_list.len())
    }

    /// Every entry: its key, its object id (0 for scenery) and where it stands, in key order.
    pub fn entries(&self) -> impl Iterator<Item = (BodyKey, u64, Position)> + '_ {
        self.entries
            .iter()
            .filter_map(|(k, e)| self.objects.get(e.handle).map(|o| (*k, e.id, o.position)))
    }

    /// Where entry `key` stands.
    #[must_use]
    pub fn position(&self, key: BodyKey) -> Option<Position> {
        let e = self.entries.get(&key)?;
        self.objects.get(e.handle).map(|o| o.position)
    }

    /// The physics types entry `key` is.
    // D2-06273
    #[must_use]
    pub fn physics_type(&self, key: BodyKey) -> Option<PhysicsTypes> {
        self.entries.get(&key).map(|e| e.physics_type)
    }

    /// The physics types the camera's line of sight passes through.
    #[must_use]
    pub fn camera_ethereal(&self) -> PhysicsTypes {
        let mut t = PhysicsTypes::default();
        t.set_camera_ethereal(&self.properties);
        t
    }

    /// Entry `key`'s collisions, once it has collision settings.
    #[must_use]
    pub fn collisions(&self, key: BodyKey) -> Option<&Collisions> {
        self.entries.get(&key)?.collisions.as_deref()
    }

    /// Takes what entry `key` reported of its collisions since last asked.
    pub fn take_reports(&mut self, key: BodyKey) -> Vec<CollisionReport> {
        self.entries
            .get_mut(&key)
            .map(|e| std::mem::take(&mut e.reports))
            .unwrap_or_default()
    }

    fn next_key(&mut self) -> BodyKey {
        self.last_key = self.last_key.wrapping_add(1).max(1);
        self.last_key
    }

    fn bit(&self, property: u32) -> PhysicsTypes {
        let mut t = PhysicsTypes::default();
        if let Some(b) = flag_bit(&self.properties, property) {
            t.set_bit(b, true);
        }
        t
    }

    /// Puts an object in the table with object id `id` (0 for none), in the collision list of
    /// each cell it reaches; returns its key.
    // D2-06246
    pub fn insert(&mut self, land: &Land, id: u64, object: &WorldObject) -> BodyKey {
        self.insert_as(land, id, object, false)
    }

    /// Puts a static object its cell or landblock places in the table, with no object id: it takes
    /// the properties of the entities it is made from, innermost first, and stands in every cell
    /// the box around what it draws reaches.
    // D2-05969
    fn add_static(&mut self, land: &Land, object: &WorldObject) -> BodyKey {
        let key = self.insert_as(land, 0, object, true);
        self.apply_placed_properties(key, &object.shape.properties);
        key
    }

    /// The property collections of the entities a placed object is made from, innermost first,
    /// each applied whole, as making the object applies them.
    fn apply_placed_properties(
        &mut self,
        key: BodyKey,
        properties: &[Arc<auberean_assets::property::PropertyCollection>],
    ) {
        for c in properties {
            self.apply_property_collection(key, c);
        }
    }

    fn insert_as(&mut self, land: &Land, id: u64, object: &WorldObject, placed: bool) -> BodyKey {
        let shape = &object.shape;
        let geometry = SetupGeometry {
            parts: shape
                .trees
                .iter()
                .map(|t| SetupPart {
                    physics_bsp: Some(Arc::clone(t)),
                    ..SetupPart::default()
                })
                .collect(),
            cyl_spheres: shape.cylinders.clone(),
            spheres: shape.spheres.clone(),
            ..SetupGeometry::default()
        };
        let key = self.next_key();
        let mut o = PhysicsObj::new(
            ObjectId(SCENERY_ID | (key & 0x7FFF_FFFF)),
            Arc::new(geometry),
            0.0,
            !object.is_static,
        );
        o.scale = shape.scale.z;
        o.state.set_has_physics_bsp(shape.has_physics_tree());
        let handle = self.objects.insert(o);
        self.entries.insert(
            key,
            Entry {
                terrain: None,
                handle,
                id,
                shape: Arc::clone(shape),
                cells: Vec::new(),
                is_static: object.is_static,
                passable: object.passable,
                physics_type: PhysicsTypes::default(),
                collisions: None,
                ignore_collisions: false,
                radius: containment(shape),
                reports: Vec::new(),
                properties: None,
                material: 0,
                graphics_state: graphics::NEW,
                game_draw: None,
                tree_scale: shape.scale,
                placed,
            },
        );
        self.by_handle.insert(handle, key);
        if id != 0 {
            self.by_id.insert(id, key);
        }
        self.place(land, key, object.position);
        key
    }

    /// Puts a body in the table with object id `id`, its spheres its shape; the body moves with
    /// it ([`crate::body::Body::tick_in`]). It starts as a new object that moves does: it is no
    /// physics type, and passes through the one physics type new movers pass through.
    pub fn insert_body(&mut self, land: &Land, id: u64, body: &mut crate::body::Body) -> BodyKey {
        if let Some(k) = body.key.filter(|k| self.entries.contains_key(k)) {
            return k;
        }
        let key = self.insert_mover(land, id, body);
        body.key = Some(key);
        key
    }

    /// A body's entry, its spheres its shape, with a new mover's collision settings.
    pub(crate) fn insert_mover(
        &mut self,
        land: &Land,
        id: u64,
        body: &crate::body::Body,
    ) -> BodyKey {
        // A body put together as its look's parts collides as they do.
        let shape = if let Some(parts) = body.part_group() {
            Shape::of_parts(parts, body.total_scale(None))
        } else {
            Shape {
                trees: Vec::new(),
                cylinders: Vec::new(),
                spheres: body.config.spheres.clone(),
                physics_box: BBox::default(),
                scale: Vec3::new(body.config.scale, body.config.scale, body.config.scale),
                containment_radius: body
                    .config
                    .spheres
                    .iter()
                    .map(|s| V3::mag2(s.center).sqrt() + s.radius)
                    .fold(0.0, f32::max),
                render_box: BBox::default(),
                properties: Vec::new(),
            }
        };
        let key = self.insert(
            land,
            id,
            &WorldObject {
                shape: Arc::new(shape),
                position: body.position,
                is_static: false,
                passable: false,
            },
        );
        self.init_mover(key);
        self.set_size(key, body.setup_radius, body.setup_height);
        key
    }

    /// What a new object that moves gets: collision settings in which it is no physics type and
    /// passes through, moving and being placed, the physics type new movers pass through.
    // D2-06115
    fn init_mover(&mut self, key: BodyKey) {
        let pass = self.bit(MOVER_ETHEREAL);
        let s = CollisionSettings {
            bits: settings::PHYSICS_TYPE | settings::MOVEMENT_PHYSICS | settings::PLACEMENT_PHYSICS,
            movement_physics: pass,
            placement_physics: pass,
            ..CollisionSettings::default()
        };
        self.set_collision_state(key, &s);
    }

    /// The containment radius of entry `key`'s setup, unscaled, when the host knows it.
    pub fn set_containment_radius(&mut self, key: BodyKey, radius: f32) {
        if let Some(e) = self.entries.get_mut(&key) {
            let mut shape = (*e.shape).clone();
            shape.containment_radius = radius;
            e.radius = containment(&shape);
            e.shape = Arc::new(shape);
        }
    }

    /// How far entry `key` reaches from its place: its containment radius at its scale.
    #[must_use]
    pub fn reach(&self, key: BodyKey) -> Option<f32> {
        self.entries.get(&key).map(|e| e.radius)
    }

    /// Entry `key`'s shape is sized by `scale` (its body's whole scale), and its reach with it.
    pub fn set_scale(&mut self, key: BodyKey, scale: Vec3) {
        if let Some(e) = self.entries.get_mut(&key) {
            let mut shape = (*e.shape).clone();
            shape.scale = scale;
            e.radius = containment(&shape);
            e.shape = Arc::new(shape);
            e.tree_scale = scale;
            if let Some(o) = self.objects.get_mut(e.handle) {
                o.scale = scale.z;
            }
        }
    }

    /// Whether bodies pass through entry `key` whatever its physics type.
    pub fn set_passable(&mut self, key: BodyKey, passable: bool) {
        if let Some(e) = self.entries.get_mut(&key) {
            e.passable = passable;
        }
    }

    /// Whether bodies that pass through entry `key` skip testing it.
    pub fn set_ignore_collisions(&mut self, key: BodyKey, ignore: bool) {
        if let Some(e) = self.entries.get_mut(&key) {
            e.ignore_collisions = ignore;
        }
    }

    /// Takes the parts of `s` it says are set into entry `key`'s collision settings, giving the
    /// entry collisions first if it has none.
    // D2-05925
    pub fn set_collision_state(&mut self, key: BodyKey, s: &CollisionSettings) {
        let Some(e) = self.entries.get_mut(&key) else {
            return;
        };
        let mut c = e.collisions.take().unwrap_or_default();
        c.set_collision_state(s, &mut Host { world: self, key });
        if let Some(e) = self.entries.get_mut(&key) {
            e.collisions = Some(c);
        }
    }

    /// What an object's description says it is and passes through, as its script applies it when
    /// the object is made: its physics type, then what it passes through moving, then being
    /// placed.
    pub fn set_ethereality(&mut self, key: BodyKey, e: Ethereality) {
        let all = e.settings();
        for bit in [
            settings::PHYSICS_TYPE,
            settings::MOVEMENT_PHYSICS,
            settings::PLACEMENT_PHYSICS,
        ] {
            let mut s = all.clone();
            s.bits = bit;
            self.set_collision_state(key, &s);
        }
    }

    /// Whether entries `a` and `b` pass through each other.
    #[must_use]
    pub fn interpermeable(&self, a: BodyKey, b: BodyKey) -> bool {
        let (Some(x), Some(y)) = (party(&self.entries, a), party(&self.entries, b)) else {
            return false;
        };
        let mut t = false;
        solid::interpermeable(&x, Some(&y), false, &mut t)
    }

    /// Files an object in a cell's collision list: what stands still comes before what moves, each
    /// in the order it came, so what a move meets first in a cell is what stands still.
    // D2-03872
    fn file_in_cell(&mut self, cell: u32, handle: PhysHandle, is_static: bool) {
        let (by_handle, entries) = (&self.by_handle, &self.entries);
        let list = &mut self.cells.entry(cell).or_default().shadow_object_list;
        let moves = |o: &PhysHandle| {
            by_handle
                .get(o)
                .and_then(|k| entries.get(k))
                .is_some_and(|e| !e.is_static)
        };
        let at = if is_static {
            list.iter().position(moves).unwrap_or(list.len())
        } else {
            list.len()
        };
        list.insert(at, handle);
    }

    /// Puts entry `key` at `position`: out of the collision lists it was in, into those of the
    /// cells it reaches there. False when the key is not in the table.
    // D2-06140, D2-06255, D2-03880, D2-03857
    pub fn place(&mut self, land: &Land, key: BodyKey, position: Position) -> bool {
        let Some(e) = self.entries.get_mut(&key) else {
            return false;
        };
        for c in &e.cells {
            if let Some(rt) = self.cells.get_mut(&c.0) {
                rt.shadow_object_list.retain(|&o| o != e.handle);
            }
        }
        e.cells = if e.placed {
            e.shape.static_cells(land, &position)
        } else {
            e.shape.cross_cells(land, &position)
        };
        if crate::interior::is_interior(position.cell) && land.cell(position.cell).is_none() {
            self.waiting.insert(key);
        } else {
            self.waiting.remove(&key);
        }
        let (reached, handle, is_static) = (e.cells.clone(), e.handle, e.is_static);
        for c in &reached {
            self.file_in_cell(c.0, handle, is_static);
        }
        if let Some(o) = self.objects.get_mut(handle) {
            o.position = position;
            o.cell = Some(position.cell);
        }
        self.refile(key, position.cell);
        true
    }

    /// Every entry put in an interior cell before it was read, its cell now read, goes into the
    /// collision lists of all the cells it reaches.
    // D2-06255
    pub fn file_those_waiting(&mut self, land: &Land) {
        if self.waiting.is_empty() {
            return;
        }
        let waiting: Vec<BodyKey> = self.waiting.iter().copied().collect();
        for key in waiting {
            match self.position(key) {
                Some(at) if land.cell(at.cell).is_some() => {
                    self.place(land, key, at);
                }
                Some(_) => {}
                None => {
                    self.waiting.remove(&key);
                }
            }
        }
    }

    /// The objects in a cell's collision list, in the order a move meets them.
    #[must_use]
    pub fn collision_list(&self, cell: CellId) -> Vec<u64> {
        self.cells
            .get(&cell.0)
            .map(|c| c.shadow_object_list.as_slice())
            .unwrap_or_default()
            .iter()
            .filter_map(|h| self.by_handle.get(h))
            .map(|k| self.object_id(*k))
            .collect()
    }

    /// How many cells' collision lists entry `key` is in.
    #[must_use]
    pub fn num_shadow_objects(&self, key: BodyKey) -> usize {
        self.entries.get(&key).map_or(0, |e| e.cells.len())
    }

    /// Whether entry `key` stands in a cell and every cell it reaches is loaded: none of its
    /// shadows has lost its cell.
    #[must_use]
    pub fn are_shadow_objects_complete(&self, land: &Land, key: BodyKey) -> bool {
        self.entries.get(&key).is_some_and(|e| {
            self.objects.get(e.handle).is_some_and(|o| o.cell.is_some())
                && !e.cells.is_empty()
                && e.cells.iter().all(|c| detection::cell_loaded(land, *c))
        })
    }

    /// Entry `key`'s collision lists made again: from where it stands when it stands anywhere and
    /// nothing holds it; else it is in none.
    pub fn recalc_cross_cells(&mut self, land: &Land, key: BodyKey, held: bool) {
        match self.position(key).filter(|p| !held && p.cell.0 != 0) {
            Some(at) => {
                self.place(land, key, at);
            }
            None => {
                let Some(e) = self.entries.get_mut(&key) else {
                    return;
                };
                for c in std::mem::take(&mut e.cells) {
                    if let Some(rt) = self.cells.get_mut(&c.0) {
                        rt.shadow_object_list.retain(|&o| o != e.handle);
                    }
                }
            }
        }
    }

    /// The objects standing in the cells of landblock `block`: its interior cells', then its land
    /// cells', only those that move when `moving`.
    #[must_use]
    pub fn block_objects(&self, land: &Land, block: CellId, moving: bool) -> Vec<u64> {
        let lb = block.landblock();
        let mut out = Vec::new();
        for c in land.interior_cells_of(block.0 & 0xFFFF_0000) {
            out.extend(self.cell_objects(c, moving));
        }
        for cx in 0..crate::land::CELLS_PER_SIDE {
            for cy in 0..crate::land::CELLS_PER_SIDE {
                let c = crate::land::land_cell_id(lb.x(), lb.y(), cx, cy);
                out.extend(self.cell_objects(c, moving));
            }
        }
        out
    }

    /// Takes entry `key` out of the table and every cell's collision list. False when the key is
    /// not in the table.
    pub fn remove(&mut self, key: BodyKey) -> bool {
        self.unfile(key);
        let Some(e) = self.entries.remove(&key) else {
            return false;
        };
        for c in &e.cells {
            if let Some(rt) = self.cells.get_mut(&c.0) {
                rt.shadow_object_list.retain(|&o| o != e.handle);
            }
        }
        if self.by_id.get(&e.id) == Some(&key) {
            self.by_id.remove(&e.id);
        }
        self.by_handle.remove(&e.handle);
        self.objects.remove(e.handle);
        true
    }

    pub(crate) fn handle(&self, key: BodyKey) -> Option<PhysHandle> {
        self.entries.get(&key).map(|e| e.handle)
    }

    pub(crate) fn key_of(&self, handle: PhysHandle) -> Option<BodyKey> {
        self.by_handle.get(&handle).copied()
    }

    /// The world as `mover`'s transition sees it: the land's cells, and this world's own test of
    /// each object the mover meets. `synchronized`: the mover keeps in step with another, and meets
    /// only objects that move.
    pub(crate) fn view<'a>(
        &'a self,
        land: &'a Land,
        mover: Option<BodyKey>,
        synchronized: bool,
    ) -> View<'a> {
        View {
            land,
            entries: &self.entries,
            by_handle: &self.by_handle,
            mover,
            synchronized,
            fuse: None,
            tested: Mutex::new(BTreeMap::new()),
            found: Mutex::new(Found::default()),
        }
    }

    /// The transition's context over `view`, the mover (when in the table) left out of its own
    /// tests.
    pub(crate) fn ctx<'a>(&'a self, view: &'a View<'_>) -> TransitionCtx<'a> {
        TransitionCtx {
            land: view,
            objects: &self.objects,
            cells: &self.cells,
            mover: view.mover.and_then(|k| self.handle(k)),
            object_table: None,
            entry_host: None,
        }
    }

    /// What the test of each object found during one move, applied once the move is done: the
    /// mover takes on the physics types of scenery it passed through, and objects placed passing
    /// through each other pass through each other for now.
    // D2-05959
    pub(crate) fn apply(&mut self, found: Found) {
        for (key, t) in found.collide_masks {
            if let Some(c) = self
                .entries
                .get_mut(&key)
                .and_then(|e| e.collisions.as_deref_mut())
            {
                c.update_collide_physics_mask(&t);
            }
        }
        for (key, object) in found.temp_ethereal {
            self.add_to_temp_ethereal_list(key, object);
        }
    }

    // D2-05958
    fn add_to_temp_ethereal_list(&mut self, key: BodyKey, object: u64) {
        if let Some(c) = self
            .entries
            .get_mut(&key)
            .and_then(|e| e.collisions.as_deref_mut())
        {
            c.add_to_temp_ethereal_list(object);
        }
    }

    /// Hands a move's collisions to the collisions of the mover and of what it hit: each object
    /// hit is touched by the mover and, when it moves and keeps collisions, touches the mover; the
    /// collision normal is kept (with the mover's velocity) or cleared. A static mover keeps none.
    pub(crate) fn handle_collisions(
        &mut self,
        key: BodyKey,
        hit: &[PhysHandle],
        normal: Option<Vec3>,
        last_hit: Option<PhysHandle>,
        velocity: Vec3,
    ) -> bool {
        let Some(e) = self.entries.get(&key) else {
            return false;
        };
        if e.collisions.is_none() {
            return false;
        }
        let mover_static = e.is_static;
        let now = self.now;
        if let Some(c) = self
            .entries
            .get_mut(&key)
            .and_then(|e| e.collisions.as_deref_mut())
        {
            c.last_check_time = now;
        }
        let mut touched = false;
        for h in hit {
            let Some(other) = self.key_of(*h) else {
                continue;
            };
            if !mover_static {
                touched |= self.add_object_collision(key, other);
            }
            let other_moves = self
                .entries
                .get(&other)
                .is_some_and(|o| !o.is_static && o.collisions.is_some());
            if other_moves {
                self.wake(other);
                self.add_object_collision(other, key);
            }
        }
        match normal {
            None => {
                if let Some(c) = self.collisions_mut(key) {
                    c.set_no_collision_normal();
                }
            }
            Some(n) => {
                if !mover_static {
                    if let Some(c) = self.collisions_mut(key) {
                        c.set_collision_normal(n, velocity);
                    }
                }
                let other = last_hit.and_then(|h| self.key_of(h));
                if let Some(o) = other.filter(|o| {
                    self.entries
                        .get(o)
                        .is_some_and(|e| !e.is_static && e.collisions.is_some())
                }) {
                    self.wake(o);
                    if let Some(c) = self.collisions_mut(o) {
                        c.set_collision_normal(n, velocity);
                    }
                }
            }
        }
        touched
    }

    /// The install's property list.
    #[must_use]
    pub const fn property_list(&self) -> &PropertyList {
        &self.properties
    }

    /// Entry `key`'s surroundings' properties are `material`'s, when it has collisions.
    pub(crate) fn add_environmental_properties(
        &mut self,
        key: BodyKey,
        material: &PhysicsMaterial,
    ) {
        if let Some(e) = self.entries.get_mut(&key) {
            if let Some(c) = e.collisions.as_deref_mut() {
                c.add_environmental_properties(Some(material), &self.properties);
            }
        }
    }

    pub(crate) fn collisions_mut(&mut self, key: BodyKey) -> Option<&mut Collisions> {
        self.entries.get_mut(&key)?.collisions.as_deref_mut()
    }

    /// `key` touches `other` in this check ([`Collisions::add_object_collision`]).
    fn add_object_collision(&mut self, key: BodyKey, other: BodyKey) -> bool {
        let Some(mut c) = self.entries.get_mut(&key).and_then(|e| e.collisions.take()) else {
            return false;
        };
        let r = c.add_object_collision(other, &mut Host { world: self, key });
        if let Some(e) = self.entries.get_mut(&key) {
            e.collisions = Some(c);
        }
        r
    }

    /// Entry `key` reports what began and ended of its collisions; a report with anything in it is
    /// kept for the host ([`World::take_reports`]). `end_all` ends every touch.
    // D2-06244
    pub fn report_collisions(&mut self, key: BodyKey, end_all: bool) -> Option<CollisionReport> {
        self.report(key, end_all, false)
    }

    /// Entry `key` ends every touch, leaving: its collision normal, water, ground and
    /// surroundings end too.
    // D2-06245
    pub fn end_all_collisions(&mut self, key: BodyKey) -> Option<CollisionReport> {
        self.report(key, true, true)
    }

    /// The report kept, when it had anything in it.
    fn report(&mut self, key: BodyKey, end_all: bool, leaving: bool) -> Option<CollisionReport> {
        let mut c = self
            .entries
            .get_mut(&key)
            .and_then(|e| e.collisions.take())?;
        let mut report = CollisionReport::default();
        c.report_collision(
            &mut report,
            end_all,
            leaving,
            &mut Host { world: self, key },
        );
        let e = self.entries.get_mut(&key)?;
        e.collisions = Some(c);
        if !report.non_empty() {
            return None;
        }
        e.reports.push(report.clone());
        Some(report)
    }

    /// Brings in the scenery of `cell`'s block, once for each part of it: the solid parts placed
    /// in its interior cells (once a body is inside), the entities placed outside that collide
    /// through cylinders or spheres (once the block is in `land`), and those placed inside (once
    /// its interior cells are).
    // D2-03678
    pub(crate) fn add_scenery(&mut self, land: &Land, cell: CellId) {
        self.add_interior_solids(land, cell);
        self.add_placed(land, cell);
    }

    /// Make the solid parts placed in the interior cells of `cell`'s block static objects of the
    /// table, once per block: each in its own cell's collision list and in that of every cell its
    /// cell sees whose bounds its sphere reaches.
    // D2-03491
    fn add_interior_solids(&mut self, land: &Land, cell: CellId) {
        let block = cell.0 & 0xFFFF_0000;
        if !crate::interior::is_interior(cell) || !self.interior_blocks.insert(block) {
            return;
        }
        for id in land.interior_cells_of(block) {
            let Some(solids) = land.cell_solids(id) else {
                continue;
            };
            if land.cell(id).is_none() {
                continue;
            }
            for s in solids.iter() {
                let mut geometry = SetupGeometry {
                    parts: vec![SetupPart {
                        default_scale: Vec3::new(1.0, 1.0, s.part.gfxobj_scale),
                        physics_bsp: s.part.physics_bsp.clone(),
                        bound_box: s.part.bound_box,
                        drawing_sphere: s.part.drawing_sphere,
                        ..SetupPart::default()
                    }],
                    ..SetupGeometry::default()
                };
                geometry.sorting_sphere = s.bounds;
                let key = self.next_key();
                let mut o = PhysicsObj::new(
                    ObjectId(SCENERY_ID | (key & 0x7FFF_FFFF)),
                    Arc::new(geometry),
                    0.0,
                    false,
                );
                o.position = s.part.pos;
                o.cell = Some(id);
                o.state.set_has_physics_bsp(true);
                let h = self.objects.insert(o);
                // Filed as the cell files what it places: in every cell the box around what its
                // object draws reaches.
                let reached = cells::find_cells_of_box(land, &s.part.pos, &s.render_box);
                for c in &reached {
                    self.file_in_cell(c.0, h, true);
                }
                let shape = Shape {
                    trees: s.part.physics_bsp.iter().cloned().collect(),
                    cylinders: Vec::new(),
                    spheres: vec![Sphere::new(s.bounds.center, s.bounds.radius)],
                    physics_box: BBox::default(),
                    scale: Vec3::new(1.0, 1.0, 1.0),
                    containment_radius: s.bounds.radius,
                    render_box: s.render_box,
                    properties: Vec::new(),
                };
                self.by_handle.insert(h, key);
                self.entries.insert(
                    key,
                    Entry {
                        terrain: None,
                        handle: h,
                        id: 0,
                        radius: s.bounds.radius,
                        shape: Arc::new(shape),
                        cells: reached,
                        is_static: true,
                        passable: false,
                        physics_type: PhysicsTypes::default(),
                        collisions: None,
                        ignore_collisions: false,
                        reports: Vec::new(),
                        properties: None,
                        material: 0,
                        graphics_state: graphics::NEW,
                        game_draw: None,
                        tree_scale: s.scale,
                        placed: true,
                    },
                );
                self.apply_placed_properties(key, &s.properties);
            }
        }
    }

    /// The placed entities of `cell`'s block and those around it that collide as objects (through
    /// cylinders or spheres, or through a tree when their properties make them something some mover
    /// passes through), static and solid: outside, the block's own, once the block is in `land`,
    /// each of the physics type the client gives them; inside, every interior cell's, once the
    /// block's interior cells are.
    fn add_placed(&mut self, land: &Land, cell: CellId) {
        let block = cell.0 & 0xFFFF_0000;
        let lb = cell.landblock();
        let mut found: Vec<(Position, Arc<Shape>, bool)> = Vec::new();
        // The blocks around it too: what stands in them reaches over their edges.
        for dx in -1i16..=1 {
            for dy in -1i16..=1 {
                let (Ok(x), Ok(y)) = (
                    u8::try_from(i16::from(lb.x()) + dx),
                    u8::try_from(i16::from(lb.y()) + dy),
                ) else {
                    continue;
                };
                let around = u32::from(x) << 24 | u32::from(y) << 16;
                if self.placed_outside.contains(&around) {
                    continue;
                }
                if let Some(b) = land.block(x, y) {
                    self.placed_outside.insert(around);
                    found.extend(b.objects.iter().map(|(p, s)| (*p, Arc::clone(s), true)));
                }
            }
        }
        if !self.placed_inside.contains(&block) {
            let cells = land.interior_cells_of(block);
            if !cells.is_empty() {
                self.placed_inside.insert(block);
                for c in cells {
                    if let Some(o) = land.cell_objects(c) {
                        found.extend(o.iter().map(|(p, s)| (*p, Arc::clone(s), false)));
                    }
                }
            }
        }
        let outside = self.bit(STATIC_ETHEREAL);
        for (position, shape, out) in found {
            let o = WorldObject {
                shape,
                position,
                is_static: true,
                passable: false,
            };
            let key = self.add_static(land, &o);
            if out {
                // The landblock's own scenery is, besides what its entities make it, of the
                // outdoor scenery's type.
                let mut physics_type = self.physics_type(key).unwrap_or_default();
                physics_type.mask |= outside.mask;
                let s = CollisionSettings {
                    bits: settings::PHYSICS_TYPE,
                    physics_type,
                    ..CollisionSettings::default()
                };
                self.set_collision_state(key, &s);
            }
        }
    }
}

/// Entry `key` as one side of a solidity question; its settings are its collisions' current ones.
// D2-05927
fn party(entries: &BTreeMap<BodyKey, Entry>, key: BodyKey) -> Option<Party<'_>> {
    let e = entries.get(&key)?;
    Some(Party {
        id: e.id,
        physics_type: e.physics_type,
        settings: e.collisions.as_deref().map(|c| &c.current),
    })
}

/// What the test of each object found during one move, for [`World::apply`].
#[derive(Debug, Default)]
pub(crate) struct Found {
    collide_masks: Vec<(BodyKey, PhysicsTypes)>,
    temp_ethereal: Vec<(BodyKey, u64)>,
}

/// The world as one mover's transition sees it: the land's cells, and this world's test of each
/// object the mover meets.
pub(crate) struct View<'a> {
    land: &'a Land,
    entries: &'a BTreeMap<BodyKey, Entry>,
    by_handle: &'a BTreeMap<PhysHandle, BodyKey>,
    mover: Option<BodyKey>,
    synchronized: bool,
    /// The mover is a missile that goes off near one object: that object's id, and how near.
    fuse: Option<(u64, f32)>,
    /// The insert attempt each object was last tested in: an object standing in several cells is
    /// tested once an attempt.
    tested: Mutex<BTreeMap<BodyKey, u64>>,
    found: Mutex<Found>,
}

impl View<'_> {
    pub(crate) fn into_found(self) -> Found {
        self.found.into_inner().unwrap_or_default()
    }

    /// The mover is a missile that goes off within `reach` of object `target`.
    pub(crate) fn with_fuse(mut self, fuse: Option<(u64, f32)>) -> Self {
        self.fuse = fuse;
        self
    }
}

impl LandSource for View<'_> {
    fn landblock(&self, id: LandblockId) -> Option<Arc<dereth_physics::LandblockCollision>> {
        self.land.landblock(id)
    }
    fn landblock_resident(&self, id: LandblockId) -> bool {
        self.land.landblock_resident(id)
    }
    fn height_table(&self) -> &[f32; dereth_physics::globals::LAND_HEIGHT_TABLE_LEN] {
        self.land.height_table()
    }
    fn env_cell(&self, cell: CellId) -> Option<Arc<dereth_physics::EnvCellGeometry>> {
        self.land.env_cell(cell)
    }
    fn building_cells(&self, cell: CellId) -> Vec<CellId> {
        self.land.building_cells(cell)
    }
    fn building(&self, cell: CellId) -> Option<Arc<dereth_physics::BuildingGeometry>> {
        self.land.building(cell)
    }
    fn host_cell(&self, id: CellId) -> Option<Arc<dyn dereth_physics::cell::HostCell>> {
        self.land.host_cell(id)
    }
    fn host_cell_list(
        &self,
        pos: &Position,
        spheres: &[dereth_physics::geom::Sphere],
        arr: &mut dereth_physics::CellArray,
        want_container: bool,
        hits_interior_cell: &mut bool,
    ) -> Option<Option<dereth_physics::Cell>> {
        self.land
            .host_cell_list(pos, spheres, arr, want_container, hits_interior_cell)
    }
    fn object_collisions(&self) -> Option<&dyn ObjectCollisions> {
        Some(self)
    }
}

impl ObjectCollisions for View<'_> {
    /// One object the mover meets, once an insert attempt: passed over when the two pass through
    /// each other and it asks
    /// to be ignored, or when stepping down; passed over when out of reach; else its shape is
    /// tested, the pair passing through each other making it an obstruction that does not block.
    /// A missile that goes off near this object hits it, without its shape, anywhere within its
    /// reach of it. A hit (not stepping down) is recorded; one passed through does not stop the
    /// move, and when it is scenery the mover takes on its physics types.
    // D2-06214
    fn find_obj_collisions(
        &self,
        ctx: &TransitionCtx<'_>,
        t: &mut Transition,
        other: PhysHandle,
    ) -> TransitionState {
        let Some(key) = self.by_handle.get(&other).copied() else {
            return TransitionState::Ok;
        };
        if let Ok(mut tested) = self.tested.lock() {
            if tested.insert(key, t.attempt) == Some(t.attempt) {
                return TransitionState::Ok;
            }
        }
        let (Some(e), Some(o)) = (self.entries.get(&key), ctx.objects.get(other)) else {
            return TransitionState::Ok;
        };
        let this = party(self.entries, key).unwrap_or(Party {
            id: 0,
            physics_type: PhysicsTypes::default(),
            settings: None,
        });
        let mover = self.mover.and_then(|k| party(self.entries, k));
        let mut by_temporary = false;
        let inter =
            e.passable || solid::interpermeable(&this, mover.as_ref(), false, &mut by_temporary);
        let moves = !e.is_static;
        if self.synchronized && !moves {
            return TransitionState::Ok;
        }
        // Another moving object is never stepped down onto, unless the mover is being placed.
        let creature = moves && t.sphere_path.begin_pos.is_some();
        if inter {
            if (e.ignore_collisions || e.passable) && !by_temporary {
                return TransitionState::Ok;
            }
            if t.sphere_path.step_down {
                return TransitionState::Ok;
            }
        }
        let max_step = t
            .object_info
            .step_down_height
            .abs()
            .max(t.object_info.step_up_height.abs());
        let mover_radius = self
            .mover
            .and_then(|k| self.entries.get(&k))
            .map_or(0.0, |m| m.radius);
        let fused = self
            .fuse
            .filter(|(target, _)| e.id != 0 && *target == e.id)
            .map(|(_, reach)| reach);
        let mut r = if let Some(reach) = fused {
            if !solid::can_collide(
                &o.position,
                e.radius,
                0.0,
                reach + mover_radius,
                &t.sphere_path.check_pos,
            ) {
                return TransitionState::Ok;
            }
            TransitionState::Collided
        } else {
            if !solid::can_collide(
                &o.position,
                e.radius,
                max_step,
                mover_radius,
                &t.sphere_path.check_pos,
            ) {
                return TransitionState::Ok;
            }
            t.sphere_path.obstruction_ethereal = inter;
            let frames = o.part_frames.clone();
            crate::obstacle::object_geometry(
                ctx,
                t,
                o.state,
                ObjGeometry::Setup(&o.geometry, frames.as_deref().map(Vec::as_slice)),
                o.scale,
                TreeScale::axes(e.tree_scale),
                &o.position,
                e.tree_scale,
                creature,
            )
        };
        if r != TransitionState::Ok && !t.sphere_path.step_down {
            t.collision_info
                .add_object(other, if inter { TransitionState::Ok } else { r });
            if inter {
                r = TransitionState::Ok;
                if e.id == 0 {
                    if let (Some(m), Ok(mut f)) = (self.mover, self.found.lock()) {
                        f.collide_masks.push((m, e.physics_type));
                    }
                }
            }
        }
        t.sphere_path.obstruction_ethereal = false;
        r
    }
}

/// The world as one entry's collisions see it.
struct Host<'a> {
    world: &'a mut World,
    key: BodyKey,
}

impl CollisionHost for Host<'_> {
    fn key(&self) -> BodyKey {
        self.key
    }
    fn object_id(&self, body: BodyKey) -> u64 {
        self.world.object_id(body)
    }
    fn find(&self, object: u64) -> Option<BodyKey> {
        self.world.find(object)
    }
    fn interpermeable(&self, body: BodyKey, other: BodyKey, own: &CollisionSettings) -> bool {
        // This body's collisions are out of its entry while they are asked: its settings are
        // the caller's.
        let side = |k: BodyKey| {
            party(&self.world.entries, k).map(|mut p| {
                if k == self.key {
                    p.settings = Some(own);
                }
                p
            })
        };
        let (Some(x), Some(y)) = (side(body), side(other)) else {
            return false;
        };
        let mut t = false;
        solid::interpermeable(&x, Some(&y), false, &mut t)
    }
    fn contact_record(&self, body: BodyKey) -> Option<ContactRecord> {
        let id = self.world.object_id(body);
        (id != 0).then(|| ContactRecord {
            object: id,
            ..ContactRecord::default()
        })
    }
    fn ignores_collisions(&self, body: BodyKey) -> bool {
        self.world
            .entries
            .get(&body)
            .is_some_and(|e| e.ignore_collisions)
    }
    fn collide_mask(&self, body: BodyKey) -> PhysicsTypes {
        self.world.physics_type(body).unwrap_or_default()
    }
    fn object_properties(&self, body: BodyKey) -> Option<Vec<PropertyGroup>> {
        self.world
            .object_properties(body)
            .1
            .map(<[PropertyGroup]>::to_vec)
    }
    fn position(&self, body: BodyKey) -> Position {
        self.world
            .position(body)
            .unwrap_or_else(|| Position::new(CellId(0), dereth_primitives::Frame::default()))
    }
    fn add_to_temp_ethereal_list(&mut self, body: BodyKey, object: u64) {
        self.world.add_to_temp_ethereal_list(body, object);
    }
    // D2-06216
    fn collision_remove_object(&mut self, body: BodyKey, object: u64) {
        if self.world.entries.get(&body).is_none_or(|e| e.is_static) {
            return;
        }
        let Some(mut c) = self
            .world
            .entries
            .get_mut(&body)
            .and_then(|e| e.collisions.take())
        else {
            return;
        };
        let mut report = CollisionReport::default();
        let reported = c.remove_object_collision(object, &mut report, &self.world.properties);
        if let Some(e) = self.world.entries.get_mut(&body) {
            e.collisions = Some(c);
            if reported {
                e.reports.push(report);
            }
        }
    }
    // D2-06339
    fn set_physics_collide_mask(&mut self, mask: &PhysicsTypes) {
        if let Some(e) = self.world.entries.get_mut(&self.key) {
            e.physics_type = *mask;
        }
    }
    fn set_terrain_type(&mut self, terrain: u32) {
        if let Some(e) = self.world.entries.get_mut(&self.key) {
            e.terrain = Some(terrain);
        }
    }
    fn reports_collisions(&self) -> bool {
        true
    }
    fn physics_material(&self, id: u32) -> Option<PhysicsMaterial> {
        self.world.materials.get(&id).cloned()
    }
    fn property_list(&self) -> &PropertyList {
        &self.world.properties
    }
    fn now(&self) -> f64 {
        self.world.now
    }
}
