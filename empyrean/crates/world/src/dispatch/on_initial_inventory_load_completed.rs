// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Container.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Container.cs`; do not edit by hand
//! Virtual dispatch for `Container.OnInitialInventoryLoadCompleted`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Container.OnInitialInventoryLoadCompleted()` (Source/ACE.Server/WorldObjects/Container.cs).
/// Overridden by: Corpse, Hook, SlumLord.
pub fn on_initial_inventory_load_completed(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::Corpse => {
            crate::world_objects::corpse::corpse_on_initial_inventory_load_completed(w, this)
        }
        Class::Hook => {
            crate::world_objects::hook::hook_on_initial_inventory_load_completed(w, this)
        }
        Class::SlumLord => {
            crate::world_objects::slum_lord::slum_lord_on_initial_inventory_load_completed(w, this)
        }
        Class::Admin
        | Class::Chest
        | Class::CombatPet
        | Class::Container
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::Storage
        | Class::Vendor => {
            crate::world_objects::container::container_on_initial_inventory_load_completed(w, this)
        }
        other => crate::dispatch::wrong_class("Container.OnInitialInventoryLoadCompleted", other),
    }
}
