// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/TreasureDeathSQLWriter.cs
//! ACE's `TreasureDeathSQLWriter`: one death-treasure profile.

use std::ops::{Deref, DerefMut};

use super::sql_writer::*;
use crate::models::world::TreasureDeath;

/// ACE's `TreasureDeathSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: TreasureDeathSQLWriter
#[derive(Debug, Clone, Default)]
pub struct TreasureDeathSQLWriter {
    pub base: SQLWriter,
}

impl Deref for TreasureDeathSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for TreasureDeathSQLWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

impl TreasureDeathSQLWriter {
    /// Default is formed from: input.TreasureType.ToString("00000")
    // ACE: TreasureDeathSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &TreasureDeath) -> String {
        let mut file_name = cur(input.treasure_type, "00000");
        file_name = replace_illegal_in_file_name(&file_name);
        file_name += ".sql";

        file_name
    }

    // ACE: TreasureDeathSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(&self, input: &TreasureDeath, writer: &mut SqlOut) {
        writer.write_line(&format!(
            "DELETE FROM `treasure_death` WHERE `treasure_Type` = {};",
            input.treasure_type
        ));
    }

    // ACE: TreasureDeathSQLWriter.CreateSQLINSERTStatement
    pub fn create_sql_insert_statement(&self, input: &TreasureDeath, writer: &mut SqlOut) {
        writer.write_line("INSERT INTO `treasure_death` (`treasure_Type`, `tier`, `loot_Quality_Mod`, `unknown_Chances`, `item_Chance`, `item_Min_Amount`, `item_Max_Amount`, `item_Treasure_Type_Selection_Chances`, `magic_Item_Chance`, `magic_Item_Min_Amount`, `magic_Item_Max_Amount`, `magic_Item_Treasure_Type_Selection_Chances`, `mundane_Item_Chance`, `mundane_Item_Min_Amount`, `mundane_Item_Max_Amount`, `mundane_Item_Type_Selection_Chances`, `last_Modified`)");

        let mut output = "VALUES (".to_owned()
            + &format!("{}, ", input.treasure_type)
            + &format!("{}, ", input.tier)
            + &format!("{}, ", cur(input.loot_quality_mod, ""))
            + &format!("{}, ", input.unknown_chances)
            + &format!("{}, ", input.item_chance)
            + &format!("{}, ", input.item_min_amount)
            + &format!("{}, ", input.item_max_amount)
            + &format!("{}, ", input.item_treasure_type_selection_chances)
            + &format!("{}, ", input.magic_item_chance)
            + &format!("{}, ", input.magic_item_min_amount)
            + &format!("{}, ", input.magic_item_max_amount)
            + &format!("{}, ", input.magic_item_treasure_type_selection_chances)
            + &format!("{}, ", input.mundane_item_chance)
            + &format!("{}, ", input.mundane_item_min_amount)
            + &format!("{}, ", input.mundane_item_max_amount)
            + &format!("{}, ", input.mundane_item_type_selection_chances)
            + &format!("'{}'", date(input.last_modified))
            + ");";

        output = SQLWriter::fix_null_fields(&output);

        writer.write_line(&output);
    }
}
