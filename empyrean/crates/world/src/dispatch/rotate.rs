// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Navigation.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Navigation.cs`; do not edit by hand
//! Virtual dispatch for `Creature.Rotate`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.Rotate(WorldObject target)` (Source/ACE.Server/WorldObjects/Creature_Navigation.cs).
/// Overridden by: none.
pub fn rotate(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) -> f32 {
    match class_of(w, this) {
        Class::Admin
        | Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::Vendor => {
            crate::world_objects::creature_navigation::creature_rotate(w, this, target)
        }
        other => crate::dispatch::wrong_class("Creature.Rotate", other),
    }
}
