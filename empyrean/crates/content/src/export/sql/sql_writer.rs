// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/SQLFormatters/SQLWriter.cs
//! ACE's `SQLWriter`, the base of the World SQL writers: the name dictionaries that feed the
//! `/* label */` comments, `ValuesWriter`, `GetSQLString`, `FixNullFields`, the enum-name helpers,
//! the treasure labels and `TrimNegativeZero`.
//!
//! Every value is printed as the C# prints it: `empyrean_common::dotnet::format` for `ToString(format)`
//! and interpolation in `en-US` (ACE sets the culture in `Program.Main`), `.NET` enum `ToString`
//! and `Enum.GetName` from empyrean-entity's generated enums.
//!
//! A C# exception (a missing dictionary key, `Nullable.Value` on null, `Substring` out of range)
//! is a [`SqlWriterError`]. As in ACE, the lines written before it stay in the [`SqlOut`].

use std::collections::HashMap;
use std::fmt;

use empyrean_common::dotnet::math::round_digits_mode;
use empyrean_common::dotnet::{
    format as dn, format_aligned, CsCast, DotNetDateTime, DotNetDict, MidpointRounding, Num,
};
use empyrean_entity::enums::*;

use crate::models::world::{TreasureDeath, TreasureWielded, Weenie};

/// A `StreamWriter` over a string.
///
/// `new_line` is the writer's `NewLine` (what `WriteLine` appends). `environment_new_line` is
/// `Environment.NewLine`, which the writers also splice into the middle of some lines (the
/// `@teleloc` comment lines and the treasure labels): `"\r\n"` on Windows, `"\n"` elsewhere. A
/// `StreamWriter` created as ACE creates it has both equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlOut {
    /// The text written so far.
    pub text: String,
    /// `StreamWriter.NewLine`.
    pub new_line: &'static str,
    /// `Environment.NewLine`.
    pub environment_new_line: &'static str,
}

impl SqlOut {
    /// A writer whose `NewLine` is also `Environment.NewLine`.
    #[must_use]
    pub fn new(new_line: &'static str) -> Self {
        Self {
            text: String::new(),
            new_line,
            environment_new_line: new_line,
        }
    }

    /// A writer whose `NewLine` differs from `Environment.NewLine`.
    #[must_use]
    pub fn with_environment_new_line(
        new_line: &'static str,
        environment_new_line: &'static str,
    ) -> Self {
        Self {
            text: String::new(),
            new_line,
            environment_new_line,
        }
    }

    /// `WriteLine(value)`.
    pub fn write_line(&mut self, value: &str) {
        self.text.push_str(value);
        self.text.push_str(self.new_line);
    }

    /// `WriteLine()`.
    pub fn write_empty_line(&mut self) {
        self.text.push_str(self.new_line);
    }
}

/// An exception a writer throws; `exception` is the .NET type ACE's code raises.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlWriterError {
    /// The .NET exception type, for example `System.Collections.Generic.KeyNotFoundException`.
    pub exception: &'static str,
    /// What was being done.
    pub message: String,
}

impl fmt::Display for SqlWriterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.exception, self.message)
    }
}

impl std::error::Error for SqlWriterError {}

pub(crate) const KEY_NOT_FOUND: &str = "System.Collections.Generic.KeyNotFoundException";
pub(crate) const ARGUMENT_OUT_OF_RANGE: &str = "System.ArgumentOutOfRangeException";
pub(crate) const ARGUMENT: &str = "System.ArgumentException";

pub(crate) fn err(exception: &'static str, message: impl Into<String>) -> SqlWriterError {
    SqlWriterError {
        exception,
        message: message.into(),
    }
}

/// The name dictionaries every World writer inherits. Each is optional, as in ACE (`null` turns
/// the labels they feed off).
#[derive(Debug, Clone, Default)]
pub struct SQLWriter {
    /// If a weenie id is found in the dictionary, the name will be added in the form of a
    /// `/* Friendly Weenie Name */`.
    // ACE: SQLWriter.WeenieNames
    pub weenie_names: Option<DotNetDict<u32, String>>,
    /// If a spell id is found in the dictionary, the name will be added in the form of a
    /// `/* Friendly Spell Name */`.
    // ACE: SQLWriter.SpellNames
    pub spell_names: Option<DotNetDict<u32, String>>,
    /// If a opcode is found in the dictionary, the name will be added in the form of a
    /// `/* Friendly Opcode Name */` (ACE's export passes `PacketOpCodeNames.Values`).
    // ACE: SQLWriter.PacketOpCodes
    pub packet_op_codes: Option<DotNetDict<u32, String>>,
    // ACE: SQLWriter.TreasureWielded
    pub treasure_wielded: Option<DotNetDict<u32, Vec<TreasureWielded>>>,
    // ACE: SQLWriter.TreasureDeath
    pub treasure_death: Option<DotNetDict<u32, TreasureDeath>>,
    // ACE: SQLWriter.Weenies
    pub weenies: Option<DotNetDict<u32, Weenie>>,
}

// ---- value text -----------------------------------------------------------------------------

/// `{value:format}` / `value.ToString(format)` in the current culture (`en-US`).
pub(crate) fn cur(v: impl Into<Num>, format: &str) -> String {
    dn(v, format)
}

/// `value.ToString(format, CultureInfo.InvariantCulture)`: as `en-US` but for the infinity symbols.
pub(crate) fn inv(v: impl Into<Num>, format: &str) -> String {
    let s = dn(v, format);
    match s.as_str() {
        "∞" => "Infinity".into(),
        "-∞" => "-Infinity".into(),
        _ => s,
    }
}

/// `{value:format}` of a nullable: nothing for null.
pub(crate) fn opt<T: Into<Num> + Copy>(v: Option<T>, format: &str) -> String {
    v.map_or_else(String::new, |v| dn(v, format))
}

/// C# `bool.ToString()`.
pub(crate) fn b(v: bool) -> &'static str {
    if v {
        "True"
    } else {
        "False"
    }
}

/// `{value}` of a `bool?`.
pub(crate) fn opt_b(v: Option<bool>) -> &'static str {
    v.map_or("", b)
}

/// `string.PadLeft(n)`.
pub(crate) fn pad_left(s: &str, n: usize) -> String {
    format!("{s:>n$}")
}

/// `string.PadRight(n)`.
pub(crate) fn pad_right(s: &str, n: usize) -> String {
    format!("{s:<n$}")
}

/// `{value:yyyy-MM-dd HH:mm:ss}`.
pub(crate) fn date(d: DotNetDateTime) -> String {
    d.format("yyyy-MM-dd HH:mm:ss")
}

/// `{string}` of a nullable string: nothing for null.
pub(crate) fn s(v: Option<&str>) -> &str {
    v.unwrap_or("")
}

/// `Enum.GetName(typeof(E), value)` for an integer of any C# type.
pub(crate) fn gn<E: AceEnum>(value: impl Into<i64>) -> Option<&'static str> {
    get_name::<E>(value.into())
}

/// `{TrimNegativeZero(x):format}`.
pub(crate) fn tnz(v: Option<f32>, format: &str) -> String {
    opt(SQLWriter::trim_negative_zero(v), format)
}

/// C#'s `Path.GetInvalidFileNameChars()` of the platform the program runs on.
fn is_invalid_file_name_char(c: char) -> bool {
    if cfg!(windows) {
        matches!(
            c,
            '"' | '<' | '>' | '|' | '\0' | '\u{1}'..='\u{1f}' | ':' | '*' | '?' | '\\' | '/'
        )
    } else {
        matches!(c, '\0' | '/')
    }
}

/// `IllegalInFileName.Replace(fileName, "_")`.
// ACE: SQLWriter.IllegalInFileName
pub(crate) fn replace_illegal_in_file_name(file_name: &str) -> String {
    file_name
        .chars()
        .map(|c| if is_invalid_file_name_char(c) { '_' } else { c })
        .collect()
}

/// `float.Parse(s)` in `en-US` of the text `ToString` produced.
fn float_parse(s: &str) -> f32 {
    match s {
        "∞" => f32::INFINITY,
        "-∞" => f32::NEG_INFINITY,
        "NaN" => f32::NAN,
        _ => s.parse().unwrap_or(0.0),
    }
}

/// 35 spaces: the indent of the treasure label lines.
const TREASURE_INDENT: &str = "                                   ";

impl SQLWriter {
    /// lineGenerator should generate the entire line after the first `(`. It should include the
    /// trailing `)` and any comments after. It should consist of only a single line. This will
    /// automatically call FixNullFields on the output created by lineGenerator().
    // ACE: SQLWriter.ValuesWriter
    pub fn values_writer(
        count: usize,
        mut line_generator: impl FnMut(usize) -> Result<String, SqlWriterError>,
        writer: &mut SqlOut,
    ) -> Result<(), SqlWriterError> {
        for i in 0..count {
            let mut output = if i == 0 {
                "VALUES (".to_owned()
            } else {
                "     , (".to_owned()
            };

            output += &line_generator(i)?;

            if i == count - 1 {
                output += ";";
            }

            output = Self::fix_null_fields(&output);

            writer.write_line(&output);
        }
        Ok(())
    }

    /// If input is null, NULL will be returned (`None`). If input is not null, a string surrounded
    /// in ' will be returned, and any ' found within the string will be replaced with ''.
    // ACE: SQLWriter.GetSQLString
    #[must_use]
    pub fn get_sql_string(input: Option<&str>) -> Option<String> {
        let input = input?;
        Some(format!("'{}'", input.replace('\'', "''")))
    }

    /// This will find values that were not output to a values line, for example, if a property is
    /// a (int?), and it has no value, you might see ", ," in the sql. This function will replace
    /// that ", ," with ", NULL,". It also removes empty comments like the following: " /*  */"
    // ACE: SQLWriter.FixNullFields
    // Not ACE's (a fix): the replacements apply to the SQL itself, not
    // inside string values ('…', with '' for a quote) or comments (/* … */); ACE ran them over the
    // whole generated line, so text containing ", ," or ", )" was written with "NULL" spliced
    // into it.
    #[must_use]
    pub fn fix_null_fields(input: &str) -> String {
        let mut out = String::with_capacity(input.len() + 16);
        let mut code = String::new();
        let mut rest = input;
        while !rest.is_empty() {
            let at = rest.find(['\'', '/']).unwrap_or(rest.len());
            code.push_str(&rest[..at]);
            rest = &rest[at..];
            if rest.starts_with('\'') {
                // a string value: up to its closing quote ('' is a quote inside it)
                let mut end = 1;
                loop {
                    match rest[end..].find('\'') {
                        Some(q) if rest[end + q + 1..].starts_with('\'') => end += q + 2,
                        Some(q) => {
                            end += q + 1;
                            break;
                        }
                        None => {
                            end = rest.len();
                            break;
                        }
                    }
                }
                out.push_str(&Self::fix_null_fields_in_code(&std::mem::take(&mut code)));
                out.push_str(&rest[..end]);
                rest = &rest[end..];
            } else if rest.starts_with("/*") {
                let end = rest[2..].find("*/").map_or(rest.len(), |e| e + 4);
                let comment = &rest[..end];
                let mut fixed = Self::fix_null_fields_in_code(&std::mem::take(&mut code));
                // Remove empty comments
                if comment == "/*  */" && fixed.ends_with(' ') {
                    fixed.pop();
                } else {
                    fixed.push_str(comment);
                }
                out.push_str(&fixed);
                rest = &rest[end..];
            } else if !rest.is_empty() {
                code.push('/');
                rest = &rest[1..];
            }
        }
        out.push_str(&Self::fix_null_fields_in_code(&code));
        out
    }

    /// ACE's replacements on a run of SQL outside string values and comments.
    fn fix_null_fields_in_code(input: &str) -> String {
        let input = input.replace(", ,", ", NULL,");
        let input = input.replace(", ,", ", NULL,");

        // Fix cases where the last field might be null
        input.replace(", )", ", NULL)")
    }

    /// `GetValueEnumName(PropertyInt property, int value)`.
    // ACE: SQLWriter.GetValueEnumName
    #[must_use]
    pub fn get_value_enum_name_int(&self, property: PropertyInt, value: i32) -> Option<String> {
        let u = value.cs_cast();
        match property {
            PropertyInt::ActivationCreateClass
            | PropertyInt::AttackersClass
            | PropertyInt::PetClass => {
                if let Some(names) = &self.weenie_names {
                    return names.get(&u).cloned();
                }
            }
            PropertyInt::ActivationResponse => {
                return Some(ActivationResponse(value).to_dotnet_string())
            }
            PropertyInt::AetheriaBitfield => {
                return Some(AetheriaBitfield(value).to_dotnet_string())
            }
            PropertyInt::AiAllowedCombatStyle => {
                return Some(CombatStyle(value).to_dotnet_string())
            }
            PropertyInt::AppraisalLongDescDecoration => {
                return Some(AppraisalLongDescDecorations(value).to_dotnet_string());
            }
            PropertyInt::AttackType => return Some(AttackType(value).to_dotnet_string()),
            PropertyInt::ChannelsActive | PropertyInt::ChannelsAllowed => {
                return Some(Channel(value).to_dotnet_string());
            }
            PropertyInt::ClothingPriority => return Some(CoverageMask(u).to_dotnet_string()),
            PropertyInt::CurrentWieldedLocation => return Some(EquipMask(u).to_dotnet_string()),
            PropertyInt::DamageType => return Some(DamageType(value).to_dotnet_string()),
            PropertyInt::DefaultCombatStyle => return Some(CombatStyle(value).to_dotnet_string()),
            PropertyInt::HookType => return Some(HookType(value).to_dotnet_string()),
            PropertyInt::ImbuedEffect
            | PropertyInt::ImbuedEffect2
            | PropertyInt::ImbuedEffect3
            | PropertyInt::ImbuedEffect4 => return Some(ImbuedEffectType(u).to_dotnet_string()),
            PropertyInt::ItemUseable => return Some(Usable(u).to_dotnet_string()),
            PropertyInt::MerchandiseItemTypes => return Some(ItemType(u).to_dotnet_string()),
            PropertyInt::PhysicsState => return Some(PhysicsState(value).to_dotnet_string()),
            PropertyInt::PortalBitmask => return Some(PortalBitmask(value).to_dotnet_string()),
            PropertyInt::SlayerCreatureType => return Some(CreatureType(u).to_dotnet_string()),
            PropertyInt::TargetType => return Some(ItemType(u).to_dotnet_string()),
            PropertyInt::UiEffects => return Some(UiEffects(u).to_dotnet_string()),
            PropertyInt::ValidLocations => return Some(EquipMask(u).to_dotnet_string()),
            PropertyInt::WieldRequirements
            | PropertyInt::WieldRequirements2
            | PropertyInt::WieldRequirements3
            | PropertyInt::WieldRequirements4 => {
                return Some(WieldRequirement(value).to_dotnet_string())
            }
            PropertyInt::CombatTactic
            | PropertyInt::HomesickTargetingTactic
            | PropertyInt::TargetingTactic => {
                return Some(TargetingTactic(value).to_dotnet_string());
            }
            PropertyInt::Tolerance => return Some(Tolerance(value).to_dotnet_string()),
            PropertyInt::AiOptions => return Some(AiOption(value).to_dotnet_string()),
            PropertyInt::Faction1Bits
            | PropertyInt::Faction2Bits
            | PropertyInt::Faction3Bits
            | PropertyInt::Hatred1Bits
            | PropertyInt::Hatred2Bits
            | PropertyInt::Hatred3Bits => return Some(FactionBits(value).to_dotnet_string()),
            PropertyInt::CharacterTitleId => return Some(CharacterTitle(u).to_dotnet_string()),
            _ => {}
        }

        property.get_value_enum_name(value)
    }

    /// `GetValueEnumName(PropertyDataId property, uint value)`.
    // ACE: SQLWriter.GetValueEnumName
    pub fn get_value_enum_name_did(
        &self,
        property: PropertyDataId,
        value: u32,
        new_line: &str,
    ) -> Result<Option<String>, SqlWriterError> {
        let i: i32 = value.cs_cast();
        match property {
            PropertyDataId::AlternateCurrency
            | PropertyDataId::AugmentationCreateItem
            | PropertyDataId::LastPortal
            | PropertyDataId::LinkedPortalOne
            | PropertyDataId::LinkedPortalTwo
            | PropertyDataId::OriginalPortal
            | PropertyDataId::UseCreateItem
            | PropertyDataId::VendorsClassId
            | PropertyDataId::PCAPPhysicsDIDDataTemplatedFrom => {
                if let Some(names) = &self.weenie_names {
                    return Ok(names.get(&value).cloned());
                }
            }
            PropertyDataId::BlueSurgeSpell
            | PropertyDataId::DeathSpell
            | PropertyDataId::ProcSpell
            | PropertyDataId::RedSurgeSpell
            | PropertyDataId::Spell
            | PropertyDataId::YellowSurgeSpell => {
                if let Some(names) = &self.spell_names {
                    return Ok(names.get(&value).cloned());
                }
            }
            PropertyDataId::WieldedTreasureType
            | PropertyDataId::InventoryTreasureType
            | PropertyDataId::ShopTreasureType => {
                if let Some(list) = self.treasure_wielded.as_ref().and_then(|t| t.get(&value)) {
                    return self.get_values_for_treasure_did(list, new_line).map(Some);
                } else if let Some(td) = self.treasure_death.as_ref().and_then(|t| t.get(&value)) {
                    return Ok(Some(format!("Loot Tier: {}", td.tier)));
                }
            }
            PropertyDataId::DeathTreasureType => {
                if let Some(td) = self.treasure_death.as_ref().and_then(|t| t.get(&value)) {
                    return Ok(Some(format!("Loot Tier: {}", td.tier)));
                } else if let Some(list) =
                    self.treasure_wielded.as_ref().and_then(|t| t.get(&value))
                {
                    return self.get_values_for_treasure_did(list, new_line).map(Some);
                }
            }
            PropertyDataId::PCAPRecordedObjectDesc => {
                return Ok(Some(ObjectDescriptionFlag(i).to_dotnet_string()))
            }
            PropertyDataId::PCAPRecordedPhysicsDesc => {
                return Ok(Some(PhysicsDescriptionFlag(i).to_dotnet_string()))
            }
            PropertyDataId::PCAPRecordedWeenieHeader => {
                return Ok(Some(WeenieHeaderFlag(value).to_dotnet_string()))
            }
            PropertyDataId::PCAPRecordedWeenieHeader2 => {
                return Ok(Some(WeenieHeaderFlag2(value).to_dotnet_string()))
            }
            _ => {}
        }

        Ok(property.get_value_enum_name(value))
    }

    /// The label of a wielded-treasure list: one line per item, grouped into sets and subsets,
    /// each line starting with `Environment.NewLine` (`new_line`).
    // ACE: SQLWriter.GetValuesForTreasureDID
    #[allow(clippy::too_many_lines, clippy::float_cmp)]
    pub fn get_values_for_treasure_did(
        &self,
        treasure_wielded_list: &[TreasureWielded],
        new_line: &str,
    ) -> Result<String, SqlWriterError> {
        let nl = new_line;
        let ind = TREASURE_INDENT;
        let mut treasure = String::new();

        let mut set_number = 0;
        let mut set_total_probability = 0.0f32;
        let mut next_item_is_start_of_sub_set = false;
        let mut next_item_is_part_of_sub_set = false;
        let mut depth: i32 = 0;
        let mut sub_set_total_probability: HashMap<i32, f32> = HashMap::new();

        let sub = |m: &HashMap<i32, f32>, depth: i32| -> Result<f32, SqlWriterError> {
            m.get(&depth)
                .copied()
                .ok_or_else(|| err(KEY_NOT_FOUND, format!("subSetTotalProbability[{depth}]")))
        };
        let spaces = |depth: i32| " ".repeat(usize::try_from(depth).unwrap_or(0));

        for item in treasure_wielded_list {
            let mut treasure_item = String::new();
            if item.stack_size > 1 && item.stack_size_variance == 0.0 {
                treasure_item += &format!("{}x ", item.stack_size);
            }

            if item.stack_size > 1 && item.stack_size_variance > 0.0 {
                #[allow(clippy::cast_precision_loss)]
                let product = item.stack_size as f32 * (1.0f32 - item.stack_size_variance);
                let rounded: i32 =
                    round_digits_mode(f64::from(product), 0, MidpointRounding::AwayFromZero)
                        .cs_cast();
                let min_stack = 1.max(rounded);
                let max_stack = item.stack_size;
                if min_stack != max_stack {
                    treasure_item += &format!("{min_stack}x to {max_stack}x ");
                } else {
                    treasure_item += &format!("{max_stack}x ");
                }
            }

            // Not ACE's (a fix): a weenie the names lack (or no names)
            // is written as its number alone; ACE indexed the names, which threw, so the whole
            // export failed.
            match self
                .weenie_names
                .as_ref()
                .and_then(|names| names.get(&item.weenie_class_id))
            {
                Some(name) => treasure_item += &format!("{name} ({})", item.weenie_class_id),
                None => treasure_item += &format!("({})", item.weenie_class_id),
            }

            if item.palette_id > 0 {
                treasure_item += &format!(
                    " | Palette: {} ({})",
                    s(gn::<PaletteTemplate>(item.palette_id)),
                    item.palette_id
                );
            }

            if item.shade > 0.0 {
                treasure_item += &format!(" | Shade: {}", cur(item.shade, ""));
            }

            if item.stack_size_variance > 0.0 {
                treasure_item += &format!(
                    " | StackSizeVariance: {}",
                    cur(item.stack_size_variance, "")
                );
            }

            if item.set_start || (set_total_probability >= 1.0 && !next_item_is_part_of_sub_set) {
                if (!next_item_is_start_of_sub_set && !item.continues_previous_set)
                    || (next_item_is_start_of_sub_set && item.continues_previous_set)
                    || (set_total_probability >= 1.0 && !next_item_is_start_of_sub_set)
                {
                    if set_total_probability > 0.0 && set_total_probability < 1.0 {
                        let set_probability_leftover = (1.0f32 - set_total_probability) * 100.0;
                        if set_probability_leftover < 0.01 {
                            treasure += &format!(
                                "{nl}{ind}| {}% chance of nothing from this set",
                                format_aligned(
                                    f64::from(set_probability_leftover).ceil(),
                                    6,
                                    "#.00"
                                )
                            );
                        } else {
                            treasure += &format!(
                                "{nl}{ind}| {}% chance of nothing from this set",
                                format_aligned((1.0f32 - set_total_probability) * 100.0, 6, "#.00")
                            );
                        }
                    }

                    if depth > 0
                        && sub(&sub_set_total_probability, depth)? > 0.0
                        && sub(&sub_set_total_probability, depth)? < 1.0
                    {
                        let set_probability_leftover =
                            (1.0f32 - sub(&sub_set_total_probability, depth)?) * 100.0;
                        if set_probability_leftover < 0.01 {
                            treasure += &format!(
                                "{nl}{ind}|          {} {}% chance of nothing from this subset",
                                spaces(depth),
                                format_aligned(
                                    f64::from(set_probability_leftover).ceil(),
                                    6,
                                    "#.00"
                                )
                            );
                        } else {
                            treasure += &format!(
                                "{nl}{ind}|          {} {}% chance of nothing from this subset",
                                spaces(depth),
                                format_aligned(
                                    (1.0f32 - sub(&sub_set_total_probability, depth)?) * 100.0,
                                    6,
                                    "#.00"
                                )
                            );
                        }
                    }

                    set_number += 1;
                    treasure += &format!("{nl}{ind}# Set: {set_number}");
                    next_item_is_start_of_sub_set = false;
                    next_item_is_part_of_sub_set = false;
                    set_total_probability = 0.0;
                    if depth > 0 {
                        depth -= 1;
                    }
                } else if next_item_is_start_of_sub_set {
                    treasure += &format!("{nl}{ind}|       {} with", spaces(depth));
                    next_item_is_start_of_sub_set = false;
                    next_item_is_part_of_sub_set = true;
                } else if next_item_is_part_of_sub_set && item.continues_previous_set {
                    next_item_is_start_of_sub_set = false;
                    next_item_is_part_of_sub_set = false;
                    if depth > 0 {
                        depth -= 1;
                    }
                }
            }

            if !next_item_is_part_of_sub_set {
                if (set_total_probability + item.probability) > 1.0 {
                    let real_probability =
                        item.probability - (set_total_probability + item.probability - 1.0);

                    if cur(real_probability, "#.00") != cur(item.probability, "#.00") {
                        treasure_item += &format!(
                            " | Chance adjusted down from {}% due to overage for this set",
                            cur(item.probability * 100.0, "#.00")
                        );
                    }

                    treasure += &format!(
                        "{nl}{ind}| {}% chance of {treasure_item}",
                        format_aligned(real_probability * 100.0, 6, "#.00")
                    );
                } else {
                    treasure += &format!(
                        "{nl}{ind}| {}% chance of {treasure_item}",
                        format_aligned(item.probability * 100.0, 6, "#.00")
                    );
                }

                set_total_probability += item.probability;
            } else {
                treasure += &format!(
                    "{nl}{ind}|          {} {}% chance of {treasure_item}",
                    spaces(depth),
                    format_aligned(item.probability * 100.0, 6, "#.00")
                );
                let v = sub(&sub_set_total_probability, depth)? + item.probability;
                sub_set_total_probability.insert(depth, v);
            }

            if item.has_sub_set {
                next_item_is_start_of_sub_set = true;
                next_item_is_part_of_sub_set = false;
                depth += 1;
                sub_set_total_probability.insert(depth, 0.0);
            }
        }

        if depth > 0
            && sub(&sub_set_total_probability, depth)? > 0.0
            && sub(&sub_set_total_probability, depth)? < 1.0
        {
            let set_probability_leftover =
                (1.0f32 - sub(&sub_set_total_probability, depth)?) * 100.0;
            if set_probability_leftover < 0.01 {
                treasure += &format!(
                    "{nl}{ind}|          {} {}% chance of nothing from this subset",
                    spaces(depth),
                    format_aligned(f64::from(set_probability_leftover).ceil(), 6, "#.00")
                );
            } else {
                treasure += &format!(
                    "{nl}{ind}|          {} {}% chance of nothing from this subset",
                    spaces(depth),
                    format_aligned(
                        (1.0f32 - sub(&sub_set_total_probability, depth)?) * 100.0,
                        6,
                        "#.00"
                    )
                );
            }
        }

        if set_total_probability > 0.0 && set_total_probability < 1.0 {
            let set_probability_leftover = (1.0f32 - set_total_probability) * 100.0;
            if set_probability_leftover < 0.01 {
                treasure += &format!(
                    "{nl}{ind}| {}% chance of nothing from this set",
                    format_aligned(f64::from(set_probability_leftover).ceil(), 6, "#.00")
                );
            } else {
                treasure += &format!(
                    "{nl}{ind}| {}% chance of nothing from this set",
                    format_aligned((1.0f32 - set_total_probability) * 100.0, 6, "#.00")
                );
            }
        }

        Ok(treasure)
    }

    /// The label of a generator row whose location is a treasure: the death-treasure tier or the
    /// wielded-treasure sets.
    // ACE: SQLWriter.GetValueForTreasureData
    pub fn get_value_for_treasure_data(
        &self,
        weenie_or_type: u32,
        is_weenie_class_id: bool,
        new_line: &str,
    ) -> Result<String, SqlWriterError> {
        let mut label = "UNKNOWN RANDOMLY GENERATED TREASURE".to_owned();

        let mut death_treasure_type: Option<u32> = None;
        let mut wielded_treasure_type: Option<u32> = None;

        if is_weenie_class_id {
            if let Some(weenie) = self.weenies.as_ref().and_then(|w| w.get(&weenie_or_type)) {
                death_treasure_type = weenie.get_property_did(PropertyDataId::DeathTreasureType);
                wielded_treasure_type =
                    weenie.get_property_did(PropertyDataId::WieldedTreasureType);
            }
        } else {
            death_treasure_type = Some(weenie_or_type);
            wielded_treasure_type = Some(weenie_or_type);
        }

        if let (Some(death), Some(wielded)) = (death_treasure_type, wielded_treasure_type) {
            if let Some(td) = self.treasure_death.as_ref().and_then(|t| t.get(&death)) {
                label = format!(
                    "RANDOMLY GENERATED TREASURE from Loot Tier {} from Death Treasure Table id: {death}",
                    td.tier
                );
            } else if let Some(list) = self.treasure_wielded.as_ref().and_then(|t| t.get(&wielded))
            {
                label = format!(
                    "something from one or more sets from Wielded Treasure Table id: {wielded}"
                );
                label += &self.get_values_for_treasure_did(list, new_line)?;
            }
        } else {
            label = "nothing".to_owned();
        }

        Ok(label)
    }

    /// Round-trips through `0.######` text so `-0` becomes `0` (and the value is rounded to six
    /// decimals).
    // ACE: SQLWriter.TrimNegativeZero
    #[must_use]
    pub fn trim_negative_zero(input: Option<f32>) -> Option<f32> {
        Self::trim_negative_zero_places(input, 6)
    }

    /// `TrimNegativeZero(input, places)`.
    // ACE: SQLWriter.TrimNegativeZero
    #[must_use]
    pub fn trim_negative_zero_places(input: Option<f32>, places: usize) -> Option<f32> {
        let v = input?;
        let hashes = "#".repeat(places);

        let mut str = dn(v, &format!("0.{hashes}"));

        if str == format!("-0.{hashes}") || str == "-0" {
            str = str[1..].to_owned();
        }

        Some(float_parse(&str))
    }
}
