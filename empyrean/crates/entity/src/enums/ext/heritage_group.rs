// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/HeritageGroup.cs

use crate::enums::HeritageGroup;

impl HeritageGroup {
    // ACE: HeritageGroupExtensions.ToSentence
    pub fn to_sentence(self) -> String {
        match self {
            HeritageGroup::Gharundim => "Gharu'ndim".into(),
            HeritageGroup::Shadowbound => "Umbraen".into(),
            HeritageGroup::OlthoiAcid => "Olthoi".into(),
            _ => self.to_dotnet_string(),
        }
    }
}
