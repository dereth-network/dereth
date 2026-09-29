//! The overlay's writer: the base content loaded into the import SQL store, so that an overlay write
//! runs exactly as a content patch does in `empyrean-import` (MySQL semantics, cascades, keys,
//! `AUTO_INCREMENT`), and the records a write touched laid out again as pack records.
//!
//! Not ACE-derived. The store is loaded on the first write (base dump, then the base patches, then
//! the overlay's journal), never at startup, so a server that only reads its overlay pays nothing.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use empyrean_common::dotnet::DotNetDateTime;

use crate::error::ImportError;
use crate::import::patch::{self, Applied, Input, InputKind};
use crate::import::sql::store::Store;
use crate::import::sql::value::Val;
use crate::import::world_rows::WorldRows;
use crate::import::{load_store, Loaded};
use crate::pack::{Pack, TableId};

use super::file::JournalEntry;
use super::layer::Record;

/// The inputs `world.pack` was built from: `empyrean-import --sql <sql> [--patches|--json <input>]...`.
#[derive(Debug, Clone)]
pub struct BaseInputs {
    /// The ACE world-database dump.
    pub sql: PathBuf,
    /// Content inputs applied over it, in order (empty for a plain import).
    pub patches: Vec<Input>,
}

/// The dataset id `empyrean-import` gives a build of `loaded` (see `import::build`).
fn dataset_id_of(loaded: &Loaded) -> [u8; 16] {
    if loaded.applied.is_empty() {
        let mut id = [0u8; 16];
        id.copy_from_slice(&loaded.base_hash[..16]);
        id
    } else {
        patch::dataset_id(&loaded.base_hash, &loaded.now_text, &loaded.applied)
    }
}

/// The loaded store.
#[derive(Debug)]
pub struct Author {
    pub store: Store,
}

impl Author {
    /// Load the base inputs and replay `journal`. The base must be the one the running pack was
    /// built from: its dataset id is checked against `pack_dataset_id`.
    pub fn load(
        base: &BaseInputs,
        now: DotNetDateTime,
        pack_dataset_id: &[u8; 16],
        journal: &[JournalEntry],
    ) -> Result<Self, ImportError> {
        let file = std::fs::File::open(&base.sql).map_err(|e| ImportError::io(&base.sql, e))?;
        let loaded = load_store(file, patch::sources(&base.patches), now)?;
        let id = dataset_id_of(&loaded);
        if &id != pack_dataset_id {
            return Err(ImportError::Sql {
                line: 0,
                what: format!(
                    "the base inputs ({} and {} patch input(s)) build dataset {}, but the running world.pack is dataset {}: \
                     point the overlay's base at the dump (and patches) world.pack was built from",
                    base.sql.display(),
                    base.patches.len(),
                    crate::pack::hex(&id),
                    crate::pack::hex(pack_dataset_id)
                ),
            });
        }
        let mut store = loaded.store;
        for e in journal {
            patch::apply_sql(&mut store, &e.sql, &e.path, InputKind::Sql, e.hash)?;
        }
        Ok(Self { store })
    }

    /// Apply one file of SQL. On an error the store may hold part of the file: the caller drops
    /// it and reloads.
    pub fn apply(&mut self, sql: &[u8], shown: &str) -> Result<Applied, ImportError> {
        patch::apply_sql(
            &mut self.store,
            sql,
            shown,
            InputKind::Sql,
            *blake3::hash(sql).as_bytes(),
        )
    }

    /// The pack records `keys` now are, laid out as `empyrean-import` would lay them out: `(table,
    /// key, bytes)`, `None` for a record that no longer exists. A weenie brings its
    /// `weenie_index` record along.
    pub fn materialize(&mut self, keys: &[(TableId, u64)]) -> Result<Vec<Record>, ImportError> {
        let fail = |what: String| ImportError::Sql { line: 0, what };
        let mut selected: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
        for &key in keys {
            rows_of_record(&mut self.store, key, &mut selected).map_err(fail)?;
        }
        let mut rows = WorldRows::default();
        self.store.emit_rows(&mut rows, &selected)?;
        let (content, _orphans) = rows.assemble();
        let (bytes, _) = content.to_pack([0; 16])?;
        let pack = Pack::from_bytes(bytes)?;
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for &(t, k) in keys {
            let mut put = |t: TableId| -> Result<(), ImportError> {
                if seen.insert((t, k)) {
                    out.push((t, k, pack.raw(t, k)?.map(Arc::from)));
                }
                Ok(())
            };
            put(t)?;
            if t == TableId::WEENIE {
                put(TableId::WEENIE_INDEX)?;
            }
        }
        Ok(out)
    }
}

fn int(k: u64) -> Val {
    Val::Int(i128::from(k))
}

fn add(selected: &mut BTreeMap<usize, Vec<u32>>, t: usize, rows: Vec<u32>) {
    selected.entry(t).or_default().extend(rows);
}

/// Values of column `col` of `rows` of table `t`, as record keys.
fn column_values(store: &Store, t: usize, rows: &[u32], col: &str) -> Result<Vec<u64>, String> {
    let c = store.column(t, col)?;
    let mut out = Vec::new();
    for &r in rows {
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        if let Some(v) = store.values(t, r)?[c].as_exact_int() {
            out.push(v as u64);
        }
    }
    Ok(out)
}

/// Every live row of every table that makes up record `key`.
fn rows_of_record(
    store: &mut Store,
    key: (TableId, u64),
    selected: &mut BTreeMap<usize, Vec<u32>>,
) -> Result<(), String> {
    for (t, r) in patch::root_rows(store, key)? {
        selected.entry(t).or_default().push(r);
    }
    let tables: Vec<(usize, String)> = store
        .table_indices()
        .into_iter()
        .map(|i| (i, store.table_name(i).to_ascii_lowercase()))
        .collect();
    let find = |store: &mut Store,
                name: &str,
                col: &str,
                k: u64|
     -> Result<Option<(usize, Vec<u32>)>, String> {
        let Ok(t) = store.table_index(name) else {
            return Ok(None);
        };
        Ok(Some((t, store.find(t, &[col], &[int(k)])?)))
    };
    match key.0 {
        TableId::WEENIE => {
            for (t, name) in &tables {
                if name.starts_with("weenie_properties_")
                    && name != "weenie_properties_emote_action"
                {
                    let rows = store.find(*t, &["object_Id"], &[int(key.1)])?;
                    if name == "weenie_properties_emote" {
                        for emote in column_values(store, *t, &rows, "id")? {
                            if let Some((a, rows)) =
                                find(store, "weenie_properties_emote_action", "emote_Id", emote)?
                            {
                                add(selected, a, rows);
                            }
                        }
                    }
                    add(selected, *t, rows);
                }
            }
        }
        TableId::LANDBLOCK_INSTANCE => {
            let Ok(t) = store.table_index("landblock_instance") else {
                return Ok(());
            };
            let roots = selected.get(&t).cloned().unwrap_or_default();
            for guid in column_values(store, t, &roots, "guid")? {
                if let Some((l, rows)) =
                    find(store, "landblock_instance_link", "parent_GUID", guid)?
                {
                    add(selected, l, rows);
                }
            }
        }
        TableId::RECIPE => {
            for (t, name) in &tables {
                if name.starts_with("recipe_requirements_") {
                    let rows = store.find(*t, &["recipe_Id"], &[int(key.1)])?;
                    add(selected, *t, rows);
                }
            }
            if let Some((m, mods)) = find(store, "recipe_mod", "recipe_Id", key.1)? {
                for id in column_values(store, m, &mods, "id")? {
                    for (t, name) in &tables {
                        if name.starts_with("recipe_mods_") {
                            let rows = store.find(*t, &["recipe_Mod_Id"], &[int(id)])?;
                            add(selected, *t, rows);
                        }
                    }
                }
                add(selected, m, mods);
            }
        }
        _ => {}
    }
    Ok(())
}
