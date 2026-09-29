// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesAttribute2nd.cs
//! `BiotaPropertiesAttribute2nd`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesAttribute2nd
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesAttribute2nd {
    // ACE: BiotaPropertiesAttribute2nd.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesAttribute2nd.Type
    pub r#type: u16,
    // ACE: BiotaPropertiesAttribute2nd.InitLevel
    pub init_level: u32,
    // ACE: BiotaPropertiesAttribute2nd.LevelFromCP
    pub level_from_cp: u32,
    // ACE: BiotaPropertiesAttribute2nd.CPSpent
    pub cp_spent: u32,
    // ACE: BiotaPropertiesAttribute2nd.CurrentLevel
    pub current_level: u32,
}
