// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Monster_Properties.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Monster_Properties.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Monster_Properties.cs`.

use empyrean_entity::enums::PropertyInt;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Creature.AiOptions
    pub fn ai_options(&self) -> i32 {
        self.get_property(PropertyInt::AiOptions).unwrap_or(0)
    }

    // ACE: Creature.AiOptions
    pub fn set_ai_options(&mut self, value: i32) {
        if value == 0 {
            self.remove_property(PropertyInt::AiOptions);
        } else {
            self.set_property(PropertyInt::AiOptions, value);
        }
    }
}
