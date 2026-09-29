// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CharacterOption.cs
//
// ACE's `GetCharacterOptions1Attribute`/`GetCharacterOptions2Attribute` read the member attributes
// by reflection; the generated `CharacterOption::character_options1`/`character_options2` return
// the attribute's `Option` directly (the `ACE: AttributeExtensions.GetAttributeOfType` pattern).

use crate::enums::CharacterOption;

/// ACE's `CharacterOptionExtensions` flag builders. ACE takes a `Dictionary<CharacterOption,
/// bool>` (or its read-only wrapper); OR-ing is order-independent, so any iterator of pairs will do.
pub mod character_option_extensions {
    use super::*;

    // ACE: CharacterOptionExtensions.GetCharacterOptions1Flag
    pub fn get_character_options1_flag(
        options: impl IntoIterator<Item = (CharacterOption, bool)>,
    ) -> u32 {
        let mut flags = 0u32;
        for (key, value) in options {
            let Some(option) = key.character_options1() else {
                continue;
            };
            if value {
                flags |= option.0;
            }
        }

        flags
    }

    // ACE: CharacterOptionExtensions.GetCharacterOptions2Flag
    pub fn get_character_options2_flag(
        options: impl IntoIterator<Item = (CharacterOption, bool)>,
    ) -> u32 {
        let mut flags = 0u32;
        for (key, value) in options {
            let Some(option) = key.character_options2() else {
                continue;
            };
            if value {
                flags |= option.0;
            }
        }

        flags
    }
}

#[cfg(test)]
mod tests {
    use crate::enums::{CharacterOption, CharacterOptions2};

    /// V384: the default second option word is the client's, with hear-PK-deaths set, and it is
    /// the word the client itself assumes when a player module carries none.
    #[test]
    fn the_default_second_option_word_hears_pk_deaths() {
        assert_eq!(CharacterOptions2::Default.0, 0x0294_8700);
        assert_eq!(
            CharacterOptions2::Default.0,
            dereth_protocol::login::PlayerModule::DEFAULT_OPTIONS2
        );
        assert_ne!(
            CharacterOptions2::Default.0 & CharacterOptions2::HearPKDeath.0,
            0
        );
        assert_eq!(
            super::character_option_extensions::get_character_options2_flag([(
                CharacterOption::CharacterOptions2Default,
                true
            )]),
            0x0294_8700
        );
    }
}
