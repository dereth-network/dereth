// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Networking.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Networking.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.SerializeGameDataOnly`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

/// Dispatch for `WorldObject.SerializeGameDataOnly(BinaryWriter writer, bool adminvision)` (Source/ACE.Server/WorldObjects/WorldObject_Networking.cs).
/// Overridden by: none.
pub fn serialize_game_data_only(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    writer: &mut Vec<u8>,
    adminvision: bool,
) {
    // No class overrides it: every object runs WorldObject's implementation.
    crate::world_objects::world_object_networking::world_object_serialize_game_data_only(
        w,
        this,
        writer,
        adminvision,
    )
}
