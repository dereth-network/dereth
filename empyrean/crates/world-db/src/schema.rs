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
//! A kind's `tables` are tables of fields under one name (a weenie's `book`). A map's key is
//! `{ "property": <type> }` (a property's name in the schema's `properties`),
//! `{ "name": <enum> }`, `"int"` (a number) or `"text"`. A field left out is at its default. A
//! kind marked `"open": true` is a serde model whose fields this doesn't list (the ac1999 world's
//! source loot inputs).
//!
//! Each map of a weenie or of the rules row folds key by key across layers (`maps`); every list is
//! one field, replaced whole ([`crate::layers`]).

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

fn weenie() -> Value {
    json!({
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
            "item": "spawn {guid}",
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
                &format!("{kind}/{{name}}.toml"),
                &format!("{item} {{name, lower case}}"),
                &["name"],
                f,
            ),
        );
    }
    kinds.insert(
        "house_portal".into(),
        plain(
            "house_portal/{house_id}.toml",
            "house-portal {house_id} {obj_cell_id}",
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
            "gem-count {gemcode} {tier} {count}",
            vec!["gemcode", "tier", "count"],
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
