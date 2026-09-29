// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.OnCollideObjectEnd`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.OnCollideObjectEnd(WorldObject target)` (Source/ACE.Server/WorldObjects/WorldObject.cs).
/// Overridden by: Hotspot, Player.
pub fn on_collide_object_end(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::Hotspot => {
            crate::world_objects::hotspot::hotspot_on_collide_object_end(w, this, target)
        }
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player::player_on_collide_object_end(w, this, target)
        }
        _ => {
            crate::world_objects::world_object::world_object_on_collide_object_end(w, this, target)
        }
    }
}
