// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Container.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Container.cs`; do not edit by hand
//! Virtual dispatch for `Container.MotionPickup`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for the getter of `Container.MotionPickup` (Source/ACE.Server/WorldObjects/Container.cs).
/// Overridden by: Hook.
pub fn motion_pickup(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::enums::MotionCommand {
    match class_of(w, this) {
        Class::Hook => crate::world_objects::hook::hook_motion_pickup(w, this),
        Class::Admin
        | Class::Chest
        | Class::CombatPet
        | Class::Container
        | Class::Corpse
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::SlumLord
        | Class::Storage
        | Class::Vendor => crate::world_objects::container::container_motion_pickup(w, this),
        other => crate::dispatch::wrong_class("Container.MotionPickup", other),
    }
}
