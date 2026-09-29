// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Portal.cs, Source/ACE.Server/WorldObjects/WorldObject.cs
// @generated from the ACE sources named above; do not edit by hand
//! Virtual dispatch for `WorldObject.OnCollideObject`, `Portal.OnCollideObject`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.OnCollideObject(WorldObject target)` (Source/ACE.Server/WorldObjects/WorldObject.cs).
/// Overridden by: Ammunition, Creature, Door, Hotspot, Player, PressurePlate, SpellProjectile.
pub fn on_collide_object(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::Ammunition => {
            crate::world_objects::ammunition::ammunition_on_collide_object(w, this, target)
        }
        Class::CombatPet
        | Class::Cow
        | Class::Creature
        | Class::GamePiece
        | Class::Pet
        | Class::Vendor => {
            crate::world_objects::creature::creature_on_collide_object(w, this, target)
        }
        Class::Door => crate::world_objects::door::door_on_collide_object(w, this, target),
        Class::Hotspot => crate::world_objects::hotspot::hotspot_on_collide_object(w, this, target),
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player::player_on_collide_object(w, this, target)
        }
        Class::PressurePlate => {
            crate::world_objects::pressure_plate::pressure_plate_on_collide_object(w, this, target)
        }
        Class::SpellProjectile => {
            crate::world_objects::spell_projectile::spell_projectile_on_collide_object(
                w, this, target,
            )
        }
        _ => crate::world_objects::world_object::world_object_on_collide_object(w, this, target),
    }
}

/// Dispatch for `Portal.OnCollideObject(Player player)` (Source/ACE.Server/WorldObjects/Portal.cs).
/// Overridden by: none.
pub fn on_collide_object_player(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    player: empyrean_entity::ObjectGuid,
) {
    match class_of(w, this) {
        Class::HousePortal | Class::Portal => {
            crate::world_objects::portal::portal_on_collide_object_player(w, this, player)
        }
        other => crate::dispatch::wrong_class("Portal.OnCollideObject", other),
    }
}
