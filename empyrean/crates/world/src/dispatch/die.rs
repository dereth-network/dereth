// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Death.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Death.cs`; do not edit by hand
//! Virtual dispatch for `Creature.Die`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.Die(DamageHistoryInfo lastDamager, DamageHistoryInfo topDamager)` (Source/ACE.Server/WorldObjects/Creature_Death.cs).
/// Overridden by: Player.
pub fn die(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    last_damager: Option<crate::entity::damage_history_info::DamageHistoryInfo>,
    top_damager: Option<crate::entity::damage_history_info::DamageHistoryInfo>,
) {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_death::player_die(w, this, last_damager, top_damager)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => {
            crate::world_objects::creature_death::creature_die(w, this, last_damager, top_damager)
        }
        other => crate::dispatch::wrong_class("Creature.Die", other),
    }
}
