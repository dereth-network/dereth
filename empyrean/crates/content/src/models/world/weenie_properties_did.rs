// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesDID.cs
//! `WeeniePropertiesDID`: one row of the world-database table `weenie_properties_d_i_d`.

/// DataID Properties of Weenies
// ACE: WeeniePropertiesDID
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesDID {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertyDataId.????)
    pub r#type: u16,
    /// Value of this Property
    pub value: u32,
    // ACE: WeeniePropertiesDID.Object navigates back to the parent row, not carried.
}
