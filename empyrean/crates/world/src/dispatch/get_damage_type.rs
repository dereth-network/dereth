// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Combat.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Combat.cs`; do not edit by hand
//! Virtual dispatch for `Creature.GetDamageType`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.GetDamageType(bool multiple, CombatType? combatType)` (Source/ACE.Server/WorldObjects/Creature_Combat.cs).
/// Overridden by: Player.
pub fn get_damage_type(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    multiple: bool,
    combat_type: Option<crate::world_objects::creature_combat::CombatType>,
) -> empyrean_entity::enums::DamageType {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_combat::player_get_damage_type(
                w,
                this,
                multiple,
                combat_type,
            )
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => crate::world_objects::creature_combat::creature_get_damage_type(
            w,
            this,
            multiple,
            combat_type,
        ),
        other => crate::dispatch::wrong_class("Creature.GetDamageType", other),
    }
}
