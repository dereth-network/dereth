// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Navigation.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Navigation.cs`; do not edit by hand
//! Virtual dispatch for `Creature.GetRotateDelay`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.GetRotateDelay(float angle)` (Source/ACE.Server/WorldObjects/Creature_Navigation.cs).
/// Overridden by: Player.
/// Overrides calling `base.GetRotateDelay`: Player.
pub fn get_rotate_delay(w: &crate::World, this: empyrean_entity::ObjectGuid, angle: f32) -> f32 {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_location::player_get_rotate_delay(w, this, angle)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => {
            crate::world_objects::creature_navigation::creature_get_rotate_delay(w, this, angle)
        }
        other => crate::dispatch::wrong_class("Creature.GetRotateDelay", other),
    }
}
