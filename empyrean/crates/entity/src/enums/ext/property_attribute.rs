// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Properties/PropertyAttribute.cs
//
// Also ports `PropertyAttribute2ndExtensions` from Properties/PropertyAttribute2nd.cs.
//
// `GetDescription` reads a `[Description]` attribute and falls back to `ToString()`. No member of
// either enum carries `[Description]` (see the attribute inventory), so the fallback is all there is.

use super::spaced_capitals;
use crate::enums::{PropertyAttribute, PropertyAttribute2nd};

impl PropertyAttribute {
    // ACE: PropertyAttributeExtensions.GetDescription
    pub fn get_description(self) -> String {
        self.to_dotnet_string()
    }
}

impl PropertyAttribute2nd {
    // ACE: PropertyAttribute2ndExtensions.GetDescription
    pub fn get_description(self) -> String {
        self.to_dotnet_string()
    }

    /// Adds a space in front of each capitalised word, spelling out "Max".
    // ACE: PropertyAttribute2ndExtensions.ToSentence
    pub fn to_sentence(self) -> String {
        let s = match self {
            PropertyAttribute2nd::Undef => "Undef",
            PropertyAttribute2nd::MaxHealth => "Maximum Health",
            PropertyAttribute2nd::Health => "Health",
            PropertyAttribute2nd::MaxStamina => "Maximum Stamina",
            PropertyAttribute2nd::Stamina => "Stamina",
            PropertyAttribute2nd::MaxMana => "Maximum Mana",
            PropertyAttribute2nd::Mana => "Mana",
            _ => return spaced_capitals(&self.to_dotnet_string().replace("Max", "Maximum")),
        };
        s.into()
    }
}
