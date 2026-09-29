// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesGenerator.cs
//! `BiotaPropertiesGenerator`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesGenerator
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesGenerator {
    // ACE: BiotaPropertiesGenerator.Id
    pub id: u32,
    // ACE: BiotaPropertiesGenerator.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesGenerator.Probability
    pub probability: f32,
    // ACE: BiotaPropertiesGenerator.WeenieClassId
    pub weenie_class_id: u32,
    // ACE: BiotaPropertiesGenerator.Delay
    pub delay: Option<f32>,
    // ACE: BiotaPropertiesGenerator.InitCreate
    pub init_create: i32,
    // ACE: BiotaPropertiesGenerator.MaxCreate
    pub max_create: i32,
    // ACE: BiotaPropertiesGenerator.WhenCreate
    pub when_create: u32,
    // ACE: BiotaPropertiesGenerator.WhereCreate
    pub where_create: u32,
    // ACE: BiotaPropertiesGenerator.StackSize
    pub stack_size: Option<i32>,
    // ACE: BiotaPropertiesGenerator.PaletteId
    pub palette_id: Option<u32>,
    // ACE: BiotaPropertiesGenerator.Shade
    pub shade: Option<f32>,
    // ACE: BiotaPropertiesGenerator.ObjCellId
    pub obj_cell_id: Option<u32>,
    // ACE: BiotaPropertiesGenerator.OriginX
    pub origin_x: Option<f32>,
    // ACE: BiotaPropertiesGenerator.OriginY
    pub origin_y: Option<f32>,
    // ACE: BiotaPropertiesGenerator.OriginZ
    pub origin_z: Option<f32>,
    // ACE: BiotaPropertiesGenerator.AnglesW
    pub angles_w: Option<f32>,
    // ACE: BiotaPropertiesGenerator.AnglesX
    pub angles_x: Option<f32>,
    // ACE: BiotaPropertiesGenerator.AnglesY
    pub angles_y: Option<f32>,
    // ACE: BiotaPropertiesGenerator.AnglesZ
    pub angles_z: Option<f32>,
}
