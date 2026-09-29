// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/ManaStone.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/ManaStone.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/ManaStone.cs`.

use empyrean_entity::enums::PropertyFloat;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: ManaStone.Efficiency
    pub fn efficiency(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ItemEfficiency)
    }

    // ACE: ManaStone.Efficiency
    pub fn set_efficiency(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ItemEfficiency),
            Some(v) => self.set_property(PropertyFloat::ItemEfficiency, v),
        }
    }

    // ACE: ManaStone.DestroyChance
    pub fn destroy_chance(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ManaStoneDestroyChance)
    }

    // ACE: ManaStone.DestroyChance
    pub fn set_destroy_chance(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ManaStoneDestroyChance),
            Some(v) => self.set_property(PropertyFloat::ManaStoneDestroyChance, v),
        }
    }
}
