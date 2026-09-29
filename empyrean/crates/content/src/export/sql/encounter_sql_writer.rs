// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/EncounterSQLWriter.cs
//! ACE's `EncounterSQLWriter`: a landblock's encounter rows.

use std::ops::{Deref, DerefMut};

use super::sql_writer::*;
use crate::models::world::Encounter;

/// ACE's `EncounterSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: EncounterSQLWriter
#[derive(Debug, Clone, Default)]
pub struct EncounterSQLWriter {
    pub base: SQLWriter,
}

impl Deref for EncounterSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for EncounterSQLWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

impl EncounterSQLWriter {
    /// Default is formed from: input.Landblock.ToString("X4")
    // ACE: EncounterSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &Encounter) -> String {
        let mut file_name = cur(input.landblock, "X4");
        file_name = replace_illegal_in_file_name(&file_name);
        file_name += ".sql";

        file_name
    }

    /// `input[0]` throws on an empty list.
    // ACE: EncounterSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(
        &self,
        input: &[Encounter],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        let first = input
            .first()
            .ok_or_else(|| err(ARGUMENT_OUT_OF_RANGE, "input[0] of an empty list"))?;
        writer.write_line(&format!(
            "DELETE FROM `encounter` WHERE `landblock` = 0x{};",
            cur(first.landblock, "X4")
        ));
        Ok(())
    }

    // ACE: EncounterSQLWriter.CreateSQLINSERTStatement
    pub fn create_sql_insert_statement(
        &self,
        input: &[Encounter],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `encounter` (`landblock`, `weenie_Class_Id`, `cell_X`, `cell_Y`, `last_Modified`)");

        let line_generator = |i: usize| {
            let e = &input[i];
            let mut label: Option<&str> = None;

            if let Some(names) = &self.weenie_names {
                label = names.get(&e.weenie_class_id).map(String::as_str);
            }

            Ok(format!(
                "0x{}, {}, {}, {}, '{}') /* {} */",
                cur(e.landblock, "X4"),
                e.weenie_class_id,
                e.cell_x,
                e.cell_y,
                date(e.last_modified),
                s(label)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }
}
