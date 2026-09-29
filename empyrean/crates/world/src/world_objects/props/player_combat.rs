// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Player_Combat.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Player_Combat.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Player_Combat.cs`.

use empyrean_entity::enums::PropertyFloat;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Player.LastPkAttackTimestamp
    pub fn last_pk_attack_timestamp(&self) -> f64 {
        self.get_property(PropertyFloat::LastPkAttackTimestamp)
            .unwrap_or(0.0)
    }

    // ACE: Player.LastPkAttackTimestamp
    pub fn set_last_pk_attack_timestamp(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::LastPkAttackTimestamp);
        } else {
            self.set_property(PropertyFloat::LastPkAttackTimestamp, value);
        }
    }

    // ACE: Player.PkTimestamp
    pub fn pk_timestamp(&self) -> f64 {
        self.get_property(PropertyFloat::PkTimestamp).unwrap_or(0.0)
    }

    // ACE: Player.PkTimestamp
    pub fn set_pk_timestamp(&mut self, value: f64) {
        if value == 0.0 {
            self.remove_property(PropertyFloat::PkTimestamp);
        } else {
            self.set_property(PropertyFloat::PkTimestamp, value);
        }
    }
}
