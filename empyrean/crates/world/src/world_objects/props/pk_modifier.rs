// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/PKModifier.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/PKModifier.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/PKModifier.cs`.

use empyrean_entity::enums::PropertyFloat;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: PKModifier.MinimumTimeSincePk, Player.MinimumTimeSincePk
    pub fn minimum_time_since_pk(&self) -> Option<f64> {
        self.get_property(PropertyFloat::MinimumTimeSincePk)
    }

    // ACE: PKModifier.MinimumTimeSincePk, Player.MinimumTimeSincePk
    pub fn set_minimum_time_since_pk_prop(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::MinimumTimeSincePk),
            Some(v) => self.set_property(PropertyFloat::MinimumTimeSincePk, v),
        }
    }
}
