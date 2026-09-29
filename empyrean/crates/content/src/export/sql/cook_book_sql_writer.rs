// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/CookBookSQLWriter.cs
//! ACE's `CookBookSQLWriter`: the cook book rows of one recipe.

use std::ops::{Deref, DerefMut};

use super::sql_writer::*;
use crate::models::world::CookBook;

/// ACE's `CookBookSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: CookBookSQLWriter
#[derive(Debug, Clone, Default)]
pub struct CookBookSQLWriter {
    pub base: SQLWriter,
}

impl Deref for CookBookSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for CookBookSQLWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

impl CookBookSQLWriter {
    /// Default is formed from: input.RecipeId.ToString("00000")
    // ACE: CookBookSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &CookBook) -> String {
        let mut file_name = cur(input.recipe_id, "00000");
        file_name = replace_illegal_in_file_name(&file_name);
        file_name += ".sql";

        file_name
    }

    /// `input[0]` throws on an empty list.
    // ACE: CookBookSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(
        &self,
        input: &[CookBook],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        let first = input
            .first()
            .ok_or_else(|| err(ARGUMENT_OUT_OF_RANGE, "input[0] of an empty list"))?;
        writer.write_line(&format!(
            "DELETE FROM `cook_book` WHERE `recipe_Id` = {};",
            first.recipe_id
        ));
        Ok(())
    }

    // ACE: CookBookSQLWriter.CreateSQLINSERTStatement
    pub fn create_sql_insert_statement(
        &self,
        input: &[CookBook],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `cook_book` (`recipe_Id`, `source_W_C_I_D`, `target_W_C_I_D`, `last_Modified`)");

        let line_generator = |i: usize| {
            let c = &input[i];
            let mut source_label: Option<&str> = None;
            if let Some(names) = &self.weenie_names {
                source_label = names.get(&c.source_wcid).map(String::as_str);
            }

            let mut target_label: Option<&str> = None;
            if let Some(names) = &self.weenie_names {
                target_label = names.get(&c.target_wcid).map(String::as_str);
            }

            Ok(format!(
                "{}, {} /* {} */, {} /* {} */, '{}')",
                c.recipe_id,
                c.source_wcid,
                s(source_label),
                pad_left(&c.target_wcid.to_string(), 5),
                s(target_label),
                date(c.last_modified)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }
}
