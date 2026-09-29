//! The GDLE bulk documents `empyrean-import --json` (and `--patches` folders of JSON) accept, beyond
//! the four per-object kinds ACE's `import-json` reads ([`crate::import::json::JsonKind`]). Not
//! ACE code: ACE has `GDLELoader`'s loaders and converters ([`super::loader`]) but no command that
//! imports these files. Here each file goes through its `...Converted` loader, then the World SQL
//! writers write `DELETE` + `INSERT` statements (as `json2sql_*` does for the per-object kinds),
//! which the patch applier runs like any SQL file. Every row gets `now` as its `last_Modified`.
//!
//! GDLE's region and terrain files have no converter in ACE and are not importable.

use empyrean_common::dotnet::DotNetDateTime;

use super::loader;
use crate::export::sql::{
    EventSQLWriter, LandblockInstanceWriter, SpellSQLWriter, SqlOut, TreasureWieldedSQLWriter,
};
use crate::import::json::sql_writer::{self, Out};
use crate::import::json::value::R;

/// A GDLE bulk document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GdleKind {
    /// `worldspawns.json`: `{"_version", "landblocks": [landblock, ...]}`.
    WorldSpawns,
    /// `{"table": {"spellBaseHash": [...]}}`.
    Spells,
    /// `[{"key": name, "value": {"startTime", "endTime", "eventState"}}, ...]`.
    Events,
    /// `[{"key": name, "value": {"fullname", "mindelta", "maxsolves"}}, ...]`.
    Quests,
    /// `[{"key": treasure type, "value": [{"weenieClassId", "probability", ...}, ...]}, ...]`.
    WieldedTreasure,
    /// `[{"RecipeID", "Skill", ...}, ...]`.
    Recipes,
    /// `[{"Tool", "Target", "RecipeId"}, ...]`.
    RecipePrecursors,
}

impl GdleKind {
    /// The kind of a document by its shape: the per-object kinds are single objects, so an
    /// array (by its first element) or an object holding `landblocks` or `table` is a bulk file.
    /// `None` for anything else, an empty array included.
    #[must_use]
    pub fn detect(doc: &serde_json::Value) -> Option<Self> {
        use serde_json::Value;
        match doc {
            Value::Object(o) if o.get("landblocks").is_some_and(Value::is_array) => {
                Some(Self::WorldSpawns)
            }
            Value::Object(o)
                if o.get("table")
                    .and_then(Value::as_object)
                    .is_some_and(|t| t.contains_key("spellBaseHash")) =>
            {
                Some(Self::Spells)
            }
            Value::Array(items) => {
                let first = items.first()?.as_object()?;
                if first.contains_key("RecipeID") {
                    return Some(Self::Recipes);
                }
                if first.contains_key("Tool") || first.contains_key("Target") {
                    return Some(Self::RecipePrecursors);
                }
                match first.get("value")? {
                    Value::Object(v) => {
                        if ["startTime", "endTime", "eventState"]
                            .iter()
                            .any(|k| v.contains_key(*k))
                        {
                            Some(Self::Events)
                        } else if ["fullname", "mindelta", "maxsolves"]
                            .iter()
                            .any(|k| v.contains_key(*k))
                        {
                            Some(Self::Quests)
                        } else {
                            None
                        }
                    }
                    Value::Array(_) => Some(Self::WieldedTreasure),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Group rows by a key, keeping the order in which each key first appears.
fn groups<T: Clone, K: PartialEq>(rows: &[T], key: impl Fn(&T) -> K) -> Vec<Vec<T>> {
    let mut out: Vec<(K, Vec<T>)> = Vec::new();
    for r in rows {
        let k = key(r);
        match out.iter_mut().find(|(g, _)| *g == k) {
            Some((_, v)) => v.push(r.clone()),
            None => out.push((k, vec![r.clone()])),
        }
    }
    out.into_iter().map(|(_, v)| v).collect()
}

fn writer_error(e: impl std::fmt::Display) -> String {
    format!("failed to write: {e}")
}

/// The SQL for one GDLE bulk document (its text, a byte order mark already dropped).
pub fn to_sql(kind: GdleKind, text: &str, now: DotNetDateTime) -> R<String> {
    let convert = |e: String| format!("failed to load or convert: {e}");
    match kind {
        GdleKind::WorldSpawns => {
            let (mut instances, _) = loader::world_spawns_converted(text, 0).map_err(convert)?;
            for i in &mut instances {
                i.last_modified = now;
                for l in &mut i.landblock_instance_link {
                    l.last_modified = now;
                }
            }
            let w = LandblockInstanceWriter::default();
            let mut out = SqlOut::new("\n");
            for group in groups(&instances, |i| i.obj_cell_id >> 16) {
                w.create_sql_delete_statement(&group, &mut out)
                    .map_err(writer_error)?;
                out.write_empty_line();
                w.create_sql_insert_statement(&group, &mut out)
                    .map_err(writer_error)?;
                out.write_empty_line();
            }
            Ok(out.text)
        }
        GdleKind::Spells => {
            let w = SpellSQLWriter::default();
            let mut out = SqlOut::new("\n");
            for mut spell in loader::spells_converted(text).map_err(convert)? {
                spell.last_modified = now;
                w.create_sql_delete_statement(&spell, &mut out);
                out.write_empty_line();
                w.create_sql_insert_statement(&spell, &mut out);
                out.write_empty_line();
            }
            Ok(out.text)
        }
        GdleKind::Events => {
            let w = EventSQLWriter::default();
            let mut out = SqlOut::new("\n");
            for mut event in loader::events_converted(text).map_err(convert)? {
                event.last_modified = now;
                w.create_sql_delete_statement(&event, &mut out);
                out.write_empty_line();
                w.create_sql_insert_statement(&event, &mut out);
                out.write_empty_line();
            }
            Ok(out.text)
        }
        GdleKind::Quests => {
            let mut out = Out::default();
            for mut quest in loader::quests_converted(text).map_err(convert)? {
                quest.last_modified = now;
                sql_writer::quest_delete(&quest, &mut out);
                out.0.push('\n');
                sql_writer::quest_insert(&quest, &mut out);
                out.0.push('\n');
            }
            Ok(out.0)
        }
        GdleKind::WieldedTreasure => {
            let mut rows = loader::wielded_treasure_table_converted(text).map_err(convert)?;
            for r in &mut rows {
                r.last_modified = now;
            }
            let w = TreasureWieldedSQLWriter::default();
            let mut out = SqlOut::new("\n");
            for group in groups(&rows, |r| r.treasure_type) {
                w.create_sql_delete_statement(&group, &mut out)
                    .map_err(writer_error)?;
                out.write_empty_line();
                // The rows are written before the closing `/* ... */` summary, which needs the
                // weenie names this build does not have (the writer throws there): the comment
                // is left off.
                let mut insert = SqlOut::new("\n");
                let _ = w.create_sql_insert_statement(&group, &mut insert);
                let values_end = insert.text.find(";\n").map_or(insert.text.len(), |i| i + 2);
                out.text.push_str(&insert.text[..values_end]);
                out.write_empty_line();
            }
            Ok(out.text)
        }
        GdleKind::Recipes => {
            let mut out = Out::default();
            for mut recipe in loader::recipes_converted(text).map_err(convert)? {
                recipe.last_modified = now;
                sql_writer::recipe_delete(&recipe, &mut out);
                out.0.push('\n');
                sql_writer::recipe_insert(&recipe, &mut out);
                out.0.push('\n');
            }
            Ok(out.0)
        }
        GdleKind::RecipePrecursors => {
            let mut cook_books = loader::recipe_precursors_converted(text).map_err(convert)?;
            for c in &mut cook_books {
                c.last_modified = now;
            }
            let mut out = Out::default();
            for group in groups(&cook_books, |c| c.recipe_id) {
                sql_writer::cook_book_delete(&group, &mut out).map_err(writer_error)?;
                out.0.push('\n');
                sql_writer::cook_book_insert(&group, &mut out);
                out.0.push('\n');
            }
            Ok(out.0)
        }
    }
}
