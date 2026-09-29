// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/PetDevice.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/PetDevice.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/PetDevice.cs`.

use empyrean_entity::enums::{PropertyInstanceId, PropertyInt};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: PetDevice.PetClass
    pub fn pet_class(&self) -> Option<i32> {
        self.get_property(PropertyInt::PetClass)
    }

    // ACE: PetDevice.PetClass
    pub fn set_pet_class(&mut self, value: Option<i32>) {
        match value {
            Some(v) => self.set_property(PropertyInt::PetClass, v),
            None => self.remove_property(PropertyInt::PetClass),
        }
    }

    // ACE: PetDevice.Pet
    pub fn pet(&self) -> Option<u32> {
        self.get_property(PropertyInstanceId::Pet)
    }

    // ACE: PetDevice.Pet
    pub fn set_pet(&mut self, value: Option<u32>) {
        match value {
            Some(v) => self.set_property(PropertyInstanceId::Pet, v),
            None => self.remove_property(PropertyInstanceId::Pet),
        }
    }
}
