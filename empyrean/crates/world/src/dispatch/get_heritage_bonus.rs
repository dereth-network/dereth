// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Combat.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Combat.cs`; do not edit by hand
//! Virtual dispatch for `Creature.GetHeritageBonus`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.GetHeritageBonus(WorldObject weapon)` (Source/ACE.Server/WorldObjects/Creature_Combat.cs).
/// Overridden by: Player.
pub fn get_heritage_bonus(
    w: &crate::World,
    this: empyrean_entity::ObjectGuid,
    weapon: empyrean_entity::ObjectGuid,
) -> bool {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_skills::player_get_heritage_bonus(w, this, weapon)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => {
            crate::world_objects::creature_combat::creature_get_heritage_bonus(w, this, weapon)
        }
        other => crate::dispatch::wrong_class("Creature.GetHeritageBonus", other),
    }
}
