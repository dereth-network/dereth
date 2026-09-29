// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/GuidManager.cs
//! Port of `Source/ACE.Server/Managers/GuidManager.cs`.
//!
//! Used to assign global guids and ensure they are unique to server.
//!
//! ACE's allocators read the shard at construction (`GetMaxGuidFoundInRange`,
//! `GetSequenceGaps`) and block on the database callback. Here the
//! caller passes those two reads in ([`ShardGuidQueries`]); the allocation logic is ACE's.
//! `lock (this)` is not needed: the world thread owns the allocators.

use std::collections::VecDeque;

use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::dotnet::format;
use empyrean_entity::ObjectGuid;

use crate::World;

// ACE: GuidManager.InvalidGuid
/// Is equal to `uint.MaxValue`.
pub const INVALID_GUID: u32 = u32::MAX;

const LOW_ID_LIMIT: u32 = 0x1000;

/// The two shard reads the allocators make when they are built.
pub trait ShardGuidQueries {
    /// `DatabaseManager.Shard.GetMaxGuidFoundInRange(min, max, callback)`: the highest guid in
    /// use in `min..=max`, or `uint.MaxValue` when there is none.
    fn get_max_guid_found_in_range(&mut self, min: u32, max: u32) -> u32;
    /// `DatabaseManager.Shard.GetSequenceGaps(min, limitAvailableIDsReturned, callback)`: the
    /// unused `(start, end)` ranges from `min`, at most `limit` ids in total.
    fn get_sequence_gaps(&mut self, min: u32, limit: u32) -> Vec<(u32, u32)>;
}

/// `current` from the shard's max: `min` for an empty range, else `dbVal + 1` (unchecked).
fn start_from_db(min: u32, max: u32, db_val: u32, name: &str) -> u32 {
    let current = if db_val == INVALID_GUID {
        min
    } else {
        // Need to start allocating at current value in db +1
        db_val.wrapping_add(1)
    };

    log::debug!("{name} GUID Allocator current is now {current:08X} of {max:08X}");

    // ACE-BUG: `max - current` is unchecked `uint` arithmetic. When the shard already holds `max`,
    // `current` is `max + 1`, the subtraction wraps and the warning is not logged.
    if max.wrapping_sub(current) < LOW_ID_LIMIT {
        log::warn!("Dangerously low on {name} GUIDs: {current:08X} of {max:08X}");
    }

    current
}

/// The last step of both `Alloc`s: hand out `current` and advance it.
fn alloc_current(current: &mut u32, max: u32, name: &str) -> u32 {
    if *current == max {
        log::error!("Out of {name} GUIDs!");
        return INVALID_GUID;
    }

    if *current == max.wrapping_sub(LOW_ID_LIMIT) {
        log::warn!("Running dangerously low on {name} GUIDs, need to defrag");
    }

    let ret = *current;
    *current = current.wrapping_add(1);

    ret
}

// ACE: GuidManager.PlayerGuidAllocator
#[derive(Debug, Clone)]
pub struct PlayerGuidAllocator {
    min: u32,
    max: u32,
    current: u32,
    name: String,
}

impl PlayerGuidAllocator {
    // ACE: GuidManager.PlayerGuidAllocator.PlayerGuidAllocator
    pub fn new(min: u32, max: u32, name: &str, shard: &mut dyn ShardGuidQueries) -> Self {
        // Read current value out of ShardDatabase
        let db_val = shard.get_max_guid_found_in_range(min, max);
        let current = start_from_db(min, max, db_val, name);
        PlayerGuidAllocator {
            min,
            max,
            current,
            name: name.to_owned(),
        }
    }

    // ACE: GuidManager.PlayerGuidAllocator.Alloc
    /// ACE-BUG: exhaustion is tested with `current == max`. An allocator that started at `max + 1`
    /// (the shard already holds `max`) never reports it and hands out guids past `max`.
    pub fn alloc(&mut self) -> u32 {
        alloc_current(&mut self.current, self.max, &self.name)
    }

    // ACE: GuidManager.PlayerGuidAllocator.Current
    /// For information purposes only: the current DbMax + 1. Use `alloc` to take a guid.
    pub fn current(&self) -> u32 {
        self.current
    }

    // ACE: GuidManager.PlayerGuidAllocator.Min
    pub fn min(&self) -> u32 {
        self.min
    }

    // ACE: GuidManager.PlayerGuidAllocator.Max
    pub fn max(&self) -> u32 {
        self.max
    }
}

/// `DynamicGuidAllocator.recycleTime`: `TimeSpan.FromMinutes(360)`.
fn recycle_time() -> TimeSpan {
    TimeSpan::from_minutes(360.0)
}

/// `DynamicGuidAllocator.limitAvailableIDsReturnedInGetSequenceGaps`.
const LIMIT_AVAILABLE_IDS_RETURNED_IN_GET_SEQUENCE_GAPS: u32 = 10_000_000;

/// The `uint` sum of `(end - start) + 1` over the gaps, unchecked as in C#.
fn total_available(ids: &VecDeque<(u32, u32)>) -> u32 {
    ids.iter().fold(0u32, |total, &(start, end)| {
        total.wrapping_add(end.wrapping_sub(start).wrapping_add(1))
    })
}

// ACE: GuidManager.DynamicGuidAllocator
/// On a server with ~500 players, about 10,000,000 dynamic GUID's will be requested every 24hr
/// period.
#[derive(Debug, Clone)]
pub struct DynamicGuidAllocator {
    min: u32,
    max: u32,
    current: u32,
    name: String,
    /// `Queue<Tuple<DateTime, uint>>`.
    recycled_guids: VecDeque<(DotNetDateTime, u32)>,
    use_sequence_gap_exhausted_message_displayed: bool,
    /// `LinkedList<(uint start, uint end)>`: only its first node is ever changed or removed.
    available_ids: VecDeque<(u32, u32)>,
}

impl DynamicGuidAllocator {
    // ACE: GuidManager.DynamicGuidAllocator.DynamicGuidAllocator
    pub fn new(
        min: u32,
        max: u32,
        name: &str,
        unlimited_gaps: bool,
        shard: &mut dyn ShardGuidQueries,
    ) -> Self {
        // Read current value out of ShardDatabase
        let db_val = shard.get_max_guid_found_in_range(min, max);
        let current = start_from_db(min, max, db_val, name);

        // Get available ids in the form of sequence gaps
        let limit = if unlimited_gaps {
            u32::MAX
        } else {
            LIMIT_AVAILABLE_IDS_RETURNED_IN_GET_SEQUENCE_GAPS
        };
        let available_ids: VecDeque<(u32, u32)> = shard
            .get_sequence_gaps(ObjectGuid::DYNAMIC_MIN, limit)
            .into();
        let total = total_available(&available_ids);
        log::debug!(
            "{name} GUID Sequence gaps initialized with total availableIDs of {}",
            format(total, "N0")
        );

        DynamicGuidAllocator {
            min,
            max,
            current,
            name: name.to_owned(),
            recycled_guids: VecDeque::new(),
            use_sequence_gap_exhausted_message_displayed: false,
            available_ids,
        }
    }

    // ACE: GuidManager.DynamicGuidAllocator.Alloc
    /// `utc_now` is `DateTime.UtcNow`. ACE-BUG: the same `current == max` test as
    /// `PlayerGuidAllocator.Alloc`.
    pub fn alloc(&mut self, utc_now: DotNetDateTime) -> u32 {
        // First, try to use a recycled Guid
        if let Some(&(time, guid)) = self.recycled_guids.front() {
            if utc_now - time > recycle_time() {
                self.recycled_guids.pop_front();
                return guid;
            }
        }

        // Second, try to use a known available Guid
        if let Some(first) = self.available_ids.front_mut() {
            let id = first.0;

            if first.0 == first.1 {
                self.available_ids.pop_front();
            } else {
                *first = (first.0.wrapping_add(1), first.1);
            }

            return id;
        } else if !self.use_sequence_gap_exhausted_message_displayed {
            log::debug!(
                "{} GUID Sequence gaps exhausted. Any new, non-recycled GUID will be current + 1. current is now {:08X}",
                self.name,
                self.current
            );
            self.use_sequence_gap_exhausted_message_displayed = true;
        }

        // Lastly, use an id that increments our max
        alloc_current(&mut self.current, self.max, &self.name)
    }

    // ACE: GuidManager.DynamicGuidAllocator.Current
    /// For information purposes only: the id used once recycled and gap ids run out.
    pub fn current(&self) -> u32 {
        self.current
    }

    // ACE: GuidManager.DynamicGuidAllocator.Min
    pub fn min(&self) -> u32 {
        self.min
    }

    // ACE: GuidManager.DynamicGuidAllocator.Max
    pub fn max(&self) -> u32 {
        self.max
    }

    // ACE: GuidManager.DynamicGuidAllocator.SequenceGapPairsTotal
    pub fn sequence_gap_pairs_total(&self) -> usize {
        self.available_ids.len()
    }

    // ACE: GuidManager.DynamicGuidAllocator.SequenceGapTotalAvailable
    pub fn sequence_gap_total_available(&self) -> u32 {
        total_available(&self.available_ids)
    }

    // ACE: GuidManager.DynamicGuidAllocator.RecycledGuidsTotal
    pub fn recycled_guids_total(&self) -> usize {
        self.recycled_guids.len()
    }

    // ACE: GuidManager.DynamicGuidAllocator.Recycle
    pub fn recycle(&mut self, guid: u32, utc_now: DotNetDateTime) {
        self.recycled_guids.push_back((utc_now, guid));
    }

    // ACE: GuidManager.DynamicGuidAllocator.ToString
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        format!(
            "DynamicGuidAllocator: {}, current: 0x{:08X}, max: 0x{:08X}, sequence gap GUIDs available: {}, recycled GUIDs available: {}",
            self.name,
            self.current,
            self.max,
            format(total_available(&self.available_ids), "N0"),
            format(self.recycled_guids.len(), "N0")
        )
    }

    // ACE: GuidManager.DynamicGuidAllocator.GetRecycleDebugInfo
    /// `(nextRecycleTime, totalPendingRecycledGuids, totalSequenceGapGuids)`.
    pub fn get_recycle_debug_info(&self) -> (DotNetDateTime, usize, u32) {
        let mut next_recycle_time = DotNetDateTime::MIN_VALUE;

        if let Some(&(time, _)) = self.recycled_guids.front() {
            next_recycle_time = time + recycle_time();
        }

        (
            next_recycle_time,
            self.recycled_guids.len(),
            total_available(&self.available_ids),
        )
    }
}

/// The mutable static state of ACE's `GuidManager`, held as a field of `World`.
#[derive(Debug, Default)]
pub struct GuidManagerState {
    /// `playerAlloc`; `None` until [`initialize`].
    pub player_alloc: Option<PlayerGuidAllocator>,
    /// `dynamicAlloc`; `None` until [`initialize`].
    pub dynamic_alloc: Option<DynamicGuidAllocator>,
}

/// `PropertyManager.GetBool("unlimited_sequence_gaps").Item`.
fn property_manager_get_bool_unlimited_sequence_gaps(w: &World) -> bool {
    crate::managers::property_manager::get_bool(w, "unlimited_sequence_gaps", false, true).item
}

// ACE: GuidManager.Initialize
/// `shard` stands for `DatabaseManager.Shard`.
pub fn initialize(w: &mut World, shard: &mut dyn ShardGuidQueries) {
    w.guid_manager.player_alloc = Some(PlayerGuidAllocator::new(
        ObjectGuid::PLAYER_MIN,
        ObjectGuid::PLAYER_MAX,
        "player",
        shard,
    ));
    let unlimited_gaps = property_manager_get_bool_unlimited_sequence_gaps(w);
    w.guid_manager.dynamic_alloc = Some(DynamicGuidAllocator::new(
        ObjectGuid::DYNAMIC_MIN,
        ObjectGuid::DYNAMIC_MAX,
        "dynamic",
        unlimited_gaps,
        shard,
    ));
}

/// `playerAlloc`, dereferenced as ACE does (a `NullReferenceException` before `Initialize`).
fn player_alloc(w: &mut World) -> &mut PlayerGuidAllocator {
    w.guid_manager
        .player_alloc
        .as_mut()
        .expect("NullReferenceException: GuidManager.Initialize has not run")
}

/// `dynamicAlloc`, dereferenced as ACE does.
fn dynamic_alloc(w: &mut World) -> &mut DynamicGuidAllocator {
    w.guid_manager
        .dynamic_alloc
        .as_mut()
        .expect("NullReferenceException: GuidManager.Initialize has not run")
}

// ACE: GuidManager.NewPlayerGuid
/// Returns New Player Guid
pub fn new_player_guid(w: &mut World) -> ObjectGuid {
    ObjectGuid::new(player_alloc(w).alloc())
}

// ACE: GuidManager.NewDynamicGuid
/// These represent items are generated in the world. Some of them will be saved to the Shard db.
/// They can be monsters, loot, etc..
///
/// DIVERGE: a guid whose object is still in `World.objects` is not handed out. Under V155 a
/// destroyed object a live container still lists (destroyed without leaving it) stays
/// in the store after `Destroy` recycled its guid; ACE reissues that guid after `recycleTime` and
/// then has two objects with one guid, which the store cannot hold (V162). Such a guid is
/// recycled again with the current time (its object already recycled it and never will again),
/// and the allocation goes on in ACE's order. A guid of a live, undestroyed object (reachable only
/// when an object was inserted under a guid the allocator did not issue) is skipped without
/// recycling, since its own `Destroy` recycles it. Each skip either re-queues behind a time not
/// yet due or consumes a gap or `current`, so the loop ends.
pub fn new_dynamic_guid(w: &mut World) -> ObjectGuid {
    let now = w.now.utc;
    loop {
        let guid = ObjectGuid::new(dynamic_alloc(w).alloc(now));
        match w.objects.get(guid) {
            Some(kept) if kept.wo.world_object.is_destroyed => {
                dynamic_alloc(w).recycle(guid.full(), now)
            }
            Some(_) => {}
            None => return guid,
        }
    }
}

// ACE: GuidManager.RecycleDynamicGuid
/// Guid will be added to the recycle queue, and available for use after `recycleTime`.
pub fn recycle_dynamic_guid(w: &mut World, guid: ObjectGuid) {
    let now = w.now.utc;
    dynamic_alloc(w).recycle(guid.full(), now);
}

// ACE: GuidManager.GetDynamicGuidDebugInfo
pub fn get_dynamic_guid_debug_info(w: &mut World) -> String {
    dynamic_alloc(w).to_string()
}

// ACE: GuidManager.GetIdListCommandOutput
pub fn get_id_list_command_output(w: &mut World) -> String {
    let player_guid_current = player_alloc(w).current();
    let dynamic_guid_current = dynamic_alloc(w).current();
    let (next_recycle_time, total_pending_recycled_guids, total_sequence_gap_guids) =
        dynamic_alloc(w).get_recycle_debug_info();

    let mut message = format!(
        "The next Player GUID to be allocated is expected to be: 0x{player_guid_current:X}\n"
    );

    let gaps = format(total_sequence_gap_guids, "N0");
    let pending = format(total_pending_recycled_guids, "N0");

    if next_recycle_time == DotNetDateTime::MIN_VALUE {
        message += &format!(
            "After {gaps} sequence gap ids have been consumed, and {pending} recycled ids have been consumed, the next id will be {dynamic_guid_current:08X}"
        );
    } else {
        let next_dynamic_is_avail_in = next_recycle_time - w.now.utc;

        if next_dynamic_is_avail_in.total_seconds() <= 0.0 {
            message += &format!(
                "After {gaps} sequence gap ids have been consumed, and {pending} recycled ids have been consumed, the next of which are available now, the next id will be: 0x{dynamic_guid_current:08X}"
            );
        } else {
            let minutes = format(next_dynamic_is_avail_in.total_minutes(), "N1");
            message += &format!(
                "After {gaps} sequence gap ids have been consumed, and {pending} recycled ids have been consumed, the next of which is available in {minutes} m, the next id will be: 0x{dynamic_guid_current:08X}"
            );
        }
    }

    message
}

// ACE: GuidManager.PlayerMin
pub fn player_min(w: &World) -> u32 {
    w.guid_manager
        .player_alloc
        .as_ref()
        .map_or(0, PlayerGuidAllocator::min)
}

// ACE: GuidManager.PlayerCurrent
pub fn player_current(w: &World) -> u32 {
    w.guid_manager
        .player_alloc
        .as_ref()
        .map_or(0, PlayerGuidAllocator::current)
}

// ACE: GuidManager.PlayerMax
pub fn player_max(w: &World) -> u32 {
    w.guid_manager
        .player_alloc
        .as_ref()
        .map_or(0, PlayerGuidAllocator::max)
}

// ACE: GuidManager.DynamicMin
pub fn dynamic_min(w: &World) -> u32 {
    w.guid_manager
        .dynamic_alloc
        .as_ref()
        .map_or(0, DynamicGuidAllocator::min)
}

// ACE: GuidManager.DynamicCurrent
pub fn dynamic_current(w: &World) -> u32 {
    w.guid_manager
        .dynamic_alloc
        .as_ref()
        .map_or(0, DynamicGuidAllocator::current)
}

// ACE: GuidManager.DynamicMax
pub fn dynamic_max(w: &World) -> u32 {
    w.guid_manager
        .dynamic_alloc
        .as_ref()
        .map_or(0, DynamicGuidAllocator::max)
}

// ACE: GuidManager.SequenceGapPairsTotal
pub fn sequence_gap_pairs_total(w: &World) -> usize {
    w.guid_manager
        .dynamic_alloc
        .as_ref()
        .map_or(0, DynamicGuidAllocator::sequence_gap_pairs_total)
}

// ACE: GuidManager.RecycledGuidsTotal
pub fn recycled_guids_total(w: &World) -> usize {
    w.guid_manager
        .dynamic_alloc
        .as_ref()
        .map_or(0, DynamicGuidAllocator::recycled_guids_total)
}
