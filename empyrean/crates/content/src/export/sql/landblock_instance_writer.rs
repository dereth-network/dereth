// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/LandblockInstanceWriter.cs
//! ACE's `LandblockInstanceWriter`: a landblock's instances and their links.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use super::sql_writer::*;
use crate::models::world::{LandblockInstance, LandblockInstanceLink};

/// ACE's `LandblockInstanceWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: LandblockInstanceWriter
#[derive(Debug, Clone, Default)]
pub struct LandblockInstanceWriter {
    pub base: SQLWriter,
}

impl Deref for LandblockInstanceWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for LandblockInstanceWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

impl LandblockInstanceWriter {
    /// Default is formed from: (input.ObjCellId >> 16).ToString("X4")
    // ACE: LandblockInstanceWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &LandblockInstance) -> String {
        let mut file_name = cur(input.obj_cell_id >> 16, "X4");
        file_name = replace_illegal_in_file_name(&file_name);
        file_name += ".sql";

        file_name
    }

    /// `input[0]` throws on an empty list.
    // ACE: LandblockInstanceWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(
        &self,
        input: &[LandblockInstance],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        let first = input
            .first()
            .ok_or_else(|| err(ARGUMENT_OUT_OF_RANGE, "input[0] of an empty list"))?;
        writer.write_line(&format!(
            "DELETE FROM `landblock_instance` WHERE `landblock` = 0x{};",
            cur(first.obj_cell_id >> 16, "X4")
        ));
        Ok(())
    }

    /// `input.ToDictionary(i => i.Guid, ...)` throws on a repeated guid.
    // ACE: LandblockInstanceWriter.CreateSQLINSERTStatement
    pub fn create_sql_insert_statement(
        &self,
        input: &[LandblockInstance],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        let mut instance_wcids: HashMap<u32, u32> = HashMap::new();
        for i in input {
            if instance_wcids.insert(i.guid, i.weenie_class_id).is_some() {
                return Err(err(
                    ARGUMENT,
                    format!(
                        "An item with the same key has already been added. Key: {}",
                        i.guid
                    ),
                ));
            }
        }

        let mut input = input.to_vec();
        input.sort_by_key(|r| r.guid);

        let nl = writer.environment_new_line;
        for (n, value) in input.iter().enumerate() {
            if n != 0 {
                writer.write_empty_line();
            }

            writer.write_line("INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)");

            let mut label: Option<&str> = None;

            if let Some(names) = &self.weenie_names {
                label = names.get(&value.weenie_class_id).map(String::as_str);
            }

            let g = "0.######";
            let f = "F6";
            let mut output = format!(
                "VALUES (0x{:08X}, {}, 0x{:08X}, {}, {}, {}, {}, {}, {}, {}, {}, '{}'); /* {} */{nl}/* @teleloc 0x{:08X} [{} {} {}] {} {} {} {} */",
                value.guid,
                pad_left(&value.weenie_class_id.to_string(), 5),
                value.obj_cell_id,
                tnz(Some(value.origin_x), g),
                tnz(Some(value.origin_y), g),
                tnz(Some(value.origin_z), g),
                tnz(Some(value.angles_w), g),
                tnz(Some(value.angles_x), g),
                tnz(Some(value.angles_y), g),
                tnz(Some(value.angles_z), g),
                pad_left(b(value.is_link_child), 5),
                date(value.last_modified),
                s(label),
                value.obj_cell_id,
                tnz(Some(value.origin_x), f),
                tnz(Some(value.origin_y), f),
                tnz(Some(value.origin_z), f),
                tnz(Some(value.angles_w), f),
                tnz(Some(value.angles_x), f),
                tnz(Some(value.angles_y), f),
                tnz(Some(value.angles_z), f),
            );

            output = SQLWriter::fix_null_fields(&output);

            writer.write_line(&output);

            if !value.landblock_instance_link.is_empty() {
                writer.write_empty_line();
                let mut links = value.landblock_instance_link.clone();
                links.sort_by_key(|r| r.child_guid);
                self.insert_links(&links, &instance_wcids, writer)?;
            }
        }
        Ok(())
    }

    /// `CreateSQLINSERTStatement(IList<LandblockInstanceLink>, Dictionary<uint, uint>, StreamWriter)`.
    // ACE: LandblockInstanceWriter.CreateSQLINSERTStatement
    fn insert_links(
        &self,
        input: &[LandblockInstanceLink],
        instance_wcids: &HashMap<u32, u32>,
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)",
        );

        let line_generator = |i: usize| {
            let mut label: Option<String> = None;

            //if (WeenieNames != null && instanceWcids.TryGetValue(input[i].ParentGuid, out var parentWcid) && WeenieNames.TryGetValue(parentWcid, out var parentWeenieName))
            //label = $"{parentWeenieName} ({parentWcid})";

            if let Some(names) = &self.weenie_names {
                if let Some(&wcid) = instance_wcids.get(&input[i].child_guid) {
                    if let Some(weenie_name) = names.get(&wcid) {
                        label = Some(match label {
                            Some(l) => format!("{l}, {weenie_name} ({wcid})"),
                            None => format!("{weenie_name} ({wcid})"),
                        });
                    }
                }
            }

            let label = label.map_or_else(String::new, |l| format!(" /* {l} */"));

            Ok(format!(
                "0x{:08X}, 0x{:08X}, '{}'){label}",
                input[i].parent_guid,
                input[i].child_guid,
                date(input[i].last_modified)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }
}
