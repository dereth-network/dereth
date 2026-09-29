// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesInt.cs
//! `WeeniePropertiesInt`: one row of the world-database table `weenie_properties_int`.

/// Int Properties of Weenies
// ACE: WeeniePropertiesInt
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesInt {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertyInt.????)
    pub r#type: u16,
    /// Value of this Property
    pub value: i32,
    // ACE: WeeniePropertiesInt.Object navigates back to the parent row, not carried.
}
