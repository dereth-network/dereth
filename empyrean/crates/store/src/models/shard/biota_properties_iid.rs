// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesIID.cs
//! `BiotaPropertiesIID`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesIID
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesIID {
    // ACE: BiotaPropertiesIID.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesIID.Type
    pub r#type: u16,
    // ACE: BiotaPropertiesIID.Value
    pub value: u32,
}
