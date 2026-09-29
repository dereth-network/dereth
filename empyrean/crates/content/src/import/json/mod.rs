// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/DeveloperContentCommands.cs
//! ACE's JSON content formats (`import-json`): each file is converted to the SQL text ACE's
//! `DeveloperContentCommands.json2sql_*` writes, which the patch applier then runs, exactly as
//! ACE's `ImportJson*` hands that file to `ImportSQL`.
//!
//! * [`value`]: `System.Text.Json`'s default deserialization rules.
//! * [`models`]: the ACE.Adapter GDLE/Lifestoned models.
//! * [`lifestoned`], [`gdle`]: `LifestonedConverter` / `GDLEConverter.TryConvert`.
//! * [`sql_writer`]: the World SQL writers, without their `/* comment */` labels (enum, weenie
//!   and spell names, `@teleloc` lines, the Lifestoned changelog), which MySQL ignores.
//!
//! Every failure ACE reports as "Failed to load/convert" (a loader or converter returning
//! `false` or throwing, or the writer block's `catch`) is an `Err` here.

pub mod gdle;
pub mod lifestoned;
pub mod models;
pub mod sql_writer;
pub mod value;
pub mod weenie_class_id;
pub mod world;

use empyrean_common::dotnet::{DotNetDateTime, DotNetDict};

use sql_writer::Out;
use value::R;

/// Which of ACE's four JSON content types a file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonKind {
    /// Lifestoned weenie (`json/weenies`).
    Weenie,
    /// GDLE recipe with its precursors (`json/recipes`).
    Recipe,
    /// GDLE landblock spawn map (`json/landblocks`).
    Landblock,
    /// GDLE quest (`json/quests`).
    Quest,
}

impl JsonKind {
    /// The kind of the file at `path`.
    ///
    /// 1. The nearest ancestor directory named `weenies`, `recipes`, `landblocks` or `quests`
    ///    (ASCII case-insensitive) decides: ACE's `import-json` reads each kind from
    ///    `<content folder>/json/<that name>/`.
    /// 2. Otherwise the document's top-level keys: `wcid` is a weenie; `recipe` or `precursors`
    ///    a recipe; an object `value` holding `weenies` or `links` a landblock, holding
    ///    `fullname`, `mindelta` or `maxsolves` a quest.
    ///
    /// `None` when neither decides.
    #[must_use]
    pub fn detect(path: &std::path::Path, doc: &serde_json::Value) -> Option<Self> {
        for dir in path.ancestors().skip(1) {
            let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let kind = match name.to_ascii_lowercase().as_str() {
                "weenies" => Self::Weenie,
                "recipes" => Self::Recipe,
                "landblocks" => Self::Landblock,
                "quests" => Self::Quest,
                _ => continue,
            };
            return Some(kind);
        }
        let obj = doc.as_object()?;
        if obj.contains_key("wcid") {
            return Some(Self::Weenie);
        }
        if obj.contains_key("recipe") || obj.contains_key("precursors") {
            return Some(Self::Recipe);
        }
        let value = obj.get("value")?.as_object()?;
        if value.contains_key("weenies") || value.contains_key("links") {
            return Some(Self::Landblock);
        }
        if ["fullname", "mindelta", "maxsolves"]
            .iter()
            .any(|k| value.contains_key(*k))
        {
            return Some(Self::Quest);
        }
        None
    }
}

/// ACE's `json2sql_*` for one file: the SQL text it writes (without its comments).
///
/// `now` stands in for `DateTime.UtcNow` where ACE stamps a missing `LastModified` (ACE's
/// converters never set one, so every row json2sql writes with a `last_Modified` gets `now`).
pub fn json_to_sql(kind: JsonKind, text: &str, now: DotNetDateTime) -> Result<String, String> {
    json_to_sql_file(kind, text, now, None).map(|j| j.sql)
}

/// What `json2sql_*` produces for one file (for the developer `import-json` command).
#[derive(Debug, Clone)]
pub struct JsonSql {
    /// The SQL text (see [`json_to_sql`]).
    pub sql: String,
    /// The SQL file's name (`GetDefaultFileName` of the writer); `None` for a quest, whose SQL
    /// file is named after its JSON file.
    pub file_name: Option<String>,
    /// What ACE writes to the session while it links a landblock's instances.
    pub messages: Vec<String>,
}

/// [`json_to_sql`], with the SQL file's name and ACE's messages. `weenie_names` is
/// `DatabaseManager.World.GetAllWeenieNames()`, which names a recipe's file.
pub fn json_to_sql_file(
    kind: JsonKind,
    text: &str,
    now: DotNetDateTime,
    weenie_names: Option<&DotNetDict<u32, String>>,
) -> Result<JsonSql, String> {
    // File.ReadAllText drops a byte order mark.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let doc = value::parse(text).map_err(|e| format!("failed to load: {e}"))?;
    match kind {
        JsonKind::Weenie => json2sql_weenie(&doc, now),
        JsonKind::Recipe => json2sql_recipe(&doc, now, weenie_names),
        JsonKind::Landblock => json2sql_landblock(&doc, now),
        JsonKind::Quest => json2sql_quest(&doc, now),
    }
}

/// `SQLWriter.IllegalInFileName`: `Path.GetInvalidFileNameChars()` of the platform, each
/// replaced by `_`.
// ACE: SQLWriter.IllegalInFileName
#[must_use]
pub fn illegal_in_file_name(file_name: &str) -> String {
    file_name
        .chars()
        .map(|c| {
            let invalid = if cfg!(windows) {
                matches!(
                    c,
                    '"' | '<' | '>' | '|' | '\0'..='\u{1f}' | ':' | '*' | '?' | '\\' | '/'
                )
            } else {
                matches!(c, '\0' | '/')
            };
            if invalid {
                '_'
            } else {
                c
            }
        })
        .collect()
}

/// `WeenieSQLWriter.GetDefaultFileName` over the converted weenie.
// ACE: WeenieSQLWriter.GetDefaultFileName
fn weenie_file_name(input: &world::Weenie) -> String {
    let name = input.string.iter().find(|r| r.r#type == 1);
    let file_name = format!(
        "{} {}",
        empyrean_common::dotnet::format(input.class_id, "00000"),
        name.and_then(|n| n.value.as_deref()).unwrap_or("")
    );
    illegal_in_file_name(&file_name) + ".sql"
}

/// `RecipeSQLWriter.GetDefaultFileName(input, cookBooks)` over the converted recipe.
// ACE: RecipeSQLWriter.GetDefaultFileName
fn recipe_file_name(
    input: &world::Recipe,
    cook_books: &[world::CookBook],
    weenie_names: Option<&DotNetDict<u32, String>>,
) -> String {
    let mut description: Option<String> = None;
    if let Some(names) = weenie_names {
        if let Some(weenie_name) = names.get(&input.success_wcid) {
            description = Some(weenie_name.clone());
        }
    }
    let mut alternate_description: Option<String> = None;
    if let (Some(names), Some(first)) = (weenie_names, cook_books.first()) {
        alternate_description = names.get(&first.source_wcid).cloned();
        for c in &cook_books[1..] {
            if let Some(source_weenie_name) = names.get(&c.source_wcid) {
                if Some(source_weenie_name) != alternate_description.as_ref() {
                    alternate_description = None;
                    break;
                }
            }
        }
    }
    let empty = |s: &Option<String>| s.as_deref().is_none_or(str::is_empty);
    if empty(&description) && !empty(&alternate_description) {
        description.clone_from(&alternate_description);
    }
    if description.as_deref() == Some("Cooking Pot") && !empty(&alternate_description) {
        description = alternate_description;
    }
    let mut file_name = empyrean_common::dotnet::format(input.id, "00000");
    if !empty(&description) {
        file_name = file_name + " " + description.as_deref().unwrap_or("");
    }
    illegal_in_file_name(&file_name) + ".sql"
}

fn load<T>(read: R<Option<T>>) -> R<T> {
    read.map_err(|e| format!("failed to load: {e}"))?
        .ok_or_else(|| "failed to load: the document is null".to_owned())
}

fn convert<T>(r: R<T>) -> R<T> {
    r.map_err(|e| format!("failed to convert: {e}"))
}

fn stamp(d: &mut DotNetDateTime, now: DotNetDateTime) {
    if *d == DotNetDateTime::MIN_VALUE {
        *d = now;
    }
}

// ACE: DeveloperContentCommands.json2sql_weenie
fn json2sql_weenie(doc: &value::Json, now: DotNetDateTime) -> R<JsonSql> {
    let weenie = load(models::LsdWeenie::read(doc))?;
    let mut output = convert(lifestoned::try_convert(&weenie))?;
    stamp(&mut output.last_modified, now);
    let file_name = weenie_file_name(&output);
    let mut w = Out::default();
    sql_writer::weenie_delete(&output, &mut w);
    w.0.push('\n');
    sql_writer::weenie_insert(&output, &mut w);
    Ok(JsonSql {
        sql: w.0,
        file_name: Some(file_name),
        messages: Vec::new(),
    })
}

// ACE: DeveloperContentCommands.json2sql_recipe
fn json2sql_recipe(
    doc: &value::Json,
    now: DotNetDateTime,
    weenie_names: Option<&DotNetDict<u32, String>>,
) -> R<JsonSql> {
    let combined = load(models::RecipeCombined::read(doc))?;
    let (mut cookbooks, mut recipe) = convert(gdle::try_convert_recipe(&combined))?;
    stamp(&mut recipe.last_modified, now);
    for c in &mut cookbooks {
        stamp(&mut c.last_modified, now);
    }
    let file_name = recipe_file_name(&recipe, &cookbooks, weenie_names);
    let mut w = Out::default();
    sql_writer::recipe_delete(&recipe, &mut w);
    w.0.push('\n');
    sql_writer::recipe_insert(&recipe, &mut w);
    w.0.push('\n');
    sql_writer::cook_book_delete(&cookbooks, &mut w)
        .map_err(|e| format!("failed to convert: {e}"))?;
    w.0.push('\n');
    sql_writer::cook_book_insert(&cookbooks, &mut w);
    Ok(JsonSql {
        sql: w.0,
        file_name: Some(file_name),
        messages: Vec::new(),
    })
}

// ACE: DeveloperContentCommands.json2sql_landblock
fn json2sql_landblock(doc: &value::Json, now: DotNetDateTime) -> R<JsonSql> {
    let lb = load(models::Landblock::read(doc))?;
    let (mut instances, mut links) = convert(gdle::try_convert_landblock(&lb))?;
    // Link up instances: a link whose parent is missing is dropped; one whose child is missing
    // stays with its parent, and no child is marked.
    let mut messages = Vec::new();
    for (i, link) in links.iter().enumerate() {
        let Some(parent) = instances.iter().position(|x| x.guid == link.parent_guid) else {
            messages.push(format!(
                "Couldn't find parent guid for {}",
                empyrean_common::dotnet::format(link.parent_guid, "X8")
            ));
            continue;
        };
        instances[parent].links.push(i);
        let Some(child) = instances.iter().position(|x| x.guid == link.child_guid) else {
            messages.push(format!(
                "Couldn't find child guid for {}",
                empyrean_common::dotnet::format(link.child_guid, "X8")
            ));
            continue;
        };
        instances[child].is_link_child = true;
    }
    for i in &mut instances {
        stamp(&mut i.last_modified, now);
    }
    for l in &mut links {
        stamp(&mut l.last_modified, now);
    }
    // LandblockInstanceWriter.GetDefaultFileName(landblockInstances[0]); an empty list throws
    // inside the writer block.
    // ACE: LandblockInstanceWriter.GetDefaultFileName
    let file_name = instances
        .first()
        .map(|i| {
            illegal_in_file_name(&empyrean_common::dotnet::format(i.obj_cell_id >> 16, "X4"))
                + ".sql"
        })
        .ok_or_else(|| "failed to convert: the landblock has no instances".to_owned())?;
    let mut w = Out::default();
    let fail = |e: String| format!("failed to convert: {e}");
    sql_writer::landblock_delete(&instances, &mut w).map_err(fail)?;
    w.0.push('\n');
    sql_writer::landblock_insert(&instances, &links, &mut w).map_err(fail)?;
    Ok(JsonSql {
        sql: w.0,
        file_name: Some(file_name),
        messages,
    })
}

// ACE: DeveloperContentCommands.json2sql_quest
fn json2sql_quest(doc: &value::Json, now: DotNetDateTime) -> R<JsonSql> {
    let q = load(models::Quest::read(doc))?;
    let mut quest = convert(gdle::try_convert_quest(&q))?;
    stamp(&mut quest.last_modified, now);
    let mut w = Out::default();
    sql_writer::quest_delete(&quest, &mut w);
    w.0.push('\n');
    sql_writer::quest_insert(&quest, &mut w);
    Ok(JsonSql {
        sql: w.0,
        file_name: None,
        messages: Vec::new(),
    })
}
