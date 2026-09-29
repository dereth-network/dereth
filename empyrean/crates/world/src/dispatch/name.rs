// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Properties.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Properties.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.Name`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for the getter of `WorldObject.Name` (Source/ACE.Server/WorldObjects/WorldObject_Properties.cs).
/// Overridden by: Player.
/// Overrides calling `base.Name`: Player.
pub fn name(w: &crate::World, this: empyrean_entity::ObjectGuid) -> Option<String> {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player::player_name(w, this)
        }
        _ => crate::world_objects::world_object::world_object_name(w, this),
    }
}

/// Dispatch for the setter of `WorldObject.Name` (Source/ACE.Server/WorldObjects/WorldObject_Properties.cs).
/// Overridden by: Player.
/// Overrides calling `base.Name`: Player.
pub fn set_name(w: &mut crate::World, this: empyrean_entity::ObjectGuid, value: String) {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player::player_set_name(w, this, value)
        }
        _ => crate::world_objects::world_object::world_object_set_name(w, this, value),
    }
}
