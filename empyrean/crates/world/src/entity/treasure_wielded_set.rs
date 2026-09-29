// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/TreasureWieldedSet.cs
//! Port of `Source/ACE.Server/Entity/TreasureWieldedSet.cs`.

use empyrean_content::models::world::TreasureWielded;

use crate::entity::treasure_wielded_node::TreasureWieldedNode;

// ACE: TreasureWieldedSet
/// Represents 1 TreasureWielded set from a TreasuredWielded table. A roll is made for each set,
/// from 0-TotalProbability (min. 1).
#[derive(Debug, Clone, PartialEq)]
pub struct TreasureWieldedSet {
    /// The list of item directly in this set. Each item in turn can link to a subset.
    pub items: Vec<TreasureWieldedNode>,
}

impl TreasureWieldedSet {
    // ACE: TreasureWieldedSet.TotalNestedItems
    /// Returns the total nested items from set items + subsets.
    #[must_use]
    pub fn total_nested_items(&self) -> i32 {
        let mut total_items = 0;

        for item in &self.items {
            total_items += item.total_nested_items();
        }

        total_items
    }

    // ACE: TreasureWieldedSet.TotalNestedSets
    /// Returns the total number of nested sets, including item subsets.
    #[must_use]
    pub fn total_nested_sets(&self) -> i32 {
        let mut total_sets = 1;

        for item in &self.items {
            total_sets += item.total_nested_sets();
        }

        total_sets
    }

    // ACE: TreasureWieldedSet.GetMaxDepth
    /// Returns the maximum depth of the nested sets (`setDepth` defaults to 0 in ACE).
    #[must_use]
    pub fn get_max_depth(&self, set_depth: i32) -> i32 {
        let mut max_depth = set_depth;

        for item in &self.items {
            let depth = item.get_max_depth(set_depth);

            if depth > max_depth {
                max_depth = depth;
            }
        }
        max_depth
    }

    // ACE: TreasureWieldedSet.TotalProbability
    /// Returns the total probability of the direct items, minimum 100%.
    #[must_use]
    pub fn total_probability(&self) -> f32 {
        let mut total_probability = 0.0f32;

        for item in &self.items {
            total_probability += item.item.probability;
        }

        empyrean_common::dotnet::math::max_f32(total_probability, 1.0)
    }

    // ACE: TreasureWieldedSet.TreasureWieldedSet
    /// Parses 1 TreasureWielded set from a TreasuredWielded table: `items` is the complete table,
    /// `start_idx` the start index of the set to parse (`depth` defaults to 0 in ACE).
    #[must_use]
    pub fn new(items: &[TreasureWielded], start_idx: usize, depth: i32) -> Self {
        let mut this = Self { items: Vec::new() };

        // add set start item
        let mut item = &items[start_idx];

        if !item.set_start {
            empyrean_common::console_write_line!("Warning: TreasureWieldedSet constructor called with startIdx {start_idx}, and SetStart=0");
        }

        // `node` is ACE's reference to the last node added: the index of it in `this.items`.
        this.items.push(TreasureWieldedNode::new(items, start_idx));
        let mut node = this.items.len() - 1;
        //Console.WriteLine($"Parsing startSet item {item.WeenieClassId} @ idx {startIdx}, depth {depth}");

        // continue parsing
        let mut idx = start_idx + 1;
        while idx < items.len() {
            let prev_item = &items[idx - 1];
            item = &items[idx];

            // handle subsets
            if prev_item.has_sub_set {
                if item.set_start {
                    let subset = TreasureWieldedSet::new(items, idx, depth + 1);
                    let total_items = subset.total_nested_items();
                    this.items[node].subset = Some(Box::new(subset));
                    // `idx += totalItems - 1` (a subset always holds at least its start item)
                    idx = idx.wrapping_add_signed((total_items - 1) as isize);
                } else {
                    empyrean_common::console_write_line!(
                        "Warning: Subset detected on line {}, but next idx is not a set start",
                        idx - 1
                    );
                }
            } else {
                // handle regular items
                if item.set_start {
                    if item.continues_previous_set {
                        // ensure subset parsed
                        if this.items[node].subset.is_some() {
                            // why is this here? causing bugs with 178..
                            this.items.push(TreasureWieldedNode::new(items, idx));
                            node = this.items.len() - 1;
                        } else if depth == 0 {
                            empyrean_common::console_write_line!("Warning: continuing a previous set on line {idx}, but no subset found!");
                        } else {
                            break; // back to parent
                        }
                    } else {
                        // set completed
                        break;
                    }
                } else {
                    // normal set continues...
                    this.items.push(TreasureWieldedNode::new(items, idx));
                    node = this.items.len() - 1;
                }
            }
            idx += 1;
        }

        this
    }
}
