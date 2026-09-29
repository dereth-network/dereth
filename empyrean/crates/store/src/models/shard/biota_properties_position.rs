// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesPosition.cs
//! `BiotaPropertiesPosition`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesPosition
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesPosition {
    // ACE: BiotaPropertiesPosition.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesPosition.PositionType
    pub position_type: u16,
    // ACE: BiotaPropertiesPosition.ObjCellId
    pub obj_cell_id: u32,
    // ACE: BiotaPropertiesPosition.OriginX
    pub origin_x: f32,
    // ACE: BiotaPropertiesPosition.OriginY
    pub origin_y: f32,
    // ACE: BiotaPropertiesPosition.OriginZ
    pub origin_z: f32,
    // ACE: BiotaPropertiesPosition.AnglesW
    pub angles_w: f32,
    // ACE: BiotaPropertiesPosition.AnglesX
    pub angles_x: f32,
    // ACE: BiotaPropertiesPosition.AnglesY
    pub angles_y: f32,
    // ACE: BiotaPropertiesPosition.AnglesZ
    pub angles_z: f32,
}
