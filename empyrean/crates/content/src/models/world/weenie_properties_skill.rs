// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/World/WeeniePropertiesSkill.cs
//! `WeeniePropertiesSkill`: one row of the world-database table `weenie_properties_skill`.

/// Skill Properties of Weenies
// ACE: WeeniePropertiesSkill
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WeeniePropertiesSkill {
    /// Unique Id of this Property
    pub id: u32,
    /// Id of the object this property belongs to
    pub object_id: u32,
    /// Type of Property the value applies to (PropertySkill.????)
    pub r#type: u16,
    /// points raised
    pub level_from_pp: u16,
    /// skill state
    pub sac: u32,
    /// XP spent on this skill
    pub pp: u32,
    /// starting point for advancement of the skill (eg bonus points)
    pub init_level: u32,
    /// last use difficulty
    pub resistance_at_last_check: u32,
    /// time skill was last used
    pub last_used_time: f64,
    // ACE: WeeniePropertiesSkill.Object navigates back to the parent row, not carried.
}
