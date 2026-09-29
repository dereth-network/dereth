// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Use.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Use.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.CheckUseRequirements`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.CheckUseRequirements(WorldObject activator)` (Source/ACE.Server/WorldObjects/WorldObject_Use.cs).
/// Overridden by: AdvocateFane, Chest, Hook, Hooker, HousePortal, PKModifier, PetDevice, Portal, Storage.
/// Overrides calling `base.CheckUseRequirements`: Chest, Hooker, PetDevice, Storage.
pub fn check_use_requirements(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) -> crate::entity::activation_result::ActivationResult {
    match class_of(w, this) {
        Class::AdvocateFane => {
            crate::world_objects::advocate_fane::advocate_fane_check_use_requirements(
                w, this, activator,
            )
        }
        Class::Chest => {
            crate::world_objects::chest::chest_check_use_requirements(w, this, activator)
        }
        Class::Hook => crate::world_objects::hook::hook_check_use_requirements(w, this, activator),
        Class::Hooker => {
            crate::world_objects::hooker::hooker_check_use_requirements(w, this, activator)
        }
        Class::HousePortal => {
            crate::world_objects::house_portal::house_portal_check_use_requirements(
                w, this, activator,
            )
        }
        Class::PKModifier => crate::world_objects::pk_modifier::pk_modifier_check_use_requirements(
            w, this, activator,
        ),
        Class::PetDevice => {
            crate::world_objects::pet_device::pet_device_check_use_requirements(w, this, activator)
        }
        Class::Portal => {
            crate::world_objects::portal::portal_check_use_requirements(w, this, activator)
        }
        Class::Storage => {
            crate::world_objects::storage::storage_check_use_requirements(w, this, activator)
        }
        _ => crate::world_objects::world_object_use::world_object_check_use_requirements(
            w, this, activator,
        ),
    }
}
