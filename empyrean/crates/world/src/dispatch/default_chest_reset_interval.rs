// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Chest.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Chest.cs`; do not edit by hand
//! Virtual dispatch for `Chest.Default_ChestResetInterval`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for the getter of `Chest.Default_ChestResetInterval` (Source/ACE.Server/WorldObjects/Chest.cs).
/// Overridden by: Storage.
pub fn default_chest_reset_interval(w: &crate::World, this: empyrean_entity::ObjectGuid) -> f64 {
    match class_of(w, this) {
        Class::Storage => {
            crate::world_objects::storage::storage_default_chest_reset_interval(w, this)
        }
        Class::Chest => crate::world_objects::chest::chest_default_chest_reset_interval(w, this),
        other => crate::dispatch::wrong_class("Chest.Default_ChestResetInterval", other),
    }
}
