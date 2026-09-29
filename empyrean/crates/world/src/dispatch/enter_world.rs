// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.EnterWorld`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.EnterWorld()` (Source/ACE.Server/WorldObjects/WorldObject.cs).
/// Overridden by: Corpse, Portal.
/// Overrides calling `base.EnterWorld`: Corpse, Portal.
pub fn enter_world(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    match class_of(w, this) {
        Class::Corpse => crate::world_objects::corpse::corpse_enter_world(w, this),
        Class::HousePortal | Class::Portal => {
            crate::world_objects::portal::portal_enter_world(w, this)
        }
        _ => crate::world_objects::world_object::world_object_enter_world(w, this),
    }
}
