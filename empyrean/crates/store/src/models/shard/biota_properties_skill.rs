// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/BiotaPropertiesSkill.cs
//! `BiotaPropertiesSkill`: a row of the `shard` database (Entity Framework model).

// ACE: BiotaPropertiesSkill
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BiotaPropertiesSkill {
    // ACE: BiotaPropertiesSkill.ObjectId
    pub object_id: u32,
    // ACE: BiotaPropertiesSkill.Type
    pub r#type: u16,
    // ACE: BiotaPropertiesSkill.LevelFromPP
    pub level_from_pp: u16,
    // ACE: BiotaPropertiesSkill.SAC
    pub sac: u32,
    // ACE: BiotaPropertiesSkill.PP
    pub pp: u32,
    // ACE: BiotaPropertiesSkill.InitLevel
    pub init_level: u32,
    // ACE: BiotaPropertiesSkill.ResistanceAtLastCheck
    pub resistance_at_last_check: u32,
    // ACE: BiotaPropertiesSkill.LastUsedTime
    pub last_used_time: f64,
}
