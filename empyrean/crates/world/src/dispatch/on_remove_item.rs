// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Container.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Container.cs`; do not edit by hand
//! Virtual dispatch for `Container.OnRemoveItem`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Container.OnRemoveItem(WorldObject worldObject)` (Source/ACE.Server/WorldObjects/Container.cs).
/// Overridden by: Hook, SlumLord, Storage.
pub fn on_remove_item(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    world_object: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::Hook => crate::world_objects::hook::hook_on_remove_item(w, this, world_object),
        Class::SlumLord => {
            crate::world_objects::slum_lord::slum_lord_on_remove_item(w, this, world_object)
        }
        Class::Storage => {
            crate::world_objects::storage::storage_on_remove_item(w, this, world_object)
        }
        Class::Admin
        | Class::Chest
        | Class::CombatPet
        | Class::Container
        | Class::Corpse
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::Vendor => {
            crate::world_objects::container::container_on_remove_item(w, this, world_object)
        }
        other => crate::dispatch::wrong_class("Container.OnRemoveItem", other),
    }
}
