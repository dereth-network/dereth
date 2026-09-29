// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesBool.cs
//! `BiotaPropertiesBool`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesBool
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesBool {
    // ACE: BiotaPropertiesBool.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesBool.Type
    pub r#type: u16,
    // ACE: BiotaPropertiesBool.Value
    pub value: bool,
}
