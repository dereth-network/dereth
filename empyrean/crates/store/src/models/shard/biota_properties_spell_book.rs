// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesSpellBook.cs
//! `BiotaPropertiesSpellBook`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesSpellBook
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesSpellBook {
    // ACE: BiotaPropertiesSpellBook.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesSpellBook.Spell
    pub spell: i32,
    // ACE: BiotaPropertiesSpellBook.Probability
    pub probability: f32,
}
