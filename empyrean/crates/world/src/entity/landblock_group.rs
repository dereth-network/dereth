// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/LandblockGroup.cs
//! Port of `Source/ACE.Server/Entity/LandblockGroup.cs`.
//!
//! The idea behind the landblock groups are that each group may contain multiple landblocks that
//! must be ticked on the same thread, but each group itself can be ticked on independent threads.
//! This port ticks serially (V5), but ACE builds the groups with threading off too, their order is
//! the tick order, and gameplay compares `CurrentLandblockGroup`s (monster magic, projectiles).
//!
//! Landblocks are held by id (`HashSet<Landblock>` compares references, and one landblock is
//! loaded per id at a time) and read through the manager's [`LandblockTable`]. A group's identity
//! (ACE compares group references) is its [`LandblockGroupId`].

use std::fmt;

use empyrean_common::dotnet::datetime::{DotNetDateTime, TimeSpan};
use empyrean_common::dotnet::DotNetHashSet;
use empyrean_common::performance::rolling_amount_over_hits_tracker::RollingAmountOverHitsTracker;
use empyrean_entity::LandblockId;

use crate::entity::landblock;
use crate::entity::landblock_group_split_helper::{should_be_added, LandblockGroupSplitHelper};
use crate::managers::landblock_manager::LandblockTable;

// ACE: LandblockGroup.LandblockGroupMinSpacing
pub const LANDBLOCK_GROUP_MIN_SPACING: i32 = 4;

// ACE: LandblockGroup.LandblockGroupMinSpacingWhenDormant
pub const LANDBLOCK_GROUP_MIN_SPACING_WHEN_DORMANT: i32 = 3;

/// `LandblockGroup.TrySplitInterval` (`Landblock.UnloadInterval`).
pub fn try_split_interval() -> TimeSpan {
    landblock::unload_interval()
}

/// A group's identity: ACE compares `LandblockGroup` references. Unique for the life of the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LandblockGroupId(pub u64);

// ACE: LandblockGroup
#[derive(Debug)]
pub struct LandblockGroup {
    pub id: LandblockGroupId,

    // ACE: LandblockGroup.IsDungeon
    is_dungeon: bool,

    // ACE: LandblockGroup.NextTrySplitTime
    next_try_split_time: DotNetDateTime,

    landblocks: DotNetHashSet<LandblockId>,

    // ACE: LandblockGroup.XMin
    x_min: i32,
    // ACE: LandblockGroup.XMax
    x_max: i32,
    // ACE: LandblockGroup.YMin
    y_min: i32,
    // ACE: LandblockGroup.YMax
    y_max: i32,

    width: i32,
    height: i32,

    // ACE: LandblockGroup.TickPhysicsTracker
    pub tick_physics_tracker: RollingAmountOverHitsTracker,
    // ACE: LandblockGroup.TickMultiThreadedWorkTracker
    pub tick_multi_threaded_work_tracker: RollingAmountOverHitsTracker,
}

impl LandblockGroup {
    // ACE: LandblockGroup.LandblockGroup
    /// `new LandblockGroup()`. `utc_now` is `DateTime.UtcNow` for the `NextTrySplitTime` initializer.
    pub fn new(id: LandblockGroupId, utc_now: DotNetDateTime) -> Self {
        LandblockGroup {
            id,
            is_dungeon: false,
            next_try_split_time: utc_now + try_split_interval(),
            landblocks: DotNetHashSet::new(),
            x_min: i32::MAX,
            x_max: i32::MIN,
            y_min: i32::MAX,
            y_max: i32::MIN,
            width: 0,
            height: 0,
            tick_physics_tracker: RollingAmountOverHitsTracker::new(500),
            tick_multi_threaded_work_tracker: RollingAmountOverHitsTracker::new(500),
        }
    }

    // ACE: LandblockGroup.LandblockGroup
    /// `new LandblockGroup(landblock)`.
    pub fn with_landblock(
        id: LandblockGroupId,
        utc_now: DotNetDateTime,
        table: &mut LandblockTable,
        landblock: LandblockId,
    ) -> Self {
        let mut group = Self::new(id, utc_now);
        group.add(table, landblock);
        group
    }

    pub fn is_dungeon(&self) -> bool {
        self.is_dungeon
    }

    pub fn next_try_split_time(&self) -> DotNetDateTime {
        self.next_try_split_time
    }

    pub fn x_min(&self) -> i32 {
        self.x_min
    }

    pub fn x_max(&self) -> i32 {
        self.x_max
    }

    pub fn y_min(&self) -> i32 {
        self.y_min
    }

    pub fn y_max(&self) -> i32 {
        self.y_max
    }

    // ACE: LandblockGroup.Count
    pub fn count(&self) -> usize {
        self.landblocks.len()
    }

    // ACE: LandblockGroup.Contains
    pub fn contains(&self, landblock: LandblockId) -> bool {
        self.landblocks.contains(&landblock)
    }

    // ACE: LandblockGroup.Add
    pub fn add(&mut self, table: &mut LandblockTable, landblock: LandblockId) -> bool {
        let landblock_is_dungeon = table.expect_mut(landblock).is_dungeon();

        if !self.landblocks.is_empty() {
            if self.is_dungeon {
                log::error!(
                    "[LANDBLOCK GROUP] You cannot add a landblock ({landblock}) to a LandblockGroup that represents a single Dungeon Landblock"
                );
                return false;
            }

            if landblock_is_dungeon {
                log::error!("[LANDBLOCK GROUP] You cannot add a dungeon landblock ({landblock}) to an existing LandblockGroup");
                return false;
            }
        }

        if self.landblocks.insert(landblock) {
            table.expect_mut(landblock).current_landblock_group = Some(self.id);

            if self.landblocks.len() == 1 {
                self.is_dungeon = landblock_is_dungeon;
            }

            self.include(landblock);

            self.width = (self.x_max - self.x_min) + 1;
            self.height = (self.y_max - self.y_min) + 1;

            return true;
        }

        false
    }

    /// Widens the bounds to take in `landblock`.
    fn include(&mut self, landblock: LandblockId) {
        let x = i32::from(landblock.landblock_x());
        let y = i32::from(landblock.landblock_y());
        if x < self.x_min {
            self.x_min = x;
        }
        if x > self.x_max {
            self.x_max = x;
        }
        if y < self.y_min {
            self.y_min = y;
        }
        if y > self.y_max {
            self.y_max = y;
        }
    }

    // ACE: LandblockGroup.Remove
    pub fn remove(&mut self, table: &mut LandblockTable, landblock: LandblockId) -> bool {
        if self.landblocks.remove(&landblock) {
            if let Some(lb) = table.get_mut(landblock) {
                lb.current_landblock_group = None;
            }

            // Empty landblock groups will be discarded immediately
            if self.landblocks.is_empty() {
                return true;
            }

            // If this landblock is on the perimeter of the group, recalculate the boundaries (they may end up the same)
            let x = i32::from(landblock.landblock_x());
            let y = i32::from(landblock.landblock_y());
            if x == self.x_min || x == self.x_max || y == self.y_min || y == self.y_max {
                self.recalculate_boundaries();
            }

            return true;
        }

        false
    }

    // ACE: LandblockGroup.GetEnumerator
    /// The landblocks in `HashSet` enumeration order.
    pub fn iter(&self) -> impl Iterator<Item = &LandblockId> {
        self.landblocks.iter()
    }

    // ACE: LandblockGroup.RecalculateBoundaries
    fn recalculate_boundaries(&mut self) {
        self.x_min = i32::MAX;
        self.x_max = i32::MIN;
        self.y_min = i32::MAX;
        self.y_max = i32::MIN;

        let members: Vec<LandblockId> = self.landblocks.iter().copied().collect();
        for existing in members {
            self.include(existing);
        }

        self.width = (self.x_max - self.x_min) + 1;
        self.height = (self.y_max - self.y_min) + 1;
    }

    // ACE: LandblockGroup.DoTrySplit
    fn do_try_split(
        &mut self,
        table: &mut LandblockTable,
        next_group_id: &mut u64,
        utc_now: DotNetDateTime,
    ) -> Option<LandblockGroup> {
        let mut landblock_group_split_helper = LandblockGroupSplitHelper::new();

        let mut remaining_landblocks: Vec<LandblockId> = self.landblocks.iter().copied().collect();

        let last = remaining_landblocks
            .pop()
            .expect("ArgumentOutOfRangeException: DoTrySplit on an empty group");
        landblock_group_split_helper.add(last);

        loop {
            // doAnotherPass:
            let mut needs_another_pass = false;

            for i in (0..remaining_landblocks.len()).rev() {
                if landblock_group_split_helper
                    .should_be_added_to_this_landblock_group(table, remaining_landblocks[i])
                {
                    landblock_group_split_helper.add(remaining_landblocks[i]);
                    remaining_landblocks.remove(i);
                    needs_another_pass = true;
                }
            }

            if !needs_another_pass {
                break;
            }
        }

        // If they're the same size, there's no split possible
        if self.count() == landblock_group_split_helper.count() {
            return None;
        }

        // Split was a success
        let mut new_landblock_group =
            LandblockGroup::new(allocate_group_id(next_group_id), utc_now);

        for &landblock in landblock_group_split_helper.iter() {
            // Remove the split landblocks. Do this manually, not through the public Remove() function
            self.landblocks.remove(&landblock);

            // Add them through the proper .Add() method to the new LandblockGroup
            new_landblock_group.add(table, landblock);
        }

        self.recalculate_boundaries();

        // This can result in returning groups that overlap this ones boundary.
        // However, that isn't a problem for processing them on separate threads.
        // In the event a new landblock is added that is within range of both this block and any new block, they will be recombined at that point.

        Some(new_landblock_group)
    }

    // ACE: LandblockGroup.TrySplit
    /// `None` if no split was possible (a dungeon group). Otherwise the new groups, whose
    /// landblocks have been removed from this one; the caller must keep them.
    pub fn try_split(
        &mut self,
        table: &mut LandblockTable,
        next_group_id: &mut u64,
        utc_now: DotNetDateTime,
    ) -> Option<Vec<LandblockGroup>> {
        if self.is_dungeon {
            return None;
        }

        let mut results = Vec::new();

        let mut new_landblock_group = self.do_try_split(table, next_group_id, utc_now);

        while let Some(group) = new_landblock_group {
            results.push(group);

            new_landblock_group = self.do_try_split(table, next_group_id, utc_now);
        }

        // If we have a very large landblock group that didn't split, we'll try to split it every 1 minute to help reduce server load
        if results.is_empty() && self.landblocks.len() >= 200 {
            self.next_try_split_time = utc_now.add_minutes(1.0);
        } else {
            self.next_try_split_time = utc_now + try_split_interval();
        }

        Some(results)
    }

    // ACE: LandblockGroup.TryThrottledSplit
    /// As [`try_split`](Self::try_split), but only once `NextTrySplitTime` has passed.
    pub fn try_throttled_split(
        &mut self,
        table: &mut LandblockTable,
        next_group_id: &mut u64,
        utc_now: DotNetDateTime,
    ) -> Option<Vec<LandblockGroup>> {
        if self.next_try_split_time > utc_now {
            return None;
        }

        self.try_split(table, next_group_id, utc_now)
    }

    // ACE: LandblockGroup.ShouldBeAddedToThisLandblockGroup
    pub fn should_be_added_to_this_landblock_group(
        &self,
        table: &LandblockTable,
        landblock: LandblockId,
    ) -> bool {
        should_be_added(self.landblocks.iter(), table, landblock)
    }
}

/// Hands out the next [`LandblockGroupId`].
pub fn allocate_group_id(next_group_id: &mut u64) -> LandblockGroupId {
    *next_group_id += 1;
    LandblockGroupId(*next_group_id)
}

// ACE: LandblockGroup.ToString
impl fmt::Display for LandblockGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "x: 0x{:02X} - 0x{:02X}, y: 0x{:02X} - 0x{:02X}, w: {:>3}, h: {:>3}, Count: {:>4}",
            self.x_min,
            self.x_max,
            self.y_min,
            self.y_max,
            self.width,
            self.height,
            self.count()
        )
    }
}
