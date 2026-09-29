// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Container.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Container.cs`; do not edit by hand
//! Virtual dispatch for `Container.DoOnCloseMotionChanges`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Container.DoOnCloseMotionChanges()` (Source/ACE.Server/WorldObjects/Container.cs).
/// Overridden by: Chest.
pub fn do_on_close_motion_changes(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> f32 {
    match class_of(w, this) {
        Class::Chest | Class::Storage => {
            crate::world_objects::chest::chest_do_on_close_motion_changes(w, this)
        }
        Class::Admin
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
        | Class::Vendor => {
            crate::world_objects::container::container_do_on_close_motion_changes(w, this)
        }
        other => crate::dispatch::wrong_class("Container.DoOnCloseMotionChanges", other),
    }
}
