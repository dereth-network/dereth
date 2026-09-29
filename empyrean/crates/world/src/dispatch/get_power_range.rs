// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Melee.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Monster_Melee.cs`; do not edit by hand
//! Virtual dispatch for `Creature.GetPowerRange`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.GetPowerRange()` (Source/ACE.Server/WorldObjects/Monster_Melee.cs).
/// Overridden by: Player.
pub fn get_power_range(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::enums::PowerAccuracy {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_melee::player_get_power_range(w, this)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => crate::world_objects::monster_melee::creature_get_power_range(w, this),
        other => crate::dispatch::wrong_class("Creature.GetPowerRange", other),
    }
}
