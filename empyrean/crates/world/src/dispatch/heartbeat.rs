// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Tick.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Tick.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.Heartbeat`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.Heartbeat(double currentUnixTime)` (Source/ACE.Server/WorldObjects/WorldObject_Tick.cs).
/// Overridden by: Container, Creature, Game, Player.
/// Overrides calling `base.Heartbeat`: Container, Creature, Game, Player.
pub fn heartbeat(w: &mut crate::World, this: empyrean_entity::ObjectGuid, current_unix_time: f64) {
    match class_of(w, this) {
        Class::Chest
        | Class::Container
        | Class::Corpse
        | Class::Hook
        | Class::SlumLord
        | Class::Storage => {
            crate::world_objects::container_tick::container_heartbeat(w, this, current_unix_time)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => {
            crate::world_objects::creature_tick::creature_heartbeat(w, this, current_unix_time)
        }
        Class::Game => crate::world_objects::game::game_heartbeat(w, this, current_unix_time),
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_tick::player_heartbeat(w, this, current_unix_time)
        }
        _ => crate::world_objects::world_object_tick::world_object_heartbeat(
            w,
            this,
            current_unix_time,
        ),
    }
}
