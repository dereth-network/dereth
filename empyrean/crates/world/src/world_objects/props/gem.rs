// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Gem.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Gem.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Gem.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyInt, PropertyString};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Gem.RareId
    pub fn rare_id(&self) -> Option<i32> {
        self.get_property(PropertyInt::RareId)
    }

    // ACE: Gem.RareId
    pub fn set_rare_id(&mut self, value: Option<i32>) {
        match value {
            None => self.remove_property(PropertyInt::RareId),
            Some(v) => self.set_property(PropertyInt::RareId, v),
        }
    }

    // ACE: Gem.RareUsesTimer
    pub fn rare_uses_timer(&self) -> bool {
        self.get_property(PropertyBool::RareUsesTimer)
            .unwrap_or(false)
    }

    // ACE: Gem.RareUsesTimer
    pub fn set_rare_uses_timer(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::RareUsesTimer);
        } else {
            self.set_property(PropertyBool::RareUsesTimer, value);
        }
    }

    // ACE: Gem.UseSendsSignal
    pub fn use_sends_signal(&self) -> Option<String> {
        self.get_property(PropertyString::UseSendsSignal)
    }

    // ACE: Gem.UseSendsSignal
    pub fn set_use_sends_signal(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::UseSendsSignal),
            Some(v) => self.set_property(PropertyString::UseSendsSignal, v),
        }
    }
}
