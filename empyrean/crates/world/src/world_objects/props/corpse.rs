// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Corpse.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Corpse.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Corpse.cs`.

use empyrean_entity::enums::PropertyBool;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Corpse.CorpseGeneratedRare
    pub fn corpse_generated_rare(&self) -> bool {
        self.get_property(PropertyBool::CorpseGeneratedRare)
            .unwrap_or(false)
    }

    // ACE: Corpse.CorpseGeneratedRare
    pub fn set_corpse_generated_rare(&mut self, value: bool) {
        if !value {
            self.remove_property(PropertyBool::CorpseGeneratedRare);
        } else {
            self.set_property(PropertyBool::CorpseGeneratedRare, value);
        }
    }
}
