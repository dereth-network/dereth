// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Container.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Container.cs`; do not edit by hand
//! Virtual dispatch for `Container.FinishClose`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Container.FinishClose(Player player)` (Source/ACE.Server/WorldObjects/Container.cs).
/// Overridden by: Chest.
/// Overrides calling `base.FinishClose`: Chest.
pub fn finish_close(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::Chest | Class::Storage => {
            crate::world_objects::chest::chest_finish_close(w, this, player)
        }
        Class::Admin
        | Class::CombatPet
        | Class::Container
        | Class::Corpse
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Hook
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::SlumLord
        | Class::Vendor => crate::world_objects::container::container_finish_close(w, this, player),
        other => crate::dispatch::wrong_class("Container.FinishClose", other),
    }
}
