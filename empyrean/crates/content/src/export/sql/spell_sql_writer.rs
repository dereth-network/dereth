// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/World/SpellSQLWriter.cs
//! ACE's `SpellSQLWriter`: one spell row, naming only the columns that are not null.

use std::ops::{Deref, DerefMut};

use empyrean_common::dotnet::CsCast;
use empyrean_entity::enums::*;

use super::sql_writer::*;
use crate::models::world::Spell;

/// ACE's `SpellSQLWriter`; the name dictionaries are on [`SQLWriter`] (through `Deref`).
// ACE: SpellSQLWriter
#[derive(Debug, Clone, Default)]
pub struct SpellSQLWriter {
    pub base: SQLWriter,
}

impl Deref for SpellSQLWriter {
    type Target = SQLWriter;
    fn deref(&self) -> &SQLWriter {
        &self.base
    }
}

impl DerefMut for SpellSQLWriter {
    fn deref_mut(&mut self) -> &mut SQLWriter {
        &mut self.base
    }
}

/// The column list and the values line being built.
struct Line {
    hdr: String,
    line: String,
}

impl Line {
    fn add(&mut self, column: &str, value: String) {
        self.hdr += ", `";
        self.hdr += column;
        self.hdr += "`";
        self.line += ", ";
        self.line += &value;
    }
}

impl SpellSQLWriter {
    /// Default is formed from: input.Id.ToString("00000") + " " + input.Name
    // ACE: SpellSQLWriter.GetDefaultFileName
    #[must_use]
    pub fn get_default_file_name(&self, input: &Spell) -> String {
        let mut file_name = cur(input.id, "00000") + " " + &input.name;
        file_name = replace_illegal_in_file_name(&file_name);
        file_name += ".sql";

        file_name
    }

    // ACE: SpellSQLWriter.CreateSQLDELETEStatement
    pub fn create_sql_delete_statement(&self, input: &Spell, writer: &mut SqlOut) {
        writer.write_line(&format!("DELETE FROM `spell` WHERE `id` = {};", input.id));
    }

    // ACE: SpellSQLWriter.CreateSQLINSERTStatement
    #[allow(clippy::too_many_lines)]
    pub fn create_sql_insert_statement(&self, input: &Spell, writer: &mut SqlOut) {
        let g = "0.######";
        let mut l = Line {
            hdr: "INSERT INTO `spell` (`id`, `name`".to_owned(),
            line: format!(
                "VALUES ({}, {}",
                input.id,
                s(SQLWriter::get_sql_string(Some(&input.name)).as_deref())
            ),
        };

        if let Some(v) = input.stat_mod_type {
            l.add(
                "stat_Mod_Type",
                format!(
                    "{v} /* {} */",
                    EnchantmentTypeFlags(v.cs_cast()).to_dotnet_string()
                ),
            );
        }
        if let Some(key) = input.stat_mod_key {
            let mut value = key.to_string();

            if let Some(smt) = input.stat_mod_type {
                let smt = EnchantmentTypeFlags(smt.cs_cast());

                if smt.contains(EnchantmentTypeFlags::Skill) {
                    if Skill(key.cs_cast()).is_defined() {
                        value += &format!(" /* {} */", s(gn::<Skill>(key)));
                    }
                } else if smt.contains(EnchantmentTypeFlags::Attribute) {
                    if PropertyAttribute(key.cs_cast()).is_defined() {
                        value += &format!(" /* {} */", s(gn::<PropertyAttribute>(key)));
                    }
                } else if smt.contains(EnchantmentTypeFlags::SecondAtt) {
                    if PropertyAttribute2nd(key.cs_cast()).is_defined() {
                        value += &format!(" /* {} */", s(gn::<PropertyAttribute2nd>(key)));
                    }
                } else if smt.contains(EnchantmentTypeFlags::Int) {
                    if PropertyInt(key.cs_cast()).is_defined() {
                        value += &format!(" /* {} */", s(gn::<PropertyInt>(key)));
                    }
                } else if smt.contains(EnchantmentTypeFlags::Float)
                    && PropertyFloat(key.cs_cast()).is_defined()
                {
                    value += &format!(" /* {} */", s(gn::<PropertyFloat>(key)));
                }
            }
            l.add("stat_Mod_Key", value);
        }
        if let Some(v) = input.stat_mod_val {
            l.add("stat_Mod_Val", cur(v, g));
        }

        if let Some(v) = input.e_type {
            l.add("e_Type", format!("{v} /* {} */", s(gn::<DamageType>(v))));
        }

        if let Some(v) = input.base_intensity {
            l.add("base_Intensity", v.to_string());
        }

        if let Some(v) = input.variance {
            l.add("variance", v.to_string());
        }

        if let Some(wcid) = input.wcid {
            match &self.weenie_names {
                Some(names) if wcid > 0 && names.contains_key(&wcid) => {
                    l.add(
                        "wcid",
                        format!(
                            "{wcid} /* {} */",
                            names.get(&wcid).map_or("", String::as_str)
                        ),
                    );
                }
                _ => l.add("wcid", wcid.to_string()),
            }
        }

        if let Some(v) = input.num_projectiles {
            l.add("num_Projectiles", v.to_string());
        }

        if let Some(v) = input.num_projectiles_variance {
            l.add("num_Projectiles_Variance", cur(v, g));
        }

        let angles: [(&str, Option<f32>); 3] = [
            ("spread_Angle", input.spread_angle),
            ("vertical_Angle", input.vertical_angle),
            ("default_Launch_Angle", input.default_launch_angle),
        ];
        for (column, v) in angles {
            if let Some(v) = v {
                l.add(column, cur(v, g));
            }
        }

        if let Some(v) = input.non_tracking {
            l.add("non_Tracking", b(v).to_owned());
        }

        let origins: [(&str, Option<f32>); 12] = [
            ("create_Offset_Origin_X", input.create_offset_origin_x),
            ("create_Offset_Origin_Y", input.create_offset_origin_y),
            ("create_Offset_Origin_Z", input.create_offset_origin_z),
            ("padding_Origin_X", input.padding_origin_x),
            ("padding_Origin_Y", input.padding_origin_y),
            ("padding_Origin_Z", input.padding_origin_z),
            ("dims_Origin_X", input.dims_origin_x),
            ("dims_Origin_Y", input.dims_origin_y),
            ("dims_Origin_Z", input.dims_origin_z),
            ("peturbation_Origin_X", input.peturbation_origin_x),
            ("peturbation_Origin_Y", input.peturbation_origin_y),
            ("peturbation_Origin_Z", input.peturbation_origin_z),
        ];
        for (column, v) in origins {
            if let Some(v) = v {
                l.add(column, cur(v, g));
            }
        }

        if let Some(v) = input.imbued_effect {
            l.add(
                "imbued_Effect",
                format!("{v} /* {} */", s(gn::<ImbuedEffectType>(v))),
            );
        }

        if let Some(v) = input.slayer_creature_type {
            l.add(
                "slayer_Creature_Type",
                format!("{v} /* {} */", s(gn::<CreatureType>(v))),
            );
        }

        if let Some(v) = input.slayer_damage_bonus {
            l.add("slayer_Damage_Bonus", cur(v, ""));
        }

        if let Some(v) = input.crit_freq {
            l.add("crit_Freq", cur(v, ""));
        }

        if let Some(v) = input.crit_multiplier {
            l.add("crit_Multiplier", cur(v, ""));
        }

        if let Some(v) = input.ignore_magic_resist {
            l.add("ignore_Magic_Resist", v.to_string());
        }

        if let Some(v) = input.elemental_modifier {
            l.add("elemental_Modifier", cur(v, ""));
        }

        if let Some(v) = input.drain_percentage {
            l.add("drain_Percentage", cur(v, g));
        }

        if let Some(v) = input.damage_ratio {
            l.add("damage_Ratio", cur(v, ""));
        }

        if let Some(v) = input.damage_type {
            l.add(
                "damage_Type",
                format!("{v} /* {} */", s(gn::<DamageType>(v))),
            );
        }

        if let Some(v) = input.boost {
            l.add("boost", v.to_string());
        }

        if let Some(v) = input.boost_variance {
            l.add("boost_Variance", v.to_string());
        }

        if let Some(v) = input.source {
            l.add(
                "source",
                format!("{v} /* {} */", s(gn::<PropertyAttribute2nd>(v))),
            );
        }

        if let Some(v) = input.destination {
            l.add(
                "destination",
                format!("{v} /* {} */", s(gn::<PropertyAttribute2nd>(v))),
            );
        }

        if let Some(v) = input.proportion {
            l.add("proportion", cur(v, ""));
        }

        if let Some(v) = input.loss_percent {
            l.add("loss_Percent", cur(v, g));
        }

        if let Some(v) = input.source_loss {
            l.add("source_Loss", v.to_string());
        }

        if let Some(v) = input.transfer_cap {
            l.add("transfer_Cap", v.to_string());
        }

        if let Some(v) = input.max_boost_allowed {
            l.add("max_Boost_Allowed", v.to_string());
        }

        if let Some(v) = input.transfer_bitfield {
            l.add(
                "transfer_Bitfield",
                format!(
                    "{v} /* {} */",
                    TransferFlags(v.cs_cast()).to_dotnet_string()
                ),
            );
        }

        if let Some(v) = input.index {
            let label = if input.name.contains("Tie") {
                "PortalLinkType.".to_owned() + &PortalLinkType(v).to_dotnet_string()
            } else {
                "PortalRecallType.".to_owned() + &PortalRecallType(v).to_dotnet_string()
            };
            l.add("index", format!("{v} /* {label} */"));
        }

        if let Some(v) = input.link {
            l.add(
                "link",
                format!(
                    "{v} /* PortalSummonType.{} */",
                    PortalSummonType(v).to_dotnet_string()
                ),
            );
        }

        if let Some(v) = input.position_obj_cell_id {
            l.add("position_Obj_Cell_ID", format!("0x{v:08X}"));
        }
        let position: [(&str, Option<f32>); 7] = [
            ("position_Origin_X", input.position_origin_x),
            ("position_Origin_Y", input.position_origin_y),
            ("position_Origin_Z", input.position_origin_z),
            ("position_Angles_W", input.position_angles_w),
            ("position_Angles_X", input.position_angles_x),
            ("position_Angles_Y", input.position_angles_y),
            ("position_Angles_Z", input.position_angles_z),
        ];
        for (column, v) in position {
            if v.is_some() {
                l.add(column, tnz(v, g));
            }
        }

        if let Some(v) = input.min_power {
            l.add("min_Power", v.to_string());
        }

        if let Some(v) = input.max_power {
            l.add("max_Power", v.to_string());
        }

        if let Some(v) = input.power_variance {
            l.add("power_Variance", cur(v, g));
        }

        if let Some(v) = input.dispel_school {
            l.add(
                "dispel_School",
                format!("{v} /* {} */", s(gn::<MagicSchool>(v))),
            );
        }

        if let Some(v) = input.align {
            l.add("align", v.to_string());
        }

        if let Some(v) = input.number {
            l.add("number", v.to_string());
        }

        if let Some(v) = input.number_variance {
            l.add("number_Variance", cur(v, g));
        }

        if let Some(v) = input.dot_duration {
            l.add("dot_Duration", cur(v, ""));
        }

        l.add("last_Modified", format!("'{}'", date(input.last_modified)));

        let spell_line_hdr = l.hdr + ")";
        let mut spell_line = l.line + ");";

        spell_line = SQLWriter::fix_null_fields(&spell_line);

        if let (Some(cell), Some(ox), Some(oy), Some(oz), Some(ax), Some(ay), Some(az), Some(aw)) = (
            input.position_obj_cell_id,
            input.position_origin_x,
            input.position_origin_y,
            input.position_origin_z,
            input.position_angles_x,
            input.position_angles_y,
            input.position_angles_z,
            input.position_angles_w,
        ) {
            let f = "F6";
            spell_line += &format!(
                "{}/* @teleloc 0x{cell:08X} [{} {} {}] {} {} {} {} */",
                writer.environment_new_line,
                tnz(Some(ox), f),
                tnz(Some(oy), f),
                tnz(Some(oz), f),
                tnz(Some(aw), f),
                tnz(Some(ax), f),
                tnz(Some(ay), f),
                tnz(Some(az), f),
            );
        }

        writer.write_line(&spell_line_hdr);
        writer.write_line(&spell_line);
    }
}
