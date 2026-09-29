// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesPalette.cs
//! `BiotaPropertiesPalette`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesPalette
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesPalette {
    // ACE: BiotaPropertiesPalette.Id
    pub id: u32,
    // ACE: BiotaPropertiesPalette.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesPalette.SubPaletteId
    pub sub_palette_id: u32,
    // ACE: BiotaPropertiesPalette.Offset
    pub offset: u16,
    // ACE: BiotaPropertiesPalette.Length
    pub length: u16,
    // ACE: BiotaPropertiesPalette.Order
    pub order: Option<u8>,
}
