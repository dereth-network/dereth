// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesBool.cs
//! `WeeniePropertiesBool`: one row of the world-database table `weenie_properties_bool`.

/// Bool Properties of Weenies
// ACE: WeeniePropertiesBool
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesBool {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertyBool.????)
    pub r#type: u16,
    /// Value of this Property
    pub value: bool,
    // ACE: WeeniePropertiesBool.Object navigates back to the parent row, not carried.
}
