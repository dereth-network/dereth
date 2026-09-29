// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Networking.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Networking.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.SendPartialUpdates`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

/// Dispatch for `WorldObject.SendPartialUpdates(Session targetSession, List<GenericPropertyId> properties)` (Source/ACE.Server/WorldObjects/WorldObject_Networking.cs).
/// Overridden by: none.
pub fn send_partial_updates(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target_session: empyrean_net::SessionId,
    properties: &[empyrean_entity::GenericPropertyId],
) {
    // No class overrides it: every object runs WorldObject's implementation.
    crate::world_objects::world_object_networking::world_object_send_partial_updates(
        w,
        this,
        target_session,
        properties,
    )
}
