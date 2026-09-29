// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature.cs`; do not edit by hand
//! Virtual dispatch for `Creature.GetBurdenMod`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.GetBurdenMod()` (Source/ACE.Server/WorldObjects/Creature.cs).
/// Overridden by: Player.
pub fn get_burden_mod(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> f32 {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player::player_get_burden_mod(w, this)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => crate::world_objects::creature::creature_get_burden_mod(w, this),
        other => crate::dispatch::wrong_class("Creature.GetBurdenMod", other),
    }
}
