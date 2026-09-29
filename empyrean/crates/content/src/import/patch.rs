//! Content inputs applied in order over a base dump: ACE-style per-object SQL files (the
//! `DELETE` + `INSERT` files of ACE's content repositories and `import-sql`) and ACE's JSON
//! content (`import-json`, converted by [`super::json`] to the SQL ACE writes for it).
//!
//! Not ACE-derived: the driver. What it follows from ACE (`DeveloperContentCommands.ImportSQL`):
//! each file runs on a fresh session, its text cut at a `/* Lifestoned Changelog:` comment. Unlike
//! ACE, which reports a failing file and goes on, a statement that fails stops the import with
//! the file and line: a build must not quietly publish half a change.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::json::{json_to_sql, JsonKind};
use super::sql::lex::tokenize;
use super::sql::parse::parse_script;
use super::sql::store::{Event, EventKind, Session, Store};
use super::sql::value::Val;
use crate::error::ImportError;
use crate::pack::TableId;
use empyrean_common::dotnet::DotNetDateTime;

/// What an input path holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    /// `.sql` files (a directory: every `*.sql` under it).
    Sql,
    /// ACE JSON content (a directory: every `*.json` under it).
    Json,
}

impl InputKind {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            InputKind::Sql => "sql",
            InputKind::Json => "json",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            InputKind::Sql => "sql",
            InputKind::Json => "json",
        }
    }
}

/// One `--patches` or `--json` argument.
#[derive(Debug, Clone)]
pub struct Input {
    pub kind: InputKind,
    pub path: PathBuf,
}

/// The files an input names, in the order they are applied: a file is itself; a directory is
/// every file with the kind's extension beneath it, sorted by relative path compared component by
/// component (ordinal, case-sensitive), so the order never depends on the file system.
pub fn expand(input: &Input) -> Result<Vec<PathBuf>, ImportError> {
    let meta = std::fs::metadata(&input.path).map_err(|e| ImportError::io(&input.path, e))?;
    if meta.is_file() {
        return Ok(vec![input.path.clone()]);
    }
    let mut out: Vec<(Vec<String>, PathBuf)> = Vec::new();
    let mut stack = vec![input.path.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| ImportError::io(&dir, e))? {
            let entry = entry.map_err(|e| ImportError::io(&dir, e))?;
            let p = entry.path();
            let ft = entry.file_type().map_err(|e| ImportError::io(&p, e))?;
            if ft.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(input.kind.extension()))
            {
                let rel = p.strip_prefix(&input.path).unwrap_or(&p);
                let key = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push((key, p));
            }
        }
    }
    out.sort();
    Ok(out.into_iter().map(|(_, p)| p).collect())
}

/// Record counts for one pack table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordCounts {
    pub added: u64,
    pub replaced: u64,
    pub deleted: u64,
}

/// Row counts for one SQL table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RowCounts {
    pub inserted: u64,
    pub deleted: u64,
    pub updated: u64,
}

/// What one file did.
#[derive(Debug, Clone)]
pub struct Applied {
    pub path: String,
    pub kind: InputKind,
    /// Per pack table: records the file added, replaced (rewrote) or deleted. `weenie_index`
    /// follows `weenie` and is not listed.
    pub records: BTreeMap<&'static str, RecordCounts>,
    /// Per SQL table: rows inserted, deleted and updated.
    pub rows: BTreeMap<String, RowCounts>,
    /// The file's bytes, hashed (BLAKE3).
    pub hash: [u8; 32],
    /// Every pack record the file's statements reached (`weenie_index` not listed), in the order
    /// first reached, whether or not it changed in the end. The content overlay rewrites these.
    pub touched: Vec<(TableId, u64)>,
}

impl Applied {
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        let records: serde_json::Map<String, serde_json::Value> = self
            .records
            .iter()
            .map(|(t, c)| {
                (
                    (*t).to_owned(),
                    json!({"added": c.added, "replaced": c.replaced, "deleted": c.deleted}),
                )
            })
            .collect();
        let rows: serde_json::Map<String, serde_json::Value> = self
            .rows
            .iter()
            .map(|(t, c)| {
                (
                    t.clone(),
                    json!({"inserted": c.inserted, "deleted": c.deleted, "updated": c.updated}),
                )
            })
            .collect();
        json!({"path": self.path, "kind": self.kind.name(), "records": records, "rows": rows})
    }
}

/// A pack record key: the pack table and its key.
type RecKey = (TableId, u64);

/// `YYYY-MM-DD HH:MM:SS`.
#[must_use]
pub fn datetime_text(d: DotNetDateTime) -> String {
    d.format("yyyy-MM-dd HH:mm:ss")
}

/// One content file: its kind, its path (shown in errors; a JSON file's folders name its kind),
/// and its bytes.
#[derive(Debug, Clone)]
pub struct Source {
    pub kind: InputKind,
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

impl Source {
    /// Read a file.
    pub fn read(kind: InputKind, path: &Path) -> Result<Self, ImportError> {
        let bytes = std::fs::read(path).map_err(|e| ImportError::io(path, e))?;
        Ok(Self {
            kind,
            path: path.to_owned(),
            bytes,
        })
    }
}

/// Every file of every input, in order, read lazily.
pub fn sources(inputs: &[Input]) -> impl Iterator<Item = Result<Source, ImportError>> + '_ {
    inputs.iter().flat_map(|input| {
        let (files, err) = match expand(input) {
            Ok(f) => (f, None),
            Err(e) => (Vec::new(), Some(Err(e))),
        };
        err.into_iter()
            .chain(files.into_iter().map(move |p| Source::read(input.kind, &p)))
    })
}

/// Apply one content file to the store.
pub fn apply_source(
    store: &mut Store,
    src: &Source,
    now: DotNetDateTime,
) -> Result<Applied, ImportError> {
    let shown = src.path.display().to_string();
    let hash = *blake3::hash(&src.bytes).as_bytes();
    let fail = |line: u32, what: String| ImportError::Sql {
        line: u64::from(line),
        what: format!("{shown}: {what}"),
    };
    let converted;
    let text: &[u8] = match src.kind {
        InputKind::Sql => &src.bytes,
        InputKind::Json => {
            let s = std::str::from_utf8(&src.bytes).map_err(|_| fail(0, "not UTF-8".into()))?;
            let s = s.strip_prefix('\u{feff}').unwrap_or(s);
            let doc: serde_json::Value =
                serde_json::from_str(s).map_err(|e| fail(0, format!("not JSON: {e}")))?;
            // A GDLE bulk document (world spawns, spells, events, quests, wielded
            // treasure, recipes, precursors) goes through GDLELoader's converters.
            if let Some(k) = crate::gdle::json_kinds::GdleKind::detect(&doc) {
                converted = crate::gdle::json_kinds::to_sql(k, s, now)
                    .map_err(|e| fail(0, e))?
                    .into_bytes();
            } else {
                let k = JsonKind::detect(&src.path, &doc).ok_or_else(|| {
                    fail(0, "not one of ACE's JSON content types (weenie, recipe, landblock, quest, or a GDLE bulk file)".into())
                })?;
                converted = json_to_sql(k, s, now).map_err(|e| fail(0, e))?.into_bytes();
            }
            &converted
        }
    };
    apply_sql(store, text, &shown, src.kind, hash)
}

/// Apply SQL text (one file's worth) to the store.
pub fn apply_sql(
    store: &mut Store,
    text: &[u8],
    shown: &str,
    kind: InputKind,
    hash: [u8; 32],
) -> Result<Applied, ImportError> {
    let fail = |line: u32, what: String| ImportError::Sql {
        line: u64::from(line),
        what: format!("{shown}: {what}"),
    };
    // ACE's ImportSQL: CRLF becomes LF (inside string literals too), and everything from the
    // Lifestoned changelog comment on is dropped.
    let text = crlf_to_lf(text);
    const CHANGELOG: &[u8] = b"/* Lifestoned Changelog:";
    let text = match text.windows(CHANGELOG.len()).position(|w| w == CHANGELOG) {
        Some(i) => &text[..i],
        None => &text[..],
    };
    let tokens = tokenize(text).map_err(|e| fail(0, e))?;
    let stmts = parse_script(&tokens).map_err(|e| fail(0, e))?;
    let mut ses = Session::default();
    let mut tracker = Tracker::new(store);
    store.events.clear();
    for s in &stmts {
        store
            .execute(&s.stmt, &mut ses)
            .map_err(|e| fail(s.line, e))?;
        tracker.absorb(store).map_err(|e| fail(s.line, e))?;
    }
    let records = tracker.finish(store).map_err(|e| fail(0, e))?;
    Ok(Applied {
        path: shown.to_owned(),
        kind,
        records,
        rows: tracker.rows,
        hash,
        touched: tracker.order,
    })
}

/// `text.Replace("\r\n", "\n")`.
fn crlf_to_lf(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'\r' && text.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(text[i]);
        i += 1;
    }
    out
}

/// Follows one file's events to the pack records they touch.
struct Tracker {
    /// Each table's slot count when the file started: an older slot is a row the file found.
    snapshot: Vec<usize>,
    /// Records touched, with whether they existed before the file.
    before: HashMap<RecKey, bool>,
    order: Vec<RecKey>,
    /// Root rows the file deleted that existed before it.
    deleted_old: HashSet<RecKey>,
    rows: BTreeMap<String, RowCounts>,
}

impl Tracker {
    fn new(store: &Store) -> Self {
        Self {
            snapshot: store.slot_counts(),
            before: HashMap::new(),
            order: Vec::new(),
            deleted_old: HashSet::new(),
            rows: BTreeMap::new(),
        }
    }

    fn old_slot(&self, table: usize, row: u32) -> bool {
        (row as usize) < self.snapshot.get(table).copied().unwrap_or(0)
    }

    fn absorb(&mut self, store: &mut Store) -> Result<(), String> {
        // Each statement's events, with `cause` indexing into them.
        let events: Vec<Event> = std::mem::take(&mut store.events);
        let mut keys: Vec<Option<RecKey>> = Vec::with_capacity(events.len());
        for e in &events {
            let name = store.table_name(e.table).to_ascii_lowercase();
            let c = self.rows.entry(name.clone()).or_default();
            match e.kind {
                EventKind::Insert => c.inserted += 1,
                EventKind::Delete => c.deleted += 1,
                EventKind::Updated => c.updated += 1,
                EventKind::UpdateOld => {}
            }
            let key = match e.cause {
                Some(cause) if needs_parent(&name) => keys[cause],
                _ => record_of(store, e.table, &name, &e.values)?,
            };
            keys.push(key);
            let Some(key) = key else { continue };
            if !self.before.contains_key(&key) {
                let existed = matches!(e.kind, EventKind::Delete | EventKind::UpdateOld)
                    || self.deleted_old.contains(&key)
                    || self.live_old_root(store, key)?;
                self.before.insert(key, existed);
                self.order.push(key);
            }
            if is_root(&name) && e.kind == EventKind::Delete && self.old_slot(e.table, e.row) {
                self.deleted_old.insert(key);
            }
        }
        Ok(())
    }

    /// Whether `key` still has a root row the file found.
    fn live_old_root(&self, store: &mut Store, key: RecKey) -> Result<bool, String> {
        Ok(root_rows(store, key)?
            .iter()
            .any(|&(t, r)| self.old_slot(t, r)))
    }

    fn finish(&self, store: &mut Store) -> Result<BTreeMap<&'static str, RecordCounts>, String> {
        let mut out: BTreeMap<&'static str, RecordCounts> = BTreeMap::new();
        for key in &self.order {
            let before = self.before[key];
            let after = !root_rows(store, *key)?.is_empty();
            let c = out.entry(key.0.name()).or_default();
            match (before, after) {
                (true, true) => c.replaced += 1,
                (true, false) => c.deleted += 1,
                (false, true) => c.added += 1,
                (false, false) => {}
            }
        }
        out.retain(|_, c| *c != RecordCounts::default());
        Ok(out)
    }
}

/// Child tables whose record is found through a parent that a cascade may already have removed.
fn needs_parent(table: &str) -> bool {
    matches!(
        table,
        "weenie_properties_emote_action" | "landblock_instance_link"
    ) || table.starts_with("recipe_mods_")
}

/// Tables whose rows are the records themselves (or a landblock's / cook book pair's rows).
fn is_root(table: &str) -> bool {
    matches!(
        table,
        "weenie"
            | "landblock_instance"
            | "encounter"
            | "cook_book"
            | "recipe"
            | "event"
            | "house_portal"
            | "points_of_interest"
            | "quest"
            | "spell"
            | "treasure_death"
            | "treasure_gem_count"
            | "treasure_material_base"
            | "treasure_material_color"
            | "treasure_material_groups"
            | "treasure_wielded"
            | "version"
    )
}

fn single_table(table: &str) -> Option<TableId> {
    Some(match table {
        "recipe" => TableId::RECIPE,
        "event" => TableId::EVENT,
        "house_portal" => TableId::HOUSE_PORTAL,
        "points_of_interest" => TableId::POINTS_OF_INTEREST,
        "quest" => TableId::QUEST,
        "spell" => TableId::SPELL,
        "treasure_death" => TableId::TREASURE_DEATH,
        "treasure_gem_count" => TableId::TREASURE_GEM_COUNT,
        "treasure_material_base" => TableId::TREASURE_MATERIAL_BASE,
        "treasure_material_color" => TableId::TREASURE_MATERIAL_COLOR,
        "treasure_material_groups" => TableId::TREASURE_MATERIAL_GROUPS,
        "treasure_wielded" => TableId::TREASURE_WIELDED,
        "version" => TableId::VERSION,
        _ => return None,
    })
}

fn int_of(store: &Store, t: usize, values: &[Val], column: &str) -> Result<Option<u64>, String> {
    let c = store.column(t, column)?;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    Ok(values[c].as_exact_int().map(|i| i as u64))
}

/// The pack record a row of `table` belongs to.
fn record_of(
    store: &mut Store,
    ti: usize,
    table: &str,
    values: &[Val],
) -> Result<Option<RecKey>, String> {
    let with = |store: &Store, t: TableId, col: &str| -> Result<Option<RecKey>, String> {
        Ok(int_of(store, ti, values, col)?.map(|k| (t, k)))
    };
    Ok(match table {
        "weenie" => with(store, TableId::WEENIE, "class_Id")?,
        "weenie_properties_emote_action" => {
            let Some(emote) = int_of(store, ti, values, "emote_Id")? else {
                return Ok(None);
            };
            parent_record(
                store,
                "weenie_properties_emote",
                "id",
                emote,
                TableId::WEENIE,
                "object_Id",
            )?
        }
        t if t.starts_with("weenie_properties_") => with(store, TableId::WEENIE, "object_Id")?,
        "landblock_instance" => with(store, TableId::LANDBLOCK_INSTANCE, "landblock")?,
        "landblock_instance_link" => {
            let Some(guid) = int_of(store, ti, values, "parent_GUID")? else {
                return Ok(None);
            };
            parent_record(
                store,
                "landblock_instance",
                "guid",
                guid,
                TableId::LANDBLOCK_INSTANCE,
                "landblock",
            )?
        }
        #[allow(clippy::cast_possible_truncation)]
        "encounter" => int_of(store, ti, values, "landblock")?
            .map(|k| (TableId::ENCOUNTER, u64::from(k as u32))),
        "cook_book" => {
            let src = int_of(store, ti, values, "source_W_C_I_D")?;
            let tgt = int_of(store, ti, values, "target_W_C_I_D")?;
            match (src, tgt) {
                (Some(s), Some(t)) => Some((TableId::COOK_BOOK, (s << 32) | (t & 0xFFFF_FFFF))),
                _ => None,
            }
        }
        "recipe_mod" => with(store, TableId::RECIPE, "recipe_Id")?,
        t if t.starts_with("recipe_requirements_") => with(store, TableId::RECIPE, "recipe_Id")?,
        t if t.starts_with("recipe_mods_") => {
            let Some(m) = int_of(store, ti, values, "recipe_Mod_Id")? else {
                return Ok(None);
            };
            parent_record(store, "recipe_mod", "id", m, TableId::RECIPE, "recipe_Id")?
        }
        other => match single_table(other) {
            Some(t) => with(store, t, "id")?,
            None => None,
        },
    })
}

fn parent_record(
    store: &mut Store,
    parent: &str,
    key_col: &str,
    key: u64,
    pack: TableId,
    col: &str,
) -> Result<Option<RecKey>, String> {
    let Ok(t) = store.table_index(parent) else {
        return Ok(None);
    };
    let rows = store.find(t, &[key_col], &[Val::Int(i128::from(key))])?;
    let Some(&r) = rows.first() else {
        return Ok(None);
    };
    let v = store.values(t, r)?;
    let c = store.column(t, col)?;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    Ok(v[c].as_exact_int().map(|i| (pack, i as u64)))
}

/// The live root rows of a record: `(table, slot)`.
pub(crate) fn root_rows(store: &mut Store, key: RecKey) -> Result<Vec<(usize, u32)>, String> {
    let (table, cols, vals): (&str, Vec<&str>, Vec<Val>) = match key.0 {
        TableId::WEENIE => (
            "weenie",
            vec!["class_Id"],
            vec![Val::Int(i128::from(key.1))],
        ),
        TableId::LANDBLOCK_INSTANCE => (
            "landblock_instance",
            vec!["landblock"],
            vec![Val::Int(i128::from(key.1))],
        ),
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        TableId::ENCOUNTER => (
            "encounter",
            vec!["landblock"],
            vec![Val::Int(i128::from(key.1 as u32 as i32))],
        ),
        TableId::COOK_BOOK => (
            "cook_book",
            vec!["source_W_C_I_D", "target_W_C_I_D"],
            vec![
                Val::Int(i128::from(key.1 >> 32)),
                Val::Int(i128::from(key.1 & 0xFFFF_FFFF)),
            ],
        ),
        other => {
            let name = TABLE_OF
                .iter()
                .find(|(t, _)| *t == other)
                .map(|(_, n)| *n)
                .ok_or("unknown record table")?;
            (name, vec!["id"], vec![Val::Int(i128::from(key.1))])
        }
    };
    let Ok(t) = store.table_index(table) else {
        return Ok(Vec::new());
    };
    Ok(store
        .find(t, &cols, &vals)?
        .into_iter()
        .map(|r| (t, r))
        .collect())
}

const TABLE_OF: &[(TableId, &str)] = &[
    (TableId::RECIPE, "recipe"),
    (TableId::EVENT, "event"),
    (TableId::HOUSE_PORTAL, "house_portal"),
    (TableId::POINTS_OF_INTEREST, "points_of_interest"),
    (TableId::QUEST, "quest"),
    (TableId::SPELL, "spell"),
    (TableId::TREASURE_DEATH, "treasure_death"),
    (TableId::TREASURE_GEM_COUNT, "treasure_gem_count"),
    (TableId::TREASURE_MATERIAL_BASE, "treasure_material_base"),
    (TableId::TREASURE_MATERIAL_COLOR, "treasure_material_color"),
    (
        TableId::TREASURE_MATERIAL_GROUPS,
        "treasure_material_groups",
    ),
    (TableId::TREASURE_WIELDED, "treasure_wielded"),
    (TableId::VERSION, "version"),
];

/// The dataset id of a patched build: the base dump's hash, the clock, and every applied file's
/// kind and hash, in order.
#[must_use]
pub fn dataset_id(base: &[u8; 32], now: &str, applied: &[Applied]) -> [u8; 16] {
    let mut h = blake3::Hasher::new();
    h.update(b"serv-import patched dataset v1\0");
    h.update(base);
    h.update(now.as_bytes());
    for a in applied {
        h.update(a.kind.name().as_bytes());
        h.update(&a.hash);
    }
    let mut id = [0u8; 16];
    id.copy_from_slice(&h.finalize().as_bytes()[..16]);
    id
}
