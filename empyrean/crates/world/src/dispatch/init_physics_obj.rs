// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.InitPhysicsObj`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.InitPhysicsObj()` (Source/ACE.Server/WorldObjects/WorldObject.cs).
/// Overridden by: Player, Sentinel.
/// Overrides calling `base.InitPhysicsObj`: Player, Sentinel.
pub fn init_physics_obj(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    match class_of(w, this) {
        Class::Player => crate::world_objects::player::player_init_physics_obj(w, this),
        Class::Admin | Class::Sentinel => {
            crate::world_objects::sentinel::sentinel_init_physics_obj(w, this)
        }
        _ => crate::world_objects::world_object::world_object_init_physics_obj(w, this),
    }
}
