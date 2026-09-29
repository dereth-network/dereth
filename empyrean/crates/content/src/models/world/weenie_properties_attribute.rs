// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesAttribute.cs
//! `WeeniePropertiesAttribute`: one row of the world-database table `weenie_properties_attribute`.

/// Attribute Properties of Weenies
// ACE: WeeniePropertiesAttribute
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesAttribute {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertyAttribute.????)
    pub r#type: u16,
    /// innate points
    pub init_level: u32,
    /// points raised
    pub level_from_cp: u32,
    /// XP spent on this attribute
    pub cp_spent: u32,
    // ACE: WeeniePropertiesAttribute.Object navigates back to the parent row, not carried.
}
