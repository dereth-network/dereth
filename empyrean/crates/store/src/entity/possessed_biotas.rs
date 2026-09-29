// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Entity/PossessedBiotas.cs
//! `PossessedBiotas`: a character's inventory and wielded items, as database biotas.

use crate::models::shard::Biota;

// ACE: PossessedBiotas
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PossessedBiotas {
    // ACE: PossessedBiotas.Inventory
    pub inventory: Vec<Biota>,
    // ACE: PossessedBiotas.WieldedItems
    pub wielded_items: Vec<Biota>,
}

impl PossessedBiotas {
    // ACE: PossessedBiotas.PossessedBiotas
    #[must_use]
    pub fn new(inventory: Vec<Biota>, wielded_items: Vec<Biota>) -> Self {
        let mut result = Self::default();
        result.inventory.extend(inventory);
        result.wielded_items.extend(wielded_items);
        result
    }
}
