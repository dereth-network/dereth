// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/LandblockGroupSplitHelper.cs
//! Port of `Source/ACE.Server/Entity/LandblockGroupSplitHelper.cs`.
//!
//! Landblocks are held by id (`HashSet<Landblock>` compares references; one landblock is loaded
//! per id at a time) and read through the manager's table.

use empyrean_common::dotnet::DotNetHashSet;
use empyrean_entity::LandblockId;

use crate::entity::landblock_group::{
    LANDBLOCK_GROUP_MIN_SPACING, LANDBLOCK_GROUP_MIN_SPACING_WHEN_DORMANT,
};
use crate::managers::landblock_manager::LandblockTable;

// ACE: LandblockGroupSplitHelper
#[derive(Debug, Default)]
pub struct LandblockGroupSplitHelper {
    landblocks: DotNetHashSet<LandblockId>,
}

impl LandblockGroupSplitHelper {
    pub fn new() -> Self {
        Self::default()
    }

    // ACE: LandblockGroupSplitHelper.Count
    pub fn count(&self) -> usize {
        self.landblocks.len()
    }

    // ACE: LandblockGroupSplitHelper.Add
    pub fn add(&mut self, landblock: LandblockId) {
        self.landblocks.insert(landblock);
    }

    // ACE: LandblockGroupSplitHelper.GetEnumerator
    pub fn iter(&self) -> impl Iterator<Item = &LandblockId> {
        self.landblocks.iter()
    }

    // ACE: LandblockGroupSplitHelper.ShouldBeAddedToThisLandblockGroup
    pub fn should_be_added_to_this_landblock_group(
        &self,
        table: &LandblockTable,
        landblock: LandblockId,
    ) -> bool {
        should_be_added(self.landblocks.iter(), table, landblock)
    }
}

/// The body both `ShouldBeAddedToThisLandblockGroup`s share: within the minimum spacing of any
/// member (a smaller spacing when either landblock is dormant).
pub(crate) fn should_be_added<'a>(
    members: impl Iterator<Item = &'a LandblockId>,
    table: &LandblockTable,
    landblock: LandblockId,
) -> bool {
    let landblock_is_dormant = table.expect(landblock).is_dormant;

    for &value in members {
        let distance = (i32::from(value.landblock_x()) - i32::from(landblock.landblock_x()))
            .abs()
            .max((i32::from(value.landblock_y()) - i32::from(landblock.landblock_y())).abs());

        if table.expect(value).is_dormant || landblock_is_dormant {
            if distance < LANDBLOCK_GROUP_MIN_SPACING_WHEN_DORMANT {
                return true;
            }
        } else if distance < LANDBLOCK_GROUP_MIN_SPACING {
            return true;
        }
    }

    false
}
