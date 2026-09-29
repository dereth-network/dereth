// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesFloat.cs
//! `BiotaPropertiesFloat`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesFloat
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesFloat {
    // ACE: BiotaPropertiesFloat.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesFloat.Type
    pub r#type: u16,
    // ACE: BiotaPropertiesFloat.Value
    pub value: f64,
}
