// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesAnimPart.cs
//! `BiotaPropertiesAnimPart`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesAnimPart
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesAnimPart {
    // ACE: BiotaPropertiesAnimPart.Id
    pub id: u32,
    // ACE: BiotaPropertiesAnimPart.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesAnimPart.Index
    pub index: u8,
    // ACE: BiotaPropertiesAnimPart.AnimationId
    pub animation_id: u32,
    // ACE: BiotaPropertiesAnimPart.Order
    pub order: Option<u8>,
}
