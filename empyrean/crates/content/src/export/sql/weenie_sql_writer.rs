// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/WeenieSQLWriter.cs
//! ACE's `WeenieSQLWriter`: a weenie as `DELETE` and `INSERT` statements, with ACE's labels.

use std::ops::{Deref, DerefMut};

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::*;

use super::sql_writer::*;
use crate::models::world::*;

/// ACE's `WeenieSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: WeenieSQLWriter
#[derive(Debug, Clone, Default)]
pub struct WeenieSQLWriter {
    pub base: SQLWriter,
}

impl Deref for WeenieSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for WeenieSQLWriter {
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

/// `{(x.HasValue ? "0x" : "")}{x:X8}`.
fn hex_opt(v: Option<u32>) -> String {
    v.map_or_else(String::new, |v| format!("0x{v:08X}"))
}

/// `{x}` of a nullable integer.
fn opt_s<T: ToString>(v: Option<T>) -> String {
    v.map_or_else(String::new, |v| v.to_string())
}

impl WeenieSQLWriter {
    /// Default is formed from: input.ClassId.ToString("00000") + " " + name
    // ACE: WeenieSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &Weenie) -> String {
        let name = input
            .weenie_properties_string
            .iter()
            .find(|r| r.r#type == PropertyString::Name.bits());

        let mut file_name =
            cur(input.class_id, "00000") + " " + name.map_or("", |n| n.value.as_str());
        file_name = replace_illegal_in_file_name(&file_name);
        file_name += ".sql";

        file_name
    }

    /// This will create a default subfolder path with the following format:
    /// `[Weenie Type]\\[Creature Type]\\` or `[Weenie Type]\\[Item Type]\\`.
    // ACE: WeenieSQLWriter.GetDefaultSubfolder
    #[must_use]
    pub fn get_default_subfolder(&self, input: &Weenie) -> String {
        let mut sub_folder = s(gn::<WeenieType>(input.r#type)).to_owned() + "\\";

        if input.r#type == CsCast::<i32>::cs_cast(WeenieType::Creature.bits()) {
            let property = input
                .weenie_properties_int
                .iter()
                .find(|r| r.r#type == PropertyInt::CreatureType.bits());

            if let Some(property) = property {
                let ct = enum_try_parse_numeric_u32(property.value);

                if CreatureType(ct).is_defined() {
                    sub_folder += s(gn::<CreatureType>(property.value));
                    sub_folder += "\\";
                } else {
                    sub_folder += &format!("UnknownCT_{}\\", property.value);
                }
            } else {
                sub_folder += "Unsorted\\";
            }
        } else if input.r#type == CsCast::<i32>::cs_cast(WeenieType::House.bits()) {
            let property = input
                .weenie_properties_int
                .iter()
                .find(|r| r.r#type == PropertyInt::HouseType.bits());

            if let Some(property) = property {
                // HouseType is `int`-backed, so `Enum.TryParse` of the number always succeeds.
                let ht = HouseType(property.value);

                if ht.is_defined() {
                    sub_folder += s(gn::<HouseType>(property.value));
                    sub_folder += "\\";
                } else {
                    sub_folder += &format!("UnknownHT_{}\\", property.value);
                }
            } else {
                sub_folder += "Unsorted\\";
            }
        } else {
            let property = input
                .weenie_properties_int
                .iter()
                .find(|r| r.r#type == PropertyInt::ItemType.bits());

            if let Some(property) = property {
                sub_folder += s(gn::<ItemType>(property.value));
                sub_folder += "\\";
            } else {
                sub_folder += s(gn::<ItemType>(ItemType::None.bits()));
                sub_folder += "\\";
            }
        }

        sub_folder
    }

    // ACE: WeenieSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(&self, input: &Weenie, writer: &mut SqlOut) {
        writer.write_line(&format!(
            "DELETE FROM `weenie` WHERE `class_Id` = {};",
            input.class_id
        ));
    }

    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    #[allow(clippy::too_many_lines)]
    pub fn create_sql_insert_statement(
        &self,
        input: &Weenie,
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer
            .write_line("INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)");

        let mut output = format!(
            "VALUES ({}, '{}', {}, '{}') /* {} */;",
            input.class_id,
            input.class_name,
            input.r#type,
            date(input.last_modified),
            s(gn::<WeenieType>(input.r#type))
        );

        output = SQLWriter::fix_null_fields(&output);

        writer.write_line(&output);

        let id = input.class_id;

        if !input.weenie_properties_int.is_empty() {
            writer.write_empty_line();
            self.insert_int(
                id,
                &ordered(&input.weenie_properties_int, |r| r.r#type),
                writer,
            )?;
        }
        if !input.weenie_properties_int64.is_empty() {
            writer.write_empty_line();
            self.insert_int64(
                id,
                &ordered(&input.weenie_properties_int64, |r| r.r#type),
                writer,
            )?;
        }
        if !input.weenie_properties_bool.is_empty() {
            writer.write_empty_line();
            self.insert_bool(
                id,
                &ordered(&input.weenie_properties_bool, |r| r.r#type),
                writer,
            )?;
        }
        if !input.weenie_properties_float.is_empty() {
            writer.write_empty_line();
            self.insert_float(
                id,
                &ordered(&input.weenie_properties_float, |r| r.r#type),
                writer,
            )?;
        }
        if !input.weenie_properties_string.is_empty() {
            writer.write_empty_line();
            self.insert_string(
                id,
                &ordered(&input.weenie_properties_string, |r| r.r#type),
                writer,
            )?;
        }
        if !input.weenie_properties_did.is_empty() {
            writer.write_empty_line();
            self.insert_did(
                id,
                &ordered(&input.weenie_properties_did, |r| r.r#type),
                writer,
            )?;
        }

        if !input.weenie_properties_position.is_empty() {
            writer.write_empty_line();
            self.insert_position(
                id,
                &ordered(&input.weenie_properties_position, |r| r.position_type),
                writer,
            )?;
        }

        if !input.weenie_properties_iid.is_empty() {
            writer.write_empty_line();
            self.insert_iid(
                id,
                &ordered(&input.weenie_properties_iid, |r| r.r#type),
                writer,
            )?;
        }

        if !input.weenie_properties_attribute.is_empty() {
            writer.write_empty_line();
            self.insert_attribute(
                id,
                &ordered(&input.weenie_properties_attribute, |r| r.r#type),
                writer,
            )?;
        }
        if !input.weenie_properties_attribute_2nd.is_empty() {
            writer.write_empty_line();
            self.insert_attribute_2nd(
                id,
                &ordered(&input.weenie_properties_attribute_2nd, |r| r.r#type),
                writer,
            )?;
        }

        if !input.weenie_properties_skill.is_empty() {
            writer.write_empty_line();
            self.insert_skill(
                id,
                &ordered(&input.weenie_properties_skill, |r| r.r#type),
                writer,
            )?;
        }

        if !input.weenie_properties_body_part.is_empty() {
            writer.write_empty_line();
            self.insert_body_part(
                id,
                &ordered(&input.weenie_properties_body_part, |r| r.key),
                writer,
            )?;
        }

        if !input.weenie_properties_spell_book.is_empty() {
            writer.write_empty_line();
            self.insert_spell_book(id, &input.weenie_properties_spell_book, writer)?;
        }

        if !input.weenie_properties_event_filter.is_empty() {
            writer.write_empty_line();
            self.insert_event_filter(
                id,
                &ordered(&input.weenie_properties_event_filter, |r| r.event),
                writer,
            )?;
        }

        if !input.weenie_properties_emote.is_empty() {
            // writer.WriteLine(); // This is not needed because CreateSQLINSERTStatement will take care of it for us on each Recipe.
            self.insert_emote(
                id,
                &ordered(&input.weenie_properties_emote, |r| r.category),
                writer,
            )?;
        }

        if !input.weenie_properties_create_list.is_empty() {
            writer.write_empty_line();
            self.insert_create_list(
                id,
                &ordered(&input.weenie_properties_create_list, |r| r.destination_type),
                writer,
            )?;
        }

        if let Some(book) = &input.weenie_properties_book {
            writer.write_empty_line();
            self.insert_book(id, book, writer);
        }
        if !input.weenie_properties_book_page_data.is_empty() {
            writer.write_empty_line();
            self.insert_book_page_data(
                id,
                &ordered(&input.weenie_properties_book_page_data, |r| r.page_id),
                writer,
            )?;
        }

        if !input.weenie_properties_generator.is_empty() {
            writer.write_empty_line();
            self.insert_generator(id, &input.weenie_properties_generator, writer)?;
        }

        if !input.weenie_properties_palette.is_empty() {
            writer.write_empty_line();
            self.insert_palette(
                id,
                &ordered(&input.weenie_properties_palette, |r| r.sub_palette_id),
                writer,
            )?;
        }
        if !input.weenie_properties_texture_map.is_empty() {
            writer.write_empty_line();
            self.insert_texture_map(
                id,
                &ordered(&input.weenie_properties_texture_map, |r| r.index),
                writer,
            )?;
        }
        if !input.weenie_properties_anim_part.is_empty() {
            writer.write_empty_line();
            self.insert_anim_part(
                id,
                &ordered(&input.weenie_properties_anim_part, |r| r.index),
                writer,
            )?;
        }

        Ok(())
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesInt>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_int(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesInt],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            let property_value_description =
                self.get_value_enum_name_int(PropertyInt(input[i].r#type), input[i].value);

            let mut comment = gn::<PropertyInt>(input[i].r#type).map(str::to_owned);
            if let Some(d) = property_value_description {
                comment = Some(comment.unwrap_or_default() + " - " + &d);
            }

            Ok(format!(
                "{weenie_class_id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_left(&input[i].value.to_string(), 10),
                s(comment.as_deref())
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesInt64>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_int64(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesInt64],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_int64` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{weenie_class_id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_left(&input[i].value.to_string(), 10),
                s(gn::<PropertyInt64>(input[i].r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesBool>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_bool(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesBool],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{weenie_class_id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_right(b(input[i].value), 5),
                s(gn::<PropertyBool>(input[i].r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesFloat>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_float(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesFloat],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{weenie_class_id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_left(&inv(input[i].value, "0.###"), 7),
                s(gn::<PropertyFloat>(input[i].r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesString>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_string(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesString],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{weenie_class_id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                s(SQLWriter::get_sql_string(Some(&input[i].value)).as_deref()),
                s(gn::<PropertyString>(input[i].r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesDID>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_did(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesDID],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)");

        let nl = writer.environment_new_line;
        let line_generator = |i: usize| {
            let property_value_description =
                self.get_value_enum_name_did(PropertyDataId(input[i].r#type), input[i].value, nl)?;

            let mut comment = gn::<PropertyDataId>(input[i].r#type).map(str::to_owned);
            if let Some(d) = property_value_description {
                comment = Some(comment.unwrap_or_default() + " - " + &d);
            }

            let value = if PropertyDataId(input[i].r#type).is_hex_data() {
                pad_left(&format!("0x{:08X}", input[i].value), 10)
            } else {
                pad_left(&input[i].value.to_string(), 10)
            };

            Ok(format!(
                "{weenie_class_id}, {}, {value}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                s(comment.as_deref())
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesPosition>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_position(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesPosition],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_position` (`object_Id`, `position_Type`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)");

        let nl = writer.environment_new_line;
        let line_generator = |i: usize| {
            let p = &input[i];
            let g = "0.######";
            let f = "F6";
            Ok(format!(
                "{weenie_class_id}, {}, 0x{:08X}, {}, {}, {}, {}, {}, {}, {}) /* {} */{nl}/* @teleloc 0x{:08X} [{} {} {}] {} {} {} {} */",
                p.position_type,
                p.obj_cell_id,
                tnz(Some(p.origin_x), g),
                tnz(Some(p.origin_y), g),
                tnz(Some(p.origin_z), g),
                tnz(Some(p.angles_w), g),
                tnz(Some(p.angles_x), g),
                tnz(Some(p.angles_y), g),
                tnz(Some(p.angles_z), g),
                s(gn::<PositionType>(p.position_type)),
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

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesIID>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_iid(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesIID],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_i_i_d` (`object_Id`, `type`, `value`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{weenie_class_id}, {}, {}) /* {} */",
                pad_left(&input[i].r#type.to_string(), 3),
                pad_left(&format!("0x{:08X}", input[i].value), 10),
                s(gn::<PropertyInstanceId>(input[i].r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesAttribute>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_attribute(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesAttribute],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)");

        let line_generator = |i: usize| {
            let a = &input[i];
            Ok(format!(
                "{weenie_class_id}, {}, {}, {}, {}) /* {} */",
                pad_left(&a.r#type.to_string(), 3),
                pad_left(&a.init_level.to_string(), 3),
                a.level_from_cp,
                a.cp_spent,
                s(gn::<PropertyAttribute>(a.r#type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesAttribute2nd>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_attribute_2nd(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesAttribute2nd],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)");

        let line_generator = |i: usize| {
            let a = &input[i];
            Ok(format!(
                "{weenie_class_id}, {}, {}, {}, {}, {}) /* {} */",
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

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesSkill>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_skill(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesSkill],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)");

        let line_generator = |i: usize| {
            let k = &input[i];
            // Not ACE's (a fix): a skill id Skill does not name is labelled
            // with its number; ACE's `Enum.GetName(typeof(Skill), Type).PadRight(19)` threw
            // NullReferenceException for it, so the whole export failed.
            let skill_name =
                gn::<Skill>(k.r#type).map_or_else(|| k.r#type.to_string(), str::to_owned);
            Ok(format!(
                "{weenie_class_id}, {}, {}, {}, {}, {}, {}, {}) /* {} {} */",
                pad_left(&k.r#type.to_string(), 2),
                k.level_from_pp,
                k.sac,
                k.pp,
                pad_left(&k.init_level.to_string(), 3),
                k.resistance_at_last_check,
                cur(k.last_used_time, ""),
                pad_right(&skill_name, 19),
                SkillAdvancementClass(k.sac).to_dotnet_string()
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesBodyPart>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_body_part(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesBodyPart],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, \
             `d_Type`, `d_Val`, `d_Var`, \
             `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, \
             `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)",
        );

        let line_generator = |i: usize| {
            let p = &input[i];
            let i4 = |v: i32| pad_left(&v.to_string(), 4);
            let f4 = |v: f32| pad_left(&inv(v, ""), 4);
            Ok(format!(
                "{weenie_class_id}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}) /* {} */",
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

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesSpellBook>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_spell_book(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesSpellBook],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `weenie_properties_spell_book` (`object_Id`, `spell`, `probability`)",
        );

        let line_generator = |i: usize| {
            let mut label: Option<&str> = None;

            if let Some(names) = &self.spell_names {
                label = names.get(&input[i].spell.cs_cast()).map(String::as_str);
            }

            Ok(format!(
                "{weenie_class_id}, {}, {})  /* {} */",
                pad_left(&input[i].spell.to_string(), 5),
                pad_left(&inv(input[i].probability, "0.######"), 6),
                s(label)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesEventFilter>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_event_filter(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesEventFilter],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_event_filter` (`object_Id`, `event`)");

        let line_generator = |i: usize| {
            let mut label: Option<&str> = None;

            if let Some(names) = &self.packet_op_codes {
                label = names.get(&input[i].event.cs_cast()).map(String::as_str);
            }

            Ok(format!(
                "{weenie_class_id}, {}) /* {} */",
                pad_left(&input[i].event.to_string(), 3),
                s(label)
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesEmote>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_emote(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesEmote],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        for value in input {
            writer.write_empty_line();
            writer.write_line("INSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`, `weenie_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)");

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
                "VALUES ({weenie_class_id}, {}{category_label}, {}, {}{weenie_class_id_label}, {}{style_label}, {}{substyle_label}, {}, {}{vendor_type_label}, {}, {});",
                pad_left(&value.category.to_string(), 2),
                pad_left(&inv(value.probability, "0.######"), 6),
                opt_s(value.weenie_class_id),
                hex_opt(value.style),
                hex_opt(value.substyle),
                s(SQLWriter::get_sql_string(value.quest.as_deref()).as_deref()),
                opt_s(value.vendor_type),
                opt(value.min_health, "0.######"),
                opt(value.max_health, "0.######"),
            );

            output = SQLWriter::fix_null_fields(&output);

            writer.write_line(&output);

            if !value.weenie_properties_emote_action.is_empty() {
                writer.write_empty_line();
                writer.write_line("SET @parent_id = LAST_INSERT_ID();");

                writer.write_empty_line();
                self.insert_emote_action(
                    &ordered(&value.weenie_properties_emote_action, |r| r.order),
                    writer,
                )?;
            }
        }
        Ok(())
    }

    /// `CreateSQLINSERTStatement(IList<WeeniePropertiesEmoteAction>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    #[allow(clippy::too_many_lines)]
    fn insert_emote_action(
        &self,
        input: &[WeeniePropertiesEmoteAction],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `delay`, `extent`, `motion`, `message`, `test_String`, `min`, `max`, `min_64`, `max_64`, `min_Dbl`, `max_Dbl`, \
             `stat`, `display`, `amount`, `amount_64`, `hero_X_P_64`, `percent`, `spell_Id`, `wealth_Rating`, `treasure_Class`, `treasure_Type`, `p_Script`, `sound`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`, \
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
            if let (Some(wcid), Some(names)) = (a.weenie_class_id, &self.weenie_names) {
                weenie_class_id_label = wrap(names.get(&wcid).map(String::as_str));
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
                teleloc_label = format!(
                    " /* @teleloc 0x{cell:08X} [{} {} {}] {} {} {} {} */",
                    tnz(Some(ox), "F6"),
                    tnz(Some(oy), "F6"),
                    tnz(Some(oz), "F6"),
                    tnz(Some(aw), "F6"),
                    tnz(Some(ax), "F6"),
                    tnz(Some(ay), "F6"),
                    tnz(Some(az), "F6"),
                );
            }

            let mut stat_label = String::new();
            if let Some(stat) = a.stat {
                match EmoteType(a.r#type.cs_cast()) {
                    EmoteType::AwardLevelProportionalSkillXP
                    | EmoteType::AwardSkillPoints
                    | EmoteType::AwardSkillXP
                    | EmoteType::InqSkillStat
                    | EmoteType::InqRawSkillStat
                    | EmoteType::InqSkillTrained
                    | EmoteType::InqSkillSpecialized
                    | EmoteType::UntrainSkill => {
                        stat_label = format!(" /* Skill.{} */", Skill(stat).to_dotnet_string());
                    }

                    EmoteType::DecrementIntStat
                    | EmoteType::IncrementIntStat
                    | EmoteType::InqIntStat
                    | EmoteType::SetIntStat => {
                        stat_label = format!(
                            " /* PropertyInt.{} */",
                            PropertyInt(stat.cs_cast()).to_dotnet_string()
                        );
                    }

                    EmoteType::InqAttributeStat | EmoteType::InqRawAttributeStat => {
                        stat_label = format!(
                            " /* PropertyAttribute.{} */",
                            PropertyAttribute(stat.cs_cast()).to_dotnet_string()
                        );
                    }

                    EmoteType::InqBoolStat | EmoteType::SetBoolStat => {
                        stat_label = format!(
                            " /* PropertyBool.{} */",
                            PropertyBool(stat.cs_cast()).to_dotnet_string()
                        );
                    }

                    EmoteType::InqFloatStat | EmoteType::SetFloatStat => {
                        stat_label = format!(
                            " /* PropertyFloat.{} */",
                            PropertyFloat(stat.cs_cast()).to_dotnet_string()
                        );
                    }

                    EmoteType::InqInt64Stat | EmoteType::SetInt64Stat => {
                        stat_label = format!(
                            " /* PropertyInt64.{} */",
                            PropertyInt64(stat.cs_cast()).to_dotnet_string()
                        );
                    }

                    EmoteType::InqSecondaryAttributeStat
                    | EmoteType::InqRawSecondaryAttributeStat => {
                        stat_label = format!(
                            " /* PropertyAttribute2nd.{} */",
                            PropertyAttribute2nd(stat.cs_cast()).to_dotnet_string()
                        );
                    }

                    EmoteType::InqStringStat => {
                        stat_label = format!(
                            " /* PropertyString.{} */",
                            PropertyString(stat.cs_cast()).to_dotnet_string()
                        );
                    }

                    _ => {}
                }
            }

            let mut amount_label = String::new();
            if let Some(amount) = a.amount {
                match EmoteType(a.r#type.cs_cast()) {
                    EmoteType::AddCharacterTitle => {
                        amount_label = format!(
                            " /* {} */",
                            CharacterTitle(amount.cs_cast()).to_dotnet_string()
                        );
                    }

                    EmoteType::AddContract | EmoteType::RemoveContract => {
                        amount_label =
                            format!(" /* {} */", ContractId(amount.cs_cast()).to_dotnet_string());
                    }

                    _ => {}
                }
            }

            let treasure_class_label = a
                .treasure_class
                .map_or_else(String::new, |v| wrap(gn::<TreasureClass>(v)));
            let treasure_type_label = a
                .treasure_type
                .map_or_else(String::new, |v| wrap(gn::<TreasureType>(v)));
            let palette_label = a
                .palette
                .map_or_else(String::new, |v| wrap(gn::<PaletteTemplate>(v)));

            let g = "0.######";
            let fields = [
                "@parent_id".to_owned(),
                pad_left(&a.order.to_string(), 2),
                format!("{}{type_label}", pad_left(&a.r#type.to_string(), 3)),
                cur(a.delay, g),
                cur(a.extent, g),
                format!("{}{motion_label}", hex_opt(a.motion)),
                s(SQLWriter::get_sql_string(a.message.as_deref()).as_deref()).to_owned(),
                s(SQLWriter::get_sql_string(a.test_string.as_deref()).as_deref()).to_owned(),
                opt_s(a.min),
                opt_s(a.max),
                opt_s(a.min_64),
                opt_s(a.max_64),
                opt(a.min_dbl, ""),
                opt(a.max_dbl, ""),
                format!("{}{stat_label}", opt_s(a.stat)),
                opt_b(a.display).to_owned(),
                format!("{}{amount_label}", opt_s(a.amount)),
                opt_s(a.amount_64),
                opt_s(a.hero_xp_64),
                opt(a.percent, ""),
                format!("{}{spell_id_label}", opt_s(a.spell_id)),
                opt_s(a.wealth_rating),
                format!("{}{treasure_class_label}", opt_s(a.treasure_class)),
                format!("{}{treasure_type_label}", opt_s(a.treasure_type)),
                format!("{}{p_script_label}", opt_s(a.p_script)),
                format!("{}{sound_label}", opt_s(a.sound)),
                format!("{}{destination_type_label}", opt_s(a.destination_type)),
                format!("{}{weenie_class_id_label}", opt_s(a.weenie_class_id)),
                opt_s(a.stack_size),
                format!("{}{palette_label}", opt_s(a.palette)),
                opt(a.shade, g),
                opt_b(a.try_to_bond).to_owned(),
                format!("{}{teleloc_label}", hex_opt(a.obj_cell_id)),
                tnz(a.origin_x, g),
                tnz(a.origin_y, g),
                tnz(a.origin_z, g),
                tnz(a.angles_w, g),
                tnz(a.angles_x, g),
                tnz(a.angles_y, g),
                tnz(a.angles_z, g),
            ];
            Ok(fields.join(", ") + ")")
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesCreateList>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_create_list(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesCreateList],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)");

        let line_generator = |i: usize| {
            let c = &input[i];
            let mut weenie_name: Option<&str> = None;

            if let Some(names) = &self.weenie_names {
                weenie_name = names.get(&c.weenie_class_id).map(String::as_str);
            }

            let mut label = format!("{} ({})", s(weenie_name), c.weenie_class_id);

            if c.weenie_class_id == 0 {
                //label = GetValueForTreasureData(weenieClassID, true);
                label = "nothing".to_owned();
            }

            Ok(format!(
                "{weenie_class_id}, {}, {}, {}, {}, {}, {}) /* Create {label} for {} */",
                c.destination_type,
                pad_left(&c.weenie_class_id.to_string(), 5),
                pad_left(&c.stack_size.to_string(), 2),
                c.palette,
                cur(c.shade, "0.######"),
                b(c.try_to_bond),
                s(gn::<DestinationType>(c.destination_type))
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, WeeniePropertiesBook, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_book(
        &self,
        weenie_class_id: u32,
        input: &WeeniePropertiesBook,
        writer: &mut SqlOut,
    ) {
        writer.write_line("INSERT INTO `weenie_properties_book` (`object_Id`, `max_Num_Pages`, `max_Num_Chars_Per_Page`)");

        writer.write_line(&format!(
            "VALUES ({weenie_class_id}, {}, {});",
            input.max_num_pages, input.max_num_chars_per_page
        ));
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesBookPageData>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_book_page_data(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesBookPageData],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_book_page_data` (`object_Id`, `page_Id`, `author_Id`, `author_Name`, `author_Account`, `ignore_Author`, `page_Text`)");

        let line_generator = |i: usize| {
            let p = &input[i];
            Ok(format!(
                "{weenie_class_id}, {}, 0x{:08X}, {}, {}, {}, {})",
                p.page_id,
                p.author_id,
                s(SQLWriter::get_sql_string(Some(&p.author_name)).as_deref()),
                s(SQLWriter::get_sql_string(Some(&p.author_account)).as_deref()),
                b(p.ignore_author),
                s(SQLWriter::get_sql_string(Some(&p.page_text)).as_deref())
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesGenerator>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_generator(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesGenerator],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `weenie_properties_generator` (`object_Id`, `probability`, `weenie_Class_Id`, \
             `delay`, `init_Create`, `max_Create`, `when_Create`, `where_Create`, `stack_Size`, `palette_Id`, `shade`, \
             `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)",
        );

        let nl = writer.environment_new_line;
        let line_generator = |i: usize| {
            let g = &input[i];
            let mut weenie_name: Option<&str> = None;

            if let Some(names) = &self.weenie_names {
                weenie_name = names.get(&g.weenie_class_id).map(String::as_str);
            }

            let mut label = format!("{} ({})", s(weenie_name), g.weenie_class_id);

            if (g.where_create & RegenLocationType::Treasure.bits()) != 0 {
                label = self.get_value_for_treasure_data(g.weenie_class_id, false, nl)?;
            }

            let cell = match g.obj_cell_id {
                Some(c) if c > 0 => format!("0x{c:08X}"),
                other => opt_s(other),
            };
            let f = "0.######";
            Ok(format!(
                "{weenie_class_id}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {cell}, {}, {}, {}, {}, {}, {}, {}) /* Generate {label} (x{} up to max of {}) - Regenerate upon {} - Location to (re)Generate: {} */",
                cur(g.probability, f),
                g.weenie_class_id,
                opt(g.delay, f),
                g.init_create,
                g.max_create,
                g.when_create,
                g.where_create,
                opt_s(g.stack_size),
                opt_s(g.palette_id),
                opt(g.shade, f),
                tnz(g.origin_x, f),
                tnz(g.origin_y, f),
                tnz(g.origin_z, f),
                tnz(g.angles_w, f),
                tnz(g.angles_x, f),
                tnz(g.angles_y, f),
                tnz(g.angles_z, f),
                cur(g.init_create, "N0"),
                cur(g.max_create, "N0"),
                s(gn::<RegenerationType>(g.when_create)),
                s(gn::<RegenLocationType>(g.where_create)),
            ))
        };
        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesPalette>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_palette(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesPalette],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_palette` (`object_Id`, `sub_Palette_Id`, `offset`, `length`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{weenie_class_id}, {}, {}, {})",
                input[i].sub_palette_id, input[i].offset, input[i].length
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesTextureMap>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_texture_map(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesTextureMap],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line("INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)");

        let line_generator = |i: usize| {
            Ok(format!(
                "{weenie_class_id}, {}, {}, {})",
                input[i].index, input[i].old_id, input[i].new_id
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }

    /// `CreateSQLINSERTStatement(uint, IList<WeeniePropertiesAnimPart>, StreamWriter)`.
    // ACE: WeenieSQLWriter.CreateSQLINSERTStatement
    pub fn insert_anim_part(
        &self,
        weenie_class_id: u32,
        input: &[WeeniePropertiesAnimPart],
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        writer.write_line(
            "INSERT INTO `weenie_properties_anim_part` (`object_Id`, `index`, `animation_Id`)",
        );

        let line_generator = |i: usize| {
            Ok(format!(
                "{weenie_class_id}, {}, {})",
                input[i].index, input[i].animation_id
            ))
        };

        SQLWriter::values_writer(input.len(), line_generator, writer)
    }
}

/// `Enum.TryParse(value.ToString(), out E e)` for a `uint`-backed enum `E`: the decimal text of an
/// `int` parses as a number; a negative one overflows `uint`, so the parse fails and `e` is 0.
fn enum_try_parse_numeric_u32(value: i32) -> u32 {
    u32::try_from(value).unwrap_or(0)
}
