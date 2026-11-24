//! The rows' vocabulary, for an editor that reads and writes a world's rows without knowing the
//! game: each kind of row, the file it lives in, the item it sets, and every field, map and list
//! it holds, with what each is written as. `empyrean-import schema` writes it as its `rows` member.
//!
//! A kind is described as
//!
//! ```json
//! { "file": "weenie/{class:05}.toml", "item": "weenie {class}", "key": ["class"],
//!   "fields": { "class": "int", "type": { "name": "WeenieType" }, ... },
//!   "maps": { "int": { "key": { "property": "int" }, "value": "int" }, ... },
//!   "lists": { "create_list": { "fields": { ... } }, ... } }
//! ```
//!
//! and a type is one of:
//!
//! - `"int"`, `"float"`, `"bool"`, `"text"`; `"hex"`, an integer written in hexadecimal; `"time"`,
//!   a moment written as text (`2005-02-09 10:00:00`);
//! - `{ "name": <enum> }`, a value written by its name in the schema's `enums`;
//! - `{ "list": <type>, "len": n }`, a list (of `n` exactly, when `len` is given);
//! - `{ "fields": { ... } }`, a table of fields written inline;
//! - `{ "either": [<type>, ...] }`, any of them.
//!
//! A kind's `item` is the item a row of it sets, as `dereth_pack::records::item_from` makes it
//! from the row: `{field}` a number in decimal, `{field:04X}` and `{field:#010X}` in hexadecimal,
//! `{field[0]}` a list's element, `{field:lower}` text trimmed and lower-cased. A kind's `file` is
//! made the same way, for a new row: `{field:05}` a number padded, `{cell >> 16:04X}` one shifted,
//! `{name:file}` a name as a file name. A row already written stays in its file, which may differ
//! (names that differ only in case share the first one's file).
//!
//! A kind's `tables` are tables of fields under one name (a weenie's `book`). A map's key is
//! `{ "property": <type> }` (a property's name in the schema's `properties`),
//! `{ "name": <enum> }`, `"int"` (a number) or `"text"`. A field left out is at its default. A
//! kind marked `"open": true` is a serde model whose fields this doesn't list (the ac1999 world's
//! source loot inputs).
//!
//! A kind's `maps` and `tables` fold key by key across layers, each key a field of its own
//! (`int.ItemType`); every list, and every other field, is one field, replaced whole
//! ([`crate::layers`]).
//!
//! A kind an editor shows as ACE's SQL (the weenie) also has `sql`, the tables and columns of the
//! same rows, shaped as the description is:
//!
//! ```json
//! "sql": { "table": "weenie", "columns": { "class": "class_Id", ... },
//!   "maps": { "int": { "table": "weenie_properties_int", "parent": "object_Id",
//!                      "key": "type", "value": "value" },
//!             "attribute": { "table": ..., "parent": ..., "key": "type", "columns": { ... } } },
//!   "tables": { "book": { "table": ..., "parent": ..., "columns": { ... } } },
//!   "lists": { "emote": { "table": ..., "parent": ..., "columns": { ... },
//!                         "lists": { "action": { ... } } } } }
//! ```
//!
//! Each entry is written in `table`, linked to the row that holds it by its `parent` column. A map
//! key is the `key` column (a property's number, an enum's value, a spell); its value is the
//! `value` column, or its fields the `columns`. A field written in several columns names them in
//! order (a position's `at` is `origin_X`, `origin_Y`, `origin_Z`). A field a row leaves out is
//! `NULL` when it is among the entry's `nullable` fields, and its default (0, false, empty)
//! otherwise; a kind's `time` left out is the world's stamp (`world-db.toml`).

use serde_json::{json, Map, Value};

use empyrean_content::models::world::{
    Encounter, Event, HousePortal, PointsOfInterest, Quest, Recipe, RecipeMod, RecipeModsBool,
    RecipeModsDID, RecipeModsFloat, RecipeModsIID, RecipeModsInt, RecipeModsString,
    RecipeRequirementsBool, RecipeRequirementsDID, RecipeRequirementsFloat, RecipeRequirementsIID,
    RecipeRequirementsInt, RecipeRequirementsString, RecipeVitalRequirement, Spell, TreasureDeath,
    TreasureGemCount, TreasureMaterialBase, TreasureMaterialColor, TreasureMaterialGroups,
    TreasureWielded, Version, WeeniePropertiesAnimPart, WeeniePropertiesAttribute,
    WeeniePropertiesAttribute2nd, WeeniePropertiesBodyPart, WeeniePropertiesBook,
    WeeniePropertiesBookPageData, WeeniePropertiesCreateList, WeeniePropertiesEmoteAction,
    WeeniePropertiesEventFilter, WeeniePropertiesGenerator, WeeniePropertiesPalette,
    WeeniePropertiesSkill, WeeniePropertiesTextureMap,
};

use empyrean_content::models::world::{
    Weenie, WeeniePropertiesBool, WeeniePropertiesDID, WeeniePropertiesEmote,
    WeeniePropertiesFloat, WeeniePropertiesIID, WeeniePropertiesInt, WeeniePropertiesInt64,
    WeeniePropertiesPosition, WeeniePropertiesSpellBook, WeeniePropertiesString,
};
use empyrean_content::records::FromRow;

use crate::kinds::Fields;
use crate::value::Ty;

fn ty(t: Ty, hex: bool) -> Value {
    Value::from(match (t, hex) {
        (Ty::Int, true) => "hex",
        (Ty::Int, false) => "int",
        (Ty::Float, _) => "float",
        (Ty::Bool, _) => "bool",
        (Ty::Text, _) => "text",
        (Ty::Time, _) => "time",
    })
}

/// The fields of `T`'s rows, without those named in `without` (keys the row writes itself).
fn fields<T: Fields>(without: &[&str]) -> Map<String, Value> {
    T::spec()
        .into_iter()
        .filter(|(n, _, _)| !without.contains(n))
        .map(|(n, t, hex)| (n.to_owned(), ty(t, hex)))
        .collect()
}

/// `T`'s rows as an inline table.
fn inline<T: Fields>() -> Value {
    json!({ "fields": fields::<T>(&[]) })
}

/// `T`'s rows as a list's entries.
fn entries<T: Fields>() -> Value {
    json!({ "fields": fields::<T>(&[]) })
}

/// `extra` fields first, then `T`'s own.
fn with<T: Fields>(extra: &[(&str, Value)], without: &[&str]) -> Map<String, Value> {
    let mut m: Map<String, Value> = extra
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect();
    m.extend(fields::<T>(without));
    m
}

fn name(e: &str) -> Value {
    json!({ "name": e })
}

fn list(t: &str, len: usize) -> Value {
    json!({ "list": t, "len": len })
}

/// A position: a cell, a spot and a turn.
fn position() -> Value {
    json!({ "fields": { "cell": "hex", "at": list("float", 3), "rotation": list("float", 4) } })
}

fn property(t: &str, value: Value) -> Value {
    json!({ "key": { "property": t }, "value": value })
}

fn named(e: &str, value: Value) -> Value {
    json!({ "key": { "name": e }, "value": value })
}

/// `T`'s column for its field `field`.
///
/// # Panics
/// For a field `T` doesn't read: the description and the dump disagree.
fn col<T: FromRow>(field: &str) -> &'static str {
    T::FIELDS
        .iter()
        .position(|f| f.trim_start_matches("r#") == field)
        .map(|i| T::COLUMNS[i])
        .unwrap_or_else(|| panic!("{} has no field {field}", T::TABLE))
}

/// `T`'s columns by its fields, but its own id, its link and those in `without`.
fn columns<T: FromRow>(link: &str, without: &[&str]) -> Map<String, Value> {
    T::FIELDS
        .iter()
        .zip(T::COLUMNS)
        .map(|(f, c)| (f.trim_start_matches("r#"), *c))
        .filter(|(f, _)| *f != "id" && *f != link && !without.contains(f))
        .map(|(f, c)| (f.to_owned(), Value::from(c)))
        .collect()
}

/// `T`'s table of a weenie's entries, linked by its `object_id`; keyed by `key`'s column, when
/// it is a map's.
fn child<T: FromRow + Fields>(key: Option<&str>) -> Value {
    let mut v = json!({
        "table": T::TABLE,
        "parent": col::<T>("object_id"),
        "columns": columns::<T>("object_id", key.as_slice()),
        "nullable": T::optional(),
    });
    if let Some(k) = key {
        v["key"] = col::<T>(k).into();
    }
    v
}

/// `T`'s table of a weenie's values, by key: a property's number, a spell.
fn values<T: FromRow>(key: &str, value: &str) -> Value {
    json!({
        "table": T::TABLE,
        "parent": col::<T>("object_id"),
        "key": col::<T>(key),
        "value": col::<T>(value),
    })
}

/// The weenie's rows as ACE's SQL tables.
fn weenie_sql() -> Value {
    let p = col::<WeeniePropertiesPosition>;
    // The emote's own row is written by hand (its category by name), the rest optional.
    let emote = json!({
        "table": WeeniePropertiesEmote::TABLE,
        "parent": col::<WeeniePropertiesEmote>("object_id"),
        "columns": columns::<WeeniePropertiesEmote>("object_id", &[]),
        "nullable": ["weenie_class_id", "style", "substyle", "quest", "vendor_type", "min_health",
                     "max_health"],
        "lists": { "action": {
            "table": WeeniePropertiesEmoteAction::TABLE,
            "parent": col::<WeeniePropertiesEmoteAction>("emote_id"),
            "columns": columns::<WeeniePropertiesEmoteAction>("emote_id", &[]),
            "nullable": WeeniePropertiesEmoteAction::optional(),
        } },
    });
    json!({
        "table": Weenie::TABLE,
        "columns": {
            "class": col::<Weenie>("class_id"),
            "class_name": col::<Weenie>("class_name"),
            "type": col::<Weenie>("type"),
            "last_modified": col::<Weenie>("last_modified"),
        },
        "maps": {
            "int": values::<WeeniePropertiesInt>("type", "value"),
            "int64": values::<WeeniePropertiesInt64>("type", "value"),
            "bool": values::<WeeniePropertiesBool>("type", "value"),
            "float": values::<WeeniePropertiesFloat>("type", "value"),
            "string": values::<WeeniePropertiesString>("type", "value"),
            "did": values::<WeeniePropertiesDID>("type", "value"),
            "iid": values::<WeeniePropertiesIID>("type", "value"),
            "position": {
                "table": WeeniePropertiesPosition::TABLE,
                "parent": p("object_id"),
                "key": p("position_type"),
                "columns": {
                    "cell": p("obj_cell_id"),
                    "at": [p("origin_x"), p("origin_y"), p("origin_z")],
                    "rotation": [p("angles_w"), p("angles_x"), p("angles_y"), p("angles_z")],
                },
            },
            "attribute": child::<WeeniePropertiesAttribute>(Some("type")),
            "vital": child::<WeeniePropertiesAttribute2nd>(Some("type")),
            "skill": child::<WeeniePropertiesSkill>(Some("type")),
            "body_part": child::<WeeniePropertiesBodyPart>(Some("key")),
            "spell_book": values::<WeeniePropertiesSpellBook>("spell", "probability"),
        },
        "tables": {
            "book": child::<WeeniePropertiesBook>(None),
        },
        "lists": {
            "anim_part": child::<WeeniePropertiesAnimPart>(None),
            "book_page": child::<WeeniePropertiesBookPageData>(None),
            "create_list": child::<WeeniePropertiesCreateList>(None),
            "emote": emote,
            "event_filter": child::<WeeniePropertiesEventFilter>(None),
            "generator": child::<WeeniePropertiesGenerator>(None),
            "palette": child::<WeeniePropertiesPalette>(None),
            "texture_map": child::<WeeniePropertiesTextureMap>(None),
        },
    })
}

fn weenie() -> Value {
    json!({
        "sql": weenie_sql(),
        "file": "weenie/{class:05}.toml",
        "item": "weenie {class}",
        "key": ["class"],
        "fields": {
            "class": "int",
            "class_name": "text",
            "type": name("WeenieType"),
            "last_modified": "time",
        },
        "maps": {
            "int": property("int", "int".into()),
            "int64": property("int64", "int".into()),
            "bool": property("bool", "bool".into()),
            "float": property("float", "float".into()),
            "string": property("string", "text".into()),
            "did": property("did", "hex".into()),
            "iid": property("iid", "hex".into()),
            "position": property("position", position()),
            "attribute": named("Attribute", inline::<WeeniePropertiesAttribute>()),
            "vital": named("Vital", inline::<WeeniePropertiesAttribute2nd>()),
            "skill": named("Skill", inline::<WeeniePropertiesSkill>()),
            "body_part": named("BodyPart", inline::<WeeniePropertiesBodyPart>()),
            "spell_book": { "key": "int", "value": "float" },
        },
        "tables": {
            "book": fields::<WeeniePropertiesBook>(&[]),
        },
        "lists": {
            "anim_part": entries::<WeeniePropertiesAnimPart>(),
            "book_page": entries::<WeeniePropertiesBookPageData>(),
            "create_list": entries::<WeeniePropertiesCreateList>(),
            "emote": {
                "fields": {
                    "category": name("EmoteCategory"),
                    "probability": "float",
                    "weenie_class_id": "int",
                    "style": "hex",
                    "substyle": "hex",
                    "quest": "text",
                    "vendor_type": "int",
                    "min_health": "float",
                    "max_health": "float",
                },
                "lists": {
                    "action": {
                        "fields": with::<WeeniePropertiesEmoteAction>(
                            &[("type", name("EmoteType"))],
                            &[],
                        ),
                    },
                },
            },
            "event_filter": entries::<WeeniePropertiesEventFilter>(),
            "generator": entries::<WeeniePropertiesGenerator>(),
            "palette": entries::<WeeniePropertiesPalette>(),
            "texture_map": entries::<WeeniePropertiesTextureMap>(),
        },
    })
}

fn recipe() -> Value {
    let mods = |m: &mut Map<String, Value>| {
        for (k, v) in [
            ("bool", entries::<RecipeModsBool>()),
            ("did", entries::<RecipeModsDID>()),
            ("float", entries::<RecipeModsFloat>()),
            ("iid", entries::<RecipeModsIID>()),
            ("int", entries::<RecipeModsInt>()),
            ("string", entries::<RecipeModsString>()),
        ] {
            m.insert(k.into(), v);
        }
    };
    let mut mod_lists = Map::new();
    mods(&mut mod_lists);
    json!({
        "file": "recipe/{id}.toml",
        "item": "recipe {id}",
        "key": ["id"],
        "fields": with::<Recipe>(&[("id", "int".into())], &[]),
        "lists": {
            "mod": { "fields": fields::<RecipeMod>(&[]), "lists": mod_lists },
            "require_bool": entries::<RecipeRequirementsBool>(),
            "require_did": entries::<RecipeRequirementsDID>(),
            "require_float": entries::<RecipeRequirementsFloat>(),
            "require_iid": entries::<RecipeRequirementsIID>(),
            "require_int": entries::<RecipeRequirementsInt>(),
            "require_string": entries::<RecipeRequirementsString>(),
        },
    })
}

/// The kinds of row a world holds, by their table's name (`[[weenie]]`).
#[must_use]
pub fn rows() -> Value {
    let plain = |file: &str, item: &str, key: &[&str], f: Map<String, Value>| json!({ "file": file, "item": item, "key": key, "fields": f });
    let mut kinds = Map::new();
    kinds.insert("weenie".into(), weenie());
    kinds.insert(
        "placement".into(),
        json!({
            "file": "landblock/{cell >> 16:04X}.toml",
            "item": "spawn {guid:#010X}",
            "key": ["guid"],
            "fields": {
                "guid": "hex",
                "class": "int",
                "cell": "hex",
                "at": list("float", 3),
                "rotation": list("float", 4),
                "link_child": "bool",
                "last_modified": "time",
                "links": { "list": { "either": [
                    "hex",
                    { "fields": { "child": "hex", "last_modified": "time" } },
                ] } },
            },
        }),
    );
    kinds.insert(
        "encounter".into(),
        json!({
            "file": "encounter/{landblock >> 8:02X}xx.toml",
            "item": "encounter {landblock:04X} {cell[0]},{cell[1]}",
            "key": ["landblock", "cell"],
            "fields": with::<Encounter>(
                &[("landblock", "hex".into()), ("cell", list("int", 2)), ("class", "int".into())],
                &[],
            ),
        }),
    );
    kinds.insert(
        "cook_book".into(),
        plain(
            "cook_book/{source:05}-{target:05}.toml",
            "cook-book {recipe} {source} {target}",
            &["recipe", "source", "target"],
            json!({ "recipe": "int", "source": "int", "target": "int", "last_modified": "time" })
                .as_object()
                .cloned()
                .unwrap_or_default(),
        ),
    );
    kinds.insert("recipe".into(), recipe());
    kinds.insert(
        "recipe_vital".into(),
        plain(
            "recipe_vital/{recipe}.toml",
            "recipe-vital {recipe}",
            &["recipe"],
            with::<RecipeVitalRequirement>(&[("recipe", "int".into())], &[]),
        ),
    );
    kinds.insert(
        "spell".into(),
        plain(
            "spell/{id}.toml",
            "spell {id}",
            &["id"],
            with::<Spell>(&[("id", "int".into())], &[]),
        ),
    );
    for (kind, item) in [("quest", "quest"), ("event", "event"), ("poi", "poi")] {
        let f = match kind {
            "quest" => fields::<Quest>(&[]),
            "event" => fields::<Event>(&[]),
            _ => fields::<PointsOfInterest>(&[]),
        };
        kinds.insert(
            kind.into(),
            plain(
                &format!("{kind}/{{name:file}}.toml"),
                &format!("{item} {{name:lower}}"),
                &["name"],
                f,
            ),
        );
    }
    kinds.insert(
        "house_portal".into(),
        plain(
            "house_portal/{house_id}.toml",
            "house-portal {house_id} {obj_cell_id:#010X}",
            &["house_id", "obj_cell_id"],
            fields::<HousePortal>(&[]),
        ),
    );
    kinds.insert(
        "treasure_death".into(),
        plain(
            "treasure_death/{type}.toml",
            "death-treasure {type}",
            &["type"],
            with::<TreasureDeath>(&[("type", "int".into())], &[]),
        ),
    );
    kinds.insert(
        "treasure_wielded".into(),
        json!({
            "file": "treasure_wielded/{type:05}.toml",
            "item": "wielded-treasure {type}",
            "key": ["type"],
            "fields": { "type": "int" },
            "lists": { "entry": entries::<TreasureWielded>() },
        }),
    );
    for (kind, file, item, key, f) in [
        (
            "treasure_gem_count",
            "treasure_gem_count.toml",
            "gem-count {gem_code} {tier} {count}",
            vec!["gem_code", "tier", "count"],
            fields::<TreasureGemCount>(&[]),
        ),
        (
            "treasure_material_base",
            "treasure_material_base.toml",
            "material-base {material_code} {tier} {material_id}",
            vec!["material_code", "tier", "material_id"],
            fields::<TreasureMaterialBase>(&[]),
        ),
        (
            "treasure_material_color",
            "treasure_material_color.toml",
            "material-color {material_id} {color_code} {palette_template}",
            vec!["material_id", "color_code", "palette_template"],
            fields::<TreasureMaterialColor>(&[]),
        ),
    ] {
        kinds.insert(kind.into(), plain(file, item, &key, f));
    }
    kinds.insert(
        "treasure_material_groups".into(),
        json!({
            "file": "treasure_material_groups.toml",
            "item": "material-group {material_group} {tier}",
            "key": ["material_group", "tier"],
            "fields": { "material_group": "int", "tier": "int" },
            "lists": { "entry": entries::<TreasureMaterialGroups>() },
        }),
    );
    kinds.insert(
        "version".into(),
        plain("version.toml", "version", &[], fields::<Version>(&[])),
    );
    for (kind, file, item, key) in [
        (
            "grouped_treasure",
            "grouped_treasure/{treasure_type}.toml",
            "grouped-treasure {treasure_type}",
            vec!["treasure_type"],
        ),
        ("loot_policy", "loot_policy.toml", "loot-policy", vec![]),
    ] {
        kinds.insert(
            kind.into(),
            json!({ "file": file, "item": item, "key": key, "open": true }),
        );
    }
    let allowed = json!({ "either": ["text", { "list": { "name": "Skill" } }] });
    kinds.insert(
        "rules".into(),
        json!({
            "file": "rules.toml",
            "item": "rules",
            "key": [],
            "fields": {
                "max_level": "int",
                "start": "text",
                "account_has_tod": "bool",
                "loot": "text",
                "loot_rules": "text",
                "starter_gear": "text",
                "placements_on_own_land": "bool",
                "welcome_letter": "int",
                "creation_skills": allowed,
                "training_skills": allowed,
                "heritages": { "either": ["text", { "list": { "name": "HeritageGroup" } }] },
            },
            "maps": {
                "features": { "key": "text", "value": "bool" },
                "formulas": { "key": "text", "value": { "either": ["bool", "int", "text"] } },
                "properties": { "key": "text", "value": { "either": ["bool", "int", "text"] } },
            },
        }),
    );
    Value::Object(kinds)
}
