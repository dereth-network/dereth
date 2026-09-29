// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/AttributeTransferDevice.cs
// @generated from ACE's `Source/ACE.Server/WorldObjects/AttributeTransferDevice.cs`; do not edit by hand
//! Typed property wrappers declared in `Source/ACE.Server/WorldObjects/AttributeTransferDevice.cs`.

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::{PropertyAttribute, PropertyInt};

use crate::world_objects::world_object::WorldObject;

impl WorldObject {
    // ACE: AttributeTransferDevice.TransferFromAttribute
    pub fn transfer_from_attribute(&self) -> PropertyAttribute {
        PropertyAttribute(
            self.get_property(PropertyInt::TransferFromAttribute)
                .unwrap_or(0)
                .cs_cast(),
        )
    }

    // ACE: AttributeTransferDevice.TransferFromAttribute
    pub fn set_transfer_from_attribute(&mut self, value: PropertyAttribute) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::TransferFromAttribute);
        } else {
            self.set_property(PropertyInt::TransferFromAttribute, value.0.cs_cast());
        }
    }

    // ACE: AttributeTransferDevice.TransferToAttribute
    pub fn transfer_to_attribute(&self) -> PropertyAttribute {
        PropertyAttribute(
            self.get_property(PropertyInt::TransferToAttribute)
                .unwrap_or(0)
                .cs_cast(),
        )
    }

    // ACE: AttributeTransferDevice.TransferToAttribute
    pub fn set_transfer_to_attribute(&mut self, value: PropertyAttribute) {
        if value.0 == 0 {
            self.remove_property(PropertyInt::TransferToAttribute);
        } else {
            self.set_property(PropertyInt::TransferToAttribute, value.0.cs_cast());
        }
    }
}
