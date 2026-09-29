// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/CharacterPropertiesSpellBar.cs
//! `CharacterPropertiesSpellBar`: a row of the `shard` database (Entity Framework model).

// ACE: CharacterPropertiesSpellBar
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterPropertiesSpellBar {
    // ACE: CharacterPropertiesSpellBar.CharacterId
    pub character_id: u32,
    // ACE: CharacterPropertiesSpellBar.SpellBarNumber
    pub spell_bar_number: u32,
    // ACE: CharacterPropertiesSpellBar.SpellBarIndex
    pub spell_bar_index: u32,
    // ACE: CharacterPropertiesSpellBar.SpellId
    pub spell_id: u32,
}
