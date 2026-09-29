// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Entity/CreateList.cs
//! Port of `Source/ACE.Server/Entity/CreateList.cs`: a create list split into its treasure sets
//! (runs of Treasure items with a probability, in 0-1 chunks).

use empyrean_entity::enums::DestinationType;
use empyrean_entity::models::PropertiesCreateList;

use crate::entity::create_list_set::CreateListSet;
use crate::entity::create_list_set_modifier::CreateListSetModifier;

// ACE: CreateList
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateList {
    // ACE: CreateList.Items
    pub items: Vec<PropertiesCreateList>,
    /// Only for items w/ Treasure flag and probability.
    // ACE: CreateList.Sets
    pub sets: Vec<CreateListSet>,
    /// Item index -> set index (-1 for an item in no set).
    // ACE: CreateList.ItemSets
    pub item_sets: Vec<i32>,
}

impl CreateList {
    // ACE: CreateList.CreateList
    #[must_use]
    pub fn new(create_list: &[PropertiesCreateList]) -> Self {
        let mut this = Self {
            items: create_list.to_vec(),
            sets: Vec::new(),
            item_sets: Vec::new(),
        };

        this.build_sets(create_list);
        this
    }

    // ACE: CreateList.BuildSets
    pub fn build_sets(&mut self, create_list: &[PropertiesCreateList]) {
        self.sets = Vec::new();
        self.item_sets = Vec::new();
        let mut set_idx: i32 = -1;

        let mut total_probability = 0.0f32;

        for item in create_list {
            let destination_type = item.destination_type;
            let use_rng = (destination_type & DestinationType::Treasure)
                == DestinationType::Treasure
                && item.shade != 0.0;

            let shade_or_probability = item.shade;

            if use_rng {
                // handle sets in 0-1 chunks
                if total_probability == 0.0 || total_probability >= 1.0 {
                    total_probability = 0.0;
                    self.sets.push(CreateListSet::new());
                    set_idx += 1;
                }

                let probability = shade_or_probability;

                total_probability += probability;

                // currentSet.Add(item): the last set added
                self.sets
                    .last_mut()
                    .expect("a set was started")
                    .add(item.clone());
                self.item_sets.push(set_idx);
            } else {
                self.item_sets.push(-1);
            }
        }
    }

    /// # Panics
    /// When `idx` is not in a set (ACE's `ArgumentOutOfRangeException` on `Sets[-1]`).
    // ACE: CreateList.GetSetModifier
    #[must_use]
    pub fn get_set_modifier(&self, idx: usize, modifier: f32) -> CreateListSetModifier {
        let set_index = usize::try_from(self.item_sets[idx])
            .expect("ACE: ArgumentOutOfRangeException (Sets[-1])");
        let set = self.sets[set_index].clone();

        CreateListSetModifier::new(set, modifier)
    }
}
