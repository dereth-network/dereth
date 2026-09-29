// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Navigation.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Navigation.cs`; do not edit by hand
//! Virtual dispatch for `Creature.BroadcastMoveTo`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `Creature.BroadcastMoveTo(Player player)` (Source/ACE.Server/WorldObjects/Creature_Navigation.cs).
/// Overridden by: GamePiece.
pub fn broadcast_move_to(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::GamePiece => {
            crate::world_objects::game_piece::game_piece_broadcast_move_to(w, this, player)
        }
        Class::Admin
        | Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::Pet
        | Class::Player
        | Class::Sentinel
        | Class::Vendor => {
            crate::world_objects::creature_navigation::creature_broadcast_move_to(w, this, player)
        }
        other => crate::dispatch::wrong_class("Creature.BroadcastMoveTo", other),
    }
}
