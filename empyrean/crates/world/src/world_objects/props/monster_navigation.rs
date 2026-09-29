// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Navigation.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Monster_Navigation.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Monster_Navigation.cs`.

use empyrean_entity::enums::PropertyFloat;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Creature.HomeRadius
    pub fn home_radius(&self) -> Option<f64> {
        self.get_property(PropertyFloat::HomeRadius)
    }

    // ACE: Creature.HomeRadius
    pub fn set_home_radius(&mut self, value: Option<f64>) {
        match value {
            None => self.remove_property(PropertyFloat::HomeRadius),
            Some(v) => self.set_property(PropertyFloat::HomeRadius, v),
        }
    }
}
