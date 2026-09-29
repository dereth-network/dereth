// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Pet.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/Pet.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/Pet.cs`.

use empyrean_entity::enums::PropertyInstanceId;

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: Pet.PetDevice
    pub fn pet_device(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::PetDevice)
    }

    // ACE: Pet.PetDevice
    pub fn set_pet_device(&mut self, value: Option<u32>) {
        match value {
            Some(v) => self.set_property(PropertyInstanceId::PetDevice, v),
            None => self.remove_property(PropertyInstanceId::PetDevice),
        }
    }
}
