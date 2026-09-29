// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/CharacterPropertiesFillCompBook.cs
//! `CharacterPropertiesFillCompBook`: a row of the `shard` database (Entity Framework model).

// ACE: CharacterPropertiesFillCompBook
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterPropertiesFillCompBook {
    // ACE: CharacterPropertiesFillCompBook.CharacterId
    pub character_id: u32,
    // ACE: CharacterPropertiesFillCompBook.SpellComponentId
    pub spell_component_id: i32,
    // ACE: CharacterPropertiesFillCompBook.QuantityToRebuy
    pub quantity_to_rebuy: i32,
}
