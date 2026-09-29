// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Awareness.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Monster_Awareness.cs`; do not edit by hand
//! Virtual dispatch for `Creature.HandleFindTarget`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.HandleFindTarget()` (Source/ACE.Server/WorldObjects/Monster_Awareness.cs).
/// Overridden by: CombatPet.
pub fn handle_find_target(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    match class_of(w, this) {
        Class::CombatPet => {
            crate::world_objects::combat_pet::combat_pet_handle_find_target(w, this)
        }
        Class::Admin
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::Vendor => {
            crate::world_objects::monster_awareness::creature_handle_find_target(w, this)
        }
        other => crate::dispatch::wrong_class("Creature.HandleFindTarget", other),
    }
}
