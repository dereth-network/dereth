// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Combat.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Combat.cs`; do not edit by hand
//! Virtual dispatch for `Creature.OnEvade`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.OnEvade(WorldObject attacker, CombatType attackType)` (Source/ACE.Server/WorldObjects/Creature_Combat.cs).
/// Overridden by: Player.
pub fn on_evade(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    attacker: empyrean_entity::ObjectGuid,
    attack_type: crate::world_objects::creature_combat::CombatType,
) {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_combat::player_on_evade(w, this, attacker, attack_type)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => {
            crate::world_objects::creature_combat::creature_on_evade(w, this, attacker, attack_type)
        }
        other => crate::dispatch::wrong_class("Creature.OnEvade", other),
    }
}
