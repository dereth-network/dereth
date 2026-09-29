// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Use.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Use.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.OnActivate`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.OnActivate(WorldObject activator)` (Source/ACE.Server/WorldObjects/WorldObject_Use.cs).
/// Overridden by: Gem, PressurePlate, Switch.
/// Overrides calling `base.OnActivate`: Gem, PressurePlate, Switch.
pub fn on_activate(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::Gem => crate::world_objects::gem::gem_on_activate(w, this, activator),
        Class::PressurePlate => {
            crate::world_objects::pressure_plate::pressure_plate_on_activate(w, this, activator)
        }
        Class::Switch => crate::world_objects::switch::switch_on_activate(w, this, activator),
        _ => crate::world_objects::world_object_use::world_object_on_activate(w, this, activator),
    }
}
