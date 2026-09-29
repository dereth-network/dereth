// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Models/PropertiesAttribute.cs
//! `PropertiesAttribute`: a primary attribute.

/// ACE: PropertiesAttribute. `Clone` copies every field; ACE's own `Clone()` is [`PropertiesAttribute::ace_clone`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PropertiesAttribute {
    // ACE: PropertiesAttribute.InitLevel
    pub init_level: u32,
    // ACE: PropertiesAttribute.LevelFromCP
    pub level_from_cp: u32,
    // ACE: PropertiesAttribute.CPSpent
    pub cp_spent: u32,
}

impl PropertiesAttribute {
    /// ACE's `Clone()`: a copy of every field.
    // ACE: PropertiesAttribute.Clone
    #[must_use]
    pub fn ace_clone(&self) -> PropertiesAttribute {
        PropertiesAttribute {
            init_level: self.init_level,
            level_from_cp: self.level_from_cp,
            cp_spent: self.cp_spent,
        }
    }
}
