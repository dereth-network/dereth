// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Properties.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Properties.cs`; do not edit by hand
//! Virtual dispatch for `Creature.GetNaturalResistance`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.GetNaturalResistance(DamageType damageType)` (Source/ACE.Server/WorldObjects/Creature_Properties.cs).
/// Overridden by: Player.
pub fn get_natural_resistance(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    damage_type: empyrean_entity::enums::DamageType,
) -> f32 {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_combat::player_get_natural_resistance(w, this, damage_type)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => {
            crate::world_objects::creature::creature_get_natural_resistance(w, this, damage_type)
        }
        other => crate::dispatch::wrong_class("Creature.GetNaturalResistance", other),
    }
}
