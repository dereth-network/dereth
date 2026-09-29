// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesInt.cs
//! `BiotaPropertiesInt`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesInt
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesInt {
    // ACE: BiotaPropertiesInt.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesInt.Type
    pub r#type: u16,
    // ACE: BiotaPropertiesInt.Value
    pub value: i32,
}
