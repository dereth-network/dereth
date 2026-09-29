// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/CharacterPropertiesContractRegistry.cs
//! `CharacterPropertiesContractRegistry`: a row of the `shard` database (Entity Framework model).

// ACE: CharacterPropertiesContractRegistry
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterPropertiesContractRegistry {
    // ACE: CharacterPropertiesContractRegistry.CharacterId
    pub character_id: u32,
    // ACE: CharacterPropertiesContractRegistry.ContractId
    pub contract_id: u32,
    // ACE: CharacterPropertiesContractRegistry.DeleteContract
    pub delete_contract: bool,
    // ACE: CharacterPropertiesContractRegistry.SetAsDisplayContract
    pub set_as_display_contract: bool,
}
