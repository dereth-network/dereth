// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Pet.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Pet.cs`; do not edit by hand
//! Virtual dispatch for `Pet.Init`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Pet.Init(Player player, PetDevice petDevice)` (Source/ACE.Server/WorldObjects/Pet.cs).
/// Overridden by: CombatPet.
/// Overrides calling `base.Init`: CombatPet.
pub fn init(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
    pet_device: empyrean_entity::ObjectGuid,
) -> Option<bool> {
    match class_of(w, this) {
        Class::CombatPet => {
            crate::world_objects::combat_pet::combat_pet_init(w, this, player, pet_device)
        }
        Class::Pet => crate::world_objects::pet::pet_init(w, this, player, pet_device),
        other => crate::dispatch::wrong_class("Pet.Init", other),
    }
}
