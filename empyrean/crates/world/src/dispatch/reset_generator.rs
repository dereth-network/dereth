// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Generators.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Generators.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.ResetGenerator`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

/// Dispatch for `WorldObject.ResetGenerator()` (Source/ACE.Server/WorldObjects/WorldObject_Generators.cs).
/// Overridden by: none.
pub fn reset_generator(w: &mut crate::World, this: empyrean_entity::ObjectGuid) {
    // No class overrides it: every object runs WorldObject's implementation.
    crate::world_objects::world_object_generators::world_object_reset_generator(w, this)
}
