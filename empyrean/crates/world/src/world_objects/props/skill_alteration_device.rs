// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/SkillAlterationDevice.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/SkillAlterationDevice.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/SkillAlterationDevice.cs`.

use empyrean_entity::enums::{PropertyInt, Skill};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: SkillAlterationDevice.TypeOfAlteration
    pub fn type_of_alteration(
        &self,
    ) -> crate::world_objects::skill_alteration_device::SkillAlterationType {
        crate::world_objects::skill_alteration_device::SkillAlterationType(
            self.get_property(PropertyInt::TypeOfAlteration)
                .unwrap_or(0),
        )
    }

    // ACE: SkillAlterationDevice.TypeOfAlteration
    pub fn set_type_of_alteration(
        &mut self,
        value: crate::world_objects::skill_alteration_device::SkillAlterationType,
    ) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::TypeOfAlteration);
        } else {
            self.set_property(PropertyInt::TypeOfAlteration, value.0);
        }
    }

    // ACE: SkillAlterationDevice.SkillToBeAltered
    pub fn skill_to_be_altered(&self) -> Skill {
        Skill(
            self.get_property(PropertyInt::SkillToBeAltered)
                .unwrap_or(0),
        )
    }

    // ACE: SkillAlterationDevice.SkillToBeAltered
    pub fn set_skill_to_be_altered(&mut self, value: Skill) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::SkillToBeAltered);
        } else {
            self.set_property(PropertyInt::SkillToBeAltered, value.0);
        }
    }
}
