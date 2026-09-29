// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.OnMoveComplete`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.OnMoveComplete(WeenieError status)` (Source/ACE.Server/WorldObjects/WorldObject.cs).
/// Overridden by: Creature, GamePiece, Pet, Player.
/// Overrides calling `base.OnMoveComplete`: GamePiece, Pet.
pub fn on_move_complete(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    status: empyrean_entity::enums::WeenieError,
) {
    match class_of(w, this) {
        Class::Cow | Class::Creature | Class::Vendor => {
            crate::world_objects::monster_navigation::creature_on_move_complete(w, this, status)
        }
        Class::GamePiece => {
            crate::world_objects::game_piece::game_piece_on_move_complete(w, this, status)
        }
        Class::CombatPet | Class::Pet => {
            crate::world_objects::pet::pet_on_move_complete(w, this, status)
        }
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_move::player_on_move_complete(w, this, status)
        }
        _ => crate::world_objects::world_object::world_object_on_move_complete(w, this, status),
    }
}
