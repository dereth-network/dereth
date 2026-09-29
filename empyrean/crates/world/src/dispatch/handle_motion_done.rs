// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.HandleMotionDone`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.HandleMotionDone(uint motionID, bool success)` (Source/ACE.Server/WorldObjects/WorldObject.cs).
/// Overridden by: Player.
pub fn handle_motion_done(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    motion_id: u32,
    success: bool,
) {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_tick::player_handle_motion_done(
                w, this, motion_id, success,
            )
        }
        _ => crate::world_objects::world_object::world_object_handle_motion_done(
            w, this, motion_id, success,
        ),
    }
}
