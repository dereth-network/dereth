// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/SlumLord.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/SlumLord.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/SlumLord.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyInt};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: SlumLord.HouseRequiresMonarch
    pub fn house_requires_monarch(&self) -> bool {
        self.get_property(PropertyBool::HouseRequiresMonarch)
            .unwrap_or(false)
    }

    // ACE: SlumLord.HouseRequiresMonarch
    pub fn set_house_requires_monarch(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::HouseRequiresMonarch);
        } else {
            self.set_property(PropertyBool::HouseRequiresMonarch, value);
        }
    }

    // ACE: SlumLord.AllegianceMinLevel
    pub fn allegiance_min_level(&self) -> Option<i32> {
        self.get_property(PropertyInt::AllegianceMinLevel)
    }

    // ACE: SlumLord.AllegianceMinLevel
    pub fn set_allegiance_min_level(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::AllegianceMinLevel),
            Some(v) => self.set_property(PropertyInt::AllegianceMinLevel, v),
        }
    }
}
