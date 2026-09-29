// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Creature_Death.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Creature_Death.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Creature_Death.cs`.

use empyrean_entity::enums::PropertyBool;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Creature.CanGenerateRare
    pub fn can_generate_rare(&self) -> bool {
        self.get_property(PropertyBool::CanGenerateRare)
            .unwrap_or(false)
    }

    // ACE: Creature.CanGenerateRare
    pub fn set_can_generate_rare(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::CanGenerateRare);
        } else {
            self.set_property(PropertyBool::CanGenerateRare, value);
        }
    }
}
