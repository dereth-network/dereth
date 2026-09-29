// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Chest.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Chest.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Chest.cs`.

use empyrean_entity::enums::{PropertyBool, PropertyString};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    /// This is used for things like Mana Forge Chests. ACE's getter first returns true when
    /// `ChestResetInterval <= 5`; that property reads the virtual `Default_ChestResetInterval`,
    /// so the caller passes it in (`chest::chest_reset_interval`).
    // ACE: Chest.ChestRegenOnClose
    pub fn chest_regen_on_close(&self, chest_reset_interval: f64) -> bool {
        if chest_reset_interval <= 5.0 {
            return true;
        }

        self.get_property(PropertyBool::ChestRegenOnClose)
            .unwrap_or(false)
    }

    // ACE: Chest.ChestRegenOnClose
    pub fn set_chest_regen_on_close(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::ChestRegenOnClose);
        } else {
            self.set_property(PropertyBool::ChestRegenOnClose, value);
        }
    }

    // ACE: Chest.ChestClearedWhenClosed
    pub fn chest_cleared_when_closed(&self) -> bool {
        self.get_property(PropertyBool::ChestClearedWhenClosed)
            .unwrap_or(false)
    }

    // ACE: Chest.ChestClearedWhenClosed
    pub fn set_chest_cleared_when_closed(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::ChestClearedWhenClosed);
        } else {
            self.set_property(PropertyBool::ChestClearedWhenClosed, value);
        }
    }

    // ACE: Chest.LockCode, Door.LockCode
    pub fn lock_code(&self) -> Option<String> {
        self.get_property(PropertyString::LockCode)
    }

    // ACE: Chest.LockCode, Door.LockCode
    pub fn set_lock_code(&mut self, value: Option<String>) {
        match value {
            None => self.remove_property(PropertyString::LockCode),
            Some(v) => self.set_property(PropertyString::LockCode, v),
        }
    }
}
