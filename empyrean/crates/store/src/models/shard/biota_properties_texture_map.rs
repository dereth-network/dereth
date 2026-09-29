// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesTextureMap.cs
//! `BiotaPropertiesTextureMap`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesTextureMap
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesTextureMap {
    // ACE: BiotaPropertiesTextureMap.Id
    pub id: u32,
    // ACE: BiotaPropertiesTextureMap.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesTextureMap.Index
    pub index: u8,
    // ACE: BiotaPropertiesTextureMap.OldId
    pub old_id: u32,
    // ACE: BiotaPropertiesTextureMap.NewId
    pub new_id: u32,
    // ACE: BiotaPropertiesTextureMap.Order
    pub order: Option<u8>,
}
