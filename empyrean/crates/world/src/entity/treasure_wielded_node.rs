// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/TreasureWieldedNode.cs
//! Port of `Source/ACE.Server/Entity/TreasureWieldedNode.cs` (ported with `TreasureWieldedSet`,
//! as the two types are mutually recursive).

use empyrean_content::models::world::TreasureWielded;

use crate::entity::treasure_wielded_set::TreasureWieldedSet;

// ACE: TreasureWieldedNode
#[derive(Debug, Clone, PartialEq)]
pub struct TreasureWieldedNode {
    pub item: TreasureWielded,
    pub subset: Option<Box<TreasureWieldedSet>>,
}

impl TreasureWieldedNode {
    // ACE: TreasureWieldedNode.TreasureWieldedNode
    #[must_use]
    pub fn new(items: &[TreasureWielded], idx: usize) -> Self {
        Self {
            item: items[idx].clone(),
            subset: None,
        }
    }

    // ACE: TreasureWieldedNode.TotalNestedItems
    #[must_use]
    pub fn total_nested_items(&self) -> i32 {
        let mut total_items = 1;

        if let Some(subset) = &self.subset {
            total_items += subset.total_nested_items();
        }

        total_items
    }

    // ACE: TreasureWieldedNode.TotalNestedSets
    #[must_use]
    pub fn total_nested_sets(&self) -> i32 {
        self.subset.as_ref().map_or(0, |s| s.total_nested_sets())
    }

    // ACE: TreasureWieldedNode.GetMaxDepth
    #[must_use]
    pub fn get_max_depth(&self, depth: i32) -> i32 {
        let subset_depth = self.subset.as_ref().map_or(0, |s| s.get_max_depth(depth));
        depth + subset_depth + 1
    }
}
