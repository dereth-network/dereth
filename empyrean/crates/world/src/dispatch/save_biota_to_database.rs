// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Database.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Database.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.SaveBiotaToDatabase`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

/// Dispatch for `WorldObject.SaveBiotaToDatabase(bool enqueueSave)` (Source/ACE.Server/WorldObjects/WorldObject_Database.cs).
/// Overridden by: none.
pub fn save_biota_to_database(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    enqueue_save: bool,
) {
    // No class overrides it: every object runs WorldObject's implementation.
    crate::world_objects::world_object_database::world_object_save_biota_to_database(
        w,
        this,
        enqueue_save,
    )
}
