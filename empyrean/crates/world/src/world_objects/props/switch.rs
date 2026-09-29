// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Switch.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Switch.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Switch.cs`.

use empyrean_entity::enums::PropertyDataId;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Switch.UseTargetAnimation
    pub fn use_target_animation(&self) -> Option<u32> {
        self.get_property(PropertyDataId::UseTargetAnimation)
    }

    // ACE: Switch.UseTargetAnimation
    pub fn set_use_target_animation(&mut self, value: Option<u32>) {
        match value {
            None => self.remove_property(PropertyDataId::UseTargetAnimation),
            Some(v) => self.set_property(PropertyDataId::UseTargetAnimation, v),
        }
    }
}
