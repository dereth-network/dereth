// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/LandblockManager.cs
//! Port of `Source/ACE.Server/Managers/LandblockManager.cs`.
//!
//! Handles loading/unloading landblocks, and their adjacencies.
//!
//! # Shape
//!
//! - Every loaded [`Landblock`] lives in [`LandblockTable`] (ACE's `Landblock[255, 255]`); every
//!   other reference to a landblock (the loaded set, groups, adjacency lists, the destruction
//!   queue, `WorldObject.CurrentLandblock`) is its [`LandblockId`], resolved on use. One landblock
//!   is loaded per id at a time, so an id identifies it as ACE's references do.
//! - The world is ticked serially (V5): ACE's `Parallel.ForEach` branches, selected by
//!   `Server.Threading.MultiThreadedLandblockGroup*Ticking`, run as the serial branch. The groups
//!   are still built, as ACE builds them with threading off, and their order is the tick order.
//! - `ReaderWriterLockSlim` is not needed: the world thread owns this state.
//! - `ConcurrentBag<T>` used from one thread is a stack: `TryTake` takes the newest item, and
//!   enumeration yields the newest first. The destruction queue and the moved-object bag keep that
//!   order.

use empyrean_common::dotnet::DotNetHashSet;
use empyrean_common::game_configuration::GameConfiguration;
use empyrean_common::performance::rolling_amount_over_time_tracker::RollingAmountOverTimeTracker;
use empyrean_common::preloaded_landblock::PreloadedLandblocks;
use empyrean_entity::enums::EnvironChangeType;
use empyrean_entity::{LandblockId, ObjectGuid};

use crate::entity::adjacency::Adjacency;
use crate::entity::landblock::{self, Landblock};
use crate::entity::landblock_group::{allocate_group_id, LandblockGroup, LandblockGroupId};
use crate::managers::server_performance_monitor::{self as perf, MonitorType};
use crate::World;

/// `Landblock[255, 255]`: the loaded landblocks by `(LandblockX, LandblockY)`.
#[derive(Debug)]
pub struct LandblockTable {
    slots: Vec<Option<Box<Landblock>>>,
}

impl Default for LandblockTable {
    fn default() -> Self {
        LandblockTable {
            slots: std::iter::repeat_with(|| None).take(255 * 255).collect(),
        }
    }
}

impl LandblockTable {
    /// `landblocks[id.LandblockX, id.LandblockY]`'s index.
    ///
    /// # Panics
    /// For `LandblockX` or `LandblockY` 255, as .NET throws `IndexOutOfRangeException` on the
    /// 255 x 255 array.
    fn index(id: LandblockId) -> usize {
        let (x, y) = (usize::from(id.landblock_x()), usize::from(id.landblock_y()));
        assert!(
            x < 255 && y < 255,
            "IndexOutOfRangeException: landblocks[0x{x:02X}, 0x{y:02X}]"
        );
        x * 255 + y
    }

    pub fn get(&self, id: LandblockId) -> Option<&Landblock> {
        self.slots[Self::index(id)].as_deref()
    }

    pub fn get_mut(&mut self, id: LandblockId) -> Option<&mut Landblock> {
        self.slots[Self::index(id)].as_deref_mut()
    }

    /// A landblock that must be loaded (ACE holds a reference to it).
    ///
    /// # Panics
    /// When it is not loaded.
    pub fn expect(&self, id: LandblockId) -> &Landblock {
        self.get(id)
            .unwrap_or_else(|| panic!("landblock {id} is not loaded"))
    }

    /// As [`expect`](Self::expect), mutably.
    ///
    /// # Panics
    /// When it is not loaded.
    pub fn expect_mut(&mut self, id: LandblockId) -> &mut Landblock {
        self.get_mut(id)
            .unwrap_or_else(|| panic!("landblock {id} is not loaded"))
    }

    fn set(
        &mut self,
        id: LandblockId,
        landblock: Option<Box<Landblock>>,
    ) -> Option<Box<Landblock>> {
        std::mem::replace(&mut self.slots[Self::index(id)], landblock)
    }
}

/// The mutable static state of ACE's `LandblockManager`, held as a field of `World`.
#[derive(Debug, Default)]
pub struct LandblockManagerState {
    /// `landblocks`: a table of all the landblocks in the world map. Landblocks which aren't
    /// currently loaded are `None` here.
    pub landblocks: LandblockTable,
    /// `loadedLandblocks`: a lookup table of all the currently loaded landblocks.
    loaded_landblocks: DotNetHashSet<LandblockId>,
    landblock_group_pending_additions: Vec<LandblockId>,
    landblock_groups: Vec<LandblockGroup>,
    /// `destructionQueue` (a `ConcurrentBag`, taken newest first).
    destruction_queue: Vec<LandblockId>,
    /// Hands out [`LandblockGroupId`]s (not ACE: its groups are identified by reference).
    next_group_id: u64,

    // ACE: LandblockManager.CurrentlyTickingLandblockGroupsMultiThreaded
    /// Used to debug cross-landblock group (and potentially cross-thread) operations. Always
    /// false: the world ticks serially (V5).
    pub currently_ticking_landblock_groups_multi_threaded: bool,
    // ACE: LandblockManager.CurrentMultiThreadedTickingLandblockGroup
    /// The group the (only) ticking thread is in; `None` outside the multithreaded branches.
    pub current_multi_threaded_ticking_landblock_group: Option<LandblockGroupId>,

    // ACE: LandblockManager.GlobalFogColor
    pub global_fog_color: Option<EnvironChangeType>,

    // ACE: LandblockManager.TickPhysicsEfficiencyTracker
    /// Registered only by the multithreaded physics branch, which never runs here (V5), so it
    /// stays empty and its average is 0.
    pub tick_physics_efficiency_tracker: EfficiencyTracker,
    // ACE: LandblockManager.TickMultiThreadedWorkEfficiencyTracker
    /// Likewise, for the multithreaded work branch.
    pub tick_multi_threaded_work_efficiency_tracker: EfficiencyTracker,

    /// `ConfigManager.Config.Server.Threading.MultiThreadedLandblockGroupPhysicsTicking`, until
    /// `World` carries the configuration (only the split in `UnloadLandblocks` reads it).
    pub multi_threaded_landblock_group_physics_ticking: bool,
    /// `ConfigManager.Config.Server.Threading.MultiThreadedLandblockGroupTicking`, likewise.
    pub multi_threaded_landblock_group_ticking: bool,
}

impl LandblockManagerState {
    /// The landblock groups, in tick order.
    pub fn landblock_groups(&self) -> &[LandblockGroup] {
        &self.landblock_groups
    }

    /// The landblocks queued for unloading, oldest first.
    pub fn destruction_queue(&self) -> &[LandblockId] {
        &self.destruction_queue
    }
}

/// `new RollingAmountOverTimeTracker(TimeSpan.FromMinutes(1))`, the initializer of both
/// LandblockManager efficiency trackers (a wrapper so the state can derive `Default`).
#[derive(Debug, Clone)]
pub struct EfficiencyTracker(pub RollingAmountOverTimeTracker);

impl Default for EfficiencyTracker {
    fn default() -> Self {
        Self(RollingAmountOverTimeTracker::new(
            empyrean_common::dotnet::datetime::TimeSpan::from_minutes(1.0),
        ))
    }
}

// ACE: LandblockManager.LandblockGroupsCount
pub fn landblock_groups_count(w: &World) -> usize {
    w.landblock_manager.landblock_groups.len()
}

// ACE: LandblockManager.GetLoadedLandblockGroups
pub fn get_loaded_landblock_groups(w: &World) -> Vec<LandblockGroupId> {
    w.landblock_manager
        .landblock_groups
        .iter()
        .map(|g| g.id)
        .collect()
}

// ACE: LandblockManager.PreloadConfigLandblocks
/// Permaloads a list of configurable landblocks if server option is set. `server` is
/// `ConfigManager.Config.Server` until `World` carries the configuration. ACE writes a default
/// entry back when `PreloadedLandblocks` is null; the configuration port has no null list (a
/// missing key gives that same default), so that branch cannot occur.
pub fn preload_config_landblocks(w: &mut World, server: &GameConfiguration) {
    if !server.landblock_preloading {
        log::info!("Preloading Landblocks Disabled...");
        log::warn!("Events may not function correctly as Preloading of Landblocks has disabled.");
        return;
    }

    log::info!("Preloading Landblocks...");

    log::info!(
        "Found {} landblock entries in PreloadedLandblocks configuration, {} are set to preload.",
        server.preloaded_landblocks.len(),
        server
            .preloaded_landblocks
            .iter()
            .filter(|x| x.enabled)
            .count()
    );

    for preload_landblock in &server.preloaded_landblocks {
        if !preload_landblock.enabled {
            log::debug!(
                "Landblock {:?} specified but not enabled in config, skipping",
                preload_landblock.id
            );
            continue;
        }

        if let Some(landblock) = uint_try_parse_hex(preload_landblock.id.as_deref()) {
            if landblock == 0 {
                if preload_landblock.description.as_deref() == Some("Apartment Landblocks") {
                    log::info!(
                        "Preloading landblock group: {}, IncludeAdjacents = {}, Permaload = {}",
                        preload_landblock.description.as_deref().unwrap_or(""),
                        preload_landblock.include_adjacents,
                        preload_landblock.permaload
                    );
                    for &apt in APARTMENT_LANDBLOCKS {
                        preload_landblock_one(w, apt, preload_landblock);
                    }
                }
            } else {
                preload_landblock_one(w, landblock, preload_landblock);
            }
        }
    }
}

/// `uint.TryParse(s, NumberStyles.HexNumber, CultureInfo.CurrentCulture, out uint)`: hex digits
/// with optional leading and trailing white space, no sign and no `0x` prefix; `null` fails.
fn uint_try_parse_hex(s: Option<&str>) -> Option<u32> {
    let s = s?.trim_matches(|c: char| matches!(c, '\u{9}'..='\u{D}' | ' '));
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}

// ACE: LandblockManager.PreloadLandblock
fn preload_landblock_one(w: &mut World, landblock: u32, preload_landblock: &PreloadedLandblocks) {
    let landblock_id = LandblockId::new(landblock);
    get_landblock(
        w,
        landblock_id,
        preload_landblock.include_adjacents,
        preload_landblock.permaload,
    );
    log::debug!(
        "Landblock {:04X}, ({}) preloaded. IncludeAdjacents = {}, Permaload = {}",
        landblock_id.landblock(),
        preload_landblock.description.as_deref().unwrap_or(""),
        preload_landblock.include_adjacents,
        preload_landblock.permaload
    );

    // Not ACE: a start-up line with the landblock's object count, for smoke runs (ACE logs none).
    // It is queued behind the load's delegates, so it counts the objects they added (and the
    // pending additions);
    // only when info logging is on, so the queue is ACE's otherwise.
    if log::log_enabled!(log::Level::Info) {
        let description = preload_landblock.description.clone().unwrap_or_default();
        if let Some(l) = w.landblock_manager.landblocks.get_mut(landblock_id) {
            l.enqueue_action(crate::entity::actions::i_action::Action::delegate(
                move |w: &mut World| {
                    let count = w
                        .landblock_manager
                        .landblocks
                        .get(landblock_id)
                        .map_or(0, |l| {
                            l.world_object_guids().count() + l.pending_addition_guids().count()
                        });
                    log::info!(
                        "Landblock {:04X} ({description}) loaded: {count} objects",
                        landblock_id.landblock()
                    );
                },
            ));
        }
    }
}

/// `apartmentLandblocks`.
const APARTMENT_LANDBLOCKS: &[u32] = &[
    0x7200FFFF, 0x7300FFFF, 0x7400FFFF, 0x7500FFFF, 0x7600FFFF, 0x7700FFFF, 0x7800FFFF, 0x7900FFFF,
    0x7A00FFFF, 0x7B00FFFF, 0x7C00FFFF, 0x7D00FFFF, 0x7E00FFFF, 0x7F00FFFF, 0x8000FFFF, 0x8100FFFF,
    0x8200FFFF, 0x8300FFFF, 0x8400FFFF, 0x8500FFFF, 0x8600FFFF, 0x8700FFFF, 0x8800FFFF, 0x8900FFFF,
    0x8A00FFFF, 0x8B00FFFF, 0x8C00FFFF, 0x8D00FFFF, 0x8E00FFFF, 0x8F00FFFF, 0x9000FFFF, 0x9100FFFF,
    0x9200FFFF, 0x9300FFFF, 0x9400FFFF, 0x9500FFFF, 0x9600FFFF, 0x9700FFFF, 0x9800FFFF, 0x9900FFFF,
    0x5360FFFF, 0x5361FFFF, 0x5362FFFF, 0x5363FFFF, 0x5364FFFF, 0x5365FFFF, 0x5366FFFF, 0x5367FFFF,
    0x5368FFFF, 0x5369FFFF,
];

// ACE: LandblockManager.ProcessPendingLandblockGroupAdditions
fn process_pending_landblock_group_additions(w: &mut World) {
    let lm = &mut w.landblock_manager;

    if lm.landblock_group_pending_additions.is_empty() {
        return;
    }

    let utc_now = w.now.utc;

    for i in (0..lm.landblock_group_pending_additions.len()).rev() {
        let pending = lm.landblock_group_pending_additions[i];

        if lm.landblocks.expect_mut(pending).is_dungeon() {
            // Each dungeon exists in its own group
            let id = allocate_group_id(&mut lm.next_group_id);
            let landblock_group =
                LandblockGroup::with_landblock(id, utc_now, &mut lm.landblocks, pending);
            lm.landblock_groups.push(landblock_group);
        } else {
            // Find out how many groups this landblock is eligible for
            let mut landblock_groups_index_matches_by_distance = Vec::new();

            for j in 0..lm.landblock_groups.len() {
                if lm.landblock_groups[j].is_dungeon() {
                    continue;
                }

                if lm.landblock_groups[j]
                    .should_be_added_to_this_landblock_group(&lm.landblocks, pending)
                {
                    landblock_groups_index_matches_by_distance.push(j);
                }
            }

            if let Some(&first) = landblock_groups_index_matches_by_distance.first() {
                // Add the landblock to the first eligible group
                lm.landblock_groups[first].add(&mut lm.landblocks, pending);

                if landblock_groups_index_matches_by_distance.len() > 1 {
                    // Merge the additional eligible groups into the first one
                    for j in (1..landblock_groups_index_matches_by_distance.len()).rev() {
                        let from = landblock_groups_index_matches_by_distance[j];

                        // Copy the j down into 0
                        let members: Vec<LandblockId> =
                            lm.landblock_groups[from].iter().copied().collect();
                        for landblock in members {
                            lm.landblock_groups[first].add(&mut lm.landblocks, landblock);
                        }

                        lm.landblock_groups.remove(from);
                    }
                }
            } else {
                // No close groups were found
                let id = allocate_group_id(&mut lm.next_group_id);
                let landblock_group =
                    LandblockGroup::with_landblock(id, utc_now, &mut lm.landblocks, pending);
                lm.landblock_groups.push(landblock_group);
            }
        }

        lm.landblock_group_pending_additions.remove(i);
    }

    // Debugging todo: comment this out after enough testing
    let count: usize = lm.landblock_groups.iter().map(LandblockGroup::count).sum();
    if count != lm.loaded_landblocks.len() {
        log::error!(
            "[LANDBLOCK GROUP] ProcessPendingAdditions count ({count}) != loadedLandblocks.Count ({})",
            lm.loaded_landblocks.len()
        );
    }
}

// ACE: LandblockManager.Tick
pub fn tick(w: &mut World, portal_year_ticks: f64) {
    // update positions through physics engine
    perf::restart_event(w, MonitorType::LandblockManagerTickPhysics);
    tick_physics(w, portal_year_ticks);
    perf::register_event_end(w, MonitorType::LandblockManagerTickPhysics);

    // Tick all of our Landblocks and WorldObjects (Work that can be multi-threaded)
    perf::restart_event(w, MonitorType::LandblockManagerTickMultiThreadedWork);
    tick_multi_threaded_work(w);
    perf::register_event_end(w, MonitorType::LandblockManagerTickMultiThreadedWork);

    // Tick all of our Landblocks and WorldObjects (Work that must be single threaded)
    perf::restart_event(w, MonitorType::LandblockManagerTickSingleThreadedWork);
    tick_single_threaded_work(w);
    perf::register_event_end(w, MonitorType::LandblockManagerTickSingleThreadedWork);

    // clean up inactive landblocks
    unload_landblocks(w);
}

/// Every landblock of every group, in `foreach (group) foreach (landblock)` order. Taken before
/// ticking: ticking never changes the groups (new landblocks wait in the pending list), and each
/// landblock is re-resolved as it is ticked.
fn landblocks_in_tick_order(w: &World) -> Vec<LandblockId> {
    w.landblock_manager
        .landblock_groups
        .iter()
        .flat_map(|g| g.iter().copied())
        .collect()
}

// ACE: LandblockManager.TickPhysics
/// Processes physics objects in all active landblocks for updating.
///
/// DIVERGE: the `MultiThreadedLandblockGroupPhysicsTicking` branch runs serially (V5), in group
/// order rather than `Partitioner` order, and its efficiency tracker is not kept.
fn tick_physics(w: &mut World, portal_year_ticks: f64) {
    process_pending_landblock_group_additions(w);

    let mut moved_objects: Vec<ObjectGuid> = Vec::new();

    for landblock in landblocks_in_tick_order(w) {
        landblock::tick_physics(w, landblock, portal_year_ticks, &mut moved_objects);
    }

    // iterate through objects that have changed landblocks (a ConcurrentBag: newest first)
    for &moved_object in moved_objects.iter().rev() {
        // NOTE: The object's Location can now be null, if a player logs out, or an item is picked up
        let has_location = w
            .objects
            .get(moved_object)
            .is_some_and(|o| o.location().is_some());
        if !has_location {
            continue;
        }

        // assume adjacency move here?
        relocate_object_for_physics(w, moved_object, true);
    }
}

// ACE: LandblockManager.TickMultiThreadedWork
/// DIVERGE: serial whatever `MultiThreadedLandblockGroupTicking` says (V5). `Time.GetUnixTime()`
/// is the tick's snapshot rather than re-read per landblock.
fn tick_multi_threaded_work(w: &mut World) {
    process_pending_landblock_group_additions(w);

    for landblock in landblocks_in_tick_order(w) {
        let now = w.now.unix_time;
        landblock::tick_multi_threaded_work(w, landblock, now);
    }
}

// ACE: LandblockManager.TickSingleThreadedWork
fn tick_single_threaded_work(w: &mut World) {
    process_pending_landblock_group_additions(w);

    for landblock in landblocks_in_tick_order(w) {
        let now = w.now.unix_time;
        landblock::tick_single_threaded_work(w, landblock, now);
    }
}

/// `worldObject.Location.LandblockId`, as ACE dereferences it (a null `Location` throws).
fn location_landblock_id(w: &World, world_object: ObjectGuid) -> LandblockId {
    w.objects
        .get(world_object)
        .and_then(|o| o.location())
        .expect("NullReferenceException: WorldObject.Location")
        .landblock_id()
}

// ACE: LandblockManager.AddObject
/// Adds a WorldObject to the landblock defined by the object's location. With `load_adjacents`,
/// ensures all of the adjacent landblocks for this WorldObject are loaded.
pub fn add_object(w: &mut World, world_object: ObjectGuid, load_adjacents: bool) -> bool {
    let id = location_landblock_id(w, world_object);
    let block = get_landblock(w, id, load_adjacents, false);

    landblock::add_world_object(w, block, world_object)
}

// ACE: LandblockManager.RelocateObjectForPhysics
/// Relocates an object to the appropriate landblock. Should only be called from
/// physics/worldmanager, not player.
pub fn relocate_object_for_physics(w: &mut World, world_object: ObjectGuid, adjacency_move: bool) {
    let old_block = w
        .objects
        .get(world_object)
        .and_then(|o| o.current_landblock);
    let id = location_landblock_id(w, world_object);
    let new_block = get_landblock(w, id, true, false);

    let is_spell_projectile = w
        .objects
        .get(world_object)
        .is_some_and(|o| o.is_spell_projectile());
    if w.landblock_manager.landblocks.expect(new_block).is_dormant && is_spell_projectile {
        landblock::physics_obj_set_active(w, world_object, false);
        landblock::world_object_destroy(w, world_object);
        return;
    }

    // Remove from the old landblock -- force
    if let Some(old_block) = old_block {
        if w.landblock_manager.landblocks.get(old_block).is_some() {
            landblock::remove_world_object_for_physics(w, old_block, world_object, adjacency_move);
        }
    }
    // Add to the new landblock
    landblock::add_world_object_for_physics(w, new_block, world_object);
}

// ACE: LandblockManager.IsLoaded
pub fn is_loaded(w: &World, landblock_id: LandblockId) -> bool {
    w.landblock_manager.landblocks.get(landblock_id).is_some()
}

// ACE: LandblockManager.GetLandblock
/// Returns the landblock, loading it if not already active. The result is the id the landblock
/// was loaded under (ACE returns the landblock; its `Id` is the id it was first loaded with).
pub fn get_landblock(
    w: &mut World,
    landblock_id: LandblockId,
    load_adjacents: bool,
    permaload: bool,
) -> LandblockId {
    let mut set_adjacents = false;

    let loaded_id = match w.landblock_manager.landblocks.get(landblock_id) {
        Some(existing) => existing.id,
        None => {
            // load up this landblock
            let new = Landblock::new(w, landblock_id);
            w.landblock_manager
                .landblocks
                .set(landblock_id, Some(Box::new(new)));

            if !w.landblock_manager.loaded_landblocks.insert(landblock_id) {
                log::error!(
                    "LandblockManager: failed to add {:08X} to active landblocks!",
                    landblock_id.raw()
                );
                return landblock_id;
            }

            w.landblock_manager
                .landblock_group_pending_additions
                .push(landblock_id);

            landblock::init(w, landblock_id, false);

            set_adjacents = true;
            landblock_id
        }
    };

    if permaload {
        w.landblock_manager
            .landblocks
            .expect_mut(loaded_id)
            .permaload = true;
    }

    // load adjacents, if applicable
    if load_adjacents {
        let adjacents = get_adjacent_ids(w, loaded_id);
        for adjacent in adjacents {
            get_landblock(w, adjacent, false, permaload);
        }

        set_adjacents = true;
    }

    // cache adjacencies
    if set_adjacents {
        set_adjacents_of(w, loaded_id, true, true);
    }

    loaded_id
}

// ACE: LandblockManager.GetLoadedLandblocks
/// Returns the list of all loaded landblocks.
pub fn get_loaded_landblocks(w: &World) -> Vec<LandblockId> {
    w.landblock_manager
        .loaded_landblocks
        .iter()
        .copied()
        .collect()
}

// ACE: LandblockManager.GetAdjacents
/// Returns the active, non-null adjacents for a landblock.
fn get_adjacents(w: &mut World, landblock: LandblockId) -> Vec<LandblockId> {
    let adjacent_ids = get_adjacent_ids(w, landblock);
    adjacents_among_loaded(w, &adjacent_ids)
}

/// The loaded landblocks among `adjacent_ids`, in order.
fn adjacents_among_loaded(w: &World, adjacent_ids: &[LandblockId]) -> Vec<LandblockId> {
    adjacent_ids
        .iter()
        .filter_map(|&id| w.landblock_manager.landblocks.get(id).map(|lb| lb.id))
        .collect()
}

// ACE: LandblockManager.GetAdjacentIDs
/// Returns the list of adjacent landblock IDs for a landblock.
fn get_adjacent_ids(w: &mut World, landblock: LandblockId) -> Vec<LandblockId> {
    let is_dungeon = w
        .landblock_manager
        .landblocks
        .expect_mut(landblock)
        .is_dungeon();
    adjacent_ids_of(landblock, is_dungeon)
}

/// `GetAdjacentIDs` for a landblock whose `IsDungeon` is known.
pub fn adjacent_ids_of(landblock: LandblockId, is_dungeon: bool) -> Vec<LandblockId> {
    let mut adjacents = Vec::new();

    if is_dungeon {
        return adjacents; // dungeons have no adjacents
    }

    let order = [
        Adjacency::North,
        Adjacency::South,
        Adjacency::West,
        Adjacency::East,
        Adjacency::NorthWest,
        Adjacency::NorthEast,
        Adjacency::SouthWest,
        Adjacency::SouthEast,
    ];
    adjacents.extend(
        order
            .into_iter()
            .filter_map(|adjacency| get_adjacent_id(landblock, adjacency)),
    );

    adjacents
}

// ACE: LandblockManager.GetAdjacentID
/// Returns an adjacent landblock ID for a landblock, `None` off the map.
pub fn get_adjacent_id(landblock: LandblockId, adjacency: Adjacency) -> Option<LandblockId> {
    let mut lbx = i32::from(landblock.landblock_x());
    let mut lby = i32::from(landblock.landblock_y());

    match adjacency {
        Adjacency::North => lby += 1,
        Adjacency::South => lby -= 1,
        Adjacency::West => lbx -= 1,
        Adjacency::East => lbx += 1,
        Adjacency::NorthWest => {
            lby += 1;
            lbx -= 1;
        }
        Adjacency::NorthEast => {
            lby += 1;
            lbx += 1;
        }
        Adjacency::SouthWest => {
            lby -= 1;
            lbx -= 1;
        }
        Adjacency::SouthEast => {
            lby -= 1;
            lbx += 1;
        }
        _ => {}
    }

    if !(0..=254).contains(&lbx) || !(0..=254).contains(&lby) {
        return None;
    }

    Some(LandblockId::from_xy(
        u8::try_from(lbx).ok()?,
        u8::try_from(lby).ok()?,
    ))
}

// ACE: LandblockManager.SetAdjacents
/// Rebuilds the adjacency cache for a landblock, and the adjacent landblocks' caches if
/// `traverse`.
fn set_adjacents_of(w: &mut World, landblock: LandblockId, traverse: bool, p_sync: bool) {
    let adjacents = get_adjacents(w, landblock);
    w.landblock_manager
        .landblocks
        .expect_mut(landblock)
        .adjacents
        .clone_from(&adjacents);

    if p_sync {
        landblock::physics_landblock_set_adjacents(w, landblock);
    }

    if traverse {
        for adjacent in adjacents {
            set_adjacents_of(w, adjacent, false, false);
        }
    }
}

// ACE: LandblockManager.AddToDestructionQueue
/// Queues a landblock for unloading.
pub fn add_to_destruction_queue(w: &mut World, landblock: LandblockId) {
    w.landblock_manager.destruction_queue.push(landblock);
}

// ACE: LandblockManager.UnloadLandblocks
/// Processes the destruction queue.
///
/// DIVERGE: the queue holds ids. A landblock queued twice (a heartbeat and the shutdown sweep) is
/// already gone the second time; ACE calls `Unload` again on the detached landblock (a no-op on
/// its empty collections) before logging the failure, this skips straight to the log.
fn unload_landblocks(w: &mut World) {
    while let Some(landblock) = w.landblock_manager.destruction_queue.pop() {
        let loaded = w.landblock_manager.landblocks.get(landblock).is_some()
            && w.landblock_manager.loaded_landblocks.contains(&landblock);

        if loaded {
            landblock::unload(w, landblock);
        }

        let utc_now = w.now.utc;
        let lm = &mut w.landblock_manager;

        // remove from list of managed landblocks
        if loaded && lm.loaded_landblocks.remove(&landblock) {
            let mut removed = lm
                .landblocks
                .set(landblock, None)
                .expect("loaded landblock");

            // remove from landblock group
            for i in (0..lm.landblock_groups.len()).rev() {
                if lm.landblock_groups[i].remove(&mut lm.landblocks, landblock) {
                    removed.current_landblock_group = None;

                    if lm.landblock_groups[i].count() == 0 {
                        lm.landblock_groups.remove(i);
                    } else if lm.multi_threaded_landblock_group_physics_ticking
                        || lm.multi_threaded_landblock_group_ticking
                    {
                        // Only try to split if multi-threading is enabled
                        let splits = lm.landblock_groups[i].try_throttled_split(
                            &mut lm.landblocks,
                            &mut lm.next_group_id,
                            utc_now,
                        );

                        if let Some(splits) = splits {
                            if !splits.is_empty() {
                                log::debug!(
                                    "[LANDBLOCK GROUP] TrySplit resulted in {} split(s)",
                                    splits.len()
                                );
                                log::debug!(
                                    "[LANDBLOCK GROUP] split for old: {}",
                                    lm.landblock_groups[i]
                                );
                            }

                            for split in splits {
                                log::debug!("[LANDBLOCK GROUP] split and new: {split}");
                                lm.landblock_groups.push(split);
                            }
                        }
                    }

                    break;
                }
            }

            let is_dungeon = removed.is_dungeon();
            notify_adjacents(w, landblock, is_dungeon);
        } else {
            log::error!("LandblockManager: failed to unload {:08X}", landblock.raw());
        }
    }
}

// ACE: LandblockManager.NotifyAdjacents
/// Notifies the adjacent landblocks to rebuild their adjacency cache. Called when a landblock is
/// unloaded (so it is no longer in the table; `is_dungeon` is its `IsDungeon`).
fn notify_adjacents(w: &mut World, landblock: LandblockId, is_dungeon: bool) {
    let adjacent_ids = adjacent_ids_of(landblock, is_dungeon);
    let adjacents = adjacents_among_loaded(w, &adjacent_ids);

    for adjacent in adjacents {
        set_adjacents_of(w, adjacent, false, true);
    }
}

// ACE: LandblockManager.AddAllActiveLandblocksToDestructionQueue
/// Used on server shutdown.
pub fn add_all_active_landblocks_to_destruction_queue(w: &mut World) {
    let loaded: Vec<LandblockId> = w
        .landblock_manager
        .loaded_landblocks
        .iter()
        .copied()
        .collect();
    for landblock in loaded {
        add_to_destruction_queue(w, landblock);
    }
}

// ACE: LandblockManager.SetGlobalFogColor
fn set_global_fog_color(w: &mut World, environ_change_type: EnvironChangeType) {
    if environ_change_type.is_fog() {
        if environ_change_type == EnvironChangeType::Clear {
            w.landblock_manager.global_fog_color = None;
        } else {
            w.landblock_manager.global_fog_color = Some(environ_change_type);
        }

        for landblock in get_loaded_landblocks(w) {
            landblock::send_current_environ(w, landblock);
        }
    }
}

// ACE: LandblockManager.SendGlobalEnvironSound
fn send_global_environ_sound(w: &mut World, environ_change_type: EnvironChangeType) {
    if environ_change_type.is_sound() {
        for landblock in get_loaded_landblocks(w) {
            landblock::send_environ_change(w, landblock, environ_change_type);
        }
    }
}

// ACE: LandblockManager.DoEnvironChange
pub fn do_environ_change(w: &mut World, environ_change_type: EnvironChangeType) {
    if environ_change_type.is_fog() {
        set_global_fog_color(w, environ_change_type);
    } else {
        send_global_environ_sound(w, environ_change_type);
    }
}
