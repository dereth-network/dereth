// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/HousePortalSQLWriter.cs
//! ACE's `HousePortalSQLWriter`: the portal destinations of one house.

use std::ops::{Deref, DerefMut};

use super::sql_writer::*;
use crate::models::world::HousePortal;

/// ACE's `HousePortalSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: HousePortalSQLWriter
#[derive(Debug, Clone, Default)]
pub struct HousePortalSQLWriter {
    pub base: SQLWriter,
}

impl Deref for HousePortalSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for HousePortalSQLWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

impl HousePortalSQLWriter {
    /// Default is formed from: input.HouseId.ToString("00000")
    // ACE: HousePortalSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &HousePortal) -> String {
        let mut file_name = cur(input.house_id, "00000");
        file_name = replace_illegal_in_file_name(&file_name);
        file_name += ".sql";

        file_name
    }

    /// `input[0]` of an empty list throws `ArgumentOutOfRangeException`.
    // ACE: HousePortalSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(
        &self,
        input: &[HousePortal],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        let first = input
            .first()
            .ok_or_else(|| err(ARGUMENT_OUT_OF_RANGE, "input[0] of an empty list"))?;
        writer.write_line(&format!(
            "DELETE FROM `house_portal` WHERE `house_Id` = {};",
            first.house_id
        ));
        Ok(())
    }

    // ACE: HousePortalSQLWriter.CreateSQLINSERTStatement
    pub fn create_sql_insert_statement(
        &self,
        input: &[HousePortal],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `house_portal` (`house_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `last_Modified`)");

        let nl = writer.environment_new_line;
        let g = "0.######";
        let f = "F6";
        let line_generator = |i: usize| {
            let p = &input[i];
            Ok(format!(
                "{}, 0x{:08X}, {}, {}, {}, {}, {}, {}, {}, '{}'){nl}/* @teleloc 0x{:08X} [{} {} {}] {} {} {} {} */",
                p.house_id,
                p.obj_cell_id,
                tnz(Some(p.origin_x), g),
                tnz(Some(p.origin_y), g),
                tnz(Some(p.origin_z), g),
                tnz(Some(p.angles_w), g),
                tnz(Some(p.angles_x), g),
                tnz(Some(p.angles_y), g),
                tnz(Some(p.angles_z), g),
                date(p.last_modified),
                p.obj_cell_id,
                tnz(Some(p.origin_x), f),
                tnz(Some(p.origin_y), f),
                tnz(Some(p.origin_z), f),
                tnz(Some(p.angles_w), f),
                tnz(Some(p.angles_x), f),
                tnz(Some(p.angles_y), f),
                tnz(Some(p.angles_z), f),
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }
}
