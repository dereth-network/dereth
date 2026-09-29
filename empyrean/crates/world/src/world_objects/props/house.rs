// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/House.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/House.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/House.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyInt};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: House.HouseHooksVisible
    pub fn house_hooks_visible(&self) -> Option<bool> {
        self.get_property(PropertyBool::HouseHooksVisible)
    }

    // ACE: House.HouseHooksVisible
    pub fn set_house_hooks_visible(&mut self, value: Option<bool>) {
        match value {
            None => self.remove_property(PropertyBool::HouseHooksVisible),
            Some(v) => self.set_property(PropertyBool::HouseHooksVisible, v),
        }
    }

    // ACE: House.OpenToEveryone
    pub fn open_to_everyone(&self) -> bool {
        self.get_property(PropertyInt::OpenToEveryone).unwrap_or(0) == 1
    }

    // ACE: House.OpenToEveryone
    pub fn set_open_to_everyone(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyInt::OpenToEveryone);
        } else {
            self.set_property(PropertyInt::OpenToEveryone, 1);
        }
    }

    // ACE: House.HouseMaxHooksUsable
    pub fn house_max_hooks_usable(&self) -> i32 {
        self.get_property(PropertyInt::HouseMaxHooksUsable)
            .unwrap_or(25)
    }

    // ACE: House.HouseMaxHooksUsable
    pub fn set_house_max_hooks_usable(&mut self, value: i32) {
        if value == 25 {
            self.remove_property(PropertyInt::HouseMaxHooksUsable);
        } else {
            self.set_property(PropertyInt::HouseMaxHooksUsable, value);
        }
    }

    // ACE: House.HouseCurrentHooksUsable
    pub fn house_current_hooks_usable(&self) -> i32 {
        self.get_property(PropertyInt::HouseCurrentHooksUsable)
            .unwrap_or_else(|| self.house_max_hooks_usable())
    }

    // ACE: House.HouseCurrentHooksUsable
    pub fn set_house_current_hooks_usable(&mut self, value: i32) {
        if value == self.house_max_hooks_usable() {
            self.remove_property(PropertyInt::HouseCurrentHooksUsable);
        } else {
            self.set_property(PropertyInt::HouseCurrentHooksUsable, value);
        }
    }
}
