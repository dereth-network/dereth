// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Equipment.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Equipment.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.OnUnWield`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.OnUnWield(Creature creature)` (Source/ACE.Server/WorldObjects/WorldObject_Equipment.cs).
/// Overridden by: AdvocateItem.
/// Overrides calling `base.OnUnWield`: AdvocateItem.
pub fn on_un_wield(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    creature: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::AdvocateItem => {
            crate::world_objects::advocate_item::advocate_item_on_un_wield(w, this, creature)
        }
        _ => crate::world_objects::world_object_equipment::world_object_on_un_wield(
            w, this, creature,
        ),
    }
}
