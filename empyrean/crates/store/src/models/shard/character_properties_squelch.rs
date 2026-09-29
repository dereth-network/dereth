// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/CharacterPropertiesSquelch.cs
//! `CharacterPropertiesSquelch`: a row of the `shard` database (Entity Framework model).

// ACE: CharacterPropertiesSquelch
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterPropertiesSquelch {
    // ACE: CharacterPropertiesSquelch.CharacterId
    pub character_id: u32,
    // ACE: CharacterPropertiesSquelch.SquelchCharacterId
    pub squelch_character_id: u32,
    // ACE: CharacterPropertiesSquelch.SquelchAccountId
    pub squelch_account_id: u32,
    // ACE: CharacterPropertiesSquelch.Type
    pub r#type: u32,
}
