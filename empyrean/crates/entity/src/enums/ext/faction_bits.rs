// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/FactionBits.cs

use super::spaced_capitals;
use crate::enums::FactionBits;

impl FactionBits {
    /// Adds a space in front of each capitalised word.
    // ACE: FactionBitsExtensions.ToSentence
    pub fn to_sentence(self) -> String {
        match self {
            FactionBits::None => return "None".into(),
            FactionBits::CelestialHand => return "Celestial Hand".into(),
            FactionBits::EldrytchWeb => return "Eldrytch Web".into(),
            FactionBits::RadiantBlood => return "Radiant Blood".into(),
            _ => {}
        }

        spaced_capitals(&self.to_dotnet_string())
    }
}
