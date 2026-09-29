// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/Character.cs
//! `Character`: a row of the `shard` database (Entity Framework model).

use super::CharacterPropertiesContractRegistry;
use super::CharacterPropertiesFillCompBook;
use super::CharacterPropertiesFriendList;
use super::CharacterPropertiesQuestRegistry;
use super::CharacterPropertiesShortcutBar;
use super::CharacterPropertiesSpellBar;
use super::CharacterPropertiesSquelch;
use super::CharacterPropertiesTitleBook;

/// `spellbook_Filters`' column default in ACE's shard schema (`ShardBase.sql`: `DEFAULT '16383'`,
/// and `ShardDbContext`: `HasDefaultValueSql("'16383'")`): every spell-book filter bit set.
pub const SPELLBOOK_FILTERS_DEFAULT: u32 = 16383;

// ACE: Character
#[derive(Debug, Clone, PartialEq)]
pub struct Character {
    // ACE: Character.Id
    pub id: u32,
    // ACE: Character.AccountId
    pub account_id: u32,
    // ACE: Character.Name
    pub name: String,
    // ACE: Character.IsPlussed
    pub is_plussed: bool,
    // ACE: Character.IsDeleted
    pub is_deleted: bool,
    // ACE: Character.DeleteTime
    pub delete_time: u64,
    // ACE: Character.LastLoginTimestamp
    pub last_login_timestamp: f64,
    // ACE: Character.TotalLogins
    pub total_logins: i32,
    // ACE: Character.CharacterOptions1
    pub character_options_1: i32,
    // ACE: Character.CharacterOptions2
    pub character_options_2: i32,
    // ACE: Character.GameplayOptions
    pub gameplay_options: Option<Vec<u8>>,
    // ACE: Character.SpellbookFilters
    pub spellbook_filters: u32,
    // ACE: Character.HairTexture
    pub hair_texture: u32,
    // ACE: Character.DefaultHairTexture
    pub default_hair_texture: u32,
    // ACE: Character.CharacterPropertiesContractRegistry
    pub character_properties_contract_registry: Vec<CharacterPropertiesContractRegistry>,
    // ACE: Character.CharacterPropertiesFillCompBook
    pub character_properties_fill_comp_book: Vec<CharacterPropertiesFillCompBook>,
    // ACE: Character.CharacterPropertiesFriendList
    pub character_properties_friend_list: Vec<CharacterPropertiesFriendList>,
    // ACE: Character.CharacterPropertiesQuestRegistry
    pub character_properties_quest_registry: Vec<CharacterPropertiesQuestRegistry>,
    // ACE: Character.CharacterPropertiesShortcutBar
    pub character_properties_shortcut_bar: Vec<CharacterPropertiesShortcutBar>,
    // ACE: Character.CharacterPropertiesSpellBar
    pub character_properties_spell_bar: Vec<CharacterPropertiesSpellBar>,
    // ACE: Character.CharacterPropertiesSquelch
    pub character_properties_squelch: Vec<CharacterPropertiesSquelch>,
    // ACE: Character.CharacterPropertiesTitleBook
    pub character_properties_title_book: Vec<CharacterPropertiesTitleBook>,
}

/// Every field at its CLR default, except `spellbook_filters`, which starts at the column default
/// [`SPELLBOOK_FILTERS_DEFAULT`].
///
/// DIVERGE: ACE's `new Character()` holds 0 until its insert: Entity Framework leaves a column
/// with a store default out of the INSERT when the property holds the CLR default, so the
/// database's 16383 applies, and reads the generated value back into the same (shared) object.
/// Snapshots cannot be written back to, so a new `Character` carries the value the insert would
/// give it; nothing reads it before the insert in ACE. The backends' inserts apply the same rule
/// to an explicit 0 (see `write_character`).
impl Default for Character {
    fn default() -> Self {
        Self {
            id: 0,
            account_id: 0,
            name: String::new(),
            is_plussed: false,
            is_deleted: false,
            delete_time: 0,
            last_login_timestamp: 0.0,
            total_logins: 0,
            character_options_1: 0,
            character_options_2: 0,
            gameplay_options: None,
            spellbook_filters: SPELLBOOK_FILTERS_DEFAULT,
            hair_texture: 0,
            default_hair_texture: 0,
            character_properties_contract_registry: Vec::new(),
            character_properties_fill_comp_book: Vec::new(),
            character_properties_friend_list: Vec::new(),
            character_properties_quest_registry: Vec::new(),
            character_properties_shortcut_bar: Vec::new(),
            character_properties_spell_bar: Vec::new(),
            character_properties_squelch: Vec::new(),
            character_properties_title_book: Vec::new(),
        }
    }
}
