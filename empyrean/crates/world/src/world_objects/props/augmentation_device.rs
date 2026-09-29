// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/AugmentationDevice.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/AugmentationDevice.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/AugmentationDevice.cs`.

use empyrean_entity::enums::{PropertyInt, PropertyInt64};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: AugmentationDevice.AugmentationCost
    pub fn augmentation_cost(&self) -> Option<i64> {
        self.get_property(PropertyInt64::AugmentationCost)
    }

    // ACE: AugmentationDevice.AugmentationCost
    pub fn set_augmentation_cost(&mut self, value: Option<i64>) {
        match value {
            None => self.remove_property(PropertyInt64::AugmentationCost),
            Some(v) => self.set_property(PropertyInt64::AugmentationCost, v),
        }
    }

    // ACE: AugmentationDevice.AugmentationStat
    pub fn augmentation_stat(&self) -> Option<i32> {
        self.get_property(PropertyInt::AugmentationStat)
    }

    // ACE: AugmentationDevice.AugmentationStat
    pub fn set_augmentation_stat(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::AugmentationStat),
            Some(v) => self.set_property(PropertyInt::AugmentationStat, v),
        }
    }
}
