// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.OnGeneration`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

/// Dispatch for `WorldObject.OnGeneration(WorldObject generator)` (Source/ACE.Server/WorldObjects/WorldObject.cs).
/// Overridden by: none.
pub fn on_generation(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    generator: empyrean_entity::ObjectGuid,
) {
    // No class overrides it: every object runs WorldObject's implementation.
    crate::world_objects::world_object::world_object_on_generation(w, this, generator)
}
