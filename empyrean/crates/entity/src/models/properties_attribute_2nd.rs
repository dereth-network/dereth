// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesAttribute2nd.cs
//! `PropertiesAttribute2nd`: a secondary attribute (vital).

/// ACE: PropertiesAttribute2nd. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesAttribute2nd::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesAttribute2nd {
    // ACE: PropertiesAttribute2nd.InitLevel
    pub init_level: u32,
    // ACE: PropertiesAttribute2nd.LevelFromCP
    pub level_from_cp: u32,
    // ACE: PropertiesAttribute2nd.CPSpent
    pub cp_spent: u32,
    // ACE: PropertiesAttribute2nd.CurrentLevel
    pub current_level: u32,
}

impl PropertiesAttribute2nd {
    /// ACE's `Clone()`: a copy of every field.
    // ACE: PropertiesAttribute2nd.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesAttribute2nd {
        PropertiesAttribute2nd {
            init_level: self.init_level,
            level_from_cp: self.level_from_cp,
            cp_spent: self.cp_spent,
            current_level: self.current_level,
        }
    }
}
