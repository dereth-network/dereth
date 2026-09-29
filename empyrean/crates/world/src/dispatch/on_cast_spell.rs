// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Use.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Use.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.OnCastSpell`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.OnCastSpell(WorldObject activator)` (Source/ACE.Server/WorldObjects/WorldObject_Use.cs).
/// Overridden by: Portal.
/// Overrides calling `base.OnCastSpell`: Portal.
pub fn on_cast_spell(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::HousePortal | Class::Portal => {
            crate::world_objects::portal::portal_on_cast_spell(w, this, activator)
        }
        _ => crate::world_objects::world_object_use::world_object_on_cast_spell(w, this, activator),
    }
}
