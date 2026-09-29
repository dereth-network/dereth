// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/TreasureWieldedSQLWriter.cs
//! ACE's `TreasureWieldedSQLWriter`: the items of one wielded-treasure table.

use std::ops::{Deref, DerefMut};

use super::sql_writer::*;
use crate::models::world::TreasureWielded;

/// 35 spaces (the indent of `GetValuesForTreasureDID`'s lines), which the closing label
/// shortens to 4.
const LABEL_INDENT: &str = "                                   ";

/// ACE's `TreasureWieldedSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: TreasureWieldedSQLWriter
#[derive(Debug, Clone, Default)]
pub struct TreasureWieldedSQLWriter {
    pub base: SQLWriter,
}

impl Deref for TreasureWieldedSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for TreasureWieldedSQLWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

impl TreasureWieldedSQLWriter {
    /// Default is formed from: input.TreasureType.ToString("00000")
    // ACE: TreasureWieldedSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &TreasureWielded) -> String {
        let mut file_name = cur(input.treasure_type, "00000");
        file_name = replace_illegal_in_file_name(&file_name);
        file_name += ".sql";

        file_name
    }

    /// `input[0]` of an empty list throws `ArgumentOutOfRangeException`.
    // ACE: TreasureWieldedSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(
        &self,
        input: &[TreasureWielded],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        let first = input
            .first()
            .ok_or_else(|| err(ARGUMENT_OUT_OF_RANGE, "input[0] of an empty list"))?;
        writer.write_line(&format!(
            "DELETE FROM `treasure_wielded` WHERE `treasure_Type` = {};",
            first.treasure_type
        ));
        Ok(())
    }

    // ACE: TreasureWieldedSQLWriter.CreateSQLINSERTStatement
    pub fn create_sql_insert_statement(
        &self,
        input: &[TreasureWielded],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `treasure_wielded` (`treasure_Type`, `weenie_Class_Id`, `palette_Id`, `unknown_1`, `shade`, `stack_Size`, `stack_Size_Variance`, `probability`, `unknown_3`, `unknown_4`, `unknown_5`, `set_Start`, `has_Sub_Set`, `continues_Previous_Set`, `unknown_9`, `unknown_10`, `unknown_11`, `unknown_12`, `last_Modified`)");

        let line_generator = |i: usize| {
            let t = &input[i];
            let mut weenie_label: Option<&str> = None;
            if let Some(names) = &self.weenie_names {
                weenie_label = names.get(&t.weenie_class_id).map(String::as_str);
            }

            Ok(format!("{}, ", t.treasure_type)
                + &format!("{}, ", pad_left(&t.weenie_class_id.to_string(), 5))
                + &format!("{}, ", pad_left(&t.palette_id.to_string(), 2))
                + &format!("{}, ", t.unknown_1)
                + &format!("{}, ", pad_left(&inv(t.shade, ""), 4))
                + &format!("{}, ", t.stack_size)
                + &format!("{}, ", cur(t.stack_size_variance, ""))
                + &format!("{}, ", pad_left(&inv(t.probability, ""), 7))
                + &format!("{}, ", t.unknown_3)
                + &format!("{}, ", t.unknown_4)
                + &format!("{}, ", t.unknown_5)
                + &format!("{}, ", pad_left(b(t.set_start), 5))
                + &format!("{}, ", pad_left(b(t.has_sub_set), 5))
                + &format!("{}, ", pad_left(b(t.continues_previous_set), 5))
                + &format!("{}, ", t.unknown_9)
                + &format!("{}, ", t.unknown_10)
                + &format!("{}, ", t.unknown_11)
                + &format!("{}, ", t.unknown_12)
                + &format!("'{}'", date(t.last_modified))
                + &format!(") /* {} */", s(weenie_label)))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)?;

        let nl = writer.environment_new_line;
        let label = self
            .get_values_for_treasure_did(input, nl)?
            .replace(LABEL_INDENT, "    ");
        writer.write_line(&format!("/* {label}{nl}*/"));
        Ok(())
    }
}
