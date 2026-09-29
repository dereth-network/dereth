// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Links.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Links.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.SetLinkProperties`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.SetLinkProperties(WorldObject wo)` (Source/ACE.Server/WorldObjects/WorldObject_Links.cs).
/// Overridden by: Door, House, HousePortal, Portal, PressurePlate, Switch.
pub fn set_link_properties(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    wo: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::Door => crate::world_objects::door::door_set_link_properties(w, this, wo),
        Class::House => crate::world_objects::house::house_set_link_properties(w, this, wo),
        Class::HousePortal => {
            crate::world_objects::house_portal::house_portal_set_link_properties(w, this, wo)
        }
        Class::Portal => crate::world_objects::portal::portal_set_link_properties(w, this, wo),
        Class::PressurePlate => {
            crate::world_objects::pressure_plate::pressure_plate_set_link_properties(w, this, wo)
        }
        Class::Switch => crate::world_objects::switch::switch_set_link_properties(w, this, wo),
        _ => {
            crate::world_objects::world_object_links::world_object_set_link_properties(w, this, wo)
        }
    }
}
