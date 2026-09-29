// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesDID.cs
//! `BiotaPropertiesDID`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesDID
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesDID {
    // ACE: BiotaPropertiesDID.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesDID.Type
    pub r#type: u16,
    // ACE: BiotaPropertiesDID.Value
    pub value: u32,
}
