// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesAttribute.cs
//! `BiotaPropertiesAttribute`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesAttribute
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesAttribute {
    // ACE: BiotaPropertiesAttribute.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesAttribute.Type
    pub r#type: u16,
    // ACE: BiotaPropertiesAttribute.InitLevel
    pub init_level: u32,
    // ACE: BiotaPropertiesAttribute.LevelFromCP
    pub level_from_cp: u32,
    // ACE: BiotaPropertiesAttribute.CPSpent
    pub cp_spent: u32,
}
