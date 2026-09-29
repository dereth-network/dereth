// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Death.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Death.cs`; do not edit by hand
//! Virtual dispatch for `Creature.OnDeath`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.OnDeath(DamageHistoryInfo lastDamager, DamageType damageType, bool criticalHit)` (Source/ACE.Server/WorldObjects/Creature_Death.cs).
/// Overridden by: Player.
/// Overrides calling `base.OnDeath`: Player.
pub fn on_death(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    last_damager: Option<crate::entity::damage_history_info::DamageHistoryInfo>,
    damage_type: empyrean_entity::enums::DamageType,
    critical_hit: bool,
) -> crate::entity::death_message::DeathMessage {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_death::player_on_death(
                w,
                this,
                last_damager,
                damage_type,
                critical_hit,
            )
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => crate::world_objects::creature_death::creature_on_death(
            w,
            this,
            last_damager,
            damage_type,
            critical_hit,
        ),
        other => crate::dispatch::wrong_class("Creature.OnDeath", other),
    }
}
