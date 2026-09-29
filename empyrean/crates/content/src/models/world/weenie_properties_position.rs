// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesPosition.cs
//! `WeeniePropertiesPosition`: one row of the world-database table `weenie_properties_position`.

/// Position Properties of Weenies
// ACE: WeeniePropertiesPosition
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesPosition {
    /// Unique Id of this Position
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Position the value applies to (PositionType.????)
    pub position_type: u16,
    pub obj_cell_id: u32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub origin_z: f32,
    pub angles_w: f32,
    pub angles_x: f32,
    pub angles_y: f32,
    pub angles_z: f32,
    // ACE: WeeniePropertiesPosition.Object navigates back to the parent row, not carried.
}
