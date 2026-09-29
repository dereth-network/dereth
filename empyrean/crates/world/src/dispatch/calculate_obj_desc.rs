// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Networking.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Networking.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.CalculateObjDesc`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.CalculateObjDesc()` (Source/ACE.Server/WorldObjects/WorldObject_Networking.cs).
/// Overridden by: Corpse, Creature.
/// Overrides calling `base.CalculateObjDesc`: Corpse, Creature.
pub fn calculate_obj_desc(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) -> empyrean_entity::ObjDesc {
    match class_of(w, this) {
        Class::Corpse => crate::world_objects::corpse::corpse_calculate_obj_desc(w, this),
        Class::Admin
        | Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::Vendor => {
            crate::world_objects::creature_networking::creature_calculate_obj_desc(w, this)
        }
        _ => {
            crate::world_objects::world_object_networking::world_object_calculate_obj_desc(w, this)
        }
    }
}
