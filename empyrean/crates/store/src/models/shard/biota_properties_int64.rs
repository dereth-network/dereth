// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesInt64.cs
//! `BiotaPropertiesInt64`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesInt64
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesInt64 {
    // ACE: BiotaPropertiesInt64.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesInt64.Type
    pub r#type: u16,
    // ACE: BiotaPropertiesInt64.Value
    pub value: i64,
}
