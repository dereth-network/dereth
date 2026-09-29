// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.OnCollideEnvironment`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.OnCollideEnvironment()` (Source/ACE.Server/WorldObjects/WorldObject.cs).
/// Overridden by: Ammunition, Player, SpellProjectile.
pub fn on_collide_environment(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    match class_of(w, this) {
        Class::Ammunition => {
            crate::world_objects::ammunition::ammunition_on_collide_environment(w, this)
        }
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player::player_on_collide_environment(w, this)
        }
        Class::SpellProjectile => {
            crate::world_objects::spell_projectile::spell_projectile_on_collide_environment(w, this)
        }
        _ => crate::world_objects::world_object::world_object_on_collide_environment(w, this),
    }
}
