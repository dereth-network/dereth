// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/Shard/CharacterSQLWriter.cs
//! ACE's `CharacterSQLWriter`: a shard character row and its property lists as SQL. ACE has no
//! caller for it.

use std::ops::{Deref, DerefMut};

use empyrean_store::models::shard::*;

use super::super::sql_writer::*;

/// ACE's `CharacterSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: CharacterSQLWriter
#[derive(Debug, Clone, Default)]
pub struct CharacterSQLWriter {
    pub base: SQLWriter,
}

impl Deref for CharacterSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for CharacterSQLWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

/// `Enumerable.OrderBy(key).ToList()`: a stable sort.
fn ordered<T: Clone, K: Ord>(v: &[T], key: impl Fn(&T) -> K) -> Vec<T> {
    let mut v = v.to_vec();
    v.sort_by_key(key);
    v
}

impl CharacterSQLWriter {
    ///Default is formed from: input.Id.ToString("X8") + " " + name
    // ACE: CharacterSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &Character) -> String {
        cur(input.id, "X8") + " " + &input.name + ".sql"
    }

    // ACE: CharacterSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(&self, input: &Character, writer: &mut SqlOut) {
        writer.write_line(&format!(
            "DELETE FROM `character` WHERE `id` = {};",
            input.id
        ));
    }

    // ACE: CharacterSQLWriter.CreateSQLINSERTStatement
    pub fn create_sql_insert_statement(&self, input: &Character, writer: &mut SqlOut) {
        writer.write_line("INSERT INTO `character` (`id`, `account_Id`, `name`, `is_Plussed`, `is_Deleted`, `delete_Time`, `last_Login_Timestamp`, `total_Logins`, `character_Options_1`, `character_Options_2`, `gameplay_Options`, `spellbook_Filters`, `hair_Texture`, `default_Hair_Texture`)");

        // Not ACE's (a fix): the gameplay options are written as a
        // binary literal (X'…'); ACE interpolated the byte array as its type name, so a character
        // with gameplay options was written as the bare word System.Byte[] (invalid SQL).
        let gameplay_options = input
            .gameplay_options
            .as_ref()
            .map_or_else(String::new, |bytes| {
                let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
                format!("X'{hex}'")
            });

        let mut output = format!(
            "VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {gameplay_options}, {}, {}, {});",
            input.id,
            input.account_id,
            s(SQLWriter::get_sql_string(Some(&input.name)).as_deref()),
            b(input.is_plussed),
            b(input.is_deleted),
            input.delete_time,
            cur(input.last_login_timestamp, ""),
            input.total_logins,
            input.character_options_1,
            input.character_options_2,
            input.spellbook_filters,
            input.hair_texture,
            input.default_hair_texture
        );

        output = SQLWriter::fix_null_fields(&output);

        writer.write_line(&output);

        if !input.character_properties_contract_registry.is_empty() {
            writer.write_empty_line();
            self.insert_contract_registry(
                input.id,
                &ordered(&input.character_properties_contract_registry, |r| {
                    r.contract_id
                }),
                writer,
            );
        }

        if !input.character_properties_fill_comp_book.is_empty() {
            writer.write_empty_line();
            self.insert_fill_comp_book(
                input.id,
                &ordered(&input.character_properties_fill_comp_book, |r| {
                    r.spell_component_id
                }),
                writer,
            );
        }

        if !input.character_properties_friend_list.is_empty() {
            writer.write_empty_line();
            self.insert_friend_list(
                input.id,
                &ordered(&input.character_properties_friend_list, |r| r.friend_id),
                writer,
            );
        }

        if !input.character_properties_quest_registry.is_empty() {
            writer.write_empty_line();
            // DIVERGE: OrderBy(r => r.QuestName) compares with the en-US culture (ICU collation);
            // this is an ordinal sort, which agrees for names of one letter case.
            self.insert_quest_registry(
                input.id,
                &ordered(&input.character_properties_quest_registry, |r| {
                    r.quest_name.clone()
                }),
                writer,
            );
        }

        if !input.character_properties_shortcut_bar.is_empty() {
            writer.write_empty_line();
            self.insert_shortcut_bar(
                input.id,
                &ordered(&input.character_properties_shortcut_bar, |r| {
                    r.shortcut_bar_index
                }),
                writer,
            );
        }

        if !input.character_properties_spell_bar.is_empty() {
            writer.write_empty_line();
            self.insert_spell_bar(
                input.id,
                &ordered(&input.character_properties_spell_bar, |r| {
                    (r.spell_bar_number, r.spell_bar_index)
                }),
                writer,
            );
        }

        if !input.character_properties_title_book.is_empty() {
            writer.write_empty_line();
            self.insert_title_book(
                input.id,
                &ordered(&input.character_properties_title_book, |r| r.title_id),
                writer,
            );
        }
    }

    /// Every line generator here is total, so `ValuesWriter` cannot fail.
    fn values(count: usize, line_generator: impl FnMut(usize) -> String, writer: &mut SqlOut) {
        let mut line_generator = line_generator;
        let written = SQLWriter::values_writer(count, |i| Ok(line_generator(i)), writer);
        debug_assert!(written.is_ok());
    }

    /// `CreateSQLINSERTStatement(uint, IList<CharacterPropertiesContractRegistry>, StreamWriter)`.
    // ACE: CharacterSQLWriter.CreateSQLINSERTStatement
    pub fn insert_contract_registry(
        &self,
        character_id: u32,
        input: &[CharacterPropertiesContractRegistry],
        writer: &mut SqlOut,
    ) {
        writer.write_line("INSERT INTO `character_properties_contract_registry` (`character_Id`, `contract_Id`, `delete_Contract`, `set_As_Display_Contract`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            format!(
                "{character_id}, {}, {}, {})",
                r.contract_id,
                b(r.delete_contract),
                b(r.set_as_display_contract)
            )
        };

        Self::values(input.len(), line_generator, writer);
    }

    /// `CreateSQLINSERTStatement(uint, IList<CharacterPropertiesFillCompBook>, StreamWriter)`.
    // ACE: CharacterSQLWriter.CreateSQLINSERTStatement
    pub fn insert_fill_comp_book(
        &self,
        character_id: u32,
        input: &[CharacterPropertiesFillCompBook],
        writer: &mut SqlOut,
    ) {
        writer.write_line("INSERT INTO `character_properties_fill_comp_book` (`character_Id`, `spell_Component_Id`, `quantity_To_Rebuy`)");

        let line_generator = |i: usize| {
            format!(
                "{character_id}, {}, {})",
                input[i].spell_component_id, input[i].quantity_to_rebuy
            )
        };

        Self::values(input.len(), line_generator, writer);
    }

    /// `CreateSQLINSERTStatement(uint, IList<CharacterPropertiesFriendList>, StreamWriter)`.
    // ACE: CharacterSQLWriter.CreateSQLINSERTStatement
    pub fn insert_friend_list(
        &self,
        character_id: u32,
        input: &[CharacterPropertiesFriendList],
        writer: &mut SqlOut,
    ) {
        writer.write_line(
            "INSERT INTO `character_properties_friend_list` ( `character_Id`, `friend_Id`)",
        );

        let line_generator = |i: usize| format!("{character_id}, {})", input[i].friend_id);

        Self::values(input.len(), line_generator, writer);
    }

    /// `CreateSQLINSERTStatement(uint, IList<CharacterPropertiesQuestRegistry>, StreamWriter)`.
    // ACE: CharacterSQLWriter.CreateSQLINSERTStatement
    pub fn insert_quest_registry(
        &self,
        character_id: u32,
        input: &[CharacterPropertiesQuestRegistry],
        writer: &mut SqlOut,
    ) {
        writer.write_line("INSERT INTO `character_properties_quest_registry` (`character_Id`, `quest_Name`, `last_Time_Completed`, `num_Times_Completed`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            format!(
                "{character_id}, {}, {}, {})",
                s(SQLWriter::get_sql_string(Some(&r.quest_name)).as_deref()),
                r.last_time_completed,
                r.num_times_completed
            )
        };

        Self::values(input.len(), line_generator, writer);
    }

    /// `CreateSQLINSERTStatement(uint, IList<CharacterPropertiesShortcutBar>, StreamWriter)`.
    // ACE: CharacterSQLWriter.CreateSQLINSERTStatement
    pub fn insert_shortcut_bar(
        &self,
        character_id: u32,
        input: &[CharacterPropertiesShortcutBar],
        writer: &mut SqlOut,
    ) {
        writer.write_line("INSERT INTO `character_properties_shortcut_bar` (`character_Id`, `shortcut_Bar_Index`, `shortcut_Object_Id`)");

        let line_generator = |i: usize| {
            format!(
                "{character_id}, {}, {})",
                input[i].shortcut_bar_index, input[i].shortcut_object_id
            )
        };

        Self::values(input.len(), line_generator, writer);
    }

    /// `CreateSQLINSERTStatement(uint, IList<CharacterPropertiesSpellBar>, StreamWriter)`.
    // ACE: CharacterSQLWriter.CreateSQLINSERTStatement
    pub fn insert_spell_bar(
        &self,
        character_id: u32,
        input: &[CharacterPropertiesSpellBar],
        writer: &mut SqlOut,
    ) {
        writer.write_line("INSERT INTO `character_properties_spell_bar` (`character_Id`, `spell_Bar_Number`, `spell_Bar_Index`, `spell_Id`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            format!(
                "{character_id}, {}, {}, {})",
                r.spell_bar_number, r.spell_bar_index, r.spell_id
            )
        };

        Self::values(input.len(), line_generator, writer);
    }

    /// `CreateSQLINSERTStatement(uint, IList<CharacterPropertiesTitleBook>, StreamWriter)`.
    // ACE: CharacterSQLWriter.CreateSQLINSERTStatement
    pub fn insert_title_book(
        &self,
        character_id: u32,
        input: &[CharacterPropertiesTitleBook],
        writer: &mut SqlOut,
    ) {
        writer.write_line(
            "INSERT INTO `character_properties_title_book` (`character_Id`, `title_Id`)",
        );

        let line_generator = |i: usize| format!("{character_id}, {})", input[i].title_id);

        Self::values(input.len(), line_generator, writer);
    }
}
