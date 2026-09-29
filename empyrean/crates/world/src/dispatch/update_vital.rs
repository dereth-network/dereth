// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Vitals.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Vitals.cs`; do not edit by hand
//! Virtual dispatch for `Creature.UpdateVital`, `Creature.UpdateVital`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.UpdateVital(CreatureVital vital, int newVal)` (Source/ACE.Server/WorldObjects/Creature_Vitals.cs).
/// Overridden by: Player.
/// Overrides calling `base.UpdateVital`: Player.
pub fn update_vital(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    vital: crate::world_objects::entity::creature_vital::CreatureVital,
    new_val: i32,
) -> i32 {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_vitals::player_update_vital(w, this, vital, new_val)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => {
            crate::world_objects::creature_vitals::creature_update_vital(w, this, vital, new_val)
        }
        other => crate::dispatch::wrong_class("Creature.UpdateVital", other),
    }
}

/// Dispatch for `Creature.UpdateVital(CreatureVital vital, uint newVal)` (Source/ACE.Server/WorldObjects/Creature_Vitals.cs).
/// Overridden by: none.
pub fn update_vital_uint(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    vital: crate::world_objects::entity::creature_vital::CreatureVital,
    new_val: u32,
) -> i32 {
    match class_of(w, this) {
        Class::Admin
        | Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::Vendor => crate::world_objects::creature_vitals::creature_update_vital_uint(
            w, this, vital, new_val,
        ),
        other => crate::dispatch::wrong_class("Creature.UpdateVital", other),
    }
}
