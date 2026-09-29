// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Navigation.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Navigation.cs`; do not edit by hand
//! Virtual dispatch for `Creature.MoveTo`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.MoveTo(WorldObject target, float runRate)` (Source/ACE.Server/WorldObjects/Creature_Navigation.cs).
/// Overridden by: Pet, Player.
/// Overrides calling `base.MoveTo`: Pet.
pub fn move_to(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    run_rate: f32,
) {
    match class_of(w, this) {
        Class::CombatPet | Class::Pet => {
            crate::world_objects::pet::pet_move_to(w, this, target, run_rate)
        }
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_move::player_move_to(w, this, target, run_rate)
        }
        Class::Cow | Class::Creature | Class::GamePiece | Class::Vendor => {
            crate::world_objects::creature_navigation::creature_move_to(w, this, target, run_rate)
        }
        other => crate::dispatch::wrong_class("Creature.MoveTo", other),
    }
}
