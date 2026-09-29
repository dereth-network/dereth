// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesString.cs
//! `BiotaPropertiesString`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesString
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesString {
    // ACE: BiotaPropertiesString.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesString.Type
    pub r#type: u16,
    // ACE: BiotaPropertiesString.Value
    pub value: String,
}
