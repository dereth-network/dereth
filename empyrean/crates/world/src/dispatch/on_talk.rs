// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Use.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Use.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.OnTalk`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.OnTalk(WorldObject activator)` (Source/ACE.Server/WorldObjects/WorldObject_Use.cs).
/// Overridden by: Container, Door.
pub fn on_talk(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    activator: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::Admin
        | Class::Chest
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
        | Class::Storage
        | Class::Vendor => crate::world_objects::container::container_on_talk(w, this, activator),
        Class::Door => crate::world_objects::door::door_on_talk(w, this, activator),
        _ => crate::world_objects::world_object_use::world_object_on_talk(w, this, activator),
    }
}
