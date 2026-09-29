// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Tick.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Tick.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.EnqueueAction`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.EnqueueAction(IAction action)` (Source/ACE.Server/WorldObjects/WorldObject_Tick.cs).
/// Overridden by: Player.
pub fn enqueue_action(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    action: crate::entity::actions::i_action::Action,
) {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_tick::player_enqueue_action(w, this, action)
        }
        _ => crate::world_objects::world_object_tick::world_object_enqueue_action(w, this, action),
    }
}
