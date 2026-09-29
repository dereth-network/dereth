// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Combat.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Monster_Combat.cs`; do not edit by hand
//! Virtual dispatch for `Creature.TakeDamage`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.TakeDamage(WorldObject source, DamageType damageType, float amount, bool crit)` (Source/ACE.Server/WorldObjects/Monster_Combat.cs).
/// Overridden by: none.
pub fn take_damage(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    source: empyrean_entity::ObjectGuid,
    damage_type: empyrean_entity::enums::DamageType,
    amount: f32,
    crit: bool,
) -> u32 {
    match class_of(w, this) {
        Class::Admin
        | Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::Vendor => crate::world_objects::monster_combat::creature_take_damage(
            w,
            this,
            source,
            damage_type,
            amount,
            crit,
        ),
        other => crate::dispatch::wrong_class("Creature.TakeDamage", other),
    }
}
