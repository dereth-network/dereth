// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/GDLE/Models/BodyPartListing.cs, Source/ACE.Adapter/GDLE/Models/BoolStat.cs, Source/ACE.Adapter/GDLE/Models/CreateItem.cs, Source/ACE.Adapter/GDLE/Models/DidStat.cs, Source/ACE.Adapter/GDLE/Models/Emote.cs, Source/ACE.Adapter/GDLE/Models/EmoteAction.cs, Source/ACE.Adapter/GDLE/Models/FloatStat.cs, Source/ACE.Adapter/GDLE/Models/GeneratorTable.cs, Source/ACE.Adapter/GDLE/Models/IidStat.cs, Source/ACE.Adapter/GDLE/Models/Int64Stat.cs, Source/ACE.Adapter/GDLE/Models/IntStat.cs, Source/ACE.Adapter/GDLE/Models/LSDWeenie.cs, Source/ACE.Adapter/GDLE/Models/PositionListing.cs, Source/ACE.Adapter/GDLE/Models/Quaternion.cs, Source/ACE.Adapter/GDLE/Models/Skill.cs, Source/ACE.Adapter/GDLE/Models/SkillListing.cs, Source/ACE.Adapter/GDLE/Models/SpellbookEntry.cs, Source/ACE.Adapter/GDLE/Models/StringStat.cs, Source/ACE.Adapter/GDLE/Models/Vital.cs, Source/ACE.Adapter/GDLE/Models/XYZ.cs
//! The `JsonIgnore`d members of the GDLE models: the editor "binders" that view a stored number
//! as a Lifestoned.DataModel enum, the display strings, `LSDWeenie`'s shortcuts, and the two
//! `IsPropertyVisible` attribute tests. ACE's server never reads them; they are the model API
//! the Lifestoned web editor shares with ACE.Adapter.
//!
//! A cast to a Lifestoned enum is a reinterpretation of the number, so a binder here returns the
//! number in the enum's underlying type (`uint` for `ItemType`, `WeenieType`, `MotionCommand` and
//! `Material`, `int` for the rest). Names come from [`super::lifestoned_enums`].
//! A C# exception is an `Err` naming its .NET type.

use empyrean_common::dotnet::{format as dn, CsCast};

use super::lifestoned_enums as names;
use crate::import::json::models::{
    BodyPartListing, BoolStat, CreateItem, DidStat, Emote, EmoteAction, FloatStat, GeneratorTable,
    IidStat, Int64Stat, IntStat, LsdWeenie, PositionListing, Quaternion, Skill, SkillListing,
    SpellbookEntry, StringStat, Vital, Xyz,
};
use crate::import::json::value::R;

const INDEX_OUT_OF_RANGE: &str = "System.IndexOutOfRangeException";
const NULL_REFERENCE: &str = "System.NullReferenceException";
const INVALID_OPERATION: &str = "System.InvalidOperationException";
const ARGUMENT_NULL: &str = "System.ArgumentNullException";
const FORMAT: &str = "System.FormatException";
const OVERFLOW: &str = "System.OverflowException";

/// `EnumExtensions.GetName(value)`: the name the table gives, else `IndexOutOfRangeException`.
fn get_name(table: &'static [(i32, &'static str)], value: i32) -> R<&'static str> {
    table
        .binary_search_by_key(&value, |&(v, _)| v)
        .map(|i| table[i].1)
        .map_err(|_| INDEX_OUT_OF_RANGE.to_owned())
}

// ---- stats -----------------------------------------------------------------------------------

impl BodyPartListing {
    /// `(BodyPartType)Key`.
    // ACE: BodyPartListing.BodyPartType
    #[must_use]
    pub fn body_part_type(&self) -> i32 {
        self.key
    }

    /// `Key = (int)value`.
    pub fn set_body_part_type(&mut self, value: i32) {
        self.key = value;
    }
}

impl BoolStat {
    // ACE: BoolStat.BoolValue
    #[must_use]
    pub fn bool_value(&self) -> bool {
        self.value != 0
    }

    pub fn set_bool_value(&mut self, value: bool) {
        self.value = i32::from(value);
    }

    // ACE: BoolStat.PropertyIdBinder
    pub fn property_id_binder(&self) -> R<&'static str> {
        get_name(names::BOOL_PROPERTY_ID, self.key)
    }
}

impl DidStat {
    // ACE: DidStat.PropertyIdBinder
    pub fn property_id_binder(&self) -> R<&'static str> {
        get_name(names::DID_PROPERTY_ID, self.key)
    }
}

impl FloatStat {
    // ACE: FloatStat.PropertyIdBinder
    pub fn property_id_binder(&self) -> R<&'static str> {
        get_name(names::DOUBLE_PROPERTY_ID, self.key)
    }
}

impl IidStat {
    // ACE: IidStat.PropertyIdBinder
    pub fn property_id_binder(&self) -> R<&'static str> {
        get_name(names::IID_PROPERTY_ID, self.key)
    }
}

impl Int64Stat {
    // ACE: Int64Stat.PropertyIdBinder
    pub fn property_id_binder(&self) -> R<&'static str> {
        get_name(names::INT64_PROPERTY_ID, self.key)
    }
}

impl StringStat {
    // ACE: StringStat.PropertyIdBinder
    pub fn property_id_binder(&self) -> R<&'static str> {
        get_name(names::STRING_PROPERTY_ID, self.key)
    }
}

/// `value.Value` of a nullable enum in a setter: `InvalidOperationException` when null.
fn has_value<T>(value: Option<T>) -> R<T> {
    value.ok_or_else(|| INVALID_OPERATION.to_owned())
}

impl IntStat {
    /// The set bits of `Value`, each as its `int` text.
    // ACE: IntStat.MultiSelect
    #[must_use]
    pub fn multi_select(&self) -> Vec<String> {
        let value = self.value;
        let mut list: Vec<i32> = Vec::new();
        for j in 0..=31 {
            // (uint)Math.Pow(2.0, j)
            let num: u32 = empyrean_common::math::pow(2.0, f64::from(j)).cs_cast();
            // uint & int: both widen to long.
            if (i64::from(num) & i64::from(value)) != 0 {
                list.push(num.cs_cast());
            }
        }

        list.iter().map(ToString::to_string).collect()
    }

    /// `MultiSelectRaw = value; Value = 0;` then `Value += int.Parse(s)` for each string.
    /// `int.Parse` throws on text that is not an `int` (leaving `Value` part-summed).
    pub fn set_multi_select(&mut self, value: Option<Vec<String>>) -> R<()> {
        self.multi_select_raw.clone_from(&value);
        self.value = 0;
        if let Some(value) = value.filter(|v| !v.is_empty()) {
            for s in &value {
                self.value = self.value.wrapping_add(int_parse(s)?);
            }
        }
        Ok(())
    }

    /// `PropertyIntExtensions.GetName`: an undefined id reads `"<id> - Unknown"`.
    // ACE: IntStat.PropertyIdBinder
    #[must_use]
    pub fn property_id_binder(&self) -> String {
        get_name(names::INT_PROPERTY_ID, self.key)
            .map_or_else(|_| format!("{} - Unknown", self.key), str::to_owned)
    }

    // ACE: IntStat.ItemTypeBoundValue
    #[must_use]
    pub fn item_type_bound_value(&self) -> Option<u32> {
        Some(self.value.cs_cast())
    }

    pub fn set_item_type_bound_value(&mut self, value: Option<u32>) -> R<()> {
        self.value = has_value(value)?.cs_cast();
        Ok(())
    }

    // ACE: IntStat.WeenieTypeBoundValue
    #[must_use]
    pub fn weenie_type_bound_value(&self) -> Option<u32> {
        Some(self.value.cs_cast())
    }

    pub fn set_weenie_type_bound_value(&mut self, value: Option<u32>) -> R<()> {
        self.value = has_value(value)?.cs_cast();
        Ok(())
    }

    // ACE: IntStat.CreatureTypeBoundValue
    #[must_use]
    pub fn creature_type_bound_value(&self) -> Option<i32> {
        Some(self.value)
    }

    pub fn set_creature_type_bound_value(&mut self, value: Option<i32>) -> R<()> {
        self.value = has_value(value)?;
        Ok(())
    }

    // ACE: IntStat.ArmorTypeBoundValue
    #[must_use]
    pub fn armor_type_bound_value(&self) -> Option<i32> {
        Some(self.value)
    }

    pub fn set_armor_type_bound_value(&mut self, value: Option<i32>) -> R<()> {
        self.value = has_value(value)?;
        Ok(())
    }

    // ACE: IntStat.WieldRequirementsBoundValue
    #[must_use]
    pub fn wield_requirements_bound_value(&self) -> Option<i32> {
        Some(self.value)
    }

    pub fn set_wield_requirements_bound_value(&mut self, value: Option<i32>) -> R<()> {
        self.value = has_value(value)?;
        Ok(())
    }

    // ACE: IntStat.PaletteTemplateBoundValue
    #[must_use]
    pub fn palette_template_bound_value(&self) -> Option<i32> {
        Some(self.value)
    }

    pub fn set_palette_template_bound_value(&mut self, value: Option<i32>) -> R<()> {
        self.value = has_value(value)?;
        Ok(())
    }

    // ACE: IntStat.Material_Binder
    #[must_use]
    pub fn material_binder(&self) -> Option<u32> {
        Some(self.value.cs_cast())
    }

    pub fn set_material_binder(&mut self, value: Option<u32>) -> R<()> {
        self.value = has_value(value)?.cs_cast();
        Ok(())
    }

    // ACE: IntStat.HeritageBinder
    #[must_use]
    pub fn heritage_binder(&self) -> Option<i32> {
        Some(self.value)
    }

    pub fn set_heritage_binder(&mut self, value: Option<i32>) -> R<()> {
        self.value = has_value(value)?;
        Ok(())
    }

    // ACE: IntStat.WeaponTypeBoundValue
    #[must_use]
    pub fn weapon_type_bound_value(&self) -> Option<i32> {
        Some(self.value)
    }

    pub fn set_weapon_type_bound_value(&mut self, value: Option<i32>) -> R<()> {
        self.value = has_value(value)?;
        Ok(())
    }

    // ACE: IntStat.SkillIdBoundValue
    #[must_use]
    pub fn skill_id_bound_value(&self) -> Option<i32> {
        Some(self.value)
    }

    pub fn set_skill_id_bound_value(&mut self, value: Option<i32>) -> R<()> {
        self.value = has_value(value)?;
        Ok(())
    }
}

/// `int.Parse(s)` in en-US (`NumberStyles.Integer`): optional surrounding white space, an
/// optional sign, decimal digits.
fn int_parse(s: &str) -> R<i32> {
    let t = s.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\u{b}' | '\u{c}' | '\r'));
    let (negative, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return Err(FORMAT.to_owned());
    }
    let mut v: i64 = 0;
    for b in digits.bytes() {
        v = v * 10 + i64::from(b - b'0');
        if v > i64::from(i32::MAX) + 1 {
            return Err(OVERFLOW.to_owned());
        }
    }
    let v = if negative { -v } else { v };
    i32::try_from(v).map_err(|_| OVERFLOW.to_owned())
}

impl CreateItem {
    /// `(Destination?)Destination`.
    // ACE: CreateItem.Destination_Binder
    #[must_use]
    pub fn destination_binder(&self) -> Option<i32> {
        self.destination.map(CsCast::cs_cast)
    }

    pub fn set_destination_binder(&mut self, value: Option<i32>) {
        self.destination = value.map(CsCast::cs_cast);
    }

    /// `TryToBond != 0`: a lifted comparison, so an absent `try_to_bond` reads as bonded.
    // ACE: CreateItem.TryToBond_BooleanBinder
    #[must_use]
    pub fn try_to_bond_boolean_binder(&self) -> bool {
        self.try_to_bond != Some(0)
    }

    pub fn set_try_to_bond_boolean_binder(&mut self, value: bool) {
        self.try_to_bond = Some(u8::from(value));
    }
}

impl GeneratorTable {
    /// `(RegenerationType)WhenCreate`.
    // ACE: GeneratorTable.WhenCreateEnum
    #[must_use]
    pub fn when_create_enum(&self) -> i32 {
        self.when_create.cs_cast()
    }

    pub fn set_when_create_enum(&mut self, value: i32) {
        self.when_create = value.cs_cast();
    }

    /// `(RegenerationLocation)WhereCreate`.
    // ACE: GeneratorTable.WhereCreateEnum
    #[must_use]
    pub fn where_create_enum(&self) -> i32 {
        self.where_create.cs_cast()
    }

    pub fn set_where_create_enum(&mut self, value: i32) {
        self.where_create = value.cs_cast();
    }
}

impl PositionListing {
    // ACE: PositionListing.PositionTypeName
    pub fn position_type_name(&self) -> R<&'static str> {
        get_name(names::POSITION_TYPE, self.position_type)
    }
}

impl Skill {
    /// `(SkillStatus?)TrainedLevel`.
    // ACE: Skill.Status_Binder
    #[must_use]
    pub fn status_binder(&self) -> Option<i32> {
        self.trained_level
    }

    pub fn set_status_binder(&mut self, value: Option<i32>) {
        self.trained_level = value;
    }
}

impl SkillListing {
    /// `SkillIdExtensions.GetName(this SkillId?)`: `""` for a null id.
    // ACE: SkillListing.SkillName
    pub fn skill_name(&self) -> R<&'static str> {
        match self.skill_id {
            None => Ok(""),
            Some(id) => get_name(names::SKILL_ID, id),
        }
    }
}

impl SpellbookEntry {
    /// `"(" + SpellId + ") "` and the `SpellId` enum name with a space before every capital
    /// letter and digit (`Regex.Replace(input, "([A-Z0-9])", " $1")`), trimmed.
    // ACE: SpellbookEntry.GetSpellDescription
    #[must_use]
    pub fn get_spell_description(&self) -> String {
        let input = match names::SPELL_ID.binary_search_by_key(&self.spell_id, |&(v, _)| v) {
            Ok(i) => names::SPELL_ID[i].1.to_owned(),
            Err(_) => self.spell_id.to_string(),
        };
        let mut spaced = String::with_capacity(input.len() * 2);
        for c in input.chars() {
            if c.is_ascii_uppercase() || c.is_ascii_digit() {
                spaced.push(' ');
            }
            spaced.push(c);
        }
        format!("({}) {}", self.spell_id, spaced.trim())
    }
}

/// The Lifestoned editor's ability value `Vital.Convert` reads (Lifestoned.DataModel's
/// `DerethForever.Ability`; only the three members it reads).
#[derive(Debug, Clone, Copy, Default)]
pub struct Ability {
    pub ranks: Option<u32>,
    pub base: Option<u32>,
    pub experience_spent: Option<u32>,
}

impl Vital {
    /// `Current = ability.Base + ability.Ranks` is lifted: null when either is null.
    // ACE: Vital.Convert
    #[must_use]
    pub fn convert(ability: &Ability) -> Vital {
        Vital {
            ranks: ability.ranks,
            current: ability
                .base
                .zip(ability.ranks)
                .map(|(b, r)| b.wrapping_add(r)),
            xp_spent: ability.experience_spent,
            level_from_cp: Some(0),
        }
    }
}

// ---- display strings -------------------------------------------------------------------------

/// `float.TryParse(s, out result)` in en-US (`NumberStyles.Float | AllowThousands`): optional
/// white space and sign, digits with group separators in the integral part, an optional fraction
/// and exponent; or the culture's `∞`/`-∞`/`NaN` symbols (not `Infinity`). 0 on failure.
fn float_try_parse(s: &str) -> f32 {
    let t = s.trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\u{b}' | '\u{c}' | '\r'));
    match t {
        "∞" | "+∞" => return f32::INFINITY,
        "-∞" => return f32::NEG_INFINITY,
        "NaN" => return f32::NAN,
        _ => {}
    }
    // Group separators are allowed in the integral part only.
    let integral_end = t.find(['.', 'e', 'E']).unwrap_or(t.len());
    if t[integral_end..].contains(',') {
        return 0.0;
    }
    let t: String = t.chars().filter(|&c| c != ',').collect();
    let unsigned = t.strip_prefix(['+', '-']).unwrap_or(&t);
    if !unsigned.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return 0.0;
    }
    t.parse().unwrap_or(0.0)
}

/// `value.Substring(start, length)`: `ArgumentOutOfRangeException` out of range.
fn substring(value: &[char], start: usize, end: Option<usize>) -> R<String> {
    let end = end.unwrap_or(value.len());
    if start > end || end > value.len() {
        return Err("System.ArgumentOutOfRangeException".to_owned());
    }
    Ok(value[start..end].iter().collect())
}

/// `value.IndexOf(' ', start)`.
fn index_of_space(value: &[char], start: usize) -> Option<usize> {
    value
        .iter()
        .skip(start)
        .position(|&c| c == ' ')
        .map(|i| i + start)
}

impl Xyz {
    /// `$"{X:0.000000} {Y:0.000000} {Z:0.000000}"`.
    // ACE: XYZ.Display
    #[must_use]
    pub fn display(&self) -> String {
        let f = "0.000000";
        format!("{} {} {}", dn(self.x, f), dn(self.y, f), dn(self.z, f))
    }

    /// The setter: three space-separated floats; no space at all zeroes the vector. A text with
    /// one space throws (`Substring` with a negative length).
    pub fn set_display(&mut self, value: Option<&str>) -> R<()> {
        let chars: Vec<char> = value.map(|v| v.chars().collect()).unwrap_or_default();
        let mut num = 0usize;
        let Some(mut num2) = value.and_then(|_| index_of_space(&chars, 0)) else {
            self.x = 0.0;
            self.y = 0.0;
            self.z = 0.0;
            return Ok(());
        };

        let result = float_try_parse(&substring(&chars, num, Some(num2))?);
        num = num2 + 1;
        let next = index_of_space(&chars, num);
        let result2 = match next {
            Some(n) => {
                num2 = n;
                float_try_parse(&substring(&chars, num, Some(num2))?)
            }
            // IndexOf returns -1: Substring(num, -1 - num) throws.
            None => return Err("System.ArgumentOutOfRangeException".to_owned()),
        };
        num = num2 + 1;
        let result3 = float_try_parse(&substring(&chars, num, None)?);
        self.x = result;
        self.y = result2;
        self.z = result3;
        Ok(())
    }
}

impl Quaternion {
    /// `$"{W:0.000000} {X:0.000000} {Y:0.000000} {Z:0.000000}"`.
    // ACE: Quaternion.Display
    #[must_use]
    pub fn display(&self) -> String {
        let f = "0.000000";
        format!(
            "{} {} {} {}",
            dn(self.w, f),
            dn(self.x, f),
            dn(self.y, f),
            dn(self.z, f)
        )
    }

    /// The setter: four space-separated floats; no space at all resets to the identity. Fewer
    /// than three spaces throw (`Substring` with a negative length).
    pub fn set_display(&mut self, value: Option<&str>) -> R<()> {
        let chars: Vec<char> = value.map(|v| v.chars().collect()).unwrap_or_default();
        let mut start_index = 0usize;
        let Some(mut num) = value.and_then(|_| index_of_space(&chars, 0)) else {
            self.w = 1.0;
            self.x = 0.0;
            self.y = 0.0;
            self.z = 0.0;
            return Ok(());
        };

        let result = float_try_parse(&substring(&chars, start_index, Some(num))?);
        let mut middle = [0.0f32; 2];
        for m in &mut middle {
            start_index = num + 1;
            num = index_of_space(&chars, start_index)
                .ok_or_else(|| "System.ArgumentOutOfRangeException".to_owned())?;
            *m = float_try_parse(&substring(&chars, start_index, Some(num))?);
        }
        start_index = num + 1;
        let result4 = float_try_parse(&substring(&chars, start_index, None)?);
        self.w = result;
        self.x = middle[0];
        self.y = middle[1];
        self.z = result4;
        Ok(())
    }
}

// ---- emotes --------------------------------------------------------------------------------

/// `Emote`'s `[EmoteCategory(...)]` attributes: property name, then the categories it shows for.
const EMOTE_PROPERTY_CATEGORIES: &[(&str, &[i32])] = &[
    ("Category", &[]),
    ("EmoteCategory", &[]),
    ("NewEmoteType", &[]),
    ("Actions", &[]),
    ("Probability", &[]),
    ("VendorType", &[2]),                     // Vendor
    ("Quest", &[13, 12, 38, 22, 23, 32, 30]), // QuestFailure, QuestSuccess, ReceiveTalkDirect, TestSuccess, TestFailure, GotoSet, QuestNoFellow
    ("ClassId", &[1, 6]),                     // Refuse, Give
    ("Style", &[5]),                          // HeartBeat
    ("SubStyle", &[5]),                       // HeartBeat
    ("MinHealth", &[15]),                     // WoundedTaunt
    ("MaxHealth", &[15]),                     // WoundedTaunt
    ("SortOrder", &[]),
    ("Deleted", &[]),
];

impl Emote {
    /// `(EmoteCategory)Category`.
    // ACE: Emote.EmoteCategory
    #[must_use]
    pub fn emote_category(&self) -> i32 {
        self.category.cs_cast()
    }

    pub fn set_emote_category(&mut self, value: i32) {
        self.category = value.cs_cast();
    }

    /// Whether the editor shows `property_name` for this emote: a property without
    /// `EmoteCategory` attributes always, else when one of them names the emote's category.
    /// A name `Emote` has no property of throws (`GetProperty` returns null).
    // ACE: Emote.IsPropertyVisible
    pub fn is_property_visible(property_name: &str, emote: &Emote) -> R<bool> {
        let (_, list) = EMOTE_PROPERTY_CATEGORIES
            .iter()
            .find(|(name, _)| *name == property_name)
            .ok_or_else(|| NULL_REFERENCE.to_owned())?;
        if list.is_empty() {
            return Ok(true);
        }

        Ok(list.contains(&emote.emote_category()))
    }
}

/// `EmoteAction`'s `[EmoteType(...)]` attributes: property name, then the action types it shows
/// for (Lifestoned `EmoteType` values, in the attributes' order).
const EMOTE_ACTION_PROPERTY_TYPES: &[(&str, &[i32])] = &[
    ("EmoteActionType", &[]),
    ("EmoteActionType_Binder", &[]),
    ("Delay", &[]),
    ("Extent", &[]),
    // DecrementQuest .. SetAltRacialSkills
    (
        "Amount",
        &[
            32, 33, 70, 72, 84, 85, 86, 89, 102, 103, 104, 105, 106, 107, 108, 109, 53, 54, 55, 69,
            34, 47, 48, 90, 119, 120, 28, 29, 111,
        ],
    ),
    ("Motion", &[5, 52]), // Motion, ForceMotion
    ("Motion_Binder", &[]),
    // Act .. InqFloatStat
    (
        "Message",
        &[
            1, 8, 10, 13, 16, 17, 18, 20, 21, 22, 23, 24, 25, 26, 31, 32, 33, 70, 51, 58, 60, 61,
            64, 65, 67, 68, 71, 79, 80, 81, 83, 84, 85, 86, 88, 89, 102, 103, 104, 105, 106, 107,
            108, 109, 121, 30, 59, 71, 82, 76, 35, 45, 46, 38, 75, 36, 39, 40, 41, 42, 43, 44, 114,
            37,
        ],
    ),
    ("Amount64", &[2, 62, 112, 113]), // AwardXP, AwardNoShareXP, SpendLuminance, AwardLuminance
    ("HeroXp64", &[2, 62, 112]),      // AwardXP, AwardNoShareXP, SpendLuminance
    ("Item", &[3, 74, 76]),           // Give, TakeItems, InqOwnsItems
    ("Minimum64", &[114, 49]),        // InqInt64Stat, AwardLevelProportionalXP
    ("Maximum64", &[114, 49]),        // InqInt64Stat, AwardLevelProportionalXP
    ("Percent", &[118, 49, 50]), // SetFloatStat, AwardLevelProportionalXP, AwardLevelProportionalSkillXP
    ("Display_Binder", &[49, 50]), // AwardLevelProportionalXP, AwardLevelProportionalSkillXP
    ("Display", &[]),
    // InqQuestSolves .. AwardLevelProportionalSkillXP
    ("Max", &[30, 59, 71, 82, 36, 39, 40, 41, 42, 43, 44, 50]),
    ("Min", &[30, 59, 71, 82, 36, 39, 40, 41, 42, 43, 44, 50]),
    ("FMax", &[37]), // InqFloatStat
    ("FMin", &[37]), // InqFloatStat
    // SetIntStat .. AwardLevelProportionalSkillXP
    (
        "Stat",
        &[
            53, 54, 55, 69, 115, 118, 28, 29, 110, 35, 45, 46, 38, 75, 36, 39, 40, 41, 42, 43, 44,
            114, 37, 50,
        ],
    ),
    ("PScript", &[7]), // PhysScript
    ("PScript_Binder", &[]),
    ("Sound", &[9]),                // Sound
    ("MPosition", &[63, 99, 100]),  // SetSanctuaryPosition, TeleportTarget, TeleportSelf
    ("Frame", &[4, 6, 11, 87]),     // MoveHome, Move, Turn, MoveToPos
    ("SpellId", &[14, 19, 27, 73]), // CastSpell, CastSpellInstant, TeachSpell, PetCastSpellOnOwner
    ("TestString", &[38, 75]),      // InqStringStat, InqYesNo
    ("WealthRating", &[56]),        // CreateTreasure
    ("WealthRating_Binder", &[]),
    ("TreasureClass", &[56]), // CreateTreasure
    ("TreasureClass_Binder", &[]),
    ("TreasureType", &[56]), // CreateTreasure
    ("SortOrder", &[]),
    ("Deleted", &[]),
];

impl EmoteAction {
    /// `(EmoteType)EmoteActionType`.
    // ACE: EmoteAction.EmoteActionType_Binder
    #[must_use]
    pub fn emote_action_type_binder(&self) -> i32 {
        self.emote_action_type.cs_cast()
    }

    pub fn set_emote_action_type_binder(&mut self, value: i32) {
        self.emote_action_type = value.cs_cast();
    }

    /// `(MotionCommand?)Motion`.
    // ACE: EmoteAction.Motion_Binder
    #[must_use]
    pub fn motion_binder(&self) -> Option<u32> {
        self.motion
    }

    pub fn set_motion_binder(&mut self, value: Option<u32>) {
        self.motion = value;
    }

    /// `(PhysicsScriptType?)PScript`.
    // ACE: EmoteAction.PScript_Binder
    #[must_use]
    pub fn p_script_binder(&self) -> Option<i32> {
        self.p_script.map(CsCast::cs_cast)
    }

    pub fn set_p_script_binder(&mut self, value: Option<i32>) {
        self.p_script = value.map(CsCast::cs_cast);
    }

    /// `(WealthRating?)WealthRating`.
    // ACE: EmoteAction.WealthRating_Binder
    #[must_use]
    pub fn wealth_rating_binder(&self) -> Option<i32> {
        self.wealth_rating.map(CsCast::cs_cast)
    }

    pub fn set_wealth_rating_binder(&mut self, value: Option<i32>) {
        self.wealth_rating = value.map(CsCast::cs_cast);
    }

    /// `(TreasureClass?)TreasureClass`.
    // ACE: EmoteAction.TreasureClass_Binder
    #[must_use]
    pub fn treasure_class_binder(&self) -> Option<i32> {
        self.treasure_class.map(CsCast::cs_cast)
    }

    pub fn set_treasure_class_binder(&mut self, value: Option<i32>) {
        self.treasure_class = value.map(CsCast::cs_cast);
    }

    /// Whether the editor shows `property_name` for this action: a property without `EmoteType`
    /// attributes always, else when one of them names the action's type. A name `EmoteAction`
    /// has no property of throws (`GetProperty` returns null).
    // ACE: EmoteAction.IsPropertyVisible
    pub fn is_property_visible(property_name: &str, emote: &EmoteAction) -> R<bool> {
        let (_, list) = EMOTE_ACTION_PROPERTY_TYPES
            .iter()
            .find(|(name, _)| *name == property_name)
            .ok_or_else(|| NULL_REFERENCE.to_owned())?;
        if list.is_empty() {
            return Ok(true);
        }

        Ok(list.contains(&emote.emote_action_type_binder()))
    }
}

// ---- LSDWeenie -----------------------------------------------------------------------------

impl LsdWeenie {
    // ACE: LSDWeenie.WeenieClassId
    #[must_use]
    pub fn weenie_class_id(&self) -> u32 {
        self.weenie_id
    }

    pub fn set_weenie_class_id(&mut self, value: u32) {
        self.weenie_id = value;
    }

    /// `(WeenieType)WeenieTypeId`.
    // ACE: LSDWeenie.WeenieType_Binder
    #[must_use]
    pub fn weenie_type_binder(&self) -> u32 {
        self.weenie_type_id.cs_cast()
    }

    pub fn set_weenie_type_binder(&mut self, value: u32) {
        self.weenie_type_id = value.cs_cast();
    }

    /// `IntStats.FirstOrDefault(d => d.Key == key)?.Value`: a null list throws
    /// `ArgumentNullException`, a null element `NullReferenceException`.
    fn int_stat(&self, key: i32) -> R<Option<i32>> {
        let stats = self
            .int_stats
            .as_ref()
            .ok_or_else(|| ARGUMENT_NULL.to_owned())?;
        for d in stats {
            let d = d.as_ref().ok_or_else(|| NULL_REFERENCE.to_owned())?;
            if d.key == key {
                return Ok(Some(d.value));
            }
        }
        Ok(None)
    }

    fn did_stat(&self, key: i32) -> R<Option<u32>> {
        let stats = self
            .did_stats
            .as_ref()
            .ok_or_else(|| ARGUMENT_NULL.to_owned())?;
        for d in stats {
            let d = d.as_ref().ok_or_else(|| NULL_REFERENCE.to_owned())?;
            if d.key == key {
                return Ok(Some(d.value));
            }
        }
        Ok(None)
    }

    // ACE: LSDWeenie.ItemType
    pub fn item_type(&self) -> R<Option<i32>> {
        Ok(Some(self.int_stat(1)?.unwrap_or(0)))
    }

    /// `(ItemType & 0x10u) > 0`: the `int?` widens to `long?` against the `uint`.
    // ACE: LSDWeenie.HasAbilities
    pub fn has_abilities(&self) -> R<bool> {
        Ok(self.item_type()?.is_some_and(|t| (i64::from(t) & 0x10) > 0))
    }

    // ACE: LSDWeenie.HasGeneratorTable
    #[must_use]
    pub fn has_generator_table(&self) -> bool {
        self.generator_table.as_ref().is_some_and(|g| !g.is_empty())
    }

    /// `(Body?.BodyParts.Count() ?? 0) > 0`: `Count()` on a null list throws.
    // ACE: LSDWeenie.HasBodyPartList
    pub fn has_body_part_list(&self) -> R<bool> {
        let count = match &self.body {
            None => 0,
            Some(body) => body
                .body_parts
                .as_ref()
                .ok_or_else(|| ARGUMENT_NULL.to_owned())?
                .len(),
        };
        Ok(count > 0)
    }

    // ACE: LSDWeenie.UIEffects
    pub fn ui_effects(&self) -> R<Option<i32>> {
        Ok(Some(self.int_stat(18)?.unwrap_or(0)))
    }

    // ACE: LSDWeenie.IconId
    pub fn icon_id(&self) -> R<Option<u32>> {
        self.did_stat(8)
    }

    // ACE: LSDWeenie.UnderlayId
    pub fn underlay_id(&self) -> R<Option<u32>> {
        self.did_stat(52)
    }

    // ACE: LSDWeenie.OverlayId
    pub fn overlay_id(&self) -> R<Option<u32>> {
        self.did_stat(50)
    }

    // ACE: LSDWeenie.OverlaySecondaryId
    pub fn overlay_secondary_id(&self) -> R<Option<u32>> {
        self.did_stat(51)
    }
}
