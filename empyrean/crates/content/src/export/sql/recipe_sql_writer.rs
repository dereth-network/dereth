// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/RecipeSQLWriter.cs
//! ACE's `RecipeSQLWriter`: a recipe with its requirements and mods, with ACE's labels.

use std::ops::{Deref, DerefMut};

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::*;

use super::sql_writer::*;
use crate::models::world::*;

/// ACE's `RecipeSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: RecipeSQLWriter
#[derive(Debug, Clone, Default)]
pub struct RecipeSQLWriter {
    pub base: SQLWriter,
}

impl Deref for RecipeSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for RecipeSQLWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

/// `string.IsNullOrEmpty`.
fn is_null_or_empty(s: Option<&str>) -> bool {
    s.is_none_or(str::is_empty)
}

/// `{((ModificationType)index).ToString().Substring(7)}`: the name less its Success/Failure.
///
/// Not ACE's (a fix): an index that is not a ModificationType is written
/// as its number; ACE's `Substring(7)` of that number threw ArgumentOutOfRangeException, so the
/// whole export failed.
#[allow(clippy::unnecessary_wraps)]
fn modification_target(index: i8) -> Result<String, SqlWriterError> {
    let name = ModificationType(index.into()).to_dotnet_string();
    Ok(name.get(7..).map_or(name.clone(), str::to_owned))
}

/// `/* On {RecipeSourceType}.{ModificationType} {ModificationOperation}`, the start of every
/// mod label.
fn mod_label_head(index: i8, r#enum: i32, source: i32) -> String {
    format!(
        "/* On {}.{} {}",
        RecipeSourceType(source).to_dotnet_string(),
        ModificationType(index.into()).to_dotnet_string(),
        ModificationOperation(r#enum).to_dotnet_string()
    )
}

impl RecipeSQLWriter {
    /// Default is formed from: input.RecipeId.ToString("00000") + " " + [SuccessWeenieName or Cook
    /// Book Source]. With `desc_only`, only the description (which may be null) is returned.
    // ACE: RecipeSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(
        &self,
        input: &Recipe,
        cook_books: Option<&[CookBook]>,
        desc_only: bool,
    ) -> Option<String> {
        let mut description: Option<String> = None;

        if let Some(names) = &self.weenie_names {
            if let Some(weenie_name) = names.get(&input.success_wcid) {
                description = Some(weenie_name.clone());
            }
        }

        let mut alternate_description: Option<String> = None;

        if let (Some(cook_books), Some(names)) =
            (cook_books.filter(|c| !c.is_empty()), &self.weenie_names)
        {
            alternate_description = names.get(&cook_books[0].source_wcid).cloned();

            for cook_book in &cook_books[1..] {
                if let Some(source_weenie_name) = names.get(&cook_book.source_wcid) {
                    if Some(source_weenie_name) != alternate_description.as_ref() {
                        alternate_description = None;
                        break;
                    }
                }
            }
        }

        if is_null_or_empty(description.as_deref())
            && !is_null_or_empty(alternate_description.as_deref())
        {
            description.clone_from(&alternate_description);
        }

        if description.as_deref() == Some("Cooking Pot")
            && !is_null_or_empty(alternate_description.as_deref())
        {
            description = alternate_description;
        }

        if desc_only {
            return description;
        }

        let mut file_name = cur(input.id, "00000");
        if let Some(d) = description.as_deref().filter(|d| !d.is_empty()) {
            file_name += " ";
            file_name += d;
        }
        file_name = replace_illegal_in_file_name(&file_name);
        file_name += ".sql";

        Some(file_name)
    }

    // ACE: RecipeSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(&self, input: &Recipe, writer: &mut SqlOut) {
        writer.write_line(&format!("DELETE FROM `recipe` WHERE `id` = {};", input.id));
    }

    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    pub fn create_sql_insert_statement(
        &self,
        input: &Recipe,
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `recipe` (`id`, `unknown_1`, `skill`, `difficulty`, `salvage_Type`, `success_W_C_I_D`, `success_Amount`, `success_Message`, `fail_W_C_I_D`, `fail_Amount`, `fail_Message`, \
             `success_Destroy_Source_Chance`, `success_Destroy_Source_Amount`, `success_Destroy_Source_Message`, `success_Destroy_Target_Chance`, `success_Destroy_Target_Amount`, `success_Destroy_Target_Message`, \
             `fail_Destroy_Source_Chance`, `fail_Destroy_Source_Amount`, `fail_Destroy_Source_Message`, `fail_Destroy_Target_Chance`, `fail_Destroy_Target_Amount`, `fail_Destroy_Target_Message`, \
             `data_Id`, `last_Modified`)",
        );

        let mut skill_label: Option<&str> = None;
        if input.skill != 0 {
            skill_label = gn::<Skill>(input.skill);
        }

        let mut success_weenie_label: Option<&str> = None;
        if let Some(names) = &self.weenie_names {
            success_weenie_label = names.get(&input.success_wcid).map(String::as_str);
        }

        let mut fail_weenie_label: Option<&str> = None;
        if let Some(names) = &self.weenie_names {
            fail_weenie_label = names.get(&input.fail_wcid).map(String::as_str);
        }

        let q =
            |v: &Option<String>| s(SQLWriter::get_sql_string(v.as_deref()).as_deref()).to_owned();
        let r = input;
        let fields = [
            r.id.to_string(),
            r.unknown_1.to_string(),
            format!("{} /* {} */", r.skill, s(skill_label)),
            r.difficulty.to_string(),
            r.salvage_type.to_string(),
            format!("{} /* {} */", r.success_wcid, s(success_weenie_label)),
            r.success_amount.to_string(),
            q(&r.success_message),
            format!("{} /* {} */", r.fail_wcid, s(fail_weenie_label)),
            r.fail_amount.to_string(),
            q(&r.fail_message),
            cur(r.success_destroy_source_chance, ""),
            r.success_destroy_source_amount.to_string(),
            q(&r.success_destroy_source_message),
            cur(r.success_destroy_target_chance, ""),
            r.success_destroy_target_amount.to_string(),
            q(&r.success_destroy_target_message),
            cur(r.fail_destroy_source_chance, ""),
            r.fail_destroy_source_amount.to_string(),
            q(&r.fail_destroy_source_message),
            cur(r.fail_destroy_target_chance, ""),
            r.fail_destroy_target_amount.to_string(),
            q(&r.fail_destroy_target_message),
            r.data_id.to_string(),
            format!("'{}'", date(r.last_modified)),
        ];
        let mut output = format!("VALUES ({});", fields.join(", "));

        output = SQLWriter::fix_null_fields(&output);

        writer.write_line(&output);

        if !input.recipe_requirements_int.is_empty() {
            writer.write_empty_line();
            self.insert_requirements_int(input.id, &input.recipe_requirements_int, writer)?;
        }
        if !input.recipe_requirements_did.is_empty() {
            writer.write_empty_line();
            self.insert_requirements_did(input.id, &input.recipe_requirements_did, writer)?;
        }
        if !input.recipe_requirements_iid.is_empty() {
            writer.write_empty_line();
            self.insert_requirements_iid(input.id, &input.recipe_requirements_iid, writer)?;
        }
        if !input.recipe_requirements_float.is_empty() {
            writer.write_empty_line();
            self.insert_requirements_float(input.id, &input.recipe_requirements_float, writer)?;
        }
        if !input.recipe_requirements_string.is_empty() {
            writer.write_empty_line();
            self.insert_requirements_string(input.id, &input.recipe_requirements_string, writer)?;
        }
        if !input.recipe_requirements_bool.is_empty() {
            writer.write_empty_line();
            self.insert_requirements_bool(input.id, &input.recipe_requirements_bool, writer)?;
        }

        if !input.recipe_mod.is_empty() {
            //writer.WriteLine(); // This is not needed because CreateSQLINSERTStatement will take care of it for us on each Recipe.
            self.insert_mod(input.id, &input.recipe_mod, writer)?;
        }

        Ok(())
    }

    /// `CreateSQLINSERTStatement(uint, IList<RecipeRequirementsInt>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    pub fn insert_requirements_int(
        &self,
        recipe_id: u32,
        input: &[RecipeRequirementsInt],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_requirements_int` (`recipe_Id`, `index`, `stat`, `value`, `enum`, `message`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            let property_value_description =
                self.get_value_enum_name_int(PropertyInt(r.stat.cs_cast()), r.value);

            let mut comment = gn::<PropertyInt>(r.stat).map(str::to_owned);
            if let Some(d) = property_value_description {
                comment = Some(comment.unwrap_or_default() + " - " + &d);
            }

            Ok(format!(
                "{recipe_id}, {}, {}, {}, {}, {}) /* {}.{} {} {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                r.value,
                r.r#enum,
                s(SQLWriter::get_sql_string(r.message.as_deref()).as_deref()),
                RequirementType(r.index.into()).to_dotnet_string(),
                s(comment.as_deref()),
                CompareType(r.r#enum).to_dotnet_string(),
                r.value
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<RecipeRequirementsDID>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    pub fn insert_requirements_did(
        &self,
        recipe_id: u32,
        input: &[RecipeRequirementsDID],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_requirements_d_i_d` (`recipe_Id`, `index`, `stat`, `value`, `enum`, `message`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            Ok(format!(
                "{recipe_id}, {}, {}, {}, {}, {}) /* {}.{} {} {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                r.value,
                r.r#enum,
                s(SQLWriter::get_sql_string(r.message.as_deref()).as_deref()),
                RequirementType(r.index.into()).to_dotnet_string(),
                s(gn::<PropertyDataId>(r.stat)),
                CompareType(r.r#enum).to_dotnet_string(),
                r.value
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<RecipeRequirementsIID>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    pub fn insert_requirements_iid(
        &self,
        recipe_id: u32,
        input: &[RecipeRequirementsIID],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_requirements_i_i_d` (`recipe_Id`, `index`, `stat`, `value`, `enum`, `message`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            Ok(format!(
                "{recipe_id}, {}, {}, {}, {}, {}) /* {}.{} {} {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                r.value,
                r.r#enum,
                s(SQLWriter::get_sql_string(r.message.as_deref()).as_deref()),
                RequirementType(r.index.into()).to_dotnet_string(),
                s(gn::<PropertyInstanceId>(r.stat)),
                CompareType(r.r#enum).to_dotnet_string(),
                r.value
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<RecipeRequirementsFloat>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    pub fn insert_requirements_float(
        &self,
        recipe_id: u32,
        input: &[RecipeRequirementsFloat],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_requirements_float` (`recipe_Id`, `index`, `stat`, `value`, `enum`, `message`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            Ok(format!(
                "{recipe_id}, {}, {}, {}, {}, {}) /* {}.{} {} {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                cur(r.value, ""),
                r.r#enum,
                s(SQLWriter::get_sql_string(r.message.as_deref()).as_deref()),
                RequirementType(r.index.into()).to_dotnet_string(),
                s(gn::<PropertyFloat>(r.stat)),
                CompareType(r.r#enum).to_dotnet_string(),
                cur(r.value, "")
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<RecipeRequirementsString>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    pub fn insert_requirements_string(
        &self,
        recipe_id: u32,
        input: &[RecipeRequirementsString],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_requirements_string` (`recipe_Id`, `index`, `stat`, `value`, `enum`, `message`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            Ok(format!(
                "{recipe_id}, {}, {}, {}, {}, {}) /* {}.{} {} {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                s(SQLWriter::get_sql_string(r.value.as_deref()).as_deref()),
                r.r#enum,
                s(SQLWriter::get_sql_string(r.message.as_deref()).as_deref()),
                RequirementType(r.index.into()).to_dotnet_string(),
                s(gn::<PropertyString>(r.stat)),
                CompareType(r.r#enum).to_dotnet_string(),
                s(r.value.as_deref())
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<RecipeRequirementsBool>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    pub fn insert_requirements_bool(
        &self,
        recipe_id: u32,
        input: &[RecipeRequirementsBool],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_requirements_bool` (`recipe_Id`, `index`, `stat`, `value`, `enum`, `message`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            Ok(format!(
                "{recipe_id}, {}, {}, {}, {}, {}) /* {}.{} {} {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                b(r.value),
                r.r#enum,
                s(SQLWriter::get_sql_string(r.message.as_deref()).as_deref()),
                RequirementType(r.index.into()).to_dotnet_string(),
                s(gn::<PropertyBool>(r.stat)),
                CompareType(r.r#enum).to_dotnet_string(),
                b(r.value)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<RecipeMod>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    pub fn insert_mod(
        &self,
        recipe_id: u32,
        input: &[RecipeMod],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        for value in input {
            writer.write_empty_line();
            writer.write_line("INSERT INTO `recipe_mod` (`recipe_Id`, `executes_On_Success`, `health`, `stamina`, `mana`, `unknown_7`, `data_Id`, `unknown_9`, `instance_Id`)");

            let data_id = if value.data_id > 0 {
                format!("0x{}", cur(value.data_id, "X8"))
            } else {
                value.data_id.to_string()
            };
            let mut output = format!(
                "VALUES ({recipe_id}, {}, {}, {}, {}, {}, {data_id}, {}, {});",
                b(value.executes_on_success),
                value.health,
                value.stamina,
                value.mana,
                b(value.unknown_7),
                value.unknown_9,
                value.instance_id
            );

            output = SQLWriter::fix_null_fields(&output);

            writer.write_line(&output);

            if !value.recipe_mods_int.is_empty()
                || !value.recipe_mods_did.is_empty()
                || !value.recipe_mods_iid.is_empty()
                || !value.recipe_mods_float.is_empty()
                || !value.recipe_mods_string.is_empty()
                || !value.recipe_mods_bool.is_empty()
            {
                writer.write_empty_line();
                writer.write_line("SET @parent_id = LAST_INSERT_ID();");
            }

            if !value.recipe_mods_int.is_empty() {
                writer.write_empty_line();
                self.insert_mods_int(&value.recipe_mods_int, writer)?;
            }
            if !value.recipe_mods_did.is_empty() {
                writer.write_empty_line();
                self.insert_mods_did(&value.recipe_mods_did, writer)?;
            }
            if !value.recipe_mods_iid.is_empty() {
                writer.write_empty_line();
                self.insert_mods_iid(&value.recipe_mods_iid, writer)?;
            }
            if !value.recipe_mods_float.is_empty() {
                writer.write_empty_line();
                self.insert_mods_float(&value.recipe_mods_float, writer)?;
            }
            if !value.recipe_mods_string.is_empty() {
                writer.write_empty_line();
                self.insert_mods_string(&value.recipe_mods_string, writer)?;
            }
            if !value.recipe_mods_bool.is_empty() {
                writer.write_empty_line();
                self.insert_mods_bool(&value.recipe_mods_bool, writer)?;
            }
        }
        Ok(())
    }

    /// `CreateSQLINSERTStatement(IList<RecipeModsInt>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    fn insert_mods_int(
        &self,
        input: &[RecipeModsInt],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_mods_int` (`recipe_Mod_Id`, `index`, `stat`, `value`, `enum`, `source`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            let property_value_description =
                self.get_value_enum_name_int(PropertyInt(r.stat.cs_cast()), r.value);

            let mut comment = gn::<PropertyInt>(r.stat).map(str::to_owned);
            if let Some(d) = property_value_description {
                comment = Some(comment.unwrap_or_default() + " - " + &d);
            }

            let comment_text = comment
                .as_ref()
                .map_or_else(String::new, |c| format!(" {c}"));
            let value_text = if comment.as_ref().is_some_and(|c| c.contains(" - ")) || r.r#enum == 7
            {
                String::new()
            } else {
                format!(" {}", r.value)
            };
            let spell_text = if r.r#enum == 7 {
                format!(" {}", SpellId(r.stat.cs_cast()).to_dotnet_string())
            } else {
                String::new()
            };

            Ok(format!(
                "@parent_id, {}, {}, {}, {}, {}) {}{comment_text}{value_text}{spell_text} to {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                r.value,
                r.r#enum,
                r.source,
                mod_label_head(r.index, r.r#enum, r.source),
                modification_target(r.index)?
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(IList<RecipeModsDID>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    fn insert_mods_did(
        &self,
        input: &[RecipeModsDID],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_mods_d_i_d` (`recipe_Mod_Id`, `index`, `stat`, `value`, `enum`, `source`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            Ok(format!(
                "@parent_id, {}, {}, {}, {}, {}) {} {} to {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                r.value,
                r.r#enum,
                r.source,
                mod_label_head(r.index, r.r#enum, r.source),
                s(gn::<PropertyDataId>(r.stat)),
                modification_target(r.index)?
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(IList<RecipeModsIID>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    fn insert_mods_iid(
        &self,
        input: &[RecipeModsIID],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_mods_i_i_d` (`recipe_Mod_Id`, `index`, `stat`, `value`, `enum`, `source`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            Ok(format!(
                "@parent_id, {}, {}, {}, {}, {}) {} {} to {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                r.value,
                r.r#enum,
                r.source,
                mod_label_head(r.index, r.r#enum, r.source),
                s(gn::<PropertyInstanceId>(r.stat)),
                modification_target(r.index)?
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(IList<RecipeModsFloat>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    fn insert_mods_float(
        &self,
        input: &[RecipeModsFloat],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_mods_float` (`recipe_Mod_Id`, `index`, `stat`, `value`, `enum`, `source`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            Ok(format!(
                "@parent_id, {}, {}, {}, {}, {}) {} {} to {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                cur(r.value, ""),
                r.r#enum,
                r.source,
                mod_label_head(r.index, r.r#enum, r.source),
                s(gn::<PropertyFloat>(r.stat)),
                modification_target(r.index)?
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(IList<RecipeModsString>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    fn insert_mods_string(
        &self,
        input: &[RecipeModsString],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_mods_string` (`recipe_Mod_Id`, `index`, `stat`, `value`, `enum`, `source`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            Ok(format!(
                "@parent_id, {}, {}, {}, {}, {}) {} {} to {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                s(SQLWriter::get_sql_string(r.value.as_deref()).as_deref()),
                r.r#enum,
                r.source,
                mod_label_head(r.index, r.r#enum, r.source),
                s(gn::<PropertyString>(r.stat)),
                modification_target(r.index)?
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(IList<RecipeModsBool>, StreamWriter)`.
    // ACE: RecipeSQLWriter.CreateSQLINSERTStatement
    fn insert_mods_bool(
        &self,
        input: &[RecipeModsBool],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `recipe_mods_bool` (`recipe_Mod_Id`, `index`, `stat`, `value`, `enum`, `source`)");

        let line_generator = |i: usize| {
            let r = &input[i];
            Ok(format!(
                "@parent_id, {}, {}, {}, {}, {}) {} {} to {} */",
                r.index,
                pad_left(&r.stat.to_string(), 3),
                b(r.value),
                r.r#enum,
                r.source,
                mod_label_head(r.index, r.r#enum, r.source),
                s(gn::<PropertyBool>(r.stat)),
                modification_target(r.index)?
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }
}
