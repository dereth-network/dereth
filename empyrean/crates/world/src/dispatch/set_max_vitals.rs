// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Vitals.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Vitals.cs`; do not edit by hand
//! Virtual dispatch for `Creature.SetMaxVitals`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.SetMaxVitals()` (Source/ACE.Server/WorldObjects/Creature_Vitals.cs).
/// Overridden by: Player.
/// Overrides calling `base.SetMaxVitals`: Player.
pub fn set_max_vitals(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_vitals::player_set_max_vitals(w, this)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => crate::world_objects::creature_vitals::creature_set_max_vitals(w, this),
        other => crate::dispatch::wrong_class("Creature.SetMaxVitals", other),
    }
}
