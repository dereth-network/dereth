// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesFloat.cs
//! `WeeniePropertiesFloat`: one row of the world-database table `weenie_properties_float`.

/// Float Properties of Weenies
// ACE: WeeniePropertiesFloat
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesFloat {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertyFloat.????)
    pub r#type: u16,
    /// Value of this Property
    pub value: f64,
    // ACE: WeeniePropertiesFloat.Object navigates back to the parent row, not carried.
}
