// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Hotspot.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Hotspot.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Hotspot.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyFloat, PropertyInt};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Hotspot.CycleTime
    pub fn cycle_time(&self) -> Option<f64> {
        self.get_property(PropertyFloat::HotspotCycleTime)
    }

    // ACE: Hotspot.CycleTime
    pub fn set_cycle_time(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::HotspotCycleTime),
            Some(v) => self.set_property(PropertyFloat::HotspotCycleTime, v),
        }
    }

    // ACE: Hotspot.CycleTimeVariance
    pub fn cycle_time_variance(&self) -> Option<f64> {
        Some(
            self.get_property(PropertyFloat::HotspotCycleTimeVariance)
                .unwrap_or(0.0),
        )
    }

    // ACE: Hotspot.CycleTimeVariance
    pub fn set_cycle_time_variance(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::HotspotCycleTimeVariance),
            Some(v) => self.set_property(PropertyFloat::HotspotCycleTimeVariance, v),
        }
    }

    // ACE: Hotspot._DamageType
    pub fn damage_type_raw(&self) -> Option<i32> {
        self.get_property(PropertyInt::DamageType)
    }

    // ACE: Hotspot._DamageType
    pub fn set_damage_type_raw(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::DamageType),
            Some(v) => self.set_property(PropertyInt::DamageType, v),
        }
    }

    // ACE: Hotspot.IsHot
    pub fn is_hot(&self) -> bool {
        self.get_property(PropertyBool::IsHot).unwrap_or(false)
    }

    // ACE: Hotspot.IsHot
    pub fn set_is_hot(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::IsHot);
        } else {
            self.set_property(PropertyBool::IsHot, value);
        }
    }

    // ACE: Hotspot.AffectsAis
    pub fn affects_ais(&self) -> bool {
        self.get_property(PropertyBool::AffectsAis).unwrap_or(false)
    }

    // ACE: Hotspot.AffectsAis
    pub fn set_affects_ais(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::AffectsAis);
        } else {
            self.set_property(PropertyBool::AffectsAis, value);
        }
    }
}
