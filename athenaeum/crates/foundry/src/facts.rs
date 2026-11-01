//! What a world changes, counted: its own rows by kind and by where they came from, the base's
//! rows it replaces and takes away, and how many rows the server will have.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use yalain_world_db::layer;
use yalain_world_db::merge::item_key;
use yalain_world_db::rows::{Database, Row};

use crate::world::Opened;

/// A world's counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    pub key: String,
    /// The base's key and its release (`""` for the one the build pins), when the world names one.
    pub base: Option<(String, String)>,
    pub packs: usize,
    /// The world's own rows, by kind (`[[remove]]` counted as `remove`).
    pub rows: BTreeMap<String, usize>,
    /// The world's own rows, by where they came from.
    pub from: BTreeMap<String, usize>,
    /// The base's rows the world's rows replace, and those it takes away.
    pub replaced: usize,
    pub removed: usize,
    /// Rows the base has, and rows the merged world has.
    pub base_rows: usize,
    pub merged_rows: usize,
    pub problems: usize,
}

/// Count a world.
#[must_use]
pub fn facts(world: &Opened) -> Facts {
    let (db, mut problems) = world.read();
    let db = db.unwrap_or_else(Database::default);
    let mut f = Facts {
        key: db.world.key.clone(),
        base: db
            .world
            .base
            .as_ref()
            .map(|b| (b.db.clone(), b.version.clone().unwrap_or_default())),
        packs: db.packs.len(),
        base_rows: world.base.merged.rows.len(),
        ..Facts::default()
    };
    let base_items: BTreeSet<String> = world.base.merged.rows.iter().filter_map(item_key).collect();
    let mut replaced = BTreeSet::new();
    for l in db.rows() {
        if let Some(k) = item_key(l).filter(|k| base_items.contains(k)) {
            replaced.insert(k);
        }
        *f.rows.entry(l.row.kind().to_string()).or_default() += 1;
        if let Some(p) = l.row.prov() {
            *f.from.entry(p.source.word().to_string()).or_default() += 1;
        }
        if let Row::Remove(items) = &l.row {
            f.removed += items.len();
        }
    }
    problems.extend(world.check());
    match layer::merge_over(&world.base, &db) {
        Ok(m) => f.merged_rows = m.rows.len(),
        Err(p) => problems.extend(p),
    }
    f.replaced = replaced.len();
    problems.sort();
    problems.dedup();
    f.problems = problems.len();
    f
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_counts(m: &BTreeMap<String, usize>) -> String {
    let parts: Vec<String> = m
        .iter()
        .map(|(k, v)| format!("{}:{v}", json_string(k)))
        .collect();
    format!("{{{}}}", parts.join(","))
}

impl Facts {
    /// One line of JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        let base = match &self.base {
            Some((db, v)) => format!(
                "{{\"db\":{},\"version\":{}}}",
                json_string(db),
                if v.is_empty() {
                    "null".to_string()
                } else {
                    json_string(v)
                }
            ),
            None => "null".into(),
        };
        format!(
            "{{\"world\":{},\"base\":{base},\"packs\":{},\"rows\":{},\"from\":{},\"replaced\":{},\"removed\":{},\"base_rows\":{},\"merged_rows\":{},\"problems\":{}}}",
            json_string(&self.key),
            self.packs,
            json_counts(&self.rows),
            json_counts(&self.from),
            self.replaced,
            self.removed,
            self.base_rows,
            self.merged_rows,
            self.problems
        )
    }
}
