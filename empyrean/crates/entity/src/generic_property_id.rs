// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/GenericPropertyId.cs
//! `GenericPropertyId`: a property id tagged with its property type.

use crate::enums::PropertyType;

/// ACE: GenericPropertyId. A value type; C# struct equality compares both fields.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct GenericPropertyId {
    // ACE: GenericPropertyId.PropertyId
    pub property_id: u32,
    // ACE: GenericPropertyId.PropertyType
    pub property_type: PropertyType,
}

impl GenericPropertyId {
    // ACE: GenericPropertyId.GenericPropertyId
    #[must_use]
    pub fn new(property_id: u32, property_type: PropertyType) -> Self {
        GenericPropertyId {
            property_id,
            property_type,
        }
    }
}
