// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Equipment.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Equipment.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Creature_Equipment.cs`.

use empyrean_entity::enums::PropertyDataId;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Creature.WieldedTreasureType
    pub fn wielded_treasure_type(&self) -> Option<u32> {
        self.get_property(PropertyDataId::WieldedTreasureType)
    }

    // ACE: Creature.WieldedTreasureType
    pub fn set_wielded_treasure_type(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::WieldedTreasureType),
            Some(v) => self.set_property(PropertyDataId::WieldedTreasureType, v),
        }
    }

    // ACE: Creature.InventoryTreasureType
    pub fn inventory_treasure_type(&self) -> Option<u32> {
        self.get_property(PropertyDataId::InventoryTreasureType)
    }

    // ACE: Creature.InventoryTreasureType
    pub fn set_inventory_treasure_type(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::InventoryTreasureType),
            Some(v) => self.set_property(PropertyDataId::InventoryTreasureType, v),
        }
    }
}
