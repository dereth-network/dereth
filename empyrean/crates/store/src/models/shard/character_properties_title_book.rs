// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/CharacterPropertiesTitleBook.cs
//! `CharacterPropertiesTitleBook`: a row of the `shard` database (Entity Framework model).

// ACE: CharacterPropertiesTitleBook
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterPropertiesTitleBook {
    // ACE: CharacterPropertiesTitleBook.CharacterId
    pub character_id: u32,
    // ACE: CharacterPropertiesTitleBook.TitleId
    pub title_id: u32,
}
