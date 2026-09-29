// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Combat.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Monster_Combat.cs`; do not edit by hand
//! Virtual dispatch for `Creature.TakeDamageOverTime`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.TakeDamageOverTime(float amount, DamageType damageType)` (Source/ACE.Server/WorldObjects/Monster_Combat.cs).
/// Overridden by: Player.
pub fn take_damage_over_time(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    amount: f32,
    damage_type: empyrean_entity::enums::DamageType,
) {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_combat::player_take_damage_over_time(
                w,
                this,
                amount,
                damage_type,
            )
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => crate::world_objects::monster_combat::creature_take_damage_over_time(
            w,
            this,
            amount,
            damage_type,
        ),
        other => crate::dispatch::wrong_class("Creature.TakeDamageOverTime", other),
    }
}
