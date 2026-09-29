// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/TreasureWieldedTable.cs
//! Port of `Source/ACE.Server/Entity/TreasureWieldedTable.cs`.

use empyrean_content::models::world::TreasureWielded;

use crate::entity::treasure_wielded_set::TreasureWieldedSet;

// ACE: TreasureWieldedTable
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TreasureWieldedTable {
    // ACE: TreasureWieldedTable.Sets
    pub sets: Vec<TreasureWieldedSet>,
}

impl TreasureWieldedTable {
    // ACE: TreasureWieldedTable.TotalNestedSets
    #[must_use]
    pub fn total_nested_sets(&self) -> i32 {
        let mut cnt = 0;

        for set in &self.sets {
            cnt += set.total_nested_sets();
        }

        cnt
    }

    // ACE: TreasureWieldedTable.MaxDepth
    #[must_use]
    pub fn max_depth(&self) -> i32 {
        let mut max_depth = 0;

        for set in &self.sets {
            let depth = set.get_max_depth(0);

            if depth > max_depth {
                max_depth = depth;
            }
        }
        max_depth
    }

    // ACE: TreasureWieldedTable.TreasureWieldedTable
    /// Splits a creature's WieldedTreasure rows into its top-level sets (each starting at a
    /// SetStart row); a row outside any set is reported and skipped.
    #[must_use]
    pub fn new(items: &[TreasureWielded]) -> Self {
        let mut sets = Vec::new();

        let mut idx = 0usize;
        while idx < items.len() {
            let item = &items[idx];
            if item.set_start {
                let current_set = TreasureWieldedSet::new(items, idx, 0);
                let total_nested_items = current_set.total_nested_items();
                sets.push(current_set);
                // `idx += totalNestedItems - 1`, then the loop's `idx++`
                idx = idx
                    .wrapping_add_signed(isize::try_from(total_nested_items - 1).expect("an int"));
            } else {
                empyrean_common::console_write_line!(
                    "Warning: started parsing set with no SetStart on line {idx}"
                );
            }
            idx += 1;
        }
        Self { sets }
    }
}
