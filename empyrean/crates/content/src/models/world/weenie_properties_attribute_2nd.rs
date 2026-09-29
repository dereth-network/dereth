// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesAttribute2nd.cs
//! `WeeniePropertiesAttribute2nd`: one row of the world-database table `weenie_properties_attribute_2nd`.

/// Attribute2nd (Vital) Properties of Weenies
// ACE: WeeniePropertiesAttribute2nd
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesAttribute2nd {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertyAttribute2nd.????)
    pub r#type: u16,
    /// innate points
    pub init_level: u32,
    /// points raised
    pub level_from_cp: u32,
    /// XP spent on this attribute
    pub cp_spent: u32,
    /// current value of the vital
    pub current_level: u32,
    // ACE: WeeniePropertiesAttribute2nd.Object navigates back to the parent row, not carried.
}
