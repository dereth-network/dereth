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
//!   attributes and skills, the emote categories and actions, the create-list destinations, and
//!   the generators' when and where), each value and its name.
//!
//! The names are the server's own, so a form names what the server reads. An enum's aliases are
//! written once, by the first name the server gives the value.

use std::collections::BTreeMap;
use std::path::PathBuf;

use empyrean_entity::enums::{
    AceEnum, CreatureType, DestinationType, EmoteCategory, EmoteType, ItemType, PositionType,
    PropertyAttribute, PropertyAttribute2nd, PropertyBool, PropertyDataId, PropertyFloat,
    PropertyInstanceId, PropertyInt, PropertyInt64, PropertyString, RegenLocationType,
    RegenerationType, Skill, WeenieType,
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
        ("EmoteCategory", names::<EmoteCategory>()),
        ("EmoteType", names::<EmoteType>()),
        ("DestinationType", names::<DestinationType>()),
        ("RegenerationType", names::<RegenerationType>()),
        ("RegenLocationType", names::<RegenLocationType>()),
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
