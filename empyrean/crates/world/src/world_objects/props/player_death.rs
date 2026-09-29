// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Death.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Player_Death.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Player_Death.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyFloat};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Player.UnderLifestoneProtection
    pub fn under_lifestone_protection(&self) -> bool {
        self.get_property(PropertyBool::UnderLifestoneProtection)
            .unwrap_or(false)
    }

    // ACE: Player.UnderLifestoneProtection
    pub fn set_under_lifestone_protection(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::UnderLifestoneProtection);
        } else {
            self.set_property(PropertyBool::UnderLifestoneProtection, value);
        }
    }

    // ACE: Player.LifestoneProtectionTimestamp
    pub fn lifestone_protection_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::LifestoneProtectionTimestamp)
    }

    // ACE: Player.LifestoneProtectionTimestamp
    pub fn set_lifestone_protection_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::LifestoneProtectionTimestamp),
            Some(v) => self.set_property(PropertyFloat::LifestoneProtectionTimestamp, v),
        }
    }
}
