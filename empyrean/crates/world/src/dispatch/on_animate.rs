// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Use.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Use.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.OnAnimate`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

/// Dispatch for `WorldObject.OnAnimate(WorldObject activator)` (Source/ACE.Server/WorldObjects/WorldObject_Use.cs).
/// Overridden by: none.
pub fn on_animate(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    // No class overrides it: every object runs WorldObject's implementation.
    crate::world_objects::world_object_use::world_object_on_animate(w, this, activator)
}
