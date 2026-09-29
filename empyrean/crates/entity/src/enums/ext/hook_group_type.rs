// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/HookGroupType.cs

use super::spaced_capitals;
use crate::enums::HookGroupType;

impl HookGroupType {
    /// Adds a space in front of each capitalised word.
    // ACE: HookGroupTypeExtensions.ToSentence
    pub fn to_sentence(self) -> String {
        match self {
            HookGroupType::Undef => return "Undef".into(),
            HookGroupType::NoisemakingItems => return "Noisemaking Items".into(),
            HookGroupType::TestItems => return "Test Items".into(),
            HookGroupType::PortalItems => return "Portal Items".into(),
            HookGroupType::WritableItems => return "Writable Items".into(),
            HookGroupType::SpellCastingItems => return "Spell Casting Items".into(),
            HookGroupType::SpellTeachingItems => return "Spell Teaching Items".into(),
            _ => {}
        }

        spaced_capitals(&self.to_dotnet_string())
    }
}
