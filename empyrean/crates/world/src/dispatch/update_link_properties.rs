// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Links.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Links.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.UpdateLinkProperties`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.UpdateLinkProperties(WorldObject wo)` (Source/ACE.Server/WorldObjects/WorldObject_Links.cs).
/// Overridden by: House.
pub fn update_link_properties(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    wo: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::House => crate::world_objects::house::house_update_link_properties(w, this, wo),
        _ => crate::world_objects::world_object_links::world_object_update_link_properties(
            w, this, wo,
        ),
    }
}
