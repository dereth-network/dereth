// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/WorldObject_Combat.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/WorldObject_Combat.cs`; do not edit by hand
//! Virtual dispatch for `WorldObject.CheckPKStatusVsTarget`: each call
//! runs the implementation of the nearest class up the object's chain that declares or overrides
//! the member. The rules are in `dispatch/mod.rs`.

use crate::dispatch::{class_of, Class};

/// Dispatch for `WorldObject.CheckPKStatusVsTarget(WorldObject target, Spell spell)` (Source/ACE.Server/WorldObjects/WorldObject_Combat.cs).
/// Overridden by: Player.
pub fn check_pk_status_vs_target(
    w: &mut crate::World,
    this: empyrean_entity::ObjectGuid,
    target: empyrean_entity::ObjectGuid,
    spell: (),
) -> Option<Vec<empyrean_entity::enums::WeenieErrorWithString>> {
    match class_of(w, this) {
        Class::Admin | Class::Player | Class::Sentinel => {
            crate::world_objects::player_combat::player_check_pk_status_vs_target(
                w, this, target, spell,
            )
        }
        _ => crate::world_objects::world_object_combat::world_object_check_pk_status_vs_target(
            w, this, target, spell,
        ),
    }
}
