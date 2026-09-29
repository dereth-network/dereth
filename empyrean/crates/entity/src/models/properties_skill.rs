// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesSkill.cs
//! `PropertiesSkill`: a skill.

use crate::enums::SkillAdvancementClass;

/// ACE: PropertiesSkill. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesSkill::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PropertiesSkill {
    // ACE: PropertiesSkill.LevelFromPP
    pub level_from_pp: u16,
    // ACE: PropertiesSkill.SAC
    pub sac: SkillAdvancementClass,
    // ACE: PropertiesSkill.PP
    pub pp: u32,
    // ACE: PropertiesSkill.InitLevel
    pub init_level: u32,
    // ACE: PropertiesSkill.ResistanceAtLastCheck
    pub resistance_at_last_check: u32,
    // ACE: PropertiesSkill.LastUsedTime
    pub last_used_time: f64,
}

impl PropertiesSkill {
    /// ACE's `Clone()`: a copy of every field.
    // ACE: PropertiesSkill.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesSkill {
        PropertiesSkill {
            level_from_pp: self.level_from_pp,
            sac: self.sac,
            pp: self.pp,
            init_level: self.init_level,
            resistance_at_last_check: self.resistance_at_last_check,
            last_used_time: self.last_used_time,
        }
    }
}
