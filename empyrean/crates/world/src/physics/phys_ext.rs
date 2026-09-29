// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Physics/PhysicsObj.cs, Source/ACE.Server/Physics/Common/Landblock.cs, Source/ACE.Server/Physics/Common/ObjCell.cs, Source/ACE.Server/Physics/Common/EnvCell.cs
//! `phys_ext`: ACE's `PhysicsObj.*` calls, over the shared `PhysicsWorld`.
//!
//! Gameplay code calls these by ACE's names (`make_object`, `set_object_guid`, `enter_world`,
//! `update_object`, `destroy_object`, ...), with the body's [`PhysHandle`] where ACE has the
//! `PhysicsObj` reference. The retail-faithful shared crate does the physics (V1). What ACE adds
//! on top of its physics port for the server is ported here, because the shared crate has no
//! place for it:
//!
//! * the server-only fields of ACE's `PhysicsObj` (`WeenieObj`, `ObjMaint`, `CurLandblock`,
//!   `entering_world`, `DatObject`, `Order`, `CollisionTable`, `InitialUpdates`), kept per body in
//!   [`PhysObjExt`];
//! * the server object lists of ACE's physics `Landblock` (`ServerObjects`, adjacents) and the
//!   object lists of its cells (`ObjCell.ObjectList`), kept in [`PhysExtState`];
//! * the server half of cell changes: `change_cell_server`, `enter_cell_server` (the visibility
//!   updates that make players create objects) and `leave_cell`'s server lists;
//! * collision reporting: the shared crate's `PhysicsNotice`s are drained right after each body's
//!   update and routed through the body's `WeenieObject` (V6), with ACE's collision table and
//!   `report_collision_end`.
//!
//! **Where the shared crate's behaviour differs from ACE's physics port, the shared crate wins
//! (V1)**, and each difference is noted at the adapter that exposes it:
//!
//! * ACE runs `enter_cell_server` inside `SetPositionInternal`, when the transition changes the
//!   cell. The shared crate changes cells internally, so the adapter compares the body's cell with
//!   the one it last saw after every placement or update and runs the server half then
//!   ([`sync_cell`]); within one update, that is after the whole update rather than mid-transition.
//! * ACE's transition reports collisions to both parties; the second report always reaches its
//!   own object (`obj.WeenieObj.DoCollision(profile, ObjID, obj)`) and is dropped as a
//!   self-collision. The shared crate raises the mover's half only, and only when the mover
//!   reports collisions, so ACE's collision table (and `OnCollideObjectEnd`) sees only reporting
//!   movers.
//! * The shared crate uses retail's 0.2 s maximum quantum and gate (ACE: 0.1 s and `TickRate`).
//! * `enter_world` always places with `PLACEMENT | SLIDE`; ACE drops `SLIDE` for a missile with a
//!   projectile target that is not a spell projectile.
//! * A body with a motion table carries `dereth-animation`'s `MotionDriver` as its `MotionSource`
//!   (the client's motion stack, not ACE's port of it); the motion calls (`DoMotion`,
//!   `MoveToObject`, ...) and their differences from ACE's port are in [`crate::physics::motion`], re-exported below.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use dereth_physics::source::LandSource;
use dereth_physics::{PhysHandle, PhysicsNotice, PhysicsWorld, SetupGeometry};
use dereth_primitives::{
    CellId, Frame, LandblockId, LocalTime, ObjectId, Position as PPosition, Quat, Vec3,
};
use empyrean_common::dotnet::datetime::TimeSpan;
use empyrean_common::dotnet::{DotNetDict, Vector3};
use empyrean_dat::physics::{linear_height_table, DatLandSource, SetupGeometryCache};
use empyrean_dat::DatManager;
use empyrean_entity::enums::{PhysicsState, PropertyBool};
use empyrean_entity::shared_types::vector3_of_data;
use empyrean_entity::ObjectGuid;

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::timers;
pub use crate::physics::motion::{
    ace_movement_parameters, apply_physics_motion, apply_raw_motion_state,
    apply_raw_motion_state_client_method, apply_raw_movement, calc_acceleration, cancel_move_to,
    cancel_moveto, clear_pending_motions, do_interpreted_motion, do_motion, execute_motion_physics,
    get_minterp, interpreted_state, is_animating, is_moving_or_animating, is_moving_to,
    last_move_complete, make_movement_manager, motions_pending, move_to_always_turn,
    move_to_fail_progress_count, move_to_initialized, move_to_object, move_to_object_internal,
    move_to_pending_actions, move_to_position, raw_motion_state_set_state, raw_state,
    restart_clock_if_idle, set_move_to_always_turn, set_move_to_fail_progress_count,
    set_on_walkable, set_run_rate_stand_in, stick_to_object, stop_completely,
    stop_interpreted_motion, stop_motion, turn_to_heading, turn_to_object, turn_to_object_internal,
    unstick_from_object,
};
use crate::physics::object_maint::{self, ObjectMaint, VisibleObjectType};
use crate::physics::server_object_manager;
use crate::physics::weenie_object::WeenieObject;
use crate::World;

/// `PhysicsObj.CollisionTable`'s value: when the other object was last touched, and whether it
/// was ethereal then.
// ACE: CollisionRecord
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CollisionRecord {
    pub touched_time: f64,
    pub ethereal: bool,
}

/// The server-only half of one ACE `PhysicsObj`.
#[derive(Debug, Default)]
pub struct PhysObjExt {
    // ACE: PhysicsObj.WeenieObj
    pub weenie_obj: WeenieObject,
    // ACE: PhysicsObj.ObjMaint
    pub obj_maint: ObjectMaint,
    /// The physics landblock the body is registered on (as a server object).
    // ACE: PhysicsObj.CurLandblock
    pub cur_landblock: Option<u16>,
    /// `CurCell` as the server half last saw it; [`sync_cell`] brings it level with the body.
    // ACE: PhysicsObj.CurCell
    pub cur_cell: Option<CellId>,
    // ACE: PhysicsObj.entering_world
    pub entering_world: bool,
    // ACE: PhysicsObj.DatObject
    pub dat_object: bool,
    // ACE: PhysicsObj.Order
    pub order: i32,
    /// ACE's `PartArray != null`: false when the setup has no geometry (the body then never
    /// enters a cell, as ACE's `enter_cell` returns early without a part array).
    pub has_part_array: bool,
    /// The motion table id `SetMotionTableID` was given.
    pub motion_table_id: u32,
    /// The setup the body was made from (the motion driver's part array reads it).
    pub setup_id: u32,
    /// The body's motion (ACE's `MovementManager`); `None` until it has one.
    pub motion: Option<crate::physics::motion::BodyMotion>,
    // ACE: PhysicsObj.CollisionTable
    pub collision_table: DotNetDict<u32, CollisionRecord>,
    // ACE: PhysicsObj.InitialUpdates
    pub initial_updates: i32,
    /// `WorldObject.LastPhysicsUpdate`. DIVERGE: a `WorldObject_Tick.cs` field (not owned here);
    /// kept with the body until that file's field struct carries it.
    pub last_physics_update: f64,
    /// `WorldObject.physicsCreationTime`. DIVERGE: ACE sets it when the WorldObject is
    /// constructed; here it is set when the body is made, until `WorldObject_Tick.cs`'s fields
    /// carry it.
    pub physics_creation_time: f64,
    /// The body a projectile is aimed at (its `ObjectInfo.TargetID` in every transition it runs,
    /// through the shared body's `projectile_target_id`): a missile ignores every other weenie
    /// creature, and every other ethereal weenie object (ACE's `ObjectInfo.MissileIgnore`, A11).
    // ACE: PhysicsObj.ProjectileTarget
    pub projectile_target: Option<PhysHandle>,
}

/// ACE's physics `Landblock`, server half: the server objects registered on it and its adjacent
/// landblocks.
#[derive(Debug, Default)]
pub struct PhysLandblock {
    // ACE: Landblock.ServerObjects
    pub server_objects: Vec<PhysHandle>,
    /// Set by the landblock manager (`SetAdjacents`); `None` until then.
    pub adjacents: Option<Vec<u16>>,
}

/// The adapter's world-scope state (`World.phys_ext`).
pub struct PhysExtState {
    pub(crate) objs: HashMap<PhysHandle, PhysObjExt>,
    pub(crate) landblocks: BTreeMap<u16, PhysLandblock>,
    /// `ObjCell.ObjectList` per cell id.
    pub(crate) cell_objects: BTreeMap<u32, Vec<PhysHandle>>,
    /// The house barriers (`ObjCell.RestrictionObj`) set on each landblock's cells, by landblock:
    /// the objects whose barrier record [`prepare_mover`] refreshes for a moving player.
    pub(crate) restriction_objs: BTreeMap<u16, BTreeSet<u32>>,
    setups: SetupGeometryCache,
    synthetic_setups: BTreeMap<u32, Arc<SetupGeometry>>,
    /// The dat land source `World.physics` runs on, for landblock residency; `None` when a test
    /// installed another source.
    land: Option<Arc<DatLandSource>>,
    /// The motion tables, animations and setups every body's motion driver reads.
    pub(crate) motion_assets: Arc<crate::physics::motion::ServerAnimAssets>,
}

impl std::fmt::Debug for PhysExtState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysExtState")
            .field("objs", &self.objs.len())
            .field("landblocks", &self.landblocks.len())
            .finish_non_exhaustive()
    }
}

impl PhysExtState {
    /// The adapter state and the physics world for `World::new`: a `DatLandSource` over the cell
    /// dat, with the region's height table (or the linear table when the dats have no region, as
    /// a `FakeDats` has not).
    #[must_use]
    pub fn new(dats: &Arc<DatManager>) -> (Self, PhysicsWorld) {
        let land = DatLandSource::new(Arc::clone(dats)).unwrap_or_else(|_| {
            DatLandSource::with_height_table(Arc::clone(dats), linear_height_table())
        });
        let land = Arc::new(land);
        let physics = PhysicsWorld::new(Arc::clone(&land) as Arc<dyn LandSource>);
        let state = PhysExtState {
            objs: HashMap::new(),
            landblocks: BTreeMap::new(),
            cell_objects: BTreeMap::new(),
            restriction_objs: BTreeMap::new(),
            setups: SetupGeometryCache::new(Arc::clone(dats)),
            synthetic_setups: BTreeMap::new(),
            land: Some(land),
            motion_assets: Arc::new(crate::physics::motion::ServerAnimAssets::new(Arc::clone(
                dats,
            ))),
        };
        (state, physics)
    }
}

// ---------------------------------------------------------------------------------------------
// Plumbing
// ---------------------------------------------------------------------------------------------

/// `PhysicsTimer.CurrentTime`, which on a running server is `Timers.PortalYearTicks`.
// ACE: PhysicsTimer.CurrentTime
#[must_use]
pub fn physics_timer_current_time(w: &World) -> f64 {
    timers::portal_year_ticks(w)
}

pub(crate) fn ext(w: &World, h: PhysHandle) -> Option<&PhysObjExt> {
    w.phys_ext.objs.get(&h)
}

pub(crate) fn ext_mut(w: &mut World, h: PhysHandle) -> Option<&mut PhysObjExt> {
    w.phys_ext.objs.get_mut(&h)
}

/// The body's server-side record, for tests and diagnostics.
#[must_use]
pub fn server_record(w: &World, h: PhysHandle) -> Option<&PhysObjExt> {
    ext(w, h)
}

/// `PhysicsObj.ID`.
#[must_use]
pub fn id(w: &World, h: PhysHandle) -> Option<u32> {
    w.physics.get(h).map(|o| o.id.0)
}

/// `PhysicsObj.IsPlayer`: the id is in the player guid range.
// ACE: PhysicsObj.IsPlayer
#[must_use]
pub const fn is_player_id(id: u32) -> bool {
    id >= 0x5000_0001 && id <= 0x5FFF_FFFF
}

/// `PhysicsObj.IsPlayer` for a body.
#[must_use]
pub fn is_player(w: &World, h: PhysHandle) -> bool {
    id(w, h).is_some_and(is_player_id)
}

/// `PhysicsObj.WeenieObj` (the dummy object for a body with no server record).
#[must_use]
pub fn weenie_obj(w: &World, h: PhysHandle) -> WeenieObject {
    ext(w, h).map(|e| e.weenie_obj.clone()).unwrap_or_default()
}

/// `PhysicsObj.Position`.
#[must_use]
pub fn position(w: &World, h: PhysHandle) -> Option<PPosition> {
    w.physics.get(h).map(|o| o.position)
}

/// `PhysicsObj.CurCell`.
#[must_use]
pub fn cur_cell(w: &World, h: PhysHandle) -> Option<CellId> {
    ext(w, h).and_then(|e| e.cur_cell)
}

/// `PhysicsObj.CurLandblock`, as the landblock id.
#[must_use]
pub fn cur_landblock(w: &World, h: PhysHandle) -> Option<u16> {
    ext(w, h).and_then(|e| e.cur_landblock)
}

/// The body of a world object (`wo.PhysicsObj`).
#[must_use]
pub fn physics_obj(w: &World, wo: ObjectGuid) -> Option<PhysHandle> {
    w.objects.get(wo).and_then(|o| o.phys)
}

/// `new Physics.Common.Position` from an ACE `Position`: its cell, origin and rotation.
#[must_use]
pub fn to_physics_position(p: &empyrean_entity::Position) -> PPosition {
    PPosition::new(
        CellId(p.cell()),
        Frame::new(
            Vec3::new(p.position_x, p.position_y, p.position_z),
            Quat::new(p.rotation_w, p.rotation_x, p.rotation_y, p.rotation_z),
        ),
    )
}

/// `Physics.Common.Position.Distance2DSquared`: same landblock, the frame offset; otherwise the
/// 192 m block offset plus the frame offset.
// ACE: Physics.Common.Position.Distance2DSquared
#[must_use]
#[allow(clippy::cast_precision_loss)] // C#'s long to float
pub fn distance_2d_squared(a: &PPosition, p: &PPosition) -> f32 {
    let (a_cell, p_cell) = (a.cell.0, p.cell.0);
    if a_cell >> 16 == p_cell >> 16 {
        let dx = a.frame.origin.x - p.frame.origin.x;
        let dy = a.frame.origin.y - p.frame.origin.y;
        dx * dx + dy * dy
    } else {
        let lbx = |c: u32| i64::from(c >> 24);
        let lby = |c: u32| i64::from((c >> 16) & 0xFF);
        let dx = ((lbx(a_cell) - lbx(p_cell)) * 192) as f32 + a.frame.origin.x - p.frame.origin.x;
        let dy = ((lby(a_cell) - lby(p_cell)) * 192) as f32 + a.frame.origin.y - p.frame.origin.y;
        dx * dx + dy * dy
    }
}

/// Collision geometry for a setup id: a test's synthetic setup, else the dat's.
fn setup_geometry(w: &World, data_did: u32) -> Option<Arc<SetupGeometry>> {
    if let Some(g) = w.phys_ext.synthetic_setups.get(&data_did) {
        return Some(Arc::clone(g));
    }
    w.phys_ext.setups.get(data_did)
}

/// Whether a test registered synthetic geometry under `setup_id` (it then stands for the dat's
/// setup of that id in every physics and motion read).
#[must_use]
pub(crate) fn has_synthetic_setup(w: &World, setup_id: u32) -> bool {
    w.phys_ext.synthetic_setups.contains_key(&setup_id)
}

/// Registers synthetic collision geometry under a setup id (unit tests: no dats).
pub fn register_setup(w: &mut World, setup_id: u32, geometry: SetupGeometry) {
    w.phys_ext
        .synthetic_setups
        .insert(setup_id, Arc::new(geometry));
}

/// Replaces the physics world with one over `land` (a flat source in tests). Every body is lost.
pub fn use_land_source(w: &mut World, land: Arc<dyn LandSource>) {
    w.physics = PhysicsWorld::new(land);
    w.phys_ext.objs.clear();
    w.phys_ext.landblocks.clear();
    w.phys_ext.cell_objects.clear();
    w.phys_ext.restriction_objs.clear();
    w.phys_ext.land = None;
    w.server_object_manager.server_objects.clear();
}

/// Loads a landblock's interior cells and buildings into the dat land source (the landblock
/// manager calls this when it loads a landblock). Answers whether the block is resident; a test
/// land source is always resident.
pub fn load_landblock(w: &mut World, landblock: u16) -> bool {
    w.phys_ext.landblocks.entry(landblock).or_default();
    let resident = match &w.phys_ext.land {
        Some(land) => land.load_landblock(LandblockId(landblock)),
        None => true,
    };
    load_cell_restrictions(w, landblock);
    resident
}

/// The landblock's house barriers from the cell dat: its `LandblockInfo` restriction table (land
/// cells) and each interior cell's own `RestrictionObj`, set on the physics world's cells.
// ACE: Landblock.init_static_objs
// ACE: EnvCell.EnvCell
fn load_cell_restrictions(w: &mut World, landblock: u16) {
    let block = LandblockId(landblock);
    let Some(lbi) = w
        .dats
        .cell_dat()
        .read_from_dat::<empyrean_dat::file_types::LandblockInfo>(block.info_id().0)
    else {
        return;
    };

    let mut cells: Vec<(CellId, u32)> = Vec::new();
    if let Some(table) = &lbi.restrictions {
        for &(key, obj) in &table.entries {
            // `LandDefs.gid_to_lcoord(kvp.Key)`, then `LandCells[(y & 7) + (x & 7) * 8]`: the land
            // cell with the key's index, on this landblock
            let index = key & 0xFFFF;
            if !(1..=64).contains(&index) {
                continue;
            }
            cells.push((CellId((u32::from(landblock) << 16) | index), obj));
        }
    }
    for i in 0..lbi.num_cells {
        let id = (u32::from(landblock) << 16) | (0x100 + i);
        let Some(cell) = w
            .dats
            .cell_dat()
            .read_from_dat::<empyrean_dat::file_types::EnvCell>(id)
        else {
            continue;
        };
        if let Some(obj) = cell.restriction_obj {
            cells.push((CellId(id), obj));
        }
    }

    for (cell, obj) in cells {
        set_cell_restriction(w, cell, obj);
    }
}

/// `ObjCell.RestrictionObj = obj`: the cell is fenced by the house `obj` (0 removes the fence).
/// The landblock load sets these from the cell dat; a test's synthetic land sets its own.
pub fn set_cell_restriction(w: &mut World, cell: CellId, obj: u32) {
    let lb = cell.landblock().0;
    if obj == 0 {
        w.physics.set_cell_restriction(cell, None);
        return;
    }
    w.physics.set_cell_restriction(cell, Some(ObjectId(obj)));
    w.phys_ext
        .restriction_objs
        .entry(lb)
        .or_default()
        .insert(obj);
}

/// Releases a landblock's interior cells and buildings from the dat land source.
pub fn unload_landblock(w: &mut World, landblock: u16) {
    w.phys_ext.landblocks.remove(&landblock);
    w.phys_ext.restriction_objs.remove(&landblock);
    if let Some(land) = &w.phys_ext.land {
        land.unload_landblock(LandblockId(landblock));
    }
}

/// `LScape.get_landcell(cellID)`: the cell, if its landblock (and, indoors, the cell) is loaded.
// ACE: LScape.get_landcell
#[must_use]
pub fn get_landcell(w: &World, cell_id: u32) -> Option<CellId> {
    let cell = CellId(cell_id);
    let land = w.physics.land();
    if is_env_cell(cell) {
        land.env_cell(cell).map(|_| cell)
    } else {
        land.landblock(cell.landblock()).map(|_| cell)
    }
}

/// `LScape.get_landcell(cellID)` with the server half of `LScape.get_landblock`: on the server,
/// ACE's `get_landblock` answers `LandblockManager.GetLandblock(lbid, false, false)`, which loads
/// the landblock (its physics cells and, queued, its objects) when it is not loaded yet. The
/// player-position paths call this one, so a teleport to an unloaded landblock places the body in
/// its cell (as ACE does) instead of losing its cell. The `&World` readers keep [`get_landcell`].
// ACE: LScape.get_landcell
pub fn get_landcell_loading(w: &mut World, cell_id: u32) -> Option<CellId> {
    // `LScape.get_landblock(blockCellID)`: `LandblockManager.GetLandblock(lbid, false, false)`
    let lbid = empyrean_entity::LandblockId::new(cell_id | 0xFFFF);
    crate::managers::landblock_manager::get_landblock(w, lbid, false, false);
    get_landcell(w, cell_id)
}

/// `cell is EnvCell`: an interior cell id (index `0x100` and up).
#[must_use]
pub const fn is_env_cell(cell: CellId) -> bool {
    cell.index() >= 0x100
}

/// `EnvCell.SeenOutside`.
#[must_use]
pub fn seen_outside(w: &World, cell: CellId) -> bool {
    w.physics
        .land()
        .env_cell(cell)
        .is_some_and(|c| c.seen_outside)
}

/// `EnvCell.VisibleCells.Values`: each visible cell of the stab list, `None` where it is not
/// loaded, in the dat's order.
#[must_use]
pub fn visible_cells(w: &World, cell: CellId) -> Vec<Option<CellId>> {
    let land = w.physics.land();
    let Some(c) = land.env_cell(cell) else {
        return Vec::new();
    };
    c.stab_list
        .iter()
        .map(|&v| land.env_cell(v).map(|_| v))
        .collect()
}

/// `ObjCell.AddObjectListTo(target)`.
// ACE: ObjCell.AddObjectListTo
pub fn add_object_list_to(w: &World, cell: CellId, target: &mut Vec<PhysHandle>) {
    if let Some(list) = w.phys_ext.cell_objects.get(&cell.0) {
        target.extend_from_slice(list);
    }
}

/// `Landblock.SetAdjacents`: the loaded landblocks around `landblock`.
// ACE: Landblock.SetAdjacents
pub fn set_adjacents(w: &mut World, landblock: u16, adjacents: Vec<u16>) {
    w.phys_ext
        .landblocks
        .entry(landblock)
        .or_default()
        .adjacents = Some(adjacents);
}

/// The server objects on a landblock, then (if asked) on each adjacent landblock.
// ACE: Landblock.GetServerObjects
#[must_use]
pub fn get_server_objects(w: &World, landblock: u16, include_adjacents: bool) -> Vec<PhysHandle> {
    let Some(lb) = w.phys_ext.landblocks.get(&landblock) else {
        return Vec::new();
    };
    let mut results = lb.server_objects.clone();

    if include_adjacents {
        // DIVERGE: ACE throws on a landblock whose adjacents were never set; that cannot happen
        // on a running ACE (the landblock manager sets them on load), so none are read here.
        for adjacent in lb.adjacents.iter().flatten() {
            if let Some(a) = w.phys_ext.landblocks.get(adjacent) {
                results.extend_from_slice(&a.server_objects);
            }
        }
    }

    results
}

/// Orders a landblock's server objects by `Order` (stable, as LINQ's `OrderBy`).
// ACE: Landblock.SortObjects
pub fn sort_objects(w: &mut World, landblock: u16) {
    let Some(lb) = w.phys_ext.landblocks.get(&landblock) else {
        return;
    };
    let mut objs = lb.server_objects.clone();
    objs.sort_by_key(|h| w.phys_ext.objs.get(h).map_or(0, |e| e.order));
    if let Some(lb) = w.phys_ext.landblocks.get_mut(&landblock) {
        lb.server_objects = objs;
    }
}

// ---------------------------------------------------------------------------------------------
// Making bodies
// ---------------------------------------------------------------------------------------------

fn new_record(w: &World, has_part_array: bool) -> PhysObjExt {
    let now = physics_timer_current_time(w);
    PhysObjExt {
        order: 1,
        has_part_array,
        physics_creation_time: now,
        ..Default::default()
    }
}

/// A body for setup (or graphics object) `data_did`, keyed `object_iid`; static unless `dynamic`.
/// A setup with no geometry gives a body with no part array, which never enters a cell.
// ACE: PhysicsObj.makeObject
pub fn make_object(w: &mut World, data_did: u32, object_iid: u32, dynamic: bool) -> PhysHandle {
    let geometry = setup_geometry(w, data_did);
    let has_part_array = geometry.is_some();
    let geometry = geometry.unwrap_or_else(|| Arc::new(SetupGeometry::dummy()));
    let h = w.physics.create(ObjectId(object_iid), geometry, dynamic);
    let mut record = new_record(w, has_part_array);
    record.setup_id = data_did;
    w.phys_ext.objs.insert(h, record);
    h
}

/// `new PhysicsObj()` then `makeAnimObject(setupID, createParts)`: a dynamic body with id 0 until
/// `set_object_guid` names it.
// ACE: PhysicsObj.makeAnimObject
pub fn make_anim_object(w: &mut World, setup_id: u32, create_parts: bool) -> PhysHandle {
    let _ = create_parts; // the shared body always carries its parts
    make_object(w, setup_id, 0, true)
}

/// Names the body with the object's guid and adds it to the server object table.
// ACE: PhysicsObj.set_object_guid
pub fn set_object_guid(w: &mut World, h: PhysHandle, guid: ObjectGuid) {
    if !w.physics.set_object_id(h, ObjectId(guid.full())) {
        // DIVERGE: ACE re-keys unconditionally; the shared object table refuses a guid another
        // live body holds, and that body keeps it.
        log::warn!(
            "set_object_guid({:08X}): another body holds this id",
            guid.full()
        );
    }

    server_object_manager::add_server_object(w, h);
}

/// Sets the body's `WeenieObject`, and gives the shared crate's body the game-record facts its
/// transitions read (`WeenieObj != null`, `IsCreature`, `IsPlayer` and the player flags): the
/// missile-ignore test, the creature test of object collision and the player pass-through.
// ACE: PhysicsObj.set_weenie_obj
pub fn set_weenie_obj(w: &mut World, h: PhysHandle, wobj: WeenieObject) {
    let record = weenie_record(w, &wobj);
    if let Some(e) = ext_mut(w, h) {
        e.weenie_obj = wobj;
    }
    w.physics.set_weenie_restrictions(h, record);
}

/// The shared crate's record of a `WeenieObject` (`None` for ACE's null `WeenieObj`: a body with
/// no world object).
fn weenie_record(
    w: &World,
    wobj: &WeenieObject,
) -> Option<dereth_physics::obj::WeenieRestrictions> {
    wobj.world_object(w)?;
    Some(dereth_physics::obj::WeenieRestrictions {
        is_creature: wobj.is_creature,
        is_player: wobj.is_player,
        is_pk: wobj.is_pk(w),
        is_pk_lite: wobj.is_pk_lite(w),
        is_impenetrable: wobj.is_impenetrable(w),
        can_bypass: wobj.can_bypass_move_restrictions(w),
        ..dereth_physics::obj::WeenieRestrictions::default()
    })
}

/// Sets the motion table: the old movement manager is dropped and, for a non-zero table, a new
/// one (`dereth-animation`'s `MotionDriver`) is made in its default state.
// ACE: PhysicsObj.SetMotionTableID
pub fn set_motion_table_id(w: &mut World, h: PhysHandle, mtable_id: u32) -> bool {
    let Some(e) = ext_mut(w, h) else { return false };
    if !e.has_part_array {
        return false;
    }
    e.motion_table_id = mtable_id;

    crate::physics::motion::set_motion_table_id(w, h, mtable_id);
    true
}

// ACE: PhysicsObj.SetScaleStatic
pub fn set_scale_static(w: &mut World, h: PhysHandle, scale: f32) {
    if let Some(o) = w.physics.get_mut(h) {
        o.scale = scale;
    }
}

/// `PhysicsObj.State`.
#[must_use]
pub fn state(w: &World, h: PhysHandle) -> PhysicsState {
    #[allow(clippy::cast_possible_wrap)] // the same 32 bits
    w.physics
        .get(h)
        .map_or(PhysicsState(0), |o| PhysicsState(o.state.0 as i32))
}

/// `PhysicsObj.State = state` (a field write).
pub fn set_state(w: &mut World, h: PhysHandle, state: PhysicsState) {
    if let Some(o) = w.physics.get_mut(h) {
        #[allow(clippy::cast_sign_loss)] // the same 32 bits
        {
            o.state.0 = state.0 as u32;
        }
    }
}

/// `PhysicsObj.Velocity`.
#[must_use]
pub fn velocity(w: &World, h: PhysHandle) -> Vector3 {
    w.physics
        .get(h)
        .map_or(Vector3::ZERO, |o| vector3_of_data(o.velocity_vector))
}

/// `PhysicsObj.get_velocity()`: the cached velocity (`CachedVelocity`), not the `Velocity` field.
// ACE: PhysicsObj.get_velocity
#[must_use]
pub fn get_velocity(w: &World, h: PhysHandle) -> Vector3 {
    w.physics
        .get(h)
        .map_or(Vector3::ZERO, |o| vector3_of_data(o.cached_velocity))
}

/// `(PhysicsObj.TransientState & TransientStateFlags.OnWalkable) != 0` (a field read).
#[must_use]
pub fn on_walkable(w: &World, h: PhysHandle) -> bool {
    w.physics
        .get(h)
        .is_some_and(|b| b.transient_state.on_walkable())
}

/// `PhysicsObj.HasDefaultAnimation`: ACE declares the field and never assigns it (only the
/// `State` bit `HasDefaultAnim` is set, by `set_setup`), so it is always false.
#[must_use]
pub fn has_default_animation(_w: &World, _h: PhysHandle) -> bool {
    false
}

/// `PhysicsObj.HasDefaultScript`: never assigned in ACE either (only the `State` bit), so false.
#[must_use]
pub fn has_default_script(_w: &World, _h: PhysHandle) -> bool {
    false
}

/// `PhysicsObj.Velocity = v` (a field write; `set_velocity` is the method).
pub fn set_velocity_field(w: &mut World, h: PhysHandle, v: Vector3) {
    if let Some(o) = w.physics.get_mut(h) {
        o.velocity_vector = Vec3::new(v.x, v.y, v.z);
    }
}

/// The velocity method: clamps to the maximum velocity and activates the body. The event is the
/// client's; the server sends its own vector updates.
// ACE: PhysicsObj.set_velocity
pub fn set_velocity(w: &mut World, h: PhysHandle, v: Vector3, send_event: bool) {
    let _ = send_event;
    let now = physics_timer_current_time(w);
    if let Some(o) = w.physics.get_mut(h) {
        o.set_velocity(Vec3::new(v.x, v.y, v.z), now);
    }
}

// ACE: PhysicsObj.is_active
#[must_use]
pub fn is_active(w: &World, h: PhysHandle) -> bool {
    w.physics
        .get(h)
        .is_some_and(|o| o.transient_state.is_active())
}

/// Sets the active transient state flag; a static body cannot be activated.
// ACE: PhysicsObj.set_active
pub fn set_active(w: &mut World, h: PhysHandle, active: bool) -> bool {
    let now = physics_timer_current_time(w);
    let Some(o) = w.physics.get_mut(h) else {
        return false;
    };
    if active {
        if o.state.is_static() {
            return false;
        }

        if !o.transient_state.is_active() {
            o.update_time = now;
        }

        o.transient_state.set_active_bit(true);
        true
    } else {
        o.transient_state.set_active_bit(false);
        true
    }
}

/// Sets or clears ethereal; clearing is refused (and retried by the update) while something stands
/// in the body.
// ACE: PhysicsObj.set_ethereal
pub fn set_ethereal(w: &mut World, h: PhysHandle, ethereal: bool, send_event: bool) -> bool {
    let r = w.physics.set_ethereal(h, ethereal, send_event);
    after_physics(w, h);
    r == dereth_physics::EtherealResult::Applied
}

// ---------------------------------------------------------------------------------------------
// Entering, moving and leaving the world
// ---------------------------------------------------------------------------------------------

/// Places a body in the world at `pos`. Storage and corpses are forced into their cell; anything
/// else is placed by the shared crate's enter-world. On a placement that crossed into another
/// landblock while entering the world, the entry is discarded (ACE's custom check).
// ACE: PhysicsObj.enter_world
pub fn enter_world(w: &mut World, h: PhysHandle, pos: &PPosition) -> bool {
    prepare_mover(w, h);
    let Some(e) = ext_mut(w, h) else { return false };
    if !e.has_part_array {
        return false;
    }
    e.entering_world = true;
    let storage_or_corpse = e.weenie_obj.is_storage || e.weenie_obj.is_corpse;

    // enter_world(bool slide): `UpdateTime = PhysicsTimer.CurrentTime`
    let now = physics_timer_current_time(w);
    if let Some(o) = w.physics.get_mut(h) {
        o.update_time = now;
    }

    let mut success = if storage_or_corpse {
        // SetPosition: `if (WeenieObj.IsStorage || WeenieObj.IsCorpse) return ForceIntoCell(...)`
        w.physics.force_into_cell(h, pos);
        let placed = w.physics.get(h).is_some_and(|o| o.cell.is_some());
        if placed {
            // enter_world(bool): a non-static body goes active
            if let Some(o) = w.physics.get_mut(h) {
                if !o.state.is_static() {
                    o.transient_state.set_active_bit(true);
                }
            }
        }
        placed
    } else if let Some(scatter) = scatter_pos(w, h) {
        set_scatter_position_internal(w, h, &scatter)
    } else {
        w.physics.enter_world(h, pos)
    };

    if success {
        let placed_block = w.physics.get(h).map(|o| o.position.cell.landblock());
        if placed_block != Some(pos.cell.landblock()) {
            // AdjustToOutside and find_cell_list can inconsistently result in 2 different cells
            // for edges; ACE discards the placement completely (before committing it; here the
            // caller destroys the body).
            log::debug!(
                "{:08X} AddPhysicsObj() - {:?} resulted in {placed_block:?}, discarding",
                id(w, h).unwrap_or(0),
                pos.cell
            );
            success = false;
        }
    }

    if success {
        sync_cell(w, h);
        process_notices(w);
        // PartArray.HandleEnterWorld: the link animations are dropped
        crate::physics::motion::handle_enter_world(w, h);
    }

    if let Some(e) = ext_mut(w, h) {
        e.entering_world = false;
    }
    success
}

/// The scatter request of the body's world object (a generator's scatter spawn), if any.
fn scatter_pos(w: &World, h: PhysHandle) -> Option<crate::world_objects::world_object::ScatterPos> {
    let wo = ext(w, h)?.weenie_obj.world_object(w)?;
    w.objects.get(wo)?.wo.world_object.scatter_pos
}

/// How far (in metres) the terrain under a scatter point may be from the point's own height and
/// still have the point moved down (or up) onto it.
// ACE: PhysicsObj.ScatterThreshold_Z
pub const SCATTER_THRESHOLD_Z: f32 = 10.0;

/// Places the body at a random point around the scatter position: up to `num_tries` points, each
/// offset by up to the radius in X and Y. Outdoors the point is kept inside the landblock
/// (0.5 m from its edges), must stand on walkable terrain, and (away from buildings) is set on
/// the ground; indoors it must lie in one of the landblock's interior cells. The first point
/// that places wins; answers whether one did.
// ACE: PhysicsObj.SetScatterPositionInternal
fn set_scatter_position_internal(
    w: &mut World,
    h: PhysHandle,
    scatter: &crate::world_objects::world_object::ScatterPos,
) -> bool {
    use empyrean_common::thread_safe_random::ThreadSafeRandom;

    let mut result = false;
    let mut env_cells: Option<Vec<CellId>> = None;

    for _ in 0..scatter.num_tries {
        let mut new_pos = scatter.pos;

        #[allow(clippy::cast_possible_truncation)]
        // ACE's `(float)ThreadSafeRandom.Next(-1.0f, 1.0f)`
        {
            new_pos.frame.origin.x +=
                ThreadSafeRandom::next_float(-1.0, 1.0) as f32 * scatter.rad_x;
            new_pos.frame.origin.y +=
                ThreadSafeRandom::next_float(-1.0, 1.0) as f32 * scatter.rad_y;
        }

        let indoors = new_pos.cell.index() >= 0x100;
        if !indoors {
            // customized: stay on this landblock
            new_pos.frame.origin.x = new_pos.frame.origin.x.clamp(0.5, 191.5);
            new_pos.frame.origin.y = new_pos.frame.origin.y.clamp(0.5, 191.5);

            dereth_physics::landdefs::adjust_to_outside(
                &mut new_pos.cell,
                &mut new_pos.frame.origin,
            );

            // ensure walkable slope
            let land = w.physics.land();
            let Some(block) = land.landblock(new_pos.cell.landblock()) else {
                continue;
            };
            let Some(terrain_poly) =
                block.find_terrain_poly(new_pos.cell.index(), new_pos.frame.origin)
            else {
                continue;
            };
            if !dereth_physics::is_valid_walkable(terrain_poly.plane.normal) {
                continue;
            }

            // account for buildings: away from one, set to ground pos
            if land.building(new_pos.cell).is_none() {
                let ground_z = w
                    .physics
                    .terrain_height_at(&new_pos)
                    .unwrap_or(new_pos.frame.origin.z)
                    + 0.05;

                if (new_pos.frame.origin.z - ground_z).abs() > SCATTER_THRESHOLD_Z {
                    log::debug!(
                        "{:08X}.SetScatterPositionInternal() - tried to spawn outdoor object @ {:?} ground Z {ground_z} (diff: {}), investigate ScatterThreshold_Z",
                        id(w, h).unwrap_or(0),
                        new_pos,
                        new_pos.frame.origin.z - ground_z
                    );
                } else {
                    new_pos.frame.origin.z = ground_z;
                }
            }
        } else {
            let cells =
                env_cells.get_or_insert_with(|| landblock_env_cells(w, new_pos.cell.landblock()));
            let land = w.physics.land();
            let found = cells.iter().copied().find(|&id| {
                land.env_cell(id).is_some_and(|geom| {
                    dereth_physics::cell::Cell::Env { id, geom }.point_in_cell(new_pos.frame.origin)
                })
            });
            let Some(found) = found else { continue };
            new_pos.cell = found;
        }

        // `SetPositionInternal(newPos, setPos, transition)`: a placement that lands on another
        // landblock while entering the world is discarded (NoValidPosition), and the next point
        // is tried; one left without a cell answers OK (the caller then drops the body).
        if !w.physics.enter_world(h, &new_pos) {
            continue;
        }
        let placed = w
            .physics
            .get(h)
            .map(|o| (o.cell.is_some(), o.position.cell.landblock()));
        if placed.is_some_and(|(in_cell, block)| in_cell && block != new_pos.cell.landblock()) {
            continue;
        }
        result = true;
        break;
    }

    result
}

/// The interior cells of a landblock, from its `LandblockInfo` (`Landblock.get_envcells()`).
fn landblock_env_cells(w: &World, landblock: LandblockId) -> Vec<CellId> {
    let Some(lbi) = w
        .dats
        .cell_dat()
        .read_from_dat::<empyrean_dat::file_types::LandblockInfo>(landblock.info_id().0)
    else {
        return Vec::new();
    };
    (0..lbi.num_cells)
        .map(|i| landblock.cell(u16::try_from(0x100 + i).unwrap_or(u16::MAX)))
        .collect()
}

/// Moves a body to `pos` (the shared crate's placement), running the server half of any cell
/// change and routing its collisions.
// ACE: PhysicsObj.SetPosition
pub fn set_position(w: &mut World, h: PhysHandle, pos: &PPosition) -> bool {
    prepare_mover(w, h);
    let ok = w.physics.set_position(h, pos);
    after_physics(w, h);
    ok
}

/// Completely removes a body from the server: its cell and landblock lists, its collisions, every
/// visibility table that names it, the server object table and the physics world.
// ACE: PhysicsObj.DestroyObject
pub fn destroy_object(w: &mut World, h: PhysHandle) {
    // Not ACE's (retail, V287): a player may hold the body in its landblock view without knowing
    // it (the view is wider than the create set), so the players around it drop it here.
    if let (Some(lb), Some(obj_id)) = (cur_landblock(w, h), id(w, h)) {
        for p in get_server_objects(w, lb, true) {
            if p != h && is_player(w, p) {
                object_maint::remove_visible_entry(w, p, obj_id, h);
            }
        }
    }
    leave_cell(w, h);
    // leave_world()
    report_collision_end(w, h, true);
    object_maint::remove_object_to_be_destroyed(w, h, h);

    object_maint::destroy_object(w, h);

    // remove_shadows_from_cells(), exit_world() and the body itself
    w.physics.destroy(h);
    w.phys_ext.objs.remove(&h);
}

/// Steps a body to the current physics time, then runs the server half of any cell change,
/// routes the collisions the step raised (V6) and ends stale ones. Answers whether time was
/// consumed (ACE's `update_object` result).
// ACE: PhysicsObj.update_object
pub fn update_object(w: &mut World, h: PhysHandle) -> bool {
    prepare_mover(w, h);
    let now = physics_timer_current_time(w);
    let Some(o) = w.physics.get(h) else {
        return false;
    };
    let elapsed = now - o.update_time;
    let stepped = o.parent.is_none()
        && o.cell.is_some()
        && !o.state.is_frozen()
        && elapsed > dereth_physics::globals::MIN_STEP
        && elapsed <= dereth_physics::globals::HUGE_QUANTUM;
    let before = o.position.frame;

    let motion = crate::physics::motion::before_update(w, h);
    w.physics.update_object(h, LocalTime(now));

    // UpdateObjectInternal: `InitialUpdates++` when the step left the frame where it was
    let unchanged = w.physics.get(h).is_some_and(|o| o.position.frame == before);
    if stepped && unchanged {
        if let Some(e) = ext_mut(w, h) {
            e.initial_updates += 1;
        }
    }

    after_physics(w, h);
    // the motion layer's requests and notices from the step
    crate::physics::motion::after_update(w, h, motion);
    stepped
}

/// `PhysicsObj.transition(oldPos, newPos, adminMove)`: a transition of the body from `from` to
/// `to` with nothing committed (the line-of-sight tests), under its projectile target (the
/// shared crate's object info reads the body's `projectile_target_id`, A11).
// ACE: PhysicsObj.transition
pub fn transition(
    w: &mut World,
    h: PhysHandle,
    from: &PPosition,
    to: &PPosition,
    admin_move: bool,
) -> Option<dereth_physics::Transition> {
    // the house barriers' answers for this mover (the transition runs the entry check)
    prepare_mover(w, h);
    w.physics.transition(h, from, to, admin_move)
}

/// Commits a finished transition of the body (the shared crate's commit, A13: frame and cell,
/// contact plane, contact, walkable and sliding state, the sweep's collisions and the shadows),
/// then the server half: the cell change, the collisions routed through the `WeenieObject` (V6)
/// and the collision ends, as `handle_all_collisions` reports them inside ACE's commit.
// ACE: PhysicsObj.SetPositionInternal
pub fn set_position_internal(w: &mut World, h: PhysHandle, transit: &dereth_physics::Transition) {
    w.physics.commit_transition(h, transit);
    after_physics(w, h);
}

/// The body's physics radius: 0 for a body with a physics BSP, else its first cylinder sphere's
/// radius, else its first sphere's, times its scale (0 with neither). Not the part array's
/// bounding radius (`GetRadius`).
///
/// # Panics
/// Without a physics body (ACE: `NullReferenceException`).
// ACE: PhysicsObj.GetPhysicsRadius
#[must_use]
pub fn get_physics_radius(w: &World, h: PhysHandle) -> f32 {
    let body = w
        .physics
        .get(h)
        .expect("ACE: PhysicsObj is null (NullReferenceException)");
    if (state(w, h).0 & PhysicsState::HasPhysicsBSP.0) != 0 {
        return 0.0;
    }

    if let Some(cyl) = body.geometry.cyl_spheres.first() {
        return cyl.radius * body.scale;
    }
    if let Some(sphere) = body.geometry.spheres.first() {
        return sphere.radius * body.scale;
    }
    0.0
}

/// `PhysicsObj.ProjectileTarget = target`. The body's transitions take the target's id as their
/// `ObjectInfo.TargetID` (ACE's `ObjectInfo.Init`: `TargetID = obj.ProjectileTarget.ID`), which
/// the shared crate's missile-ignore test reads (A11).
// ACE: ObjectInfo.Init
pub fn set_projectile_target(w: &mut World, h: PhysHandle, target: Option<PhysHandle>) {
    let target_id = target.and_then(|t| id(w, t)).unwrap_or(0);
    if let Some(e) = ext_mut(w, h) {
        e.projectile_target = target;
    }
    if let Some(o) = w.physics.get_mut(h) {
        o.projectile_target_id = ObjectId(target_id);
    }
}

/// `PhysicsObj.ProjectileTarget`.
#[must_use]
pub fn projectile_target(w: &World, h: PhysHandle) -> Option<PhysHandle> {
    ext(w, h).and_then(|e| e.projectile_target)
}

/// The server half after any physics operation on `h`: cell change, notices, collision end.
fn after_physics(w: &mut World, h: PhysHandle) {
    sync_cell(w, h);
    process_notices(w);
    report_collision_end(w, h, false);
}

// ---------------------------------------------------------------------------------------------
// Cells, landblocks and visibility (server half)
// ---------------------------------------------------------------------------------------------

/// Brings the server half level with the body's cell: ACE's `change_cell_server`, run after the
/// shared crate has moved the body.
// ACE: PhysicsObj.change_cell_server
pub fn sync_cell(w: &mut World, h: PhysHandle) {
    let body_cell = w.physics.get(h).and_then(|o| o.cell);
    let Some(e) = ext(w, h) else { return };
    if e.cur_cell == body_cell {
        return;
    }

    if e.cur_cell.is_some() {
        leave_cell(w, h);
    }
    if let Some(new_cell) = body_cell {
        enter_cell_server(w, h, new_cell);
    }
}

/// `ObjCell.AddObject`, `CurCell`, and registration as a server object on the cell's landblock.
// ACE: PhysicsObj.enter_cell
fn enter_cell(w: &mut World, h: PhysHandle, new_cell: CellId) {
    let Some(e) = ext_mut(w, h) else { return };
    if !e.has_part_array {
        return;
    }
    e.cur_cell = Some(new_cell);
    let dat_object = e.dat_object;
    w.phys_ext
        .cell_objects
        .entry(new_cell.0)
        .or_default()
        .push(h);

    if !dat_object {
        let lb = new_cell.landblock().0;
        if let Some(e) = ext_mut(w, h) {
            e.cur_landblock = Some(lb);
        }
        add_server_object(w, lb, h);
    }
}

/// `ObjCell.RemoveObject`, clearing `CurCell`, and leaving the landblock's server objects.
// ACE: PhysicsObj.leave_cell
fn leave_cell(w: &mut World, h: PhysHandle) {
    let Some(e) = ext_mut(w, h) else { return };
    let Some(cell) = e.cur_cell.take() else {
        return;
    };
    let (lb, dat_object) = (e.cur_landblock, e.dat_object);
    if let Some(list) = w.phys_ext.cell_objects.get_mut(&cell.0) {
        if let Some(i) = list.iter().position(|x| *x == h) {
            list.remove(i);
        }
    }

    if let (Some(lb), false) = (lb, dat_object) {
        remove_server_object(w, lb, h);
        if let Some(e) = ext_mut(w, h) {
            e.cur_landblock = None;
        }
    }
}

// ACE: Landblock.add_server_object
fn add_server_object(w: &mut World, landblock: u16, h: PhysHandle) -> bool {
    let lb = w.phys_ext.landblocks.entry(landblock).or_default();
    if !lb.server_objects.contains(&h) {
        lb.server_objects.push(h);
        return true;
    }
    false
}

// ACE: Landblock.remove_server_object
fn remove_server_object(w: &mut World, landblock: u16, h: PhysHandle) -> bool {
    let Some(lb) = w.phys_ext.landblocks.get_mut(&landblock) else {
        return false;
    };
    match lb.server_objects.iter().position(|x| *x == h) {
        Some(i) => {
            lb.server_objects.remove(i);
            true
        }
        None => false,
    }
}

/// Entering a cell on the server: the lists, the initial location sync while entering the world,
/// this body's visibility, and every player that knows it.
// ACE: PhysicsObj.enter_cell_server
fn enter_cell_server(w: &mut World, h: PhysHandle, new_cell: CellId) {
    enter_cell(w, h, new_cell);

    // sync location for initial CO
    let (entering_world, wo) = ext(w, h).map_or((false, None), |e| {
        (e.entering_world, e.weenie_obj.world_object(w))
    });
    if entering_world {
        if let Some(wo) = wo {
            crate::world_objects::world_object::sync_location(w, wo);
        }
    }

    // handle self
    if is_player(w, h) {
        let newly_visible = handle_visible_cells(w, h);
        enqueue_objs(w, h, &newly_visible);
    } else {
        handle_visible_cells_non_player(w, h);
    }

    // handle known players
    // Not ACE's (retail, V287; the retail captures): every player in the body's
    // reach (its landblock and the adjacent ones) is evaluated, not only those who already know it,
    // so an object moving into a standing player's create set is created for that player (retail
    // created 16,888 such). The known players come first (ACE's order), then the rest of the
    // reach, then any player the body targets (the landblock view it feeds is kept level).
    for player in players_to_evaluate(w, h) {
        let added = handle_visible_obj(w, player, h);

        if added {
            enqueue_obj(w, player, h);
        }
    }
}

/// Not ACE's (retail, V287): the players whose view of body `h` is evaluated when it changes
/// cell: its known players, the players among the server objects of its landblock and the adjacent
/// ones, and the players among its visible targets; each once, never `h` itself. None for a dat
/// object, which is never sent.
fn players_to_evaluate(w: &World, h: PhysHandle) -> Vec<PhysHandle> {
    if ext(w, h).is_none_or(|e| e.dat_object) {
        return Vec::new();
    }
    let mut players = object_maint::get_known_players_values(w, h);
    let reach = cur_landblock(w, h).map_or_else(Vec::new, |lb| get_server_objects(w, lb, true));
    for p in reach
        .into_iter()
        .chain(object_maint::get_visible_targets_values(w, h))
    {
        if p != h && is_player(w, p) && !players.contains(&p) {
            players.push(p);
        }
    }
    players
}

/// Maintains the list of visible objects for a player; returns the objects newly visible (and
/// previously unknown) since the last call.
// ACE: PhysicsObj.handle_visible_cells
pub fn handle_visible_cells(w: &mut World, h: PhysHandle) -> Vec<PhysHandle> {
    // remove any objects that have been in the destruction queue > DestructionTime
    object_maint::destroy_objects(w, h);

    // get the list of visible objects from this cell
    let visible_objects =
        object_maint::get_visible_objects(w, h, cur_cell(w, h), VisibleObjectType::All);

    // get the difference between current and previous visible
    let newly_occluded =
        object_maint::get_visible_objects_where(w, h, |_, o| !visible_objects.contains(&o));

    // Not ACE's (retail, V287; the retail captures): the visible objects keep
    // ACE's landblock view (with its first-sight clamp), which the monsters' targets and wake-ups
    // read; what the player is sent, and forgets, is its create set below. A body destroyed while
    // in view is dropped by its id.
    for &o in &visible_objects {
        object_maint::add_visible_object(w, h, o);
    }
    for (id, o) in newly_occluded {
        object_maint::remove_visible_entry(w, h, id, o);
    }

    // the create set: the newly known are created, known objects outside it start their forget
    // clock, and one back inside it before its deadline is kept
    let create_set = object_maint::get_create_set(w, h);
    let mut create_objs = Vec::new();
    for &o in &create_set {
        if object_maint::enter_create_set(w, h, o) {
            create_objs.push(o);
        }
    }
    for o in object_maint::get_known_objects_values(w, h) {
        if !create_set.contains(&o) {
            object_maint::leave_create_set(w, h, o);
        }
    }

    create_objs
}

/// A monster tracks its attack targets; anything else registers the players that can see it
/// (within the initial clamp); a combat pet tracks monsters.
// ACE: PhysicsObj.handle_visible_cells_non_player
pub fn handle_visible_cells_non_player(w: &mut World, h: PhysHandle) {
    let wo = weenie_obj(w, h);
    let cell = cur_cell(w, h);
    if wo.is_monster {
        // players and combat pets
        let visible_targets =
            object_maint::get_visible_objects(w, h, cell, VisibleObjectType::AttackTargets);

        object_maint::add_visible_targets(w, h, &visible_targets);
    } else {
        // everything except monsters
        // usually these are server objects whose position never changes
        // Not ACE's (retail, V287; the retail captures): ACE made the players
        // within 112.5 m in view known players here, so that they were sent the object; a player
        // now knows an object only when it was created for it, through the create set evaluated
        // for every player in the object's reach (`enter_cell_server`), so nothing is added.
    }

    if wo.is_combat_pet {
        let visible_monsters =
            object_maint::get_visible_objects(w, h, cell, VisibleObjectType::AttackTargets);

        object_maint::add_visible_targets(w, h, &visible_monsters);
    }
}

/// A player's view of one object that moved: visible objects are added (answering whether it is
/// newly visible and was unknown), occluded ones are queued for destruction.
// ACE: PhysicsObj.handle_visible_obj
pub fn handle_visible_obj(w: &mut World, h: PhysHandle, obj: PhysHandle) -> bool {
    let (Some(cell), Some(obj_cell)) = (cur_cell(w, h), cur_cell(w, obj)) else {
        if cur_cell(w, h).is_none() {
            log::warn!(
                "{:08X}.handle_visible_obj({:08X}): CurCell null",
                id(w, h).unwrap_or(0),
                id(w, obj).unwrap_or(0)
            );
        } else {
            log::warn!(
                "{:08X}.handle_visible_obj({:08X}): obj.CurCell null",
                id(w, h).unwrap_or(0),
                id(w, obj).unwrap_or(0)
            );
        }
        return false;
    };

    let is_visible = is_visible(w, cell, obj_cell);

    // Not ACE's (retail, V287; the retail captures): the visible objects keep
    // ACE's landblock view (the monsters' targets and wake-ups read it); knowledge, the create and
    // the forget clock follow the create set.
    let obj_id = id(w, obj).unwrap_or(0);
    if is_visible {
        object_maint::add_visible_object(w, h, obj);
    } else if object_maint::visible_objects_contains_key(w, h, obj_id) {
        object_maint::remove_visible_object(w, h, obj, true);
    }

    if object_maint::in_create_set(w, h, obj) {
        // Not ACE's (V260, V278): a return after the forget deadline
        // is created afresh now, and any earlier return cancels the pending forget
        // (`enter_create_set`).
        object_maint::enter_create_set(w, h, obj)
    } else {
        object_maint::leave_create_set(w, h, obj);

        false
    }
}

/// `ObjCell.IsVisible(cell)`: the same cell; an interior cell's PVS (either way round); or,
/// outdoors, landblocks at most one apart.
// ACE: ObjCell.IsVisible
#[must_use]
pub fn is_visible(w: &World, this_cell: CellId, cell: CellId) -> bool {
    if this_cell == cell {
        return true;
    }

    if is_env_cell(this_cell) {
        is_visible_indoors(w, this_cell, cell)
    } else if is_env_cell(cell) {
        is_visible_indoors(w, cell, this_cell)
    } else {
        // outdoors
        get_block_dist(this_cell.0, cell.0) <= 1
    }
}

/// `EnvCell.IsVisibleIndoors(cell)`: in the same landblock, `cell` is in the PVS; otherwise the
/// cell must be seen outside and the landblocks at most one apart.
// ACE: EnvCell.IsVisibleIndoors
#[must_use]
pub fn is_visible_indoors(w: &World, env_cell: CellId, cell: CellId) -> bool {
    let block_dist = get_block_dist(env_cell.0, cell.0);

    // if landblocks equal
    if block_dist == 0 {
        // check env VisibleCells (keyed by the low word)
        let cell_id = cell.0 & 0xFFFF;
        let land = w.physics.land();
        if land
            .env_cell(env_cell)
            .is_some_and(|c| c.stab_list.iter().any(|v| v.0 & 0xFFFF == cell_id))
        {
            return true;
        }
    }
    seen_outside(w, env_cell) && block_dist <= 1
}

/// The larger of the landblock x and y distances between two cell ids.
// ACE: PhysicsObj.GetBlockDist
#[must_use]
pub fn get_block_dist(a: u32, b: u32) -> i32 {
    let lbx_a = i64::from(a >> 24);
    let lby_a = i64::from((a >> 16) & 0xFF);

    let lbx_b = i64::from(b >> 24);
    let lby_b = i64::from((b >> 16) & 0xFF);

    let dx = (lbx_a - lbx_b).abs();
    let dy = (lby_a - lby_b).abs();

    i32::try_from(dx.max(dy)).unwrap_or(i32::MAX)
}

// ACE: PhysicsObj.TeleportCreateObjectDelay
/// How long after a teleport a player's newly visible objects wait before they are sent.
pub const TELEPORT_CREATE_OBJECT_DELAY_SECONDS: f64 = 1.0;

/// `DateTime.UtcNow - player.LastTeleportTime < TeleportCreateObjectDelay`.
fn within_teleport_create_object_delay(w: &World, player: ObjectGuid) -> bool {
    let last_teleport_time =
        crate::world_objects::player_location::fields(w, player).last_teleport_time;
    w.now.utc - last_teleport_time < TimeSpan::from_seconds(TELEPORT_CREATE_OBJECT_DELAY_SECONDS)
}

/// A player's newly visible objects: each is tracked (the client is sent its create-object); a
/// second later when the player teleported less than a second ago, one tick later for an object
/// that is itself teleporting (so its post-teleport position is sent).
// ACE: PhysicsObj.enqueue_objs
pub fn enqueue_objs(w: &mut World, h: PhysHandle, newly_visible: &[PhysHandle]) {
    let Some(player) = player_of(w, h) else {
        return;
    };

    if within_teleport_create_object_delay(w, player) {
        // DIVERGE: ACE's lambda resolves `obj.WeenieObj.WorldObject` when it runs, and its
        // reference keeps a destroyed object sendable; here the objects are resolved now and one
        // destroyed during the delay is skipped (it has left the store).
        let objs: Vec<ObjectGuid> = newly_visible
            .iter()
            .filter_map(|&obj| weenie_obj(w, obj).world_object(w))
            .collect();
        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, TELEPORT_CREATE_OBJECT_DELAY_SECONDS);
        action_chain.add_action(Actor::Object(player), move |w: &mut World| {
            for &wo in &objs {
                track_object(w, player, wo, true);
            }
        });
        action_chain.enqueue_chain(w);
    } else {
        for &obj in newly_visible {
            let Some(wo) = weenie_obj(w, obj).world_object(w) else {
                continue;
            };

            if w.objects
                .get(wo)
                .is_some_and(|o| o.wo.world_object.teleporting)
            {
                // ensure post-teleport position is sent
                let mut action_chain = ActionChain::new();
                action_chain.add_delay_for_one_tick(w);
                action_chain.add_action(Actor::Object(player), move |w: &mut World| {
                    track_object(w, player, wo, false)
                });
                action_chain.enqueue_chain(w);
            } else {
                track_object(w, player, wo, false);
            }
        }
    }
}

/// One newly visible object for a player, with [`enqueue_objs`]' delays.
// ACE: PhysicsObj.enqueue_obj
pub fn enqueue_obj(w: &mut World, h: PhysHandle, newly_visible: PhysHandle) {
    let Some(player) = player_of(w, h) else {
        return;
    };

    let Some(wo) = weenie_obj(w, newly_visible).world_object(w) else {
        return;
    };

    if within_teleport_create_object_delay(w, player) {
        let mut action_chain = ActionChain::new();
        action_chain.add_delay_seconds(w, TELEPORT_CREATE_OBJECT_DELAY_SECONDS);
        action_chain.add_action(Actor::Object(player), move |w: &mut World| {
            track_object(w, player, wo, true)
        });
        action_chain.enqueue_chain(w);
    } else if w
        .objects
        .get(wo)
        .is_some_and(|o| o.wo.world_object.teleporting)
    {
        // ensure post-teleport position is sent
        let mut action_chain = ActionChain::new();
        action_chain.add_delay_for_one_tick(w);
        action_chain.add_action(Actor::Object(player), move |w: &mut World| {
            track_object(w, player, wo, false)
        });
        action_chain.enqueue_chain(w);
    } else {
        track_object(w, player, wo, false);
    }
}

/// `IsPlayer && WeenieObj.WorldObject is Player player`.
fn player_of(w: &World, h: PhysHandle) -> Option<ObjectGuid> {
    if !is_player(w, h) {
        return None;
    }
    weenie_obj(w, h)
        .world_object(w)
        .filter(|g| w.objects.get(*g).is_some_and(|o| o.is_player()))
}

/// `player.TrackObject(wo, delayCreate)`: sends the create-object.
fn track_object(w: &mut World, player: ObjectGuid, wo: ObjectGuid, delay_create: bool) {
    crate::world_objects::player_tracking::track_object(w, player, wo, delay_create);
}

// ---------------------------------------------------------------------------------------------
// Collisions (V6)
// ---------------------------------------------------------------------------------------------

/// Drains the physics notices and routes each through the body's `WeenieObject`, as ACE's
/// `handle_all_collisions` / `track_object_collision` / `report_*_collision` would have.
fn process_notices(w: &mut World) {
    let notices: Vec<PhysicsNotice> = w.physics.drain_notices().collect();
    for notice in notices {
        match notice {
            PhysicsNotice::ObjectCollision { object, other, .. }
            | PhysicsNotice::MissileCollision { object, other, .. } => {
                let Some(mover) = server_object_manager::get_object_a(w, object.0) else {
                    continue;
                };
                let target = server_object_manager::get_object_a(w, other.0);

                // track_object_collision: `CollisionTable[obj.ID] = new CollisionRecord(now, obj ethereal)`
                let now = physics_timer_current_time(w);
                let ethereal = target
                    .and_then(|t| w.physics.get(t))
                    .is_some_and(|o| o.state.is_ethereal());
                if let Some(e) = ext_mut(w, mover) {
                    e.collision_table.insert(
                        other.0,
                        CollisionRecord {
                            touched_time: now,
                            ethereal,
                        },
                    );
                }

                // report_object_collision: the mover's report
                let mover_wo = weenie_obj(w, mover);
                let target_wo = target.map(|t| weenie_obj(w, t)).unwrap_or_default();
                mover_wo.do_collision_object(w, &target_wo);

                // ... and the other object's, which ACE addresses to that object itself; its
                // DoCollision drops a self-collision.
                let other_reports = target
                    .and_then(|t| w.physics.get(t))
                    .is_some_and(|o| o.state.reports_collisions());
                let mover_ignores = w
                    .physics
                    .get(mover)
                    .is_some_and(|o| o.state.ignores_collisions());
                if other_reports && !mover_ignores {
                    target_wo.do_collision_object(w, &target_wo);
                }
            }
            PhysicsNotice::EnvironmentCollision { object, .. } => {
                let Some(mover) = server_object_manager::get_object_a(w, object.0) else {
                    continue;
                };
                weenie_obj(w, mover).do_collision_environment(w);
            }
            // The shared crate raises no end notice; ends come from the collision table below.
            // ACE's `handle_move_restriction` does nothing on the server, and the player notice
            // is the client's (the server sets no player).
            PhysicsNotice::CollisionEnd { .. }
            | PhysicsNotice::MoveRestricted { .. }
            | PhysicsNotice::PlayerPhysicsUpdated => {}
        }
    }
}

/// Ends the collisions last touched over a second ago (an ethereal one at once), or all of them
/// when forced, reporting each end to both parties.
// ACE: PhysicsObj.report_collision_end
pub fn report_collision_end(w: &mut World, h: PhysHandle, force_end: bool) {
    let now = physics_timer_current_time(w);
    let Some(e) = ext(w, h) else { return };

    let mut ends = Vec::new();
    for (collision_id, collision) in e.collision_table.iter() {
        let delta_time = now - collision.touched_time;

        if delta_time > 1.0 || collision.ethereal && delta_time > 0.0 || force_end {
            ends.push(*collision_id);
        }
    }

    for end in ends {
        if let Some(e) = ext_mut(w, h) {
            e.collision_table.remove(&end);
        }
        report_object_collision_end(w, h, end);
    }
}

// ACE: PhysicsObj.report_object_collision_end
fn report_object_collision_end(w: &mut World, h: PhysHandle, object_id: u32) -> bool {
    // `ObjMaint != null` always holds on the server.
    let Some(collision) = server_object_manager::get_object_a(w, object_id) else {
        return true;
    };
    let Some(c) = w.physics.get(collision) else {
        return true;
    };
    if !c.state.reports_as_environment() {
        let collision_reports = c.state.reports_collisions();
        let reports = w
            .physics
            .get(h)
            .is_some_and(|o| o.state.reports_collisions());
        let my_id = id(w, h).unwrap_or(0);
        if reports {
            weenie_obj(w, h).do_collision_end(w, ObjectGuid::new(object_id));
        }

        if collision_reports {
            weenie_obj(w, collision).do_collision_end(w, ObjectGuid::new(my_id));
        }
    }
    true
}

// ---------------------------------------------------------------------------------------------
// WorldObject physics state (WorldObject_Properties.cs), world-level
// ---------------------------------------------------------------------------------------------

/// `(PhysicsObj.State & state) != 0`; false without a body.
// ACE: WorldObject.GetPhysicsState
#[must_use]
pub fn get_physics_state(w: &World, wo: ObjectGuid, flag: PhysicsState) -> bool {
    let Some(h) = physics_obj(w, wo) else {
        return false;
    };
    (state(w, h).0 & flag.0) != 0
}

/// Sets or clears `flag` on the body (null clears); nothing without a body. The `&mut self`
/// `WorldObject::set_physics_state` cannot reach `World.physics`; world-level callers use this.
// ACE: WorldObject.SetPhysicsState
pub fn set_physics_state(w: &mut World, wo: ObjectGuid, flag: PhysicsState, value: Option<bool>) {
    let Some(h) = physics_obj(w, wo) else { return };
    let mut s = state(w, h);
    if value == Some(true) {
        s.0 |= flag.0;
    } else {
        s.0 &= !flag.0; // default to false for null, should get real physics default for this field
    }
    set_state(w, h, s);
}

/// Stores a `bool?` physics property and mirrors it onto the body's state (null removes the
/// property and clears the flag). The world-level form of the `physics_property!` setters.
// ACE: WorldObject.SetPhysicsPropertyState
pub fn set_physics_property_state(
    w: &mut World,
    wo: ObjectGuid,
    property: PropertyBool,
    flag: PhysicsState,
    value: Option<bool>,
) {
    let Some(o) = w.objects.get_mut(wo) else {
        return;
    };
    match value {
        Some(v) => {
            o.set_property(property, v);
            set_physics_state(w, wo, flag, value);
        }
        None => {
            o.remove_property(property);
            set_physics_state(w, wo, flag, Some(false)); // default to false for null, should get real physics default for this field
        }
    }
}

/// The physics part of `Landblock.AddWorldObjectInternal`: make the body (or re-add it to the
/// server object table) and place it. The landblock calls this; `false` means the object
/// could not be placed (ACE then clears `CurrentLandblock` and notifies the generator).
pub fn add_world_object_physics(w: &mut World, wo: ObjectGuid) -> bool {
    match physics_obj(w, wo) {
        None => crate::dispatch::init_physics_obj::init_physics_obj(w, wo),
        Some(h) => set_object_guid(w, h, wo), // re-add to ServerObjectManager
    }

    let placed = physics_obj(w, wo).is_some_and(|h| cur_cell(w, h).is_some());
    if !placed {
        return crate::world_objects::world_object::add_physics_obj(w, wo);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// The house barrier (V1: the shared crate's entry check, ACE's CanMoveInto as the host's answer)
// ---------------------------------------------------------------------------------------------

/// The server's answer to the shared transition's entry-restriction question (A12): ACE's
/// `check_entry_restrictions` tail (`ServerObjectManager.GetObjectA(RestrictionObj)`, a missing
/// object or `WeenieObj` collides, else `WeenieObj.CanMoveInto(mover)`) for one mover, evaluated
/// by [`prepare_mover`] for each barrier the mover can reach.
#[derive(Debug)]
struct BarrierAnswers {
    mover: ObjectId,
    /// Barrier id to `CanMoveInto`'s answer.
    answers: BTreeMap<u32, bool>,
}

impl dereth_physics::transition::EntryRestrictionHost for BarrierAnswers {
    fn can_move_into(&self, restriction: ObjectId, mover: ObjectId) -> bool {
        if mover != self.mover {
            log::warn!(
                "check_entry_restrictions: {:08X} moved without prepare_mover ({:08X} prepared)",
                mover.0,
                self.mover.0
            );
            return false;
        }
        // a barrier not evaluated is one GetObjectA does not find: `return TransitionState.Collided`
        self.answers.get(&restriction.0).copied().unwrap_or(false)
    }
}

/// Before a player's body moves: its game record (`is_player`, the PK flags and the barrier
/// bypass the shared crate's transition reads), and the host's answers to the entry-restriction
/// question for this mover: for each house barrier on its landblock and the adjacent ones, what
/// ACE's `WeenieObject.CanMoveInto` answers (A12). Anything that is not a player is never
/// restricted (the transition's `IsPlayer` test) and needs no answers.
///
/// DIVERGE (V180): ACE asks `CanMoveInto` at the moment the transition enters the cell; here the
/// answers for the barriers the mover can reach are taken just before its move and handed to the
/// shared transition's host hook (a transition cannot reach the world). Nothing a transition does
/// changes an answer, so each is the one ACE would get.
// ACE: ObjCell.check_entry_restrictions
pub fn prepare_mover(w: &mut World, h: PhysHandle) {
    if !is_player(w, h) {
        return;
    }
    let mover = weenie_obj(w, h);
    if mover.world_object(w).is_none() {
        return;
    }

    let record = weenie_record(w, &mover);
    w.physics.set_weenie_restrictions(h, record);

    let Some((cell, mover_id)) = w.physics.get(h).map(|o| (o.position.cell.0, o.id)) else {
        return;
    };
    let barriers: Vec<u32> = w
        .phys_ext
        .restriction_objs
        .iter()
        .filter(|(&lb, _)| get_block_dist(u32::from(lb) << 16, cell) <= 1)
        .flat_map(|(_, objs)| objs.iter().copied())
        .collect();
    let mut answers = BTreeMap::new();
    for id in barriers {
        // `var restrictionObj = ServerObjectManager.GetObjectA(RestrictionObj);` (a missing one
        // collides: no answer, which the host reads as closed)
        let Some(barrier) = server_object_manager::get_object_a(w, id) else {
            continue;
        };
        answers.insert(id, weenie_obj(w, barrier).can_move_into(w, &mover));
    }
    w.physics
        .set_entry_restriction_host(Some(Arc::new(BarrierAnswers {
            mover: mover_id,
            answers,
        })));
}
