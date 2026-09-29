// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/CharacterPropertiesShortcutBar.cs
//! `CharacterPropertiesShortcutBar`: a row of the `shard` database (Entity Framework model).

// ACE: CharacterPropertiesShortcutBar
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharacterPropertiesShortcutBar {
    // ACE: CharacterPropertiesShortcutBar.CharacterId
    pub character_id: u32,
    // ACE: CharacterPropertiesShortcutBar.ShortcutBarIndex
    pub shortcut_bar_index: u32,
    // ACE: CharacterPropertiesShortcutBar.ShortcutObjectId
    pub shortcut_object_id: u32,
}
