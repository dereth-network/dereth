// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Container_Properties.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Container_Properties.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Container_Properties.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyFloat, PropertyInstanceId};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Container.Viewer
    pub fn viewer(&self) -> u32 {
        self.get_property(PropertyInstanceId::Viewer).unwrap_or(0)
    }

    // ACE: Container.Viewer
    pub fn set_viewer(&mut self, value: u32) {
        if value == 0 {
            self.remove_property(PropertyInstanceId::Viewer);
        } else {
            self.set_property(PropertyInstanceId::Viewer, value);
        }
    }

    // ACE: Container.LastUnlocker
    pub fn last_unlocker(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::LastUnlocker)
    }

    // ACE: Container.LastUnlocker
    pub fn set_last_unlocker(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyInstanceId::LastUnlocker),
            Some(v) => self.set_property(PropertyInstanceId::LastUnlocker, v),
        }
    }

    // ACE: Container.UseLockTimestamp
    pub fn use_lock_timestamp(&self) -> Option<f64> {
        self.get_property(PropertyFloat::UseLockTimestamp)
    }

    // ACE: Container.UseLockTimestamp
    pub fn set_use_lock_timestamp(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::UseLockTimestamp),
            Some(v) => self.set_property(PropertyFloat::UseLockTimestamp, v),
        }
    }

    // ACE: Container.ResetMessagePending
    pub fn reset_message_pending(&self) -> bool {
        self.get_property(PropertyBool::ResetMessagePending)
            .unwrap_or(false)
    }

    // ACE: Container.ResetMessagePending
    pub fn set_reset_message_pending(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::ResetMessagePending);
        } else {
            self.set_property(PropertyBool::ResetMessagePending, value);
        }
    }
}
