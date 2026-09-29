// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Vitals.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Vitals.cs`; do not edit by hand
//! Virtual dispatch for `Creature.VitalHeartBeat`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.VitalHeartBeat()` (Source/ACE.Server/WorldObjects/Creature_Vitals.cs).
/// Overridden by: Player.
/// Overrides calling `base.VitalHeartBeat`: Player.
pub fn vital_heart_beat(w: &mut crate::World, this: empyrean_entity::ObjectGuid) -> bool {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_vitals::player_vital_heart_beat(w, this)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => {
            crate::world_objects::creature_vitals::creature_vital_heart_beat(w, this)
        }
        other => crate::dispatch::wrong_class("Creature.VitalHeartBeat", other),
    }
}
