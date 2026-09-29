// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesString.cs
//! `WeeniePropertiesString`: one row of the world-database table `weenie_properties_string`.

/// String Properties of Weenies
// ACE: WeeniePropertiesString
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesString {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertyString.????)
    pub r#type: u16,
    /// Value of this Property
    pub value: String,
    // ACE: WeeniePropertiesString.Object navigates back to the parent row, not carried.
}
