// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Common/Extensions/CharacterNameExtensions.cs
//! `CharacterNameExtensions`: joins command arguments back into a name.

// ACE: CharacterNameExtensions.StringArrayToCharacterName
/// Joins `name_strings[starting_element..]` with single spaces.
///
/// # Panics
/// When `starting_element` is past the end (`IndexOutOfRangeException`).
#[must_use]
pub fn string_array_to_character_name(name_strings: &[&str], starting_element: usize) -> String {
    // Store the first part of the player name.
    let mut character_name = name_strings[starting_element].to_owned();
    // ACE-BUG: the join is skipped unless the array has more than two elements, so
    // `["A", "B"]` from element 0 yields "A".
    if name_strings.len() > 2 {
        for name in &name_strings[starting_element + 1..] {
            character_name = character_name + " " + name;
        }
    }
    character_name
}
