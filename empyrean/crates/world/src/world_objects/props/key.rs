// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Key.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Key.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Key.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyString};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Key.KeyCode
    pub fn key_code(&self) -> Option<String> {
        self.get_property(PropertyString::KeyCode)
    }

    // ACE: Key.KeyCode
    pub fn set_key_code(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::KeyCode),
            Some(v) => self.set_property(PropertyString::KeyCode, v),
        }
    }

    // ACE: Key.OpensAnyLock
    pub fn opens_any_lock(&self) -> bool {
        self.get_property(PropertyBool::OpensAnyLock)
            .unwrap_or(false)
    }

    // ACE: Key.OpensAnyLock
    pub fn set_opens_any_lock(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::OpensAnyLock);
        } else {
            self.set_property(PropertyBool::OpensAnyLock, value);
        }
    }
}
