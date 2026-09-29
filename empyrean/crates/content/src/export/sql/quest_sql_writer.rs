// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/QuestSQLWriter.cs
//! ACE's `QuestSQLWriter`: one quest row.

use std::ops::{Deref, DerefMut};

use super::sql_writer::*;
use crate::models::world::Quest;

/// ACE's `QuestSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: QuestSQLWriter
#[derive(Debug, Clone, Default)]
pub struct QuestSQLWriter {
    pub base: SQLWriter,
}

impl Deref for QuestSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for QuestSQLWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

impl QuestSQLWriter {
    /// Default is formed from: input.Name
    // ACE: QuestSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &Quest) -> String {
        let mut file_name = replace_illegal_in_file_name(&input.name);
        file_name += ".sql";

        file_name
    }

    // ACE: QuestSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(&self, input: &Quest, writer: &mut SqlOut) {
        writer.write_line(&format!(
            "DELETE FROM `quest` WHERE `name` = {};",
            s(SQLWriter::get_sql_string(Some(&input.name)).as_deref())
        ));
    }

    // ACE: QuestSQLWriter.CreateSQLINSERTStatement
    pub fn create_sql_insert_statement(&self, input: &Quest, writer: &mut SqlOut) {
        writer.write_line(
            "INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`, `last_Modified`)",
        );

        let mut output = format!(
            "VALUES ({}, {}, {}, {}, '{}');",
            s(SQLWriter::get_sql_string(Some(&input.name)).as_deref()),
            input.min_delta,
            input.max_solves,
            s(SQLWriter::get_sql_string(input.message.as_deref()).as_deref()),
            date(input.last_modified)
        );

        output = SQLWriter::fix_null_fields(&output);

        writer.write_line(&output);
    }
}
