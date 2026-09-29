// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Use.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Use.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/WorldObject_Use.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyFloat};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: WorldObject.UseTimestamp
    pub fn use_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::UseTimestamp)
    }

    // ACE: WorldObject.UseTimestamp
    pub fn set_use_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::UseTimestamp),
            Some(v) => self.set_property(PropertyFloat::UseTimestamp, v),
        }
    }

    // ACE: WorldObject.ResetTimestamp
    pub fn reset_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResetTimestamp)
    }

    // ACE: WorldObject.ResetTimestamp
    pub fn set_reset_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResetTimestamp),
            Some(v) => self.set_property(PropertyFloat::ResetTimestamp, v),
        }
    }

    // ACE: WorldObject.ResetInterval
    pub fn reset_interval(&self) -> Option<f64> {
        self.get_property(PropertyFloat::ResetInterval)
    }

    // ACE: WorldObject.ResetInterval
    pub fn set_reset_interval(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::ResetInterval),
            Some(v) => self.set_property(PropertyFloat::ResetInterval, v),
        }
    }

    // ACE: WorldObject.DefaultLocked
    pub fn default_locked(&self) -> bool {
        self.get_property(PropertyBool::DefaultLocked)
            .unwrap_or(false)
    }

    // ACE: WorldObject.DefaultLocked
    pub fn set_default_locked(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::DefaultLocked);
        } else {
            self.set_property(PropertyBool::DefaultLocked, value);
        }
    }

    // ACE: WorldObject.DefaultOpen
    pub fn default_open(&self) -> bool {
        self.get_property(PropertyBool::DefaultOpen)
            .unwrap_or(false)
    }

    // ACE: WorldObject.DefaultOpen
    pub fn set_default_open(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::DefaultOpen);
        } else {
            self.set_property(PropertyBool::DefaultOpen, value);
        }
    }
}
