// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Set.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Set.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/WorldObject_Set.cs`.

use empyrean_entity::enums::{EquipmentSet, ItemXpStyle, PropertyInt, PropertyInt64};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: WorldObject.ItemBaseXp
    pub fn item_base_xp(&self) -> Option<i64> {
        self.get_property(PropertyInt64::ItemBaseXp)
    }

    // ACE: WorldObject.ItemBaseXp
    pub fn set_item_base_xp(&mut self, value: Option<i64>) {
        match value {
            None => self.remove_property(PropertyInt64::ItemBaseXp),
            Some(v) => self.set_property(PropertyInt64::ItemBaseXp, v),
        }
    }

    // ACE: WorldObject.ItemMaxLevel
    pub fn item_max_level(&self) -> Option<i32> {
        self.get_property(PropertyInt::ItemMaxLevel)
    }

    // ACE: WorldObject.ItemMaxLevel
    pub fn set_item_max_level(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::ItemMaxLevel),
            Some(v) => self.set_property(PropertyInt::ItemMaxLevel, v),
        }
    }

    // ACE: WorldObject.ItemTotalXp
    pub fn item_total_xp(&self) -> Option<i64> {
        self.get_property(PropertyInt64::ItemTotalXp)
    }

    // ACE: WorldObject.ItemTotalXp
    pub fn set_item_total_xp(&mut self, value: Option<i64>) {
        match value {
            None => self.remove_property(PropertyInt64::ItemTotalXp),
            Some(v) => self.set_property(PropertyInt64::ItemTotalXp, v),
        }
    }

    // ACE: WorldObject.ItemXpStyle
    pub fn item_xp_style(&self) -> Option<ItemXpStyle> {
        self.get_property(PropertyInt::ItemXpStyle).map(ItemXpStyle)
    }

    // ACE: WorldObject.ItemXpStyle
    pub fn set_item_xp_style(&mut self, value: Option<ItemXpStyle>) {
        match value {
            None => self.remove_property(PropertyInt::ItemXpStyle),
            Some(v) => self.set_property(PropertyInt::ItemXpStyle, v.0),
        }
    }

    // ACE: WorldObject.EquipmentSetId
    pub fn equipment_set_id(&self) -> Option<EquipmentSet> {
        self.get_property(PropertyInt::EquipmentSetId)
            .map(EquipmentSet)
    }

    // ACE: WorldObject.EquipmentSetId
    pub fn set_equipment_set_id(&mut self, value: Option<EquipmentSet>) {
        match value {
            None => self.remove_property(PropertyInt::EquipmentSetId),
            Some(v) => self.set_property(PropertyInt::EquipmentSetId, v.0),
        }
    }
}
