// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Missile.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Missile.cs`; do not edit by hand
//! Virtual dispatch for `Creature.UpdateAmmoAfterLaunch`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.UpdateAmmoAfterLaunch(WorldObject ammo)` (Source/ACE.Server/WorldObjects/Creature_Missile.cs).
/// Overridden by: Player.
pub fn update_ammo_after_launch(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    ammo: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_missile::player_update_ammo_after_launch(w, this, ammo)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => {
            crate::world_objects::creature_missile::creature_update_ammo_after_launch(w, this, ammo)
        }
        other => crate::dispatch::wrong_class("Creature.UpdateAmmoAfterLaunch", other),
    }
}
