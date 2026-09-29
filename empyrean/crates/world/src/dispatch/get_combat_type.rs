// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Combat.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Combat.cs`; do not edit by hand
//! Virtual dispatch for `Creature.GetCombatType`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.GetCombatType()` (Source/ACE.Server/WorldObjects/Creature_Combat.cs).
/// Overridden by: Player.
pub fn get_combat_type(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> crate::world_objects::creature_combat::CombatType {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_combat::player_get_combat_type(w, this)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => crate::world_objects::creature_combat::creature_get_combat_type(w, this),
        other => crate::dispatch::wrong_class("Creature.GetCombatType", other),
    }
}
