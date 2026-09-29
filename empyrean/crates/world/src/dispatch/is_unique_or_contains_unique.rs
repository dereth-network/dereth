// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.IsUniqueOrContainsUnique`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for the getter of `WorldObject.IsUniqueOrContainsUnique` (Source/ACE.Server/WorldObjects/WorldObject.cs).
/// Overridden by: Container.
/// Overrides calling `base.IsUniqueOrContainsUnique`: Container.
pub fn is_unique_or_contains_unique(w: &crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    match class_of(w, this) {
        Class::Admin
        | Class::Chest
        | Class::CombatPet
        | Class::Container
        | Class::Corpse
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Hook
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::SlumLord
        | Class::Storage
        | Class::Vendor => {
            crate::world_objects::container::container_is_unique_or_contains_unique(w, this)
        }
        _ => crate::world_objects::world_object::world_object_is_unique_or_contains_unique(w, this),
    }
}
