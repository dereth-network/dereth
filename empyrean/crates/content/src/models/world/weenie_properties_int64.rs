// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesInt64.cs
//! `WeeniePropertiesInt64`: one row of the world-database table `weenie_properties_int64`.

/// Int64 Properties of Weenies
// ACE: WeeniePropertiesInt64
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesInt64 {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertyInt64.????)
    pub r#type: u16,
    /// Value of this Property
    pub value: i64,
    // ACE: WeeniePropertiesInt64.Object navigates back to the parent row, not carried.
}
