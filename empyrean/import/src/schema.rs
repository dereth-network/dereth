//! `empyrean-import schema`: the names of the world's content, for an editor's forms.
//!
//! ```text
//! empyrean-import schema [--out <schema.json>]
//! ```
//!
//! It writes `--out` (default `schema.json`): an object with two members.
//! - `properties`: for each property type (`int`, `int64`, `bool`, `float`, `string`, `did`,
//!   `iid`, `position`), each property id the server knows and its name.
//! - `enums`: for each enum a weenie's form shows (the weenie, item and creature types, the
//!   attributes and skills, the palettes, the emote categories and actions, the motions and
//!   stances an emote plays, the vendors' kinds, the create-list destinations, the generators'
//!   when and where and what becomes of what they made, the combat styles, attacks, weapon types
//!   and equipment sets, how a creature picks and tolerates its targets, genders, heritages and
//!   player killer statuses, the radar's colors and when it shows a thing, and the house hooks a
//!   thing hangs on), each value and its name.
//!
//! The names are the server's own, so a form names what the server reads. An enum's aliases are
//! written once, by the first name the server gives the value.

use std::collections::BTreeMap;
use std::path::PathBuf;

use empyrean_entity::enums::{
    AceEnum, AttackType, CombatStyle, CreatureType, DestinationType, EmoteCategory, EmoteType,
    EquipmentSet, Gender, GeneratorDestruct, HeritageGroup, HookType, ItemType, MotionCommand,
    MotionStance, PaletteTemplate, PlayerKillerStatus, PositionType, PropertyAttribute,
    PropertyAttribute2nd, PropertyBool, PropertyDataId, PropertyFloat, PropertyInstanceId,
    PropertyInt, PropertyInt64, PropertyString, RadarBehavior, RadarColor, RegenLocationType,
    RegenerationType, Skill, TargetingTactic, Tolerance, VendorType, WeaponType, WeenieType,
};

/// `schema`'s options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub out: PathBuf,
}

/// Reads `schema`'s options (the arguments after `schema`).
pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut out = PathBuf::from("schema.json");
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => {
                out = it.next().map(PathBuf::from).ok_or("--out takes a file")?;
            }
            other => return Err(format!("schema: {other} is not an option")),
        }
    }
    Ok(Options { out })
}

/// Each value of `E` and its name, the first name of a value with aliases.
fn names<E: AceEnum>() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (m, n) in E::MEMBERS.iter().zip(E::MEMBER_NAMES) {
        out.entry(m.key().to_string())
            .or_insert_with(|| (*n).to_owned());
    }
    out
}

/// The schema as a JSON value.
#[must_use]
pub fn schema() -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    for (key, table) in [
        ("int", names::<PropertyInt>()),
        ("int64", names::<PropertyInt64>()),
        ("bool", names::<PropertyBool>()),
        ("float", names::<PropertyFloat>()),
        ("string", names::<PropertyString>()),
        ("did", names::<PropertyDataId>()),
        ("iid", names::<PropertyInstanceId>()),
        ("position", names::<PositionType>()),
    ] {
        properties.insert(
            key.to_owned(),
            serde_json::to_value(table).unwrap_or_default(),
        );
    }
    let mut enums = serde_json::Map::new();
    for (key, table) in [
        ("WeenieType", names::<WeenieType>()),
        ("ItemType", names::<ItemType>()),
        ("CreatureType", names::<CreatureType>()),
        ("Attribute", names::<PropertyAttribute>()),
        ("Vital", names::<PropertyAttribute2nd>()),
        ("Skill", names::<Skill>()),
        ("PaletteTemplate", names::<PaletteTemplate>()),
        ("EmoteCategory", names::<EmoteCategory>()),
        ("EmoteType", names::<EmoteType>()),
        ("DestinationType", names::<DestinationType>()),
        ("RegenerationType", names::<RegenerationType>()),
        ("RegenLocationType", names::<RegenLocationType>()),
        ("MotionCommand", names::<MotionCommand>()),
        ("MotionStance", names::<MotionStance>()),
        ("VendorType", names::<VendorType>()),
        ("GeneratorDestruct", names::<GeneratorDestruct>()),
        ("CombatStyle", names::<CombatStyle>()),
        ("AttackType", names::<AttackType>()),
        ("WeaponType", names::<WeaponType>()),
        ("EquipmentSet", names::<EquipmentSet>()),
        ("TargetingTactic", names::<TargetingTactic>()),
        ("Tolerance", names::<Tolerance>()),
        ("Gender", names::<Gender>()),
        ("HeritageGroup", names::<HeritageGroup>()),
        ("PlayerKillerStatus", names::<PlayerKillerStatus>()),
        ("RadarBehavior", names::<RadarBehavior>()),
        ("RadarColor", names::<RadarColor>()),
        ("HookType", names::<HookType>()),
    ] {
        enums.insert(
            key.to_owned(),
            serde_json::to_value(table).unwrap_or_default(),
        );
    }
    serde_json::json!({ "properties": properties, "enums": enums })
}

/// Writes the schema to `opts.out`.
pub fn run(opts: &Options) -> Result<(), String> {
    let text = serde_json::to_string_pretty(&schema()).map_err(|e| e.to_string())?;
    std::fs::write(&opts.out, text + "\n").map_err(|e| format!("{}: {e}", opts.out.display()))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the names an editor's forms show)
    use super::*;

    #[test]
    fn the_schema_names_each_enum_a_weenies_form_chooses_from() {
        let s = schema();
        for e in [
            "CreatureType",
            "Skill",
            "PaletteTemplate",
            "EmoteType",
            "MotionCommand",
            "VendorType",
        ] {
            assert!(
                s["enums"][e].as_object().is_some_and(|m| !m.is_empty()),
                "{e}"
            );
        }
        assert_eq!(s["enums"]["PaletteTemplate"]["39"], "Black");
        assert_eq!(s["enums"]["HookType"]["2"], "Wall");
        assert_eq!(s["enums"]["CombatStyle"]["2"], "OneHanded");
        assert_eq!(s["enums"]["Gender"]["2"], "Female");
        assert_eq!(s["enums"]["MotionCommand"]["318767229"], "BowDeep");
        assert_eq!(s["properties"]["int"]["25"], "Level");
    }
}
