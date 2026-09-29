// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/Landblock.cs
//! Port of `Source/ACE.Server/Entity/Landblock.cs`.
//!
//! The gist of a landblock is that, generally, everything on it publishes to and subscribes to
//! everything else in the landblock. x/y in an outdoor landblock goes from 0 to 192. "indoor"
//! (dungeon) landblocks have no functional limit as players can't freely roam in/out of them.
//!
//! # Shape
//!
//! A [`Landblock`] lives in `World.landblock_manager.landblocks` and holds its objects as guids.
//! Members that touch only the landblock are methods; members that touch objects, other
//! landblocks or managers are free functions taking `(w, id)`, `id` naming a loaded landblock.
//!
//! - `worldObjects`/`pendingAdditions` are [`DotNetDict`]s, so their enumeration order is .NET's.
//!   Each value caches the object's `WeenieClassId` (it never changes), so the KeepAlive
//!   bookkeeping still works for an object that has already left the store.
//! - The `LinkedList`s sorted by time are [`SortedByTime`]. Their sort keys are read live from the
//!   objects on every comparison, as ACE reads `NextHeartbeatTime` off each node.
//! - `Task.Run` in [`init`] runs inline (DIVERGE, arch): the factory work happens at load, and the
//!   delegates it enqueues run on the landblock's first tick, as in ACE.
//! - A world object that is gone from the store is ACE's destroyed object. The tick loops drop
//!   such a node and go on (DIVERGE: ACE would tick the destroyed object once more).
//!
//! Members of other classes that are not ported yet are called through the `pub` pointer
//! functions at the end of this file, each a `not_ported!` site returning ACE's null/false/0.

// C# float and double casts.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::collections::VecDeque;
use std::sync::Arc;

use dereth_physics::landdefs;
use dereth_primitives::{CellId, Frame, Position as PPosition, Quat, Vec3};
use empyrean_common::dotnet::cast::CsCast;
use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::dotnet::DotNetDict;
use empyrean_common::performance::rate_monitor::RateMonitor;
use empyrean_content::models::world::LandblockInstance;
use empyrean_dat::file_types::{CellLandblock, LandblockInfo};
use empyrean_entity::enums::{
    EnvironChangeType, HouseType, PropertyString, RegenerationType, WeenieType,
};
use empyrean_entity::numerics::{Quaternion, Vector3};
use empyrean_entity::{Biota, LandblockId, ObjectGuid, Position};

use crate::entity::actions::action_queue::{self, ActionQueue};
use crate::entity::actions::i_action::Action;
use crate::entity::actions::i_actor::Actor;
use crate::entity::landblock_group::LandblockGroupId;
use crate::entity::landblock_mesh::LandblockMesh;
use crate::factories::world_object_factory;
use crate::managers::landblock_manager;
use crate::managers::server_performance_monitor::{self as perf, CumulativeEventHistoryType};
use crate::object_store::ObjectStore;
use crate::physics::phys_ext;
use crate::world_objects::world_object::{CtorEnv, WorldObject};
use crate::world_objects::{world_object_generators, world_object_tick};
use crate::World;

// ACE: Landblock.AdjacencyLoadRange
pub const ADJACENCY_LOAD_RANGE: f32 = 96.0;
// ACE: Landblock.OutdoorChatRange
pub const OUTDOOR_CHAT_RANGE: f32 = 75.0;
// ACE: Landblock.IndoorChatRange
pub const INDOOR_CHAT_RANGE: f32 = 25.0;
// ACE: Landblock.MaxXY
pub const MAX_XY: f32 = 192.0;
// ACE: Landblock.MaxObjectRange
pub const MAX_OBJECT_RANGE: f32 = 192.0;
// ACE: Landblock.MaxObjectGhostRange
pub const MAX_OBJECT_GHOST_RANGE: f32 = 250.0;

/// Landblocks heartbeat every 5 seconds.
fn heartbeat_interval() -> TimeSpan {
    TimeSpan::from_seconds(5.0)
}

/// Landblock items will be saved to the database every 5 minutes.
fn database_save_interval() -> TimeSpan {
    TimeSpan::from_minutes(5.0)
}

/// Landblocks which have been inactive for this long will be dormant.
fn dormant_interval() -> TimeSpan {
    TimeSpan::from_minutes(1.0)
}

/// `Landblock.UnloadInterval`: landblocks which have been inactive for this long will be unloaded.
pub fn unload_interval() -> TimeSpan {
    TimeSpan::from_minutes(5.0)
}

/// `Landblock KeepAlive weenie (ACE custom)`.
const KEEP_ALIVE_WCID: u32 = 80007;

/// `Corpse.EmptyDecayTime`.
const CORPSE_EMPTY_DECAY_TIME: f64 = 15.0;

/// ACE's `LinkedList<WorldObject>` kept sorted by a time the objects carry (`NextHeartbeatTime`,
/// `NextGeneratorUpdateTime`, ...), or appended to in tick order (`sortedCreaturesByNextTick`).
/// Nodes are guids; the times are read through a key function at each comparison.
#[derive(Debug, Default, Clone)]
pub struct SortedByTime {
    list: VecDeque<ObjectGuid>,
}

impl SortedByTime {
    pub fn new() -> Self {
        Self::default()
    }

    /// `Count`.
    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// `First.Value`.
    pub fn first(&self) -> Option<ObjectGuid> {
        self.list.front().copied()
    }

    /// `RemoveFirst()`.
    pub fn remove_first(&mut self) {
        self.list.pop_front();
    }

    /// `AddLast(value)`.
    pub fn add_last(&mut self, g: ObjectGuid) {
        self.list.push_back(g);
    }

    /// `Remove(value)`: the first node holding it.
    pub fn remove(&mut self, g: ObjectGuid) -> bool {
        match self.list.iter().position(|&x| x == g) {
            Some(i) => {
                self.list.remove(i);
                true
            }
            None => false,
        }
    }

    /// `Contains(value)`.
    pub fn contains(&self, g: ObjectGuid) -> bool {
        self.list.contains(&g)
    }

    /// The nodes, first to last.
    pub fn iter(&self) -> impl Iterator<Item = &ObjectGuid> {
        self.list.iter()
    }

    /// The body shared by ACE's three `InsertWorldObjectIntoSorted*List`s. `key` reads the
    /// object's time; `double.MaxValue` means "never", and such an object is not listed.
    pub fn insert_sorted(&mut self, world_object: ObjectGuid, key: impl Fn(ObjectGuid) -> f64) {
        let time = key(world_object);

        // If you want to add checks to exclude certain object types from heartbeating, you would do it here
        #[allow(clippy::float_cmp)]
        if time == f64::MAX {
            return;
        }

        let Some(&last) = self.list.back() else {
            self.list.push_front(world_object);
            return;
        };

        if key(last) <= time {
            self.list.push_back(world_object);
            return;
        }

        for i in 0..self.list.len() {
            if time <= key(self.list[i]) {
                self.list.insert(i, world_object);
                return;
            }
        }

        self.list.push_back(world_object); // This line really shouldn't be hit
    }
}

// ACE: Landblock
#[derive(Debug)]
pub struct Landblock {
    // ACE: Landblock.Id
    pub id: LandblockId,

    // ACE: Landblock.Permaload
    /// Flag indicates if this landblock is permanently loaded (for example, towns on
    /// high-traffic servers).
    pub permaload: bool,

    // ACE: Landblock.HasNoKeepAliveObjects
    /// Flag indicates if this landblock has no keep alive objects.
    pub has_no_keep_alive_objects: bool,

    // ACE: Landblock.CreateWorldObjectsCompleted
    /// This must be true before a player enters a landblock. This prevents a player from possibly
    /// passing through a door that hasn't spawned in yet, and other scenarios.
    create_world_objects_completed: bool,

    last_active_time: DotNetDateTime,

    // ACE: Landblock.IsDormant
    /// Dormant landblocks suppress Monster AI ticking and physics processing.
    pub is_dormant: bool,

    /// `worldObjects`; the value is the object's `WeenieClassId`.
    world_objects: DotNetDict<ObjectGuid, u32>,
    /// `pendingAdditions`; the value is the object's `WeenieClassId`.
    pending_additions: DotNetDict<ObjectGuid, u32>,
    pending_removals: Vec<ObjectGuid>,

    // Cache used for Tick efficiency
    players: Vec<ObjectGuid>,
    sorted_creatures_by_next_tick: SortedByTime,
    sorted_world_objects_by_next_heartbeat: SortedByTime,
    sorted_generators_by_next_generator_update: SortedByTime,
    sorted_generators_by_next_regeneration: SortedByTime,

    // ACE: Landblock.CurrentLandblockGroup
    /// Used to detect and manage cross-landblock group (potentially cross-thread) operations.
    pub current_landblock_group: Option<LandblockGroupId>,

    // ACE: Landblock.Adjacents
    pub adjacents: Vec<LandblockId>,

    action_queue: ActionQueue,

    last_heart_beat: DotNetDateTime,
    last_database_save: DotNetDateTime,

    // ACE: Landblock.CellLandblock
    /// `None` where ACE's `ReadFromDat` returns an empty `CellLandblock` (no file).
    pub cell_landblock: Option<Arc<CellLandblock>>,
    // ACE: Landblock.LandblockInfo
    /// `None` where ACE's `ReadFromDat` returns an empty `LandblockInfo` (no file).
    pub landblock_info: Option<Arc<LandblockInfo>>,

    // ACE: Landblock.LandblockMesh
    /// The landblock static meshes for collision detection and physics simulation (only
    /// `LoadMeshes`, which ACE never calls, builds it).
    pub landblock_mesh: Option<LandblockMesh>,

    // ACE: Landblock.Monitor5m
    pub monitor_5m: RateMonitor,
    last_5m_clear: DotNetDateTime,
    // ACE: Landblock.Monitor1h
    pub monitor_1h: RateMonitor,
    last_1h_clear: DotNetDateTime,
    monitors_require_event_start: bool,

    fog_color: EnvironChangeType,

    is_dungeon: Option<bool>,
    has_dungeon: Option<bool>,

    // ACE: Landblock.Houses
    pub houses: Vec<ObjectGuid>,
}

impl Landblock {
    // ACE: Landblock.Landblock
    /// Reads the landblock's dat files; `lastActiveTime` is now. Builds the `PhysicsLandblock`
    /// (the physics world's landblock, through `phys_ext`).
    pub fn new(w: &mut World, id: LandblockId) -> Self {
        let cell_landblock = w
            .dats
            .cell_dat()
            .read_from_dat::<CellLandblock>(id.raw() | 0xFFFF);
        let landblock_info = w
            .dats
            .cell_dat()
            .read_from_dat::<LandblockInfo>(u32::from(id.landblock()) << 16 | 0xFFFE);

        // `PhysicsLandblock = new Physics.Common.Landblock(cellLandblock)`; its `PostInit` (land
        // cells, buildings, static objects) is part of the same load in the shared crate.
        phys_ext::load_landblock(w, id.landblock());

        Landblock {
            id,
            permaload: false,
            has_no_keep_alive_objects: true,
            create_world_objects_completed: false,
            last_active_time: w.now.utc,
            is_dormant: false,
            world_objects: DotNetDict::new(),
            pending_additions: DotNetDict::new(),
            pending_removals: Vec::new(),
            players: Vec::new(),
            sorted_creatures_by_next_tick: SortedByTime::new(),
            sorted_world_objects_by_next_heartbeat: SortedByTime::new(),
            sorted_generators_by_next_generator_update: SortedByTime::new(),
            sorted_generators_by_next_regeneration: SortedByTime::new(),
            current_landblock_group: None,
            adjacents: Vec::new(),
            action_queue: ActionQueue::new(),
            last_heart_beat: DotNetDateTime::MIN_VALUE,
            last_database_save: DotNetDateTime::MIN_VALUE,
            cell_landblock,
            landblock_info,
            landblock_mesh: None,
            monitor_5m: RateMonitor::new(),
            last_5m_clear: DotNetDateTime::MIN_VALUE,
            monitor_1h: RateMonitor::new(),
            last_1h_clear: DotNetDateTime::MIN_VALUE,
            monitors_require_event_start: true,
            fog_color: EnvironChangeType::Clear,
            is_dungeon: None,
            has_dungeon: None,
            houses: Vec::new(),
        }
    }

    pub fn create_world_objects_completed(&self) -> bool {
        self.create_world_objects_completed
    }

    pub fn last_active_time(&self) -> DotNetDateTime {
        self.last_active_time
    }

    /// `worldObjects.Keys`, in .NET order.
    pub fn world_object_guids(&self) -> impl Iterator<Item = &ObjectGuid> {
        self.world_objects.keys()
    }

    /// `pendingAdditions.Keys`, in .NET order.
    pub fn pending_addition_guids(&self) -> impl Iterator<Item = &ObjectGuid> {
        self.pending_additions.keys()
    }

    pub fn pending_removals(&self) -> &[ObjectGuid] {
        &self.pending_removals
    }

    /// The `players` tick cache.
    pub fn players(&self) -> &[ObjectGuid] {
        &self.players
    }

    pub fn sorted_creatures_by_next_tick(&self) -> &SortedByTime {
        &self.sorted_creatures_by_next_tick
    }

    pub fn sorted_world_objects_by_next_heartbeat(&self) -> &SortedByTime {
        &self.sorted_world_objects_by_next_heartbeat
    }

    pub fn sorted_generators_by_next_generator_update(&self) -> &SortedByTime {
        &self.sorted_generators_by_next_generator_update
    }

    pub fn sorted_generators_by_next_regeneration(&self) -> &SortedByTime {
        &self.sorted_generators_by_next_regeneration
    }

    /// The landblock's `actionQueue` (for `ActionQueue.RunActions` through its [`Actor`]).
    pub fn action_queue_mut(&mut self) -> &mut ActionQueue {
        &mut self.action_queue
    }

    pub fn action_queue(&self) -> &ActionQueue {
        &self.action_queue
    }

    // ACE: Landblock.ProcessPendingWorldObjectAdditionsAndRemovals
    fn process_pending_world_object_additions_and_removals(&mut self, objects: &ObjectStore) {
        if !self.pending_additions.is_empty() {
            let additions: Vec<(ObjectGuid, u32)> = self
                .pending_additions
                .iter()
                .map(|(&g, &wcid)| (g, wcid))
                .collect();
            for (guid, wcid) in additions {
                self.world_objects.insert(guid, wcid);

                if let Some(o) = objects.get(guid) {
                    if o.is_player() {
                        self.players.push(guid);
                    } else if o.is_creature() {
                        self.sorted_creatures_by_next_tick.add_last(guid);
                    }
                }

                self.insert_world_object_into_sorted_heartbeat_list(objects, guid);
                self.insert_world_object_into_sorted_generator_update_list(objects, guid);
                self.insert_world_object_into_sorted_generator_regeneration_list(objects, guid);

                if wcid == KEEP_ALIVE_WCID {
                    // Landblock KeepAlive weenie (ACE custom)
                    self.has_no_keep_alive_objects = false;
                }
            }

            self.pending_additions.clear();
        }

        if !self.pending_removals.is_empty() {
            for object_guid in std::mem::take(&mut self.pending_removals) {
                if let Some(wcid) = self.world_objects.remove(&object_guid) {
                    // `wo is Player` / `wo is Creature`: a guid is only ever in the list for its class.
                    if let Some(i) = self.players.iter().position(|&p| p == object_guid) {
                        self.players.remove(i);
                    } else {
                        self.sorted_creatures_by_next_tick.remove(object_guid);
                    }

                    self.sorted_world_objects_by_next_heartbeat
                        .remove(object_guid);
                    self.sorted_generators_by_next_generator_update
                        .remove(object_guid);
                    self.sorted_generators_by_next_regeneration
                        .remove(object_guid);

                    if wcid == KEEP_ALIVE_WCID {
                        // Landblock KeepAlive weenie (ACE custom)
                        let keep_alive_object =
                            self.world_objects.values().any(|&w| w == KEEP_ALIVE_WCID);

                        if !keep_alive_object {
                            self.has_no_keep_alive_objects = true;
                        }
                    }
                }
            }
        }
    }

    // ACE: Landblock.InsertWorldObjectIntoSortedHeartbeatList
    fn insert_world_object_into_sorted_heartbeat_list(
        &mut self,
        objects: &ObjectStore,
        world_object: ObjectGuid,
    ) {
        self.sorted_world_objects_by_next_heartbeat
            .insert_sorted(world_object, |g| {
                objects.get(g).map_or(f64::MAX, next_heartbeat_time)
            });
    }

    // ACE: Landblock.InsertWorldObjectIntoSortedGeneratorUpdateList
    fn insert_world_object_into_sorted_generator_update_list(
        &mut self,
        objects: &ObjectStore,
        world_object: ObjectGuid,
    ) {
        self.sorted_generators_by_next_generator_update
            .insert_sorted(world_object, |g| {
                objects.get(g).map_or(f64::MAX, next_generator_update_time)
            });
    }

    // ACE: Landblock.InsertWorldObjectIntoSortedGeneratorRegenerationList
    fn insert_world_object_into_sorted_generator_regeneration_list(
        &mut self,
        objects: &ObjectStore,
        world_object: ObjectGuid,
    ) {
        self.sorted_generators_by_next_regeneration
            .insert_sorted(world_object, |g| {
                objects
                    .get(g)
                    .map_or(f64::MAX, next_generator_regeneration_time)
            });
    }

    // ACE: Landblock.ResortWorldObjectIntoSortedGeneratorRegenerationList
    pub fn resort_world_object_into_sorted_generator_regeneration_list(
        &mut self,
        objects: &ObjectStore,
        world_object: ObjectGuid,
    ) {
        if self
            .sorted_generators_by_next_regeneration
            .contains(world_object)
        {
            self.sorted_generators_by_next_regeneration
                .remove(world_object);
            self.insert_world_object_into_sorted_generator_regeneration_list(objects, world_object);
        }
    }

    // ACE: Landblock.EnqueueAction
    pub fn enqueue_action(&mut self, action: Action) {
        self.action_queue.enqueue_action(action);
    }

    // ACE: Landblock.GetAllWorldObjectsForDiagnostics
    /// We do not process pending changes here.
    pub fn get_all_world_objects_for_diagnostics(&self) -> Vec<ObjectGuid> {
        self.world_objects.keys().copied().collect()
    }

    // ACE: Landblock.IsDungeon
    /// True if this landblock is a dungeon, with no traversable overworld. A missing dat file
    /// reads as ACE's empty object (no heights, no cells).
    pub fn is_dungeon(&mut self) -> bool {
        // return cached value
        if let Some(v) = self.is_dungeon {
            return v;
        }

        // hack for NW island
        // did a worldwide analysis for adding watercells into the formula,
        // but they are inconsistently defined for some of the edges of map unfortunately
        if self.id.landblock_x() < 0x08 && self.id.landblock_y() > 0xF8 {
            self.is_dungeon = Some(false);
            return false;
        }

        // a dungeon landblock is determined by:
        // - all heights being 0
        // - having at least 1 EnvCell (0x100+)
        // - contains no buildings
        if let Some(cell_landblock) = &self.cell_landblock {
            if cell_landblock.height.iter().any(|&height| height != 0) {
                self.is_dungeon = Some(false);
                return false;
            }
        }
        let v = self.info_has_cells_and_no_buildings();
        self.is_dungeon = Some(v);
        v
    }

    /// `LandblockInfo != null && LandblockInfo.NumCells > 0 && LandblockInfo.Buildings != null &&
    /// LandblockInfo.Buildings.Count == 0`.
    fn info_has_cells_and_no_buildings(&self) -> bool {
        self.landblock_info
            .as_ref()
            .is_some_and(|info| info.num_cells > 0 && info.buildings.is_empty())
    }

    // ACE: Landblock.HasDungeon
    /// True if this landblock contains a dungeon. If a landblock contains both a dungeon and
    /// traversable overworld, this is true whereas `IsDungeon` is false. Only for very specific
    /// scenarios, such as determining if a landblock contains a mansion basement.
    pub fn has_dungeon(&mut self) -> bool {
        // return cached value
        if let Some(v) = self.has_dungeon {
            return v;
        }

        let v = self.info_has_cells_and_no_buildings();
        self.has_dungeon = Some(v);
        v
    }
}

/// The landblock `id` names, which must be loaded.
fn lb(w: &World, id: LandblockId) -> &Landblock {
    w.landblock_manager.landblocks.expect(id)
}

/// The landblock `id` names, mutably.
fn lb_mut(w: &mut World, id: LandblockId) -> &mut Landblock {
    w.landblock_manager.landblocks.expect_mut(id)
}

/// `ProcessPendingWorldObjectAdditionsAndRemovals()` on a landblock of the world.
fn process_pending(w: &mut World, id: LandblockId) {
    let World {
        landblock_manager,
        objects,
        ..
    } = w;
    landblock_manager
        .landblocks
        .expect_mut(id)
        .process_pending_world_object_additions_and_removals(objects);
}

// ACE: Landblock.FogColor
pub fn fog_color(w: &World, id: LandblockId) -> EnvironChangeType {
    if let Some(global) = w.landblock_manager.global_fog_color {
        return global;
    }

    lb(w, id).fog_color
}

// ACE: Landblock.FogColor
pub fn set_fog_color_value(w: &mut World, id: LandblockId, value: EnvironChangeType) {
    lb_mut(w, id).fog_color = value;
}

// ACE: Landblock.Init
/// `Task.Run` runs inline (see the module docs).
///
/// `PhysicsLandblock.PostInit()` (when not `reload`) is done by [`Landblock::new`]'s
/// `phys_ext::load_landblock`, which builds the cells, buildings and static objects in one step.
///
/// When `CreateWorldObjects` throws (an instance of an Undef weenie), the task ends there,
/// as ACE's unobserved task exception does: no shard objects and no encounters are spawned, and
/// `CreateWorldObjectsCompleted` stays false.
pub fn init(w: &mut World, id: LandblockId, reload: bool) {
    let _ = reload;

    if !create_world_objects(w, id) {
        return;
    }

    spawn_dynamic_shard_objects(w, id);

    spawn_encounters(w, id);

    //LoadMeshes(objects);
}

// ACE: Landblock.CreateWorldObjects
/// Monster Locations, Generators.
/// `false` where ACE's `CreateNewWorldObjects` throws (the load task ends; see [`init`]).
fn create_world_objects(w: &mut World, id: LandblockId) -> bool {
    let objects = w.content.get_cached_instances_by_landblock(id.landblock());
    let shard_objects = shard_get_static_objects_by_landblock(w, id.landblock());
    let factory_objects = match CtorEnv::with_world(w, |env| {
        world_object_factory::create_new_world_objects(env, &objects, &shard_objects, None)
    }) {
        Ok(factory_objects) => factory_objects,
        Err(aborted) => {
            // DIVERGE: ACE's task exception goes unobserved (nothing is logged); this says why the landblock is empty.
            log::error!(
                "Landblock 0x{id}: CreateNewWorldObjects threw a NullReferenceException for instance 0x{:08X} (weenie {}, WeenieType Undef); the landblock's load ends here, as in ACE",
                aborted.instance_guid,
                aborted.weenie_class_id
            );
            return false;
        }
    };

    lb_mut(w, id).enqueue_action(Action::delegate(move |w: &mut World| {
        // Not ACE: offline house copies (`House.Load`) leave the store first.
        crate::world_objects::house::evict_offline_copies(w, id.landblock());

        // for mansion linking
        let mut houses: Vec<ObjectGuid> = Vec::new();

        // the factory's objects join the store first, so each creature runs the object-creating
        // half of its constructor (`Creature::post_insert`) in the factory's order, before any
        // of them is added to the landblock (ACE built them all before this delegate)
        let mut built = Vec::with_capacity(factory_objects.len());
        for fo in factory_objects {
            let fo = Box::new(fo);
            let guid = fo.guid;
            let weenie_type = fo.biota.weenie_type;
            let house_type = fo.house_type();
            insert_into_store(w, fo);
            built.push((guid, weenie_type, house_type));
        }

        for (guid, weenie_type, house_type) in built {
            let mut parent: Option<ObjectGuid> = None;
            if weenie_type == WeenieType::House {
                let house = guid;
                lb_mut(w, id).houses.push(house);

                if house_type == HouseType::Mansion {
                    houses.push(house);
                    house_linked_houses_add(w, house, houses[0]);

                    if houses.len() > 1 {
                        house_linked_houses_add(w, houses[0], house);
                        parent = Some(houses[0]);
                    }
                }
            }

            add_world_object(w, id, guid);
            world_object_activate_links(w, guid, &objects, &shard_objects, parent);

            if let Some(h) = phys_ext::physics_obj(w, guid) {
                // `fo.PhysicsObj.Order = 0`
                if let Some(e) = phys_ext::ext_mut(w, h) {
                    e.order = 0;
                }
            }
        }

        lb_mut(w, id).create_world_objects_completed = true;

        // `PhysicsLandblock.SortObjects()`
        phys_ext::sort_objects(w, id.landblock());
    }));

    true
}

/// Hands a factory-built object to the store (the owner of every live object).
fn insert_into_store(w: &mut World, wo: Box<WorldObject>) {
    let guid = wo.guid;
    if let Err(dup) = w.objects.insert(wo) {
        log::error!(
            "Landblock: object 0x{} is already in the world; the new copy is dropped",
            dup.guid
        );
        return;
    }
    crate::world_objects::creature::post_insert(w, guid);
}

// ACE: Landblock.SpawnDynamicShardObjects
/// Corpses.
fn spawn_dynamic_shard_objects(w: &mut World, id: LandblockId) {
    let dynamics = shard_get_dynamic_objects_by_landblock(w, id.landblock());
    let factory_shard_objects = world_object_factory_create_world_objects(w, dynamics);

    lb_mut(w, id).enqueue_action(Action::delegate(move |w: &mut World| {
        for fso in factory_shard_objects {
            let guid = fso.guid;
            insert_into_store(w, fso);
            add_world_object(w, id, guid);
        }
    }));
}

// ACE: Landblock.SpawnEncounters
/// Spawns the semi-randomized monsters scattered around the outdoors.
fn spawn_encounters(w: &mut World, id: LandblockId) {
    // get the encounter spawns for this landblock
    let encounters = w.content.get_cached_encounters_by_landblock(id.landblock());

    for encounter in encounters.iter().cloned() {
        let Some(wo) = world_object_factory_create_new_world_object(w, encounter.weenie_class_id)
        else {
            continue;
        };

        lb_mut(w, id).enqueue_action(Action::delegate(move |w: &mut World| {
            let guid = wo.guid;
            insert_into_store(w, wo);

            let x_pos = (encounter.cell_x as f32 * 24.0).clamp(0.5, 191.5);
            let y_pos = (encounter.cell_y as f32 * 24.0).clamp(0.5, 191.5);

            let obj_cell_id = u32::from(id.landblock()) << 16 | 1;
            let mut origin = Vector3::new(x_pos, y_pos, 0.0);
            let obj_cell_id = physics_position_adjust_to_outside(obj_cell_id, &mut origin);

            origin.z = physics_landblock_get_z(w, id, origin);

            let location = Position::from_vectors(obj_cell_id, origin, Quaternion::IDENTITY);
            if let Some(o) = w.objects.get_mut(guid) {
                o.set_location(Some(location));
            }

            if lscape_get_landcell_has_building(w, id, obj_cell_id) {
                world_object_destroy(w, guid);
                return;
            }

            if property_manager_get_bool_override_encounter_spawn_rates(w) {
                let regen_interval = property_manager_get_double(w, "encounter_regen_interval");
                if let Some(o) = w.objects.get_mut(guid) {
                    o.set_regeneration_interval(regen_interval);
                }

                world_object_reinitialize_heartbeats(w, guid);

                let encounter_delay = property_manager_get_double(w, "encounter_delay") as f32;
                if let Some(o) = w.objects.get_mut(guid) {
                    // While this may be ugly, it's done for performance reasons: common weenie
                    // collections are shared with the weenie, so clone before changing them.
                    if let Some(generators) = o.biota.properties_generator_mut() {
                        for profile in generators.iter_mut() {
                            profile.delay = Some(encounter_delay);
                        }
                    }
                }
            }

            if !add_world_object(w, id, guid) {
                world_object_destroy(w, guid);
            }
        }));
    }
}

// ACE: Landblock.LoadMeshes
/// Loads the meshes for the landblock. This isn't used by ACE.
pub fn load_meshes(w: &mut World, id: LandblockId, objects: &[LandblockInstance]) {
    let mesh = LandblockMesh::new(&w.dats, id);
    lb_mut(w, id).landblock_mesh = Some(mesh);
    load_land_objects();
    load_buildings();
    load_weenies(objects);
    load_scenery();
}

/// `LoadLandObjects`: the static objects' `ModelMesh`es. Not ported: `LoadMeshes` is never
/// called (its call is commented out in ACE too); collision uses the shared physics' geometry.
// ACE: Landblock.LoadLandObjects
fn load_land_objects() {}

/// `LoadBuildings`: the buildings' `ModelMesh`es. Not ported, as [`load_land_objects`].
// ACE: Landblock.LoadBuildings
fn load_buildings() {}

/// `LoadWeenies`: the instances' `ModelMesh`es. Not ported, as [`load_land_objects`].
// ACE: Landblock.LoadWeenies
fn load_weenies(_objects: &[LandblockInstance]) {}

/// `LoadScenery`: `Scenery.Load(this)`. Not ported, as [`load_land_objects`].
// ACE: Landblock.LoadScenery
fn load_scenery() {}

/// Restarts both rate monitors (at the start of a tick's first stage).
fn restart_monitors(w: &mut World, id: LandblockId) {
    // the monitor's clock, as the stopwatches (the tick's frozen clock timed every event as 0)
    let clock = perf::monitor_clock(w);
    let l = lb_mut(w, id);
    l.monitor_5m.restart(&*clock);
    l.monitor_1h.restart(&*clock);
}

fn pause_monitors(w: &mut World, id: LandblockId) {
    // the monitor's clock, as the stopwatches (the tick's frozen clock timed every event as 0)
    let clock = perf::monitor_clock(w);
    let l = lb_mut(w, id);
    l.monitor_5m.pause(&*clock);
    l.monitor_1h.pause(&*clock);
}

/// `if (monitorsRequireEventStart) Restart else Resume`.
fn start_or_resume_monitors(w: &mut World, id: LandblockId) {
    // the monitor's clock, as the stopwatches (the tick's frozen clock timed every event as 0)
    let clock = perf::monitor_clock(w);
    let l = lb_mut(w, id);
    if l.monitors_require_event_start {
        l.monitor_5m.restart(&*clock);
        l.monitor_1h.restart(&*clock);
    } else {
        l.monitor_5m.resume(&*clock);
        l.monitor_1h.resume(&*clock);
    }
}

/// `ServerPerformanceMonitor.AddToCumulativeEvent(type, stopwatch.Elapsed.TotalSeconds)`: the
/// stopwatch (`stopwatch.Restart()` is [`perf::stopwatch_start`]) runs on the monitor's clock, so
/// it reads zero unless a clock is installed (the server installs the machine's).
fn add_cumulative(w: &mut World, t: CumulativeEventHistoryType, stopwatch: std::time::Duration) {
    let seconds = perf::stopwatch_elapsed_seconds(w, stopwatch);
    perf::add_to_cumulative_event(w, t, seconds);
}

// ACE: Landblock.TickPhysics
/// Called before `TickMultiThreadedWork` and `TickSingleThreadedWork`. Objects that changed
/// landblock are appended to `moved_objects`.
pub fn tick_physics(
    w: &mut World,
    id: LandblockId,
    _portal_year_ticks: f64,
    moved_objects: &mut Vec<ObjectGuid>,
) {
    if lb(w, id).is_dormant {
        return;
    }

    restart_monitors(w, id);
    lb_mut(w, id).monitors_require_event_start = false;

    process_pending(w, id);

    let world_objects: Vec<ObjectGuid> = lb(w, id).world_objects.keys().copied().collect();
    for wo in world_objects {
        if !w.objects.contains(wo) {
            continue;
        }

        // set to TRUE if object changes landblock
        let landblock_update = crate::dispatch::update_object_physics::update_object_physics(w, wo);

        if landblock_update {
            moved_objects.push(wo);
        }
    }

    pause_monitors(w, id);
}

/// One pass of a time-ordered tick loop: while the first object is due, take it off, run `f` on
/// it and, if `requeue`, put it back through `requeue`. Objects gone from the store are dropped.
fn run_due(
    w: &mut World,
    id: LandblockId,
    current_unix_time: f64,
    list: fn(&mut Landblock) -> &mut SortedByTime,
    key: fn(&WorldObject) -> f64,
    mut f: impl FnMut(&mut World, ObjectGuid),
    requeue: fn(&mut Landblock, &ObjectStore, ObjectGuid),
) {
    while let Some(first) = list(lb_mut(w, id)).first() {
        let Some(o) = w.objects.get(first) else {
            list(lb_mut(w, id)).remove_first();
            continue;
        };

        // If they wanted to run before or at now
        if key(o) <= current_unix_time {
            list(lb_mut(w, id)).remove_first();
            f(w, first);
            if w.landblock_manager.landblocks.get(id).is_none() {
                return;
            }
            let World {
                landblock_manager,
                objects,
                ..
            } = w;
            requeue(landblock_manager.landblocks.expect_mut(id), objects, first);
        } else {
            break;
        }
    }
}

// ACE: Landblock.TickMultiThreadedWork
/// Ticks what ACE may run on a landblock group's thread. Called after `TickPhysics`, before
/// `TickSingleThreadedWork`.
pub fn tick_multi_threaded_work(w: &mut World, id: LandblockId, current_unix_time: f64) {
    start_or_resume_monitors(w, id);

    // This will consist of the following work:
    // - this.CreateWorldObjects
    // - this.SpawnDynamicShardObjects
    // - this.SpawnEncounters
    // - Adding items back onto the landblock from failed player movements: Player_Inventory.cs DoHandleActionPutItemInContainer()
    // - Executing trade between two players: Player_Trade.cs FinalizeTrade()
    let stopwatch = perf::stopwatch_start(w);
    action_queue::run_actions(w, Actor::Landblock(id));
    add_cumulative(
        w,
        CumulativeEventHistoryType::LandblockTickRunActions,
        stopwatch,
    );

    process_pending(w, id);

    // When a WorldObject Ticks, it can end up adding additional WorldObjects to this landblock
    if !lb(w, id).is_dormant {
        let stopwatch = perf::stopwatch_start(w);
        run_due(
            w,
            id,
            current_unix_time,
            |l| &mut l.sorted_creatures_by_next_tick,
            next_monster_tick_time,
            |w, first| creature_monster_tick(w, first, current_unix_time),
            // All creatures tick at a fixed interval
            |l, _, first| l.sorted_creatures_by_next_tick.add_last(first),
        );
        add_cumulative(
            w,
            CumulativeEventHistoryType::LandblockTickMonsterTick,
            stopwatch,
        );
    }

    let stopwatch = perf::stopwatch_start(w);
    run_due(
        w,
        id,
        current_unix_time,
        |l| &mut l.sorted_generators_by_next_generator_update,
        next_generator_update_time,
        |w, first| world_object_generator_update(w, first, current_unix_time),
        //InsertWorldObjectIntoSortedGeneratorUpdateList(first);
        |l, _, first| l.sorted_generators_by_next_generator_update.add_last(first),
    );
    add_cumulative(
        w,
        CumulativeEventHistoryType::LandblockTickGeneratorUpdate,
        stopwatch,
    );

    let stopwatch = perf::stopwatch_start(w);
    run_due(
        w,
        id,
        current_unix_time,
        |l| &mut l.sorted_generators_by_next_regeneration,
        next_generator_regeneration_time,
        |w, first| world_object_generator_regeneration(w, first, current_unix_time),
        // Generators can have regnerations at different intervals
        |l, objects, first| {
            l.insert_world_object_into_sorted_generator_regeneration_list(objects, first)
        },
    );
    add_cumulative(
        w,
        CumulativeEventHistoryType::LandblockTickGeneratorRegeneration,
        stopwatch,
    );

    // Heartbeat
    let stopwatch = perf::stopwatch_start(w);
    let utc_now = w.now.utc;
    if lb(w, id).last_heart_beat + heartbeat_interval() <= utc_now {
        let this_heart_beat = utc_now;

        process_pending(w, id);

        // Decay world objects
        let last_heart_beat = lb(w, id).last_heart_beat;
        if last_heart_beat != DotNetDateTime::MIN_VALUE {
            let world_objects: Vec<ObjectGuid> = lb(w, id).world_objects.keys().copied().collect();
            for wo in world_objects {
                if w.objects.contains(wo) && world_object_is_decayable(w, wo) {
                    world_object_decay(w, wo, this_heart_beat - last_heart_beat);
                }
            }
        }

        let l = lb(w, id);
        if !l.permaload && l.has_no_keep_alive_objects {
            if l.last_active_time + dormant_interval() < this_heart_beat {
                if !l.is_dormant {
                    let spell_projectiles: Vec<ObjectGuid> = l
                        .world_objects
                        .keys()
                        .copied()
                        .filter(|&g| {
                            w.objects
                                .get(g)
                                .is_some_and(WorldObject::is_spell_projectile)
                        })
                        .collect();
                    for spell_projectile in spell_projectiles {
                        physics_obj_set_active(w, spell_projectile, false);
                        world_object_destroy(w, spell_projectile);
                    }
                }

                lb_mut(w, id).is_dormant = true;
            }
            if lb(w, id).last_active_time + unload_interval() < this_heart_beat {
                landblock_manager::add_to_destruction_queue(w, id);
            }
        }

        lb_mut(w, id).last_heart_beat = this_heart_beat;
    }
    add_cumulative(
        w,
        CumulativeEventHistoryType::LandblockTickHeartbeat,
        stopwatch,
    );

    // Database Save
    let stopwatch = perf::stopwatch_start(w);
    if lb(w, id).last_database_save + database_save_interval() <= w.now.utc {
        process_pending(w, id);

        save_db(w, id);
        lb_mut(w, id).last_database_save = w.now.utc;
    }
    add_cumulative(
        w,
        CumulativeEventHistoryType::LandblockTickDatabaseSave,
        stopwatch,
    );

    pause_monitors(w, id);
}

// ACE: Landblock.TickSingleThreadedWork
/// Ticks what must run on the world thread. Called after `TickPhysics` and
/// `TickMultiThreadedWork`.
pub fn tick_single_threaded_work(w: &mut World, id: LandblockId, current_unix_time: f64) {
    start_or_resume_monitors(w, id);

    process_pending(w, id);

    let stopwatch = perf::stopwatch_start(w);
    let players = lb(w, id).players.clone();
    for player in players {
        if w.objects.contains(player) {
            player_player_tick(w, player, current_unix_time);
        }
    }
    add_cumulative(
        w,
        CumulativeEventHistoryType::LandblockTickPlayerTick,
        stopwatch,
    );

    let stopwatch = perf::stopwatch_start(w);
    run_due(
        w,
        id,
        current_unix_time,
        |l| &mut l.sorted_world_objects_by_next_heartbeat,
        next_heartbeat_time,
        |w, first| crate::dispatch::heartbeat::heartbeat(w, first, current_unix_time),
        // WorldObjects can have heartbeats at different intervals
        |l, objects, first| l.insert_world_object_into_sorted_heartbeat_list(objects, first),
    );
    add_cumulative(
        w,
        CumulativeEventHistoryType::LandblockTickWorldObjectHeartbeat,
        stopwatch,
    );

    // the monitor's clock, as the stopwatches (the tick's frozen clock timed every event as 0)
    let clock = perf::monitor_clock(w);
    let utc_now = w.now.utc;
    let l = lb_mut(w, id);
    l.monitor_5m.register_event_end(&*clock);
    l.monitor_1h.register_event_end(&*clock);
    l.monitors_require_event_start = true;

    if utc_now - l.last_5m_clear >= TimeSpan::from_minutes(5.0) {
        l.monitor_5m.clear_event_history();
        l.last_5m_clear = utc_now;
    }

    if utc_now - l.last_1h_clear >= TimeSpan::from_hours(1.0) {
        l.monitor_1h.clear_event_history();
        l.last_1h_clear = utc_now;
    }
}

// ACE: Landblock.AddWorldObject
/// Adds an object that is in the store. This will fail if the object doesn't have a valid
/// location.
pub fn add_world_object(w: &mut World, id: LandblockId, wo: ObjectGuid) -> bool {
    let Some(o) = w.objects.get(wo) else {
        return false;
    };

    if o.location().is_none() {
        log::debug!(
            "Landblock 0x{} failed to add 0x{:08X} {}. Invalid Location",
            lb(w, id).id,
            o.biota.id,
            o.get_property(PropertyString::Name).unwrap_or_default()
        );
        return false;
    }

    add_world_object_internal(w, id, wo)
}

// ACE: Landblock.AddWorldObjectForPhysics
pub fn add_world_object_for_physics(w: &mut World, id: LandblockId, wo: ObjectGuid) {
    add_world_object_internal(w, id, wo);
}

// ACE: Landblock.AddWorldObjectInternal
fn add_world_object_internal(w: &mut World, id: LandblockId, wo: ObjectGuid) -> bool {
    let lm = &w.landblock_manager;
    if lm.currently_ticking_landblock_groups_multi_threaded {
        let current = lm.landblocks.expect(id).current_landblock_group;
        if current.is_some() && current != lm.current_multi_threaded_ticking_landblock_group {
            let weenie_type = w.objects.get(wo).map(|o| o.biota.weenie_type);
            // Prevent possible multi-threaded crash: cloak projectiles are simply not added.
            if weenie_type == Some(WeenieType::ProjectileSpell) {
                log::warn!(
                    "Landblock 0x{id} entered AddWorldObjectInternal in a cross-thread operation for a ProjectileSpell. This is normally not an issue unless it's happening more than once an hour."
                );
                return false;
            }

            log::error!(
                "Landblock 0x{id} entered AddWorldObjectInternal in a cross-thread operation."
            );
            log::error!("Landblock 0x{id} CurrentLandblockGroup: {current:?}");
            log::error!(
                "LandblockManager.CurrentMultiThreadedTickingLandblockGroup.Value: {:?}",
                lm.current_multi_threaded_ticking_landblock_group
            );
            log::error!("wo: 0x{wo}");
            // DIVERGE: points at Empyrean's issue tracker where ACE's names the ACE team (brand).
            log::error!(
                "PLEASE REPORT THIS AT {} !!!",
                empyrean_common::brand::ISSUES_URL
            );

            // This may still crash...
        }
    }

    let Some(o) = w.objects.get_mut(wo) else {
        return false;
    };
    o.current_landblock = Some(id);

    // `if (wo.PhysicsObj == null) wo.InitPhysicsObj(); else wo.PhysicsObj.set_object_guid(wo.Guid);`
    // (re-add to ServerObjectManager), then `if (wo.PhysicsObj.CurCell == null)`
    // `success = wo.AddPhysicsObj()`: all in `phys_ext::add_world_object_physics`, which answers
    // true for a body already in a cell.
    {
        let success = phys_ext::add_world_object_physics(w, wo);
        if !success {
            let Some(o) = w.objects.get_mut(wo) else {
                return false;
            };
            o.current_landblock = None;

            let has_generator = o.wo.world_object_generators.generator.is_some();
            let is_generator = o.is_generator();
            let no_projectile_target = o.projectile.is_none_or(|p| p.target.is_none());
            let is_spell_projectile = o.is_spell_projectile();
            let name = o.get_property(PropertyString::Name).unwrap_or_default();

            if has_generator {
                log::debug!("AddWorldObjectInternal: couldn't spawn 0x{wo}:{name} from generator");
                world_object_generators::notify_of_event(w, wo, RegenerationType::PickUp);
            // Notify generator the generated object is effectively destroyed, use Pickup to catch both cases.
            } else if is_generator {
                // Some generators will fail random spawns if they're circumference spans over water or cliff edges
                log::debug!("AddWorldObjectInternal: couldn't spawn generator 0x{wo}:{name}");
            } else if no_projectile_target && !is_spell_projectile {
                log::warn!("AddWorldObjectInternal: couldn't spawn 0x{wo}:{name}");
            }

            return false;
        }
    }

    let wcid = w.objects.get(wo).map_or(0, |o| o.biota.weenie_class_id);
    let l = lb_mut(w, id);
    if !l.world_objects.contains_key(&wo) {
        l.pending_additions.insert(wo, wcid);
    } else if let Some(i) = l.pending_removals.iter().position(|&g| g == wo) {
        l.pending_removals.remove(i);
    }

    // broadcast to nearby players
    world_object_notify_players(w, wo);

    let Some(o) = w.objects.get(wo) else {
        return true;
    };

    if o.is_player() {
        let fog = fog_color(w, id);
        player_set_fog_color(w, wo, fog);
    }

    let Some(o) = w.objects.get(wo) else {
        return true;
    };
    if o.is_corpse() && o.level().is_some() {
        let corpse_limit = property_manager_get_long(w, "corpse_spam_limit");
        let victim_id = o.victim_id();
        let name = o.get_property(PropertyString::Name).unwrap_or_default();

        // worldObjects.Values.Union(pendingAdditions.Values): distinct, world objects first.
        let l = lb(w, id);
        let mut union: Vec<ObjectGuid> = l.world_objects.keys().copied().collect();
        for &g in l.pending_additions.keys() {
            if !union.contains(&g) {
                union.push(g);
            }
        }
        let mut corpse_list: Vec<(ObjectGuid, Option<i32>, Option<f64>)> = union
            .into_iter()
            .filter_map(|g| w.objects.get(g).map(|c| (g, c)))
            .filter(|(_, c)| c.is_corpse() && c.level().is_some() && c.victim_id() == victim_id)
            .map(|(g, c)| (g, c.creation_timestamp(), c.time_to_rot()))
            .collect();
        // OrderBy(w => w.CreationTimestamp): stable, null first.
        corpse_list.sort_by_key(|&(_, ts, _)| ts);

        if i64::try_from(corpse_list.len()).unwrap_or(i64::MAX) > corpse_limit {
            // Not ACE's (a fix, V340): when no corpse has more than
            // Corpse.EmptyDecayTime left to rot, none is shortened and the add completes. ACE's
            // search threw when it found none, failing the add, so the death never finished.
            let first = corpse_list
                .iter()
                .find(|&&(_, _, rot)| rot.is_some_and(|r| r > CORPSE_EMPTY_DECAY_TIME))
                .map(|&(g, _, _)| g);
            let corpse = first.and_then(|first| get_object(w, id, first, true));

            if let Some(corpse) = corpse {
                log::warn!(
                    "[CORPSE] Landblock.AddWorldObjectInternal(): {name} (0x{wo}) exceeds the per player limit of {corpse_limit} corpses for 0x{:04X}. Adjusting TimeToRot for oldest corpse (0x{corpse}) to Corpse.EmptyDecayTime({CORPSE_EMPTY_DECAY_TIME}).",
                    id.landblock()
                );
                if let Some(c) = w.objects.get_mut(corpse) {
                    c.set_time_to_rot(Some(CORPSE_EMPTY_DECAY_TIME));
                }
            }
        }
    }

    true
}

// ACE: Landblock.RemoveWorldObject
pub fn remove_world_object(
    w: &mut World,
    id: LandblockId,
    object_id: ObjectGuid,
    adjacency_move: bool,
    from_pickup: bool,
    show_error: bool,
) {
    remove_world_object_internal(w, id, object_id, adjacency_move, from_pickup, show_error);
}

// ACE: Landblock.RemoveWorldObjectForPhysics
/// Should only be called by physics/relocation engines, not from player. `adjacency_move`: the
/// object is moving to an adjacent landblock.
pub fn remove_world_object_for_physics(
    w: &mut World,
    id: LandblockId,
    object_id: ObjectGuid,
    adjacency_move: bool,
) {
    remove_world_object_internal(w, id, object_id, adjacency_move, false, true);
}

// ACE: Landblock.RemoveWorldObjectInternal
fn remove_world_object_internal(
    w: &mut World,
    id: LandblockId,
    object_id: ObjectGuid,
    adjacency_move: bool,
    from_pickup: bool,
    show_error: bool,
) {
    let lm = &w.landblock_manager;
    if lm.currently_ticking_landblock_groups_multi_threaded {
        let current = lm.landblocks.expect(id).current_landblock_group;
        if current.is_some() && current != lm.current_multi_threaded_ticking_landblock_group {
            log::error!(
                "Landblock 0x{id} entered RemoveWorldObjectInternal in a cross-thread operation."
            );
            log::error!("Landblock 0x{id} CurrentLandblockGroup: {current:?}");
            log::error!(
                "LandblockManager.CurrentMultiThreadedTickingLandblockGroup.Value: {:?}",
                lm.current_multi_threaded_ticking_landblock_group
            );
            log::error!("objectId: 0x{object_id}");
            // DIVERGE: points at Empyrean's issue tracker where ACE's names the ACE team (brand).
            log::error!(
                "PLEASE REPORT THIS AT {} !!!",
                empyrean_common::brand::ISSUES_URL
            );

            // This may still crash...
        }
    }

    let l = lb_mut(w, id);
    if l.world_objects.contains_key(&object_id) {
        l.pending_removals.push(object_id);
    } else if l.pending_additions.remove(&object_id).is_none() {
        if show_error {
            log::warn!(
                "RemoveWorldObjectInternal: Couldn't find {:08X}",
                object_id.full()
            );
        }
        return;
    }

    let Some(wo) = w.objects.get_mut(object_id) else {
        return;
    };
    wo.current_landblock = None;

    // Weenies can come with a default of 0 (Instant Rot) or -1 (Never Rot). If they still have that value, we want to retain it.
    // We also want to make sure fromPickup is true so that we're not clearing out TimeToRot on server shutdown (unloads all landblocks and removed all objects).
    #[allow(clippy::float_cmp)]
    if from_pickup {
        if let Some(rot) = wo.time_to_rot() {
            if rot != 0.0 && rot != -1.0 {
                wo.set_time_to_rot(None);
            }
        }
    }

    if !adjacency_move {
        // really remove it - send message to client to remove object
        world_object_enqueue_action_broadcast_remove_tracked_object(w, object_id, from_pickup);

        if let Some(h) = phys_ext::physics_obj(w, object_id) {
            // what the destroyed body still answers in ACE (its position with cell 0, as
            // `leave_world` leaves it; its height; its cached velocity)
            let destroyed = w.physics.get(h).map(|b| {
                let mut position = b.position;
                position.cell = dereth_primitives::CellId(0);
                crate::world_objects::world_object::DestroyedPhysicsObj {
                    position,
                    height: b.height(),
                    cached_velocity: b.cached_velocity,
                }
            });
            phys_ext::destroy_object(w, h);
            // DIVERGE: ACE keeps the destroyed PhysicsObj on the object and re-enters it on a
            // later add; the shared body is gone, so a later add makes a new one (InitPhysicsObj).
            // What ACE's references still read from the destroyed body is kept beside it.
            if let Some(o) = w.objects.get_mut(object_id) {
                o.phys = None;
                o.wo.world_object.destroyed_physics_obj = destroyed;
            }
        }
    }
}

// ACE: Landblock.EmitSignal
pub fn emit_signal(w: &mut World, id: LandblockId, emitter: ObjectGuid, message: &str) {
    if message.trim().is_empty() {
        return;
    }

    let listeners: Vec<ObjectGuid> = lb(w, id)
        .world_objects
        .keys()
        .copied()
        .filter(|&g| {
            w.objects
                .get(g)
                .is_some_and(WorldObject::hear_local_signals)
        })
        .collect();

    for wo in listeners {
        if emitter == wo {
            continue;
        }

        let radius = w
            .objects
            .get(wo)
            .map_or(0, WorldObject::hear_local_signals_radius);
        if world_object_is_within_use_radius_of(w, emitter, wo, Some(radius as f32)) {
            emote_manager_on_local_signal(w, wo, emitter, message);
        }
    }
}

// ACE: Landblock.WithinUseRadius
/// Check to see if we are close enough to interact (adds a fudge factor of 1.5). Returns
/// `(within, validTargetGuid)`.
pub fn within_use_radius(
    w: &mut World,
    id: LandblockId,
    player: ObjectGuid,
    target_guid: ObjectGuid,
    use_radius: Option<f32>,
) -> (bool, bool) {
    let target = get_object(w, id, target_guid, true);

    let valid_target_guid = target.is_some();

    if let Some(target) = target {
        return (
            world_object_is_within_use_radius_of(w, player, target, use_radius),
            valid_target_guid,
        );
    }

    (false, valid_target_guid)
}

// ACE: Landblock.GetWorldObjectsForPhysicsHandling
/// Returns landblock objects with physics initialized.
pub fn get_world_objects_for_physics_handling(w: &mut World, id: LandblockId) -> Vec<ObjectGuid> {
    // If a missile is destroyed when it runs it's UpdateObjectPhysics(), it will remove itself from the landblock, thus, modifying the worldObjects collection.
    process_pending(w, id);

    lb(w, id).world_objects.keys().copied().collect()
}

// ACE: Landblock.GetObject
/// The object if it is in this landblock (or, with `search_adjacents`, an adjacent one); `None`
/// otherwise. The `GetObject(uint)` overload is `ObjectGuid::new(id)` here.
pub fn get_object(
    w: &World,
    id: LandblockId,
    guid: ObjectGuid,
    search_adjacents: bool,
) -> Option<ObjectGuid> {
    let l = lb(w, id);

    if l.pending_removals.contains(&guid) {
        return None;
    }

    if l.world_objects.contains_key(&guid) || l.pending_additions.contains_key(&guid) {
        return Some(guid);
    }

    if search_adjacents {
        for &adjacent in &l.adjacents {
            if w.landblock_manager.landblocks.get(adjacent).is_some() {
                if let Some(wo) = get_object(w, adjacent, guid, false) {
                    return Some(wo);
                }
            }
        }
    }

    None
}

// ACE: Landblock.GetWieldedObject
/// Searches this landblock (and possibly adjacents) for an object wielded by a creature.
pub fn get_wielded_object(
    w: &mut World,
    id: LandblockId,
    guid: ObjectGuid,
    search_adjacents: bool,
) -> Option<ObjectGuid> {
    // search creature wielded items in current landblock
    let creatures: Vec<ObjectGuid> = lb(w, id)
        .world_objects
        .keys()
        .copied()
        .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_creature))
        .collect();
    for creature in creatures {
        if let Some((wielded_item, selectable)) = creature_get_equipped_item(w, creature, guid) {
            if selectable {
                return Some(wielded_item);
            }

            return None;
        }
    }

    // try searching adjacent landblocks if not found
    if search_adjacents {
        let adjacents = lb(w, id).adjacents.clone();
        for adjacent in adjacents {
            if w.landblock_manager.landblocks.get(adjacent).is_none() {
                continue;
            }

            if let Some(wielded_item) = get_wielded_object(w, adjacent, guid, false) {
                return Some(wielded_item);
            }
        }
    }
    None
}

// ACE: Landblock.SetActive
/// Sets a landblock to active state, with the current time as the LastActiveTime. Public calls
/// always pass `is_adjacent = false`.
pub fn set_active(w: &mut World, id: LandblockId, is_adjacent: bool) {
    let now = w.now.utc;
    let l = lb_mut(w, id);
    l.last_active_time = now;
    l.is_dormant = false;

    if is_adjacent || physics_landblock_is_dungeon(l) {
        return;
    }

    // for outdoor landblocks, recursively call 1 iteration to set adjacents to active
    let adjacents = l.adjacents.clone();
    for landblock in adjacents {
        if w.landblock_manager.landblocks.get(landblock).is_some() {
            set_active(w, landblock, true);
        }
    }
}

// ACE: Landblock.Unload
/// Handles the cleanup process for a landblock. Called by `LandblockManager`.
pub fn unload(w: &mut World, id: LandblockId) {
    let landblock_id = lb(w, id).id.raw() | 0xFFFF;

    process_pending(w, id);

    save_db(w, id);

    // remove all objects
    let world_objects: Vec<ObjectGuid> = lb(w, id).world_objects.keys().copied().collect();
    for wo in world_objects {
        if !world_object_biota_originated_from_or_has_been_saved_to_database(w, wo) {
            world_object_destroy_ex(w, wo, false, true);
        } else {
            remove_world_object_internal(w, id, wo, false, false, true);

            // Not ACE (its GC reclaims the unreferenced object): saved by SaveDB above and
            // now detached, the object leaves the store. A player is left to the PlayerManager.
            // ACE never unloads a landblock holding a player (players keep it awake), so a
            // player here is a porting bug.
            let is_player = w.objects.get(wo).is_some_and(WorldObject::is_player);
            debug_assert!(
                !is_player,
                "landblock {:04X} unloaded while holding player {wo}",
                id.landblock()
            );
            if w.objects.get(wo).is_some() && !is_player {
                w.objects.remove(wo);
            }
        }
    }

    process_pending(w, id);

    lb_mut(w, id).action_queue.clear();

    // remove physics landblock: `LScape.unload_landblock(landblockID)`; the physics world is
    // keyed by the landblock
    let _ = landblock_id;
    phys_ext::unload_landblock(w, id.landblock());

    physics_landblock_release_shadow_objs(w, id);
}

// ACE: Landblock.DestroyAllNonPlayerObjects
pub fn destroy_all_non_player_objects(w: &mut World, id: LandblockId) {
    process_pending(w, id);

    save_db(w, id);

    // remove all objects
    let world_objects: Vec<ObjectGuid> = lb(w, id)
        .world_objects
        .keys()
        .copied()
        .filter(|&g| !w.objects.get(g).is_some_and(WorldObject::is_player))
        .collect();
    for wo in world_objects {
        if !world_object_biota_originated_from_or_has_been_saved_to_database(w, wo) {
            world_object_destroy_ex(w, wo, false, false);
        } else {
            remove_world_object_internal(w, id, wo, false, false, true);
        }
    }

    process_pending(w, id);

    lb_mut(w, id).action_queue.clear();
}

// ACE: Landblock.SaveDB
fn save_db(w: &mut World, id: LandblockId) {
    let mut biotas: Vec<ObjectGuid> = Vec::new();

    let world_objects: Vec<ObjectGuid> = lb(w, id).world_objects.keys().copied().collect();
    for wo in world_objects {
        if !w.objects.contains(wo) {
            continue;
        }
        if world_object_is_static_that_should_persist_to_shard(w, wo)
            || world_object_is_dynamic_that_should_persist_to_shard(w, wo)
        {
            add_world_object_to_biotas_save_collection(w, wo, &mut biotas);
        }
    }

    shard_save_biotas_in_parallel(w, &biotas);
}

// ACE: Landblock.AddWorldObjectToBiotasSaveCollection
fn add_world_object_to_biotas_save_collection(
    w: &mut World,
    wo: ObjectGuid,
    biotas: &mut Vec<ObjectGuid>,
) {
    if world_object_changes_detected(w, wo) {
        crate::dispatch::save_biota_to_database::save_biota_to_database(w, wo, false);
        biotas.push(wo);
    }

    if w.objects.get(wo).is_some_and(WorldObject::is_container) {
        for item in container_inventory(w, wo) {
            add_world_object_to_biotas_save_collection(w, item, biotas);
        }
    }
}

// ACE: Landblock.EnqueueBroadcast
/// Broadcasts network messages to all of the players within a landblock, and possibly the
/// adjacent landblocks. Only used for very specific instances, such as broadcasting player
/// deaths to the destination lifestone block.
pub fn enqueue_broadcast(
    w: &mut World,
    id: LandblockId,
    exclude_list: Option<&[ObjectGuid]>,
    adjacents: bool,
    pos: Option<&Position>,
    max_range_sq: Option<f32>,
    msgs: &[crate::network::game_messages::game_message::GameMessage],
) {
    let mut players: Vec<ObjectGuid> = lb(w, id)
        .world_objects
        .keys()
        .copied()
        .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_player))
        .collect();

    // for landblock death broadcasts:
    // exclude players that have already been broadcast to within range of the death
    if let Some(exclude_list) = exclude_list {
        // `Except`: set difference, also dropping duplicates.
        let mut kept = Vec::new();
        for p in players {
            if !exclude_list.contains(&p) && !kept.contains(&p) {
                kept.push(p);
            }
        }
        players = kept;
    }

    // broadcast messages to player in this landblock
    for player in players {
        if let (Some(pos), Some(max_range_sq)) = (pos, max_range_sq) {
            let dist_sq = w
                .objects
                .get(player)
                .and_then(|p| p.location())
                .map_or(0.0, |l| l.squared_distance_to(pos));
            if dist_sq > max_range_sq {
                continue;
            }
        }
        player_session_network_enqueue_send(w, player, msgs);
    }

    // if applicable, iterate into adjacent landblocks
    if adjacents {
        let adjacent_ids = lb(w, id).adjacents.clone();
        for adjacent in adjacent_ids {
            if w.landblock_manager.landblocks.get(adjacent).is_some() {
                enqueue_broadcast(w, adjacent, exclude_list, false, pos, max_range_sq, msgs);
            }
        }
    }
}

// ACE: Landblock.SetFogColor
pub fn set_fog_color(w: &mut World, id: LandblockId, environ_change_type: EnvironChangeType) {
    if environ_change_type.is_fog() {
        set_fog_color_value(w, id, environ_change_type);

        let adjacents = lb(w, id).adjacents.clone();
        for adjacent in adjacents {
            if w.landblock_manager.landblocks.get(adjacent).is_some() {
                set_fog_color_value(w, adjacent, environ_change_type);
            }
        }

        let players = lb(w, id).players.clone();
        for player in players {
            let fog = fog_color(w, id);
            player_set_fog_color(w, player, fog);
        }
    }
}

// ACE: Landblock.SendEnvironSound
pub fn send_environ_sound(w: &mut World, id: LandblockId, environ_change_type: EnvironChangeType) {
    if environ_change_type.is_sound() {
        send_environ_change(w, id, environ_change_type);

        let adjacents = lb(w, id).adjacents.clone();
        for adjacent in adjacents {
            if w.landblock_manager.landblocks.get(adjacent).is_some() {
                send_environ_change(w, adjacent, environ_change_type);
            }
        }
    }
}

// ACE: Landblock.SendEnvironChange
pub fn send_environ_change(w: &mut World, id: LandblockId, environ_change_type: EnvironChangeType) {
    let players = lb(w, id).players.clone();
    for player in players {
        player_send_environ_change(w, player, environ_change_type);
    }
}

// ACE: Landblock.SendCurrentEnviron
pub fn send_current_environ(w: &mut World, id: LandblockId) {
    let players = lb(w, id).players.clone();
    for player in players {
        let fog = fog_color(w, id);
        if fog.is_fog() {
            player_set_fog_color(w, player, fog);
        } else {
            player_send_environ_change(w, player, fog);
        }
    }
}

// ACE: Landblock.DoEnvironChange
pub fn do_environ_change(w: &mut World, id: LandblockId, environ_change_type: EnvironChangeType) {
    if environ_change_type.is_fog() {
        set_fog_color(w, id, environ_change_type);
    } else {
        send_environ_sound(w, id, environ_change_type);
    }
}

// ---- pointers to members of other classes that are not ported yet ----
//
// Each is a `not_ported!` site that returns what ACE's member returns for "nothing" (false, null,
// 0, an empty list). The owning unit replaces the body with a call to its port.

// Sort keys: the WorldObject_Tick.cs fields, and Monster_Tick.cs's (not carried yet).
// `double.MaxValue` is ACE's "never", which keeps the object out of the lists.

/// `wo.NextHeartbeatTime` (set by `InitializeHeartbeats` and `Heartbeat`).
pub fn next_heartbeat_time(o: &WorldObject) -> f64 {
    world_object_tick::next_heartbeat_time(o)
}

/// `wo.NextGeneratorUpdateTime` (set by `InitializeHeartbeats` and `GeneratorUpdate`).
pub fn next_generator_update_time(o: &WorldObject) -> f64 {
    world_object_tick::next_generator_update_time(o)
}

/// `wo.NextGeneratorRegenerationTime` (set by `InitializeHeartbeats` and `GeneratorRegeneration`).
pub fn next_generator_regeneration_time(o: &WorldObject) -> f64 {
    world_object_tick::next_generator_regeneration_time(o)
}

/// `creature.NextMonsterTickTime` (set by `Monster_Tick`).
pub fn next_monster_tick_time(o: &WorldObject) -> f64 {
    crate::world_objects::monster_tick::next_monster_tick_time(o)
}

/// `creature.Monster_Tick(currentUnixTime)`.
pub fn creature_monster_tick(w: &mut World, creature: ObjectGuid, current_unix_time: f64) {
    crate::world_objects::monster_tick::monster_tick(w, creature, current_unix_time);
}

/// `wo.GeneratorUpdate(currentUnixTime)` (WorldObject_Tick.cs).
pub fn world_object_generator_update(w: &mut World, wo: ObjectGuid, current_unix_time: f64) {
    world_object_tick::generator_update(w, wo, current_unix_time);
}

/// `wo.GeneratorRegeneration(currentUnixTime)` (WorldObject_Tick.cs).
pub fn world_object_generator_regeneration(w: &mut World, wo: ObjectGuid, current_unix_time: f64) {
    world_object_tick::generator_regeneration(w, wo, current_unix_time);
}

/// `wo.ReinitializeHeartbeats()`.
pub fn world_object_reinitialize_heartbeats(w: &mut World, wo: ObjectGuid) {
    world_object_tick::reinitialize_heartbeats(w, wo);
}

/// `player.Player_Tick(currentUnixTime)`.
pub fn player_player_tick(w: &mut World, player: ObjectGuid, current_unix_time: f64) {
    crate::world_objects::player_tick::player_tick(w, player, current_unix_time);
}

/// `wo.IsDecayable()`.
pub fn world_object_is_decayable(w: &World, wo: ObjectGuid) -> bool {
    crate::world_objects::world_object_decay::is_decayable(w, wo)
}

/// `wo.Decay(elapsed)`.
pub fn world_object_decay(w: &mut World, wo: ObjectGuid, elapsed: TimeSpan) {
    crate::world_objects::world_object_decay::decay(w, wo, elapsed);
}

/// `wo.Destroy()` (defaults: `raiseNotifyOfDestructionEvent = true`, `fromLandblockUnload = false`).
pub fn world_object_destroy(w: &mut World, wo: ObjectGuid) {
    world_object_destroy_ex(w, wo, true, false);
}

/// `wo.Destroy(raiseNotifyOfDestructionEvent, fromLandblockUnload)`.
pub fn world_object_destroy_ex(
    w: &mut World,
    wo: ObjectGuid,
    raise_notify_of_destruction_event: bool,
    from_landblock_unload: bool,
) {
    crate::world_objects::world_object::destroy(
        w,
        wo,
        raise_notify_of_destruction_event,
        from_landblock_unload,
    );
}

/// `wo.BiotaOriginatedFromOrHasBeenSavedToDatabase()`: false for an object built from a weenie
/// and never saved. A guid missing from the store reads false (never reached: the callers pass
/// the landblock's live objects).
pub fn world_object_biota_originated_from_or_has_been_saved_to_database(
    w: &World,
    wo: ObjectGuid,
) -> bool {
    w.objects
        .get(wo)
        .is_some_and(WorldObject::biota_originated_from_or_has_been_saved_to_database)
}

/// `wo.IsStaticThatShouldPersistToShard()`.
pub fn world_object_is_static_that_should_persist_to_shard(w: &World, wo: ObjectGuid) -> bool {
    crate::world_objects::world_object_database::is_static_that_should_persist_to_shard(w, wo)
}

/// `wo.IsDynamicThatShouldPersistToShard()`.
pub fn world_object_is_dynamic_that_should_persist_to_shard(w: &World, wo: ObjectGuid) -> bool {
    crate::world_objects::world_object_database::is_dynamic_that_should_persist_to_shard(w, wo)
}

/// `wo.ChangesDetected` (a `WorldObject_Database.cs` field).
pub fn world_object_changes_detected(w: &World, wo: ObjectGuid) -> bool {
    w.objects
        .get(wo)
        .is_some_and(|o| o.wo.world_object_database.changes_detected)
}

/// `container.Inventory.Values` (the inventory dictionary, in its order).
pub fn container_inventory(w: &World, container: ObjectGuid) -> Vec<ObjectGuid> {
    w.objects
        .get(container)
        .and_then(|o| o.container.as_ref())
        .map(|c| c.container.inventory.keys().copied().collect())
        .unwrap_or_default()
}

/// `DatabaseManager.Shard.SaveBiotasInParallel(biotas, null)`: one batch of the objects' biota
/// snapshots, taken now (after `SaveBiotaToDatabase(false)` brought each up to date).
pub fn shard_save_biotas_in_parallel(w: &mut World, biotas: &[ObjectGuid]) {
    let snapshots: Vec<Biota> = biotas
        .iter()
        .filter_map(|&g| w.objects.get(g))
        .map(|o| o.biota.clone())
        .collect();
    w.shard.save_biotas_in_parallel(snapshots, None, false);
}

/// `DatabaseManager.Shard.BaseDatabase.GetStaticObjectsByLandblock(landblock)`, each biota as
/// the factory's `CreateWorldObject(Biota)` converts it.
pub fn shard_get_static_objects_by_landblock(w: &World, landblock: u16) -> Vec<Biota> {
    let biotas = w
        .shard
        .base_database()
        .get_static_objects_by_landblock(landblock);
    biotas
        .iter()
        .map(|b| {
            empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(
                b, false,
            )
        })
        .collect()
}

/// `DatabaseManager.Shard.BaseDatabase.GetDynamicObjectsByLandblock(landblock)`, each biota as
/// the factory's `CreateWorldObject(Biota)` converts it.
pub fn shard_get_dynamic_objects_by_landblock(w: &World, landblock: u16) -> Vec<Biota> {
    let biotas = w
        .shard
        .base_database()
        .get_dynamic_objects_by_landblock(landblock);
    biotas
        .iter()
        .map(|b| {
            empyrean_store::adapter::biota_converter::BiotaConverter::convert_to_entity_biota(
                b, false,
            )
        })
        .collect()
}

/// `WorldObjectFactory.CreateWorldObjects(biotas)`.
pub fn world_object_factory_create_world_objects(
    w: &mut World,
    biotas: Vec<Biota>,
) -> Vec<Box<WorldObject>> {
    CtorEnv::with_world(w, |env| {
        world_object_factory::create_world_objects(env, biotas)
    })
    .into_iter()
    .map(Box::new)
    .collect()
}

/// `WorldObjectFactory.CreateNewWorldObject(weenieClassId)`: `None` without a
/// weenie; else `CreateNewWorldObject(weenie)`, which draws a new dynamic guid from
/// `GuidManager` and recycles it when `CreateWorldObject` returns null.
pub fn world_object_factory_create_new_world_object(
    w: &mut World,
    weenie_class_id: u32,
) -> Option<Box<WorldObject>> {
    world_object_factory::create_new_world_object_by_wcid_in_world(w, weenie_class_id).map(Box::new)
}

/// `house.LinkedHouses.Add(linked)` (a `House.cs` field).
pub fn house_linked_houses_add(w: &mut World, house: ObjectGuid, linked: ObjectGuid) {
    crate::world_objects::house::fields_mut(w, house)
        .linked_houses
        .push(linked);
}

/// `fo.ActivateLinks(objects, shardObjects, parent)`.
pub fn world_object_activate_links(
    w: &mut World,
    wo: ObjectGuid,
    objects: &[LandblockInstance],
    shard_objects: &[Biota],
    parent: Option<ObjectGuid>,
) {
    crate::world_objects::world_object_links::activate_links(w, wo, objects, shard_objects, parent);
}

/// `wo.NotifyPlayers()`.
pub fn world_object_notify_players(w: &mut World, wo: ObjectGuid) {
    crate::world_objects::world_object_networking::notify_players(w, wo);
}

/// `wo.EnqueueActionBroadcast(p => p.RemoveTrackedObject(wo, fromPickup))`.
pub fn world_object_enqueue_action_broadcast_remove_tracked_object(
    w: &mut World,
    wo: ObjectGuid,
    from_pickup: bool,
) {
    crate::world_objects::player_tracking::enqueue_action_broadcast_remove_tracked_object(
        w,
        wo,
        from_pickup,
    );
}

/// `emitter.IsWithinUseRadiusOf(wo, useRadius)`.
pub fn world_object_is_within_use_radius_of(
    w: &World,
    a: ObjectGuid,
    b: ObjectGuid,
    use_radius: Option<f32>,
) -> bool {
    crate::world_objects::world_object_use::is_within_use_radius_of(w, a, b, use_radius)
}

/// `wo.EmoteManager.OnLocalSignal(emitter, message)`.
pub fn emote_manager_on_local_signal(
    w: &mut World,
    wo: ObjectGuid,
    emitter: ObjectGuid,
    message: &str,
) {
    crate::world_objects::managers::emote_manager::on_local_signal(w, wo, emitter, message);
}

/// `creature.GetEquippedItem(guid)`: the item and whether its `CurrentWieldedLocation` is
/// `Selectable`.
pub fn creature_get_equipped_item(
    w: &World,
    creature: ObjectGuid,
    guid: ObjectGuid,
) -> Option<(ObjectGuid, bool)> {
    let item = crate::world_objects::creature_equipment::get_equipped_item(w, creature, guid)?;
    let selectable = w
        .objects
        .get(item)
        .and_then(WorldObject::current_wielded_location)
        // C#'s lifted `(null & Selectable) != 0` is true
        .is_none_or(|l| {
            (l & empyrean_entity::enums::EquipMask::Selectable)
                != empyrean_entity::enums::EquipMask::default()
        });
    Some((item, selectable))
}

/// `player.SetFogColor(fogColor)`.
pub fn player_set_fog_color(w: &mut World, player: ObjectGuid, fog_color: EnvironChangeType) {
    crate::world_objects::player_networking::set_fog_color(w, player, fog_color);
}

/// `player.SendEnvironChange(environChangeType)`.
pub fn player_send_environ_change(
    w: &mut World,
    player: ObjectGuid,
    environ_change_type: EnvironChangeType,
) {
    crate::world_objects::player_networking::send_environ_change(w, player, environ_change_type);
}

/// `player.Session.Network.EnqueueSend(msgs)`.
pub fn player_session_network_enqueue_send(
    w: &mut World,
    player: ObjectGuid,
    msgs: &[crate::network::game_messages::game_message::GameMessage],
) {
    if let Some(session) = crate::managers::player_manager::player_session(w, player) {
        crate::network::game_messages::game_message::enqueue_send_many(
            w,
            session,
            msgs.iter().cloned(),
        );
    }
}

/// `PropertyManager.GetBool("override_encounter_spawn_rates").Item`.
fn property_manager_get_bool_override_encounter_spawn_rates(w: &World) -> bool {
    crate::managers::property_manager::get_bool(w, "override_encounter_spawn_rates", false, true)
        .item
}

/// `PropertyManager.GetDouble(name).Item`.
fn property_manager_get_double(w: &World, name: &str) -> f64 {
    crate::managers::property_manager::get_double(w, name, 0.0, true).item
}

/// `PropertyManager.GetLong(name).Item`.
fn property_manager_get_long(w: &World, name: &str) -> i64 {
    crate::managers::property_manager::get_long(w, name, 0, true).item
}

// Physics.

/// `landblock.PhysicsLandblock.SetAdjacents(landblock.Adjacents)` (from `LandblockManager.SetAdjacents`).
pub fn physics_landblock_set_adjacents(w: &mut World, id: LandblockId) {
    let adjacents: Vec<u16> = lb(w, id).adjacents.iter().map(|a| a.landblock()).collect();
    phys_ext::set_adjacents(w, id.landblock(), adjacents);
}

/// `PhysicsLandblock.release_shadow_objs()`: every body overlapping one of this landblock's land
/// cells (one in a neighbouring landblock whose geometry reaches across the boundary) forgets
/// that cell, and the cell's collision list is emptied.
// ACE: Physics.Common.Landblock.release_shadow_objs
fn physics_landblock_release_shadow_objs(w: &mut World, id: LandblockId) {
    w.physics
        .release_shadows_into_landblock(dereth_primitives::LandblockId(id.landblock()));
}

/// `PhysicsLandblock.GetZ(origin)`: the terrain height under a point of this landblock, or the
/// point's own Z where there is no land cell or no terrain polygon under it. The cell is ACE's
/// `GetCell`; `find_terrain_poly` and the plane solve are the shared physics crate's
/// (`terrain_height_at`), which computes ACE's formula (`(p.Dot2D(N) + D) / N.Z * -1`, refused
/// for a near-vertical plane).
// ACE: Physics.Common.Landblock.GetZ
pub fn physics_landblock_get_z(w: &World, id: LandblockId, point: Vector3) -> f32 {
    let Some(cell) = physics_landblock_get_cell(w, id, point) else {
        return point.z;
    };
    let pos = PPosition::new(
        CellId(cell),
        Frame::new(Vec3::new(point.x, point.y, point.z), Quat::IDENTITY),
    );
    w.physics.terrain_height_at(&pos).unwrap_or(point.z)
}

/// `PhysicsLandblock.GetCell(point)`: the land cell under a landblock-relative point, if its
/// landblock is in the landscape (`LScape.get_landcell`).
// ACE: Physics.Common.Landblock.GetCell
fn physics_landblock_get_cell(w: &World, id: LandblockId, point: Vector3) -> Option<u32> {
    if point.x < 0.0 || point.y < 0.0 || point.x > 192.0 || point.y > 192.0 {
        return None;
    }

    let cell_x: i32 = point.x.cs_cast();
    let cell_y: i32 = point.y.cs_cast();
    let (cell_x, cell_y) = (cell_x / 24, cell_y / 24);

    let cell_index: u32 = (cell_x * 8 + cell_y).cs_cast();
    let block_cell_id = (u32::from(id.landblock()) << 16) | (cell_index + 1);
    phys_ext::get_landcell(w, block_cell_id).map(|c| c.0)
}

/// `PhysicsLandblock.IsDungeon`: the physics landblock's formula, evaluated over the same dat
/// data. It differs from `Landblock.IsDungeon` in the NW island test
/// (`BlockCoord.Y > 1976`, so `LandblockY >= 0xF8`, where the server's is `> 0xF8`).
#[must_use]
pub fn physics_landblock_is_dungeon(l: &Landblock) -> bool {
    if l.id.landblock_x() < 8 && l.id.landblock_y() >= 0xF8 {
        return false;
    }
    if let Some(cell_landblock) = &l.cell_landblock {
        if cell_landblock.height.iter().any(|&height| height != 0) {
            return false;
        }
    }
    l.info_has_cells_and_no_buildings()
}

/// `CurrentLandblock.PhysicsLandblock != null && CurrentLandblock.PhysicsLandblock.IsDungeon`.
/// ACE's `Landblock` constructor always builds its `PhysicsLandblock`, so only the dungeon test
/// remains.
///
/// # Panics
/// When the object has no current landblock (ACE: `NullReferenceException`).
#[must_use]
pub fn current_physics_landblock_is_dungeon(w: &World, this: ObjectGuid) -> bool {
    let id = w
        .objects
        .get(this)
        .and_then(|o| o.current_landblock)
        .expect("ACE: CurrentLandblock is null (NullReferenceException)");
    w.landblock_manager
        .landblocks
        .get(id)
        .is_some_and(physics_landblock_is_dungeon)
}

/// `pos.adjust_to_outside()` on `(objCellId, origin)`: `LandDefs.AdjustToOutside`, the shared
/// physics crate's (V1). Returns the adjusted cell id, 0 when the point is off the world.
pub fn physics_position_adjust_to_outside(obj_cell_id: u32, origin: &mut Vector3) -> u32 {
    let mut cell = CellId(obj_cell_id);
    let mut loc = Vec3::new(origin.x, origin.y, origin.z);
    landdefs::adjust_to_outside(&mut cell, &mut loc);
    origin.x = loc.x;
    origin.y = loc.y;
    origin.z = loc.z;
    cell.0
}

/// `LScape.get_landcell(cellId) as SortCell`, then `sortCell != null && sortCell.has_building()`.
/// An interior cell is not a `SortCell`. A land cell's `Building` is set by the physics
/// landblock's `init_buildings`: each building of the landblock's `LandblockInfo`, in the land
/// cell its frame origin adjusts to (`LandDefs.AdjustToOutside` from the landblock id).
// ACE: SortCell.has_building
pub fn lscape_get_landcell_has_building(w: &World, id: LandblockId, obj_cell_id: u32) -> bool {
    if phys_ext::is_env_cell(CellId(obj_cell_id))
        || phys_ext::get_landcell(w, obj_cell_id).is_none()
    {
        return false;
    }
    let Some(info) = lb(w, id).landblock_info.as_ref() else {
        return false;
    };
    // ACE: Physics.Common.Landblock.init_buildings
    info.buildings.iter().any(|building| {
        let mut cell = CellId((u32::from(id.landblock()) << 16) | 0xFFFF);
        let mut origin = building.frame.origin;
        landdefs::adjust_to_outside(&mut cell, &mut origin);
        cell.0 == obj_cell_id && phys_ext::get_landcell(w, cell.0).is_some()
    })
}

/// `wo.PhysicsObj.set_active(active)`.
pub fn physics_obj_set_active(w: &mut World, wo: ObjectGuid, active: bool) {
    if let Some(h) = phys_ext::physics_obj(w, wo) {
        phys_ext::set_active(w, h, active);
    }
}
