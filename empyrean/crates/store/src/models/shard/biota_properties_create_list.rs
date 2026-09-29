// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesCreateList.cs
//! `BiotaPropertiesCreateList`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesCreateList
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesCreateList {
    // ACE: BiotaPropertiesCreateList.Id
    pub id: u32,
    // ACE: BiotaPropertiesCreateList.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesCreateList.DestinationType
    pub destination_type: i8,
    // ACE: BiotaPropertiesCreateList.WeenieClassId
    pub weenie_class_id: u32,
    // ACE: BiotaPropertiesCreateList.StackSize
    pub stack_size: i32,
    // ACE: BiotaPropertiesCreateList.Palette
    pub palette: i8,
    // ACE: BiotaPropertiesCreateList.Shade
    pub shade: f32,
    // ACE: BiotaPropertiesCreateList.TryToBond
    pub try_to_bond: bool,
}
