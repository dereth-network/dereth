// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/Shard/BiotaSQLWriter.cs
//! ACE's `BiotaSQLWriter`: a shard biota as `DELETE` and `INSERT` statements, with ACE's labels.
//! ACE has no caller for it; it is the shard twin of `WeenieSQLWriter`, over empyrean-store's rows.

use std::ops::{Deref, DerefMut};

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::*;
use empyrean_store::models::shard::*;

use super::super::sql_writer::*;

/// ACE's `BiotaSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: BiotaSQLWriter
#[derive(Debug, Clone, Default)]
pub struct BiotaSQLWriter {
    pub base: SQLWriter,
}

impl Deref for BiotaSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for BiotaSQLWriter {
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

/// `" /* {label} */"` when `label` is not null.
fn wrap(label: Option<&str>) -> String {
    label.map_or_else(String::new, |l| format!(" /* {l} */"))
}

/// `{x}` of a nullable integer.
fn opt_s<T: ToString>(v: Option<T>) -> String {
    v.map_or_else(String::new, |v| v.to_string())
}

impl BiotaSQLWriter {
    /// Default is formed from: input.Id.ToString("X8") + " " + name
    // ACE: BiotaSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &Biota) -> String {
        let name = input.get_property_string(PropertyString::Name);

        Self::get_default_file_name_for(input.id, name)
    }

    /// `GetDefaultFileName(uint id, string name)`: `id.ToString("X8") + " " + name`.
    // ACE: BiotaSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name_for(id: u32, name: Option<&str>) -> String {
        let mut result = cur(id, "X8");

        // String.IsNullOrWhiteSpace
        if let Some(name) = name.filter(|n| !n.chars().all(char::is_whitespace)) {
            result = result + " " + name;
        }

        result + ".sql"
    }

    // ACE: BiotaSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(&self, input: &Biota, writer: &mut SqlOut) {
        writer.write_line(&format!("DELETE FROM `biota` WHERE `id` = {};", input.id));
    }

    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    #[allow(clippy::too_many_lines)]
    pub fn create_sql_insert_statement(
        &self,
        input: &Biota,
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `biota` (`id`, `weenie_Class_Id`, `weenie_Type`, `populated_Collection_Flags`)",
        );

        // Default to all flags if none are set
        let flags = if input.populated_collection_flags != 0 {
            u64::from(input.populated_collection_flags)
        } else {
            4_294_967_295
        };
        let mut output = format!(
            "VALUES ({}, {}, {}, {flags}) /* {} */;",
            input.id,
            input.weenie_class_id,
            input.weenie_type,
            s(gn::<WeenieType>(input.weenie_type))
        );

        output = SQLWriter::fix_null_fields(&output);

        writer.write_line(&output);

        let id = input.id;

        if !input.biota_properties_int.is_empty() {
            writer.write_empty_line();
            self.insert_int(
                id,
                &ordered(&input.biota_properties_int, |r| r.r#type),
                writer,
            )?;
        }
        if !input.biota_properties_int64.is_empty() {
            writer.write_empty_line();
            self.insert_int64(
                id,
                &ordered(&input.biota_properties_int64, |r| r.r#type),
                writer,
            )?;
        }
        if !input.biota_properties_bool.is_empty() {
            writer.write_empty_line();
            self.insert_bool(
                id,
                &ordered(&input.biota_properties_bool, |r| r.r#type),
                writer,
            )?;
        }
        if !input.biota_properties_float.is_empty() {
            writer.write_empty_line();
            self.insert_float(
                id,
                &ordered(&input.biota_properties_float, |r| r.r#type),
                writer,
            )?;
        }
        if !input.biota_properties_string.is_empty() {
            writer.write_empty_line();
            self.insert_string(
                id,
                &ordered(&input.biota_properties_string, |r| r.r#type),
                writer,
            )?;
        }
        if !input.biota_properties_did.is_empty() {
            writer.write_empty_line();
            self.insert_did(
                id,
                &ordered(&input.biota_properties_did, |r| r.r#type),
                writer,
            )?;
        }

        if !input.biota_properties_position.is_empty() {
            writer.write_empty_line();
            self.insert_position(
                id,
                &ordered(&input.biota_properties_position, |r| r.position_type),
                writer,
            )?;
        }

        if !input.biota_properties_iid.is_empty() {
            writer.write_empty_line();
            self.insert_iid(
                id,
                &ordered(&input.biota_properties_iid, |r| r.r#type),
                writer,
            )?;
        }

        if !input.biota_properties_attribute.is_empty() {
            writer.write_empty_line();
            self.insert_attribute(
                id,
                &ordered(&input.biota_properties_attribute, |r| r.r#type),
                writer,
            )?;
        }
        if !input.biota_properties_attribute_2nd.is_empty() {
            writer.write_empty_line();
            self.insert_attribute_2nd(
                id,
                &ordered(&input.biota_properties_attribute_2nd, |r| r.r#type),
                writer,
            )?;
        }

        if !input.biota_properties_skill.is_empty() {
            writer.write_empty_line();
            self.insert_skill(
                id,
                &ordered(&input.biota_properties_skill, |r| r.r#type),
                writer,
            )?;
        }

        if !input.biota_properties_body_part.is_empty() {
            writer.write_empty_line();
            self.insert_body_part(
                id,
                &ordered(&input.biota_properties_body_part, |r| r.key),
                writer,
            )?;
        }

        if !input.biota_properties_spell_book.is_empty() {
            writer.write_empty_line();
            self.insert_spell_book(
                id,
                &ordered(&input.biota_properties_spell_book, |r| r.spell),
                writer,
            )?;
        }

        if !input.biota_properties_event_filter.is_empty() {
            writer.write_empty_line();
            self.insert_event_filter(
                id,
                &ordered(&input.biota_properties_event_filter, |r| r.event),
                writer,
            )?;
        }

        if !input.biota_properties_emote.is_empty() {
            //writer.WriteLine(); // This is not needed because CreateSQLINSERTStatement will take care of it for us on each Recipe.
            self.insert_emote(
                id,
                &ordered(&input.biota_properties_emote, |r| r.category),
                writer,
            )?;
        }

        if !input.biota_properties_create_list.is_empty() {
            writer.write_empty_line();
            self.insert_create_list(
                id,
                &ordered(&input.biota_properties_create_list, |r| r.destination_type),
                writer,
            )?;
        }

        if let Some(book) = &input.biota_properties_book {
            writer.write_empty_line();
            self.insert_book(id, book, writer);
        }
        if !input.biota_properties_book_page_data.is_empty() {
            writer.write_empty_line();
            self.insert_book_page_data(
                id,
                &ordered(&input.biota_properties_book_page_data, |r| r.page_id),
                writer,
            )?;
        }

        if !input.biota_properties_generator.is_empty() {
            writer.write_empty_line();
            self.insert_generator(id, &input.biota_properties_generator, writer)?;
        }

        if !input.biota_properties_palette.is_empty() {
            writer.write_empty_line();
            self.insert_palette(
                id,
                &ordered(&input.biota_properties_palette, |r| {
                    (r.order, r.sub_palette_id)
                }),
                writer,
            )?;
        }
        if !input.biota_properties_texture_map.is_empty() {
            writer.write_empty_line();
            self.insert_texture_map(
                id,
                &ordered(&input.biota_properties_texture_map, |r| (r.order, r.index)),
                writer,
            )?;
        }
        if !input.biota_properties_anim_part.is_empty() {
            writer.write_empty_line();
            self.insert_anim_part(
                id,
                &ordered(&input.biota_properties_anim_part, |r| (r.order, r.index)),
                writer,
            )?;
        }

        if !input.biota_properties_enchantment_registry.is_empty() {
            writer.write_empty_line();
            self.insert_enchantment_registry(
                id,
                &input.biota_properties_enchantment_registry,
                writer,
            )?;
        }

        Ok(())
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesInt>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_int(
        &self,
        id: u32,
        input: &[BiotaPropertiesInt],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_int` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            let property_value_description =
                self.get_value_enum_name_int(PropertyInt(input[i].r#type), input[i].value);

            let mut comment = gn::<PropertyInt>(input[i].r#type).map(str::to_owned);
            if let Some(d) = property_value_description {
                comment = Some(comment.unwrap_or_default() + " - " + &d);
            }

            Ok(format!(
                "{id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_left(&input[i].value.to_string(), 10),
                s(comment.as_deref())
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesInt64>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_int64(
        &self,
        id: u32,
        input: &[BiotaPropertiesInt64],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_int64` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_left(&input[i].value.to_string(), 12),
                s(gn::<PropertyInt64>(input[i].r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesBool>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_bool(
        &self,
        id: u32,
        input: &[BiotaPropertiesBool],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_bool` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_right(b(input[i].value), 5),
                s(gn::<PropertyBool>(input[i].r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesFloat>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_float(
        &self,
        id: u32,
        input: &[BiotaPropertiesFloat],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_float` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_left(&inv(input[i].value, ""), 7),
                s(gn::<PropertyFloat>(input[i].r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesString>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_string(
        &self,
        id: u32,
        input: &[BiotaPropertiesString],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_string` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                s(SQLWriter::get_sql_string(Some(&input[i].value)).as_deref()),
                s(gn::<PropertyString>(input[i].r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesDID>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_did(
        &self,
        id: u32,
        input: &[BiotaPropertiesDID],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_d_i_d` (`object_Id`, `type`, `value`)");

        let nl = writer.environment_new_line;
        let line_generator = |i: usize| {
            let property_value_description =
                self.get_value_enum_name_did(PropertyDataId(input[i].r#type), input[i].value, nl)?;

            let mut comment = gn::<PropertyDataId>(input[i].r#type).map(str::to_owned);
            if let Some(d) = property_value_description {
                comment = Some(comment.unwrap_or_default() + " - " + &d);
            }

            Ok(format!(
                "{id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_left(&input[i].value.to_string(), 10),
                s(comment.as_deref())
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesPosition>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_position(
        &self,
        id: u32,
        input: &[BiotaPropertiesPosition],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)");

        let nl = writer.environment_new_line;
        let line_generator = |i: usize| {
            let p = &input[i];
            let f = "F6";
            Ok(format!(
                "{id}, {}, {}, {}, {}, {}, {}, {}, {}, {}) /* {} */{nl}/* @teleloc 0x{:08X} [{} {} {}] {} {} {} {} */",
                p.position_type,
                p.obj_cell_id,
                cur(p.origin_x, ""),
                cur(p.origin_y, ""),
                cur(p.origin_z, ""),
                cur(p.angles_w, ""),
                cur(p.angles_x, ""),
                cur(p.angles_y, ""),
                cur(p.angles_z, ""),
                s(gn::<PositionType>(p.position_type)),
                p.obj_cell_id,
                cur(p.origin_x, f),
                cur(p.origin_y, f),
                cur(p.origin_z, f),
                cur(p.angles_w, f),
                cur(p.angles_x, f),
                cur(p.angles_y, f),
                cur(p.angles_z, f),
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesIID>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_iid(
        &self,
        id: u32,
        input: &[BiotaPropertiesIID],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_i_i_d` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_left(&input[i].value.to_string(), 10),
                s(gn::<PropertyInstanceId>(input[i].r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesAttribute>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_attribute(
        &self,
        id: u32,
        input: &[BiotaPropertiesAttribute],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)");

        let line_generator = |i: usize| {
            let a = &input[i];
            Ok(format!(
                "{id}, {}, {}, {}, {}) /* {} */",
                pad_left(&a.r#type.to_string(), 3),
                pad_left(&a.init_level.to_string(), 3),
                a.level_from_cp,
                a.cp_spent,
                s(gn::<PropertyAttribute>(a.r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesAttribute2nd>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_attribute_2nd(
        &self,
        id: u32,
        input: &[BiotaPropertiesAttribute2nd],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)");

        let line_generator = |i: usize| {
            let a = &input[i];
            Ok(format!(
                "{id}, {}, {}, {}, {}, {}) /* {} */",
                pad_left(&a.r#type.to_string(), 3),
                pad_left(&a.init_level.to_string(), 5),
                a.level_from_cp,
                a.cp_spent,
                a.current_level,
                s(gn::<PropertyAttribute2nd>(a.r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesSkill>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_skill(
        &self,
        id: u32,
        input: &[BiotaPropertiesSkill],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)");

        let line_generator = |i: usize| {
            let k = &input[i];
            // Not ACE's (a fix): a skill id Skill does not name is labelled
            // with its number; ACE's `Enum.GetName(typeof(Skill), Type).PadRight(19)` threw
            // NullReferenceException for it, so the whole export failed.
            let skill_name =
                gn::<Skill>(k.r#type).map_or_else(|| k.r#type.to_string(), str::to_owned);
            Ok(format!(
                "{id}, {}, {}, {}, {}, {}, {}, {}) /* {} {} */",
                pad_left(&k.r#type.to_string(), 2),
                pad_left(&k.level_from_pp.to_string(), 3),
                k.sac,
                pad_left(&k.pp.to_string(), 11),
                pad_left(&k.init_level.to_string(), 3),
                pad_left(&k.resistance_at_last_check.to_string(), 3),
                cur(k.last_used_time, ""),
                pad_right(&skill_name, 19),
                SkillAdvancementClass(k.sac).to_dotnet_string()
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesBodyPart>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_body_part(
        &self,
        id: u32,
        input: &[BiotaPropertiesBodyPart],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `biota_properties_body_part` (`object_Id`, `key`, \
             `d_Type`, `d_Val`, `d_Var`, \
             `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, \
             `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)",
        );

        let line_generator = |i: usize| {
            let p = &input[i];
            let i4 = |v: i32| pad_left(&v.to_string(), 4);
            let f4 = |v: f32| pad_left(&inv(v, ""), 4);
            Ok(format!(
                "{id}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) /* {} */",
                pad_left(&p.key.to_string(), 2),
                pad_left(&p.d_type.to_string(), 2),
                pad_left(&p.d_val.to_string(), 2),
                f4(p.d_var),
                i4(p.base_armor),
                i4(p.armor_vs_slash),
                i4(p.armor_vs_pierce),
                i4(p.armor_vs_bludgeon),
                i4(p.armor_vs_cold),
                i4(p.armor_vs_fire),
                i4(p.armor_vs_acid),
                i4(p.armor_vs_electric),
                i4(p.armor_vs_nether),
                p.bh,
                f4(p.hlf),
                f4(p.mlf),
                f4(p.llf),
                f4(p.hrf),
                f4(p.mrf),
                f4(p.lrf),
                f4(p.hlb),
                f4(p.mlb),
                f4(p.llb),
                f4(p.hrb),
                f4(p.mrb),
                f4(p.lrb),
                s(gn::<CombatBodyPart>(p.key))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesSpellBook>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_spell_book(
        &self,
        id: u32,
        input: &[BiotaPropertiesSpellBook],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `biota_properties_spell_book` (`object_Id`, `spell`, `probability`)",
        );

        let line_generator = |i: usize| {
            let mut label: Option<&str> = None;

            if let Some(names) = &self.spell_names {
                label = names.get(&input[i].spell.cs_cast()).map(String::as_str);
            }

            Ok(format!(
                "{id}, {}, {})  /* {} */",
                pad_left(&input[i].spell.to_string(), 5),
                pad_left(&inv(input[i].probability, ""), 6),
                s(label)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesEventFilter>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_event_filter(
        &self,
        id: u32,
        input: &[BiotaPropertiesEventFilter],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_event_filter` (`object_Id`, `event`)");

        let line_generator = |i: usize| {
            let mut label: Option<&str> = None;

            if let Some(names) = &self.packet_op_codes {
                label = names.get(&input[i].event.cs_cast()).map(String::as_str);
            }

            Ok(format!(
                "{id}, {}) /* {} */",
                pad_left(&input[i].event.to_string(), 3),
                s(label)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesEmote>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_emote(
        &self,
        id: u32,
        input: &[BiotaPropertiesEmote],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        for value in input {
            writer.write_empty_line();
            writer.write_line("INSERT INTO `biota_properties_emote` (`object_Id`, `category`, `probability`, `biota_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)");

            let category_label = wrap(gn::<EmoteCategory>(value.category));

            let mut weenie_class_id_label = String::new();
            if let (Some(names), Some(wcid)) = (&self.weenie_names, value.weenie_class_id) {
                weenie_class_id_label = wrap(names.get(&wcid).map(String::as_str));
            }

            let style_label = value
                .style
                .map_or_else(String::new, |v| wrap(gn::<MotionStance>(v)));
            let substyle_label = value
                .substyle
                .map_or_else(String::new, |v| wrap(gn::<MotionCommand>(v)));
            let vendor_type_label = value
                .vendor_type
                .map_or_else(String::new, |v| wrap(gn::<VendorType>(v)));

            let mut output = format!(
                "VALUES ({id}, {}{category_label}, {}, {}{weenie_class_id_label}, {}{style_label}, {}{substyle_label}, {}, {}{vendor_type_label}, {}, {});",
                pad_left(&value.category.to_string(), 2),
                pad_left(&inv(value.probability, ""), 6),
                opt_s(value.weenie_class_id),
                opt_s(value.style),
                opt_s(value.substyle),
                s(SQLWriter::get_sql_string(value.quest.as_deref()).as_deref()),
                opt_s(value.vendor_type),
                opt(value.min_health, ""),
                opt(value.max_health, ""),
            );

            output = SQLWriter::fix_null_fields(&output);

            writer.write_line(&output);

            if !value.biota_properties_emote_action.is_empty() {
                writer.write_empty_line();
                writer.write_line("SET @parent_id = LAST_INSERT_ID();");

                writer.write_empty_line();
                self.insert_emote_action(
                    &ordered(&value.biota_properties_emote_action, |r| r.order),
                    writer,
                )?;
            }
        }
        Ok(())
    }

    /// `CreateSQLINSERTStatement(IList<BiotaPropertiesEmoteAction>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    #[allow(clippy::too_many_lines)]
    fn insert_emote_action(
        &self,
        input: &[BiotaPropertiesEmoteAction],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `biota_properties_emote_action` (`emote_Id`, `order`, `type`, `delay`, `extent`, `motion`, `message`, `test_String`, `min`, `max`, `min_64`, `max_64`, `min_Dbl`, `max_Dbl`, \
             `stat`, `display`, `amount`, `amount_64`, `hero_X_P_64`, `percent`, `spell_Id`, `wealth_Rating`, `treasure_Class`, `treasure_Type`, `p_Script`, `sound`, `destination_Type`, `biota_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`, \
             `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)",
        );

        let line_generator = |i: usize| -> Result<String, SqlWriterError> {
            let a = &input[i];
            let type_label = wrap(gn::<EmoteType>(a.r#type));

            let motion_label = a
                .motion
                .map_or_else(String::new, |v| wrap(gn::<MotionCommand>(v)));

            let mut spell_id_label = String::new();
            if let (Some(names), Some(spell_id)) = (&self.spell_names, a.spell_id) {
                spell_id_label = wrap(names.get(&spell_id.cs_cast()).map(String::as_str));
            }

            let p_script_label = a
                .p_script
                .map_or_else(String::new, |v| wrap(gn::<PlayScript>(v)));
            let sound_label = a.sound.map_or_else(String::new, |v| wrap(gn::<Sound>(v)));

            let mut weenie_class_id_label = String::new();
            if let Some(wcid) = a.weenie_class_id {
                // Not ACE's (a fix): with no weenie names the action has
                // no name label; ACE's lookup in the missing names threw NullReferenceException, so
                // the whole export failed.
                weenie_class_id_label = wrap(
                    self.weenie_names
                        .as_ref()
                        .and_then(|names| names.get(&wcid))
                        .map(String::as_str),
                );
            }

            let destination_type_label = a
                .destination_type
                .map_or_else(String::new, |v| wrap(gn::<DestinationType>(v)));

            let mut teleloc_label = String::new();
            // Not ACE's (a fix): an action with a cell but no origin or
            // angles gets no @teleloc label; ACE read the missing values (`OriginX.Value` and the
            // rest), which threw InvalidOperationException, so the whole export failed.
            if let (
                Some(cell),
                Some(ox),
                Some(oy),
                Some(oz),
                Some(aw),
                Some(ax),
                Some(ay),
                Some(az),
            ) = (
                a.obj_cell_id.filter(|&c| c > 0),
                a.origin_x,
                a.origin_y,
                a.origin_z,
                a.angles_w,
                a.angles_x,
                a.angles_y,
                a.angles_z,
            ) {
                let f = "F6";
                teleloc_label = format!(
                    " /* @teleloc 0x{cell:08X} [{} {} {}] {} {} {} {} */",
                    cur(ox, f),
                    cur(oy, f),
                    cur(oz, f),
                    cur(aw, f),
                    cur(ax, f),
                    cur(ay, f),
                    cur(az, f),
                );
            }

            let fields = [
                "@parent_id".to_owned(),
                pad_left(&a.order.to_string(), 2),
                format!("{}{type_label}", pad_left(&a.r#type.to_string(), 3)),
                cur(a.delay, ""),
                cur(a.extent, ""),
                format!("{}{motion_label}", opt_s(a.motion)),
                s(SQLWriter::get_sql_string(a.message.as_deref()).as_deref()).to_owned(),
                s(SQLWriter::get_sql_string(a.test_string.as_deref()).as_deref()).to_owned(),
                opt_s(a.min),
                opt_s(a.max),
                opt_s(a.min_64),
                opt_s(a.max_64),
                opt(a.min_dbl, ""),
                opt(a.max_dbl, ""),
                opt_s(a.stat),
                opt_b(a.display).to_owned(),
                opt_s(a.amount),
                opt_s(a.amount_64),
                opt_s(a.hero_xp_64),
                opt(a.percent, ""),
                format!("{}{spell_id_label}", opt_s(a.spell_id)),
                opt_s(a.wealth_rating),
                opt_s(a.treasure_class),
                opt_s(a.treasure_type),
                format!("{}{p_script_label}", opt_s(a.p_script)),
                format!("{}{sound_label}", opt_s(a.sound)),
                format!("{}{destination_type_label}", opt_s(a.destination_type)),
                format!("{}{weenie_class_id_label}", opt_s(a.weenie_class_id)),
                opt_s(a.stack_size),
                opt_s(a.palette),
                opt(a.shade, ""),
                opt_b(a.try_to_bond).to_owned(),
                format!("{}{teleloc_label}", opt_s(a.obj_cell_id)),
                opt(a.origin_x, ""),
                opt(a.origin_y, ""),
                opt(a.origin_z, ""),
                opt(a.angles_w, ""),
                opt(a.angles_x, ""),
                opt(a.angles_y, ""),
                opt(a.angles_z, ""),
            ];
            Ok(fields.join(", ") + ")")
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesCreateList>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_create_list(
        &self,
        id: u32,
        input: &[BiotaPropertiesCreateList],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_create_list` (`object_Id`, `destination_Type`, `biota_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)");

        let line_generator = |i: usize| {
            let c = &input[i];
            let mut biota_name: Option<&str> = None;

            if let Some(names) = &self.weenie_names {
                biota_name = names.get(&c.weenie_class_id).map(String::as_str);
            }

            let mut label = format!("{} ({})", s(biota_name), c.weenie_class_id);

            if c.weenie_class_id == 0 {
                //label = GetValueForTreasureData(id, true);
                label = "nothing".to_owned();
            }

            Ok(format!(
                "{id}, {}, {}, {}, {}, {}, {}) /* Create {label} for {} */",
                c.destination_type,
                pad_left(&c.weenie_class_id.to_string(), 5),
                pad_left(&c.stack_size.to_string(), 2),
                c.palette,
                cur(c.shade, ""),
                b(c.try_to_bond),
                s(gn::<DestinationType>(c.destination_type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, BiotaPropertiesBook, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_book(&self, id: u32, input: &BiotaPropertiesBook, writer: &mut SqlOut) {
        writer.write_line("INSERT INTO `biota_properties_book` (`object_Id`, `max_Num_Pages`, `max_Num_Chars_Per_Page`)");

        writer.write_line(&format!(
            "VALUES ({id}, {}, {});",
            input.max_num_pages, input.max_num_chars_per_page
        ));
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesBookPageData>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_book_page_data(
        &self,
        id: u32,
        input: &[BiotaPropertiesBookPageData],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_book_page_data` (`object_Id`, `page_Id`, `author_Id`, `author_Name`, `author_Account`, `ignore_Author`, `page_Text`)");

        let line_generator = |i: usize| {
            let p = &input[i];
            Ok(format!(
                "{id}, {}, {}, {}, {}, {}, {})",
                p.page_id,
                p.author_id,
                s(SQLWriter::get_sql_string(p.author_name.as_deref()).as_deref()),
                s(SQLWriter::get_sql_string(p.author_account.as_deref()).as_deref()),
                b(p.ignore_author),
                s(SQLWriter::get_sql_string(p.page_text.as_deref()).as_deref())
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesGenerator>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_generator(
        &self,
        id: u32,
        input: &[BiotaPropertiesGenerator],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `biota_properties_generator` (`object_Id`, `probability`, `biota_Class_Id`, \
             `delay`, `init_Create`, `max_Create`, `when_Create`, `where_Create`, `stack_Size`, `palette_Id`, `shade`, \
             `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)",
        );

        let nl = writer.environment_new_line;
        let line_generator = |i: usize| {
            let g = &input[i];
            let mut biota_name: Option<&str> = None;

            if let Some(names) = &self.weenie_names {
                biota_name = names.get(&g.weenie_class_id).map(String::as_str);
            }

            let mut label = format!("{} ({})", s(biota_name), g.weenie_class_id);

            if (g.where_create & RegenLocationType::Treasure.bits()) != 0 {
                label = self.get_value_for_treasure_data(g.weenie_class_id, false, nl)?;
            }

            Ok(format!(
                "{id}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) /* Generate {label} (x{} up to max of {}) - Regenerate upon {} - Location to (re)Generate: {} */",
                cur(g.probability, ""),
                g.weenie_class_id,
                opt(g.delay, ""),
                g.init_create,
                g.max_create,
                g.when_create,
                g.where_create,
                opt_s(g.stack_size),
                opt_s(g.palette_id),
                opt(g.shade, ""),
                opt_s(g.obj_cell_id),
                opt(g.origin_x, ""),
                opt(g.origin_y, ""),
                opt(g.origin_z, ""),
                opt(g.angles_w, ""),
                opt(g.angles_x, ""),
                opt(g.angles_y, ""),
                opt(g.angles_z, ""),
                cur(g.init_create, "N0"),
                cur(g.max_create, "N0"),
                s(gn::<RegenerationType>(g.when_create)),
                s(gn::<RegenLocationType>(g.where_create)),
            ))
        };
        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesPalette>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_palette(
        &self,
        id: u32,
        input: &[BiotaPropertiesPalette],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_palette` (`object_Id`, `sub_Palette_Id`, `offset`, `length`, `order`)");

        let line_generator = |i: usize| {
            let p = &input[i];
            Ok(format!(
                "{id}, {}, {}, {}, {})",
                p.sub_palette_id,
                p.offset,
                p.length,
                opt_s(p.order)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesTextureMap>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_texture_map(
        &self,
        id: u32,
        input: &[BiotaPropertiesTextureMap],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `biota_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`, `order`)");

        let line_generator = |i: usize| {
            let t = &input[i];
            Ok(format!(
                "{id}, {}, {}, {}, {})",
                t.index,
                t.old_id,
                t.new_id,
                opt_s(t.order)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesAnimPart>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_anim_part(
        &self,
        id: u32,
        input: &[BiotaPropertiesAnimPart],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `biota_properties_anim_part` (`object_Id`, `index`, `animation_Id`, `order`)",
        );

        let line_generator = |i: usize| {
            let a = &input[i];
            Ok(format!(
                "{id}, {}, {}, {})",
                a.index,
                a.animation_id,
                opt_s(a.order)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<BiotaPropertiesEnchantmentRegistry>, StreamWriter)`.
    // ACE: BiotaSQLWriter.CreateSQLINSERTStatement
    pub fn insert_enchantment_registry(
        &self,
        id: u32,
        input: &[BiotaPropertiesEnchantmentRegistry],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `biota_properties_enchantment_registry` (`object_Id`, `enchantment_Category`, `spell_Id`, `layer_Id`, `has_Spell_Set_Id`, `spell_Category`, `power_Level`, `start_Time`, `duration`, `caster_Object_Id`, `degrade_Modifier`, `degrade_Limit`, `last_Time_Degraded`, `stat_Mod_Type`, `stat_Mod_Key`, `stat_Mod_Value`, `spell_Set_Id`)",
        );

        let line_generator = |i: usize| {
            let e = &input[i];
            Ok(format!(
                "{id}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {})",
                e.enchantment_category,
                e.spell_id,
                e.layer_id,
                b(e.has_spell_set_id),
                e.spell_category,
                e.power_level,
                cur(e.start_time, ""),
                cur(e.duration, ""),
                e.caster_object_id,
                cur(e.degrade_modifier, ""),
                cur(e.degrade_limit, ""),
                cur(e.last_time_degraded, ""),
                e.stat_mod_type,
                e.stat_mod_key,
                cur(e.stat_mod_value, ""),
                e.spell_set_id
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }
}
