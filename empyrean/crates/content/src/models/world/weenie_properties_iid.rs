// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesIID.cs
//! `WeeniePropertiesIID`: one row of the world-database table `weenie_properties_i_i_d`.

/// InstanceID Properties of Weenies
// ACE: WeeniePropertiesIID
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesIID {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertyInstanceId.????)
    pub r#type: u16,
    /// Value of this Property
    pub value: u32,
    // ACE: WeeniePropertiesIID.Object navigates back to the parent row, not carried.
}
