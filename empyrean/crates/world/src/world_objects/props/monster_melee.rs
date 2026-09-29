// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Melee.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Monster_Melee.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Monster_Melee.cs`.

use empyrean_entity::enums::PropertyFloat;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Creature.PowerupTime
    pub fn powerup_time(&self) -> Option<f64> {
        self.get_property(PropertyFloat::PowerupTime)
    }

    // ACE: Creature.PowerupTime
    pub fn set_powerup_time(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::PowerupTime),
            Some(v) => self.set_property(PropertyFloat::PowerupTime, v),
        }
    }
}
