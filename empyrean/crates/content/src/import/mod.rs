//! The importer: ACE's `mysqldump` world database to a `world.pack` (carried from v1's
//! `empyrean-import`, retargeted to the ACE World-DB models, with every dat-derived table dropped).
//!
//! `dump -> WorldRows -> WorldContent -> pack`. [`WorldContent`] is also what
//! [`crate::MemContent`] builds by hand, so tests and production share the pack path.

pub mod check;
pub mod json;
pub mod mysqldump;
pub mod patch;
pub mod sql;
pub mod world_rows;

use std::collections::BTreeMap;
use std::io::Read;

use empyrean_common::dotnet::DotNetDateTime;

use crate::error::ImportError;
use crate::models::world::*;
use crate::pack::{BuildStats, PackWriter, TableId};
use crate::records::WeenieIndex;
use world_rows::{Orphans, WorldRows};

/// Bumped whenever the importer's output changes for the same dump.
pub const IMPORTER_VERSION: u32 = 100;

/// Every world table, each child row inside its parent.
#[derive(Debug, Clone, Default)]
pub struct WorldContent {
    pub weenies: Vec<Weenie>,
    /// `landblock` is set (`obj_Cell_Id >> 16`, as MySQL generates it) and links are embedded.
    pub landblock_instances: Vec<LandblockInstance>,
    pub encounters: Vec<Encounter>,
    /// `recipe` is `None`: the recipe is its own table, joined at query time as EF's `Include` does.
    pub cook_books: Vec<CookBook>,
    pub recipes: Vec<Recipe>,
    pub events: Vec<Event>,
    pub house_portals: Vec<HousePortal>,
    pub points_of_interest: Vec<PointsOfInterest>,
    pub quests: Vec<Quest>,
    pub spells: Vec<Spell>,
    pub treasure_death: Vec<TreasureDeath>,
    pub treasure_gem_count: Vec<TreasureGemCount>,
    pub treasure_material_base: Vec<TreasureMaterialBase>,
    pub treasure_material_color: Vec<TreasureMaterialColor>,
    pub treasure_material_groups: Vec<TreasureMaterialGroups>,
    pub treasure_wielded: Vec<TreasureWielded>,
    pub versions: Vec<Version>,
}

/// `PropertyString.Name` and `PropertyDataId.Spell`, as the ids the dump stores.
const PROPERTY_STRING_NAME: u16 = 1;
const PROPERTY_DATA_ID_SPELL: u16 = 28;

fn index_of(w: &Weenie) -> WeenieIndex {
    WeenieIndex {
        class_name: w.class_name.clone(),
        r#type: w.r#type,
        name: w
            .weenie_properties_string
            .iter()
            .find(|p| p.r#type == PROPERTY_STRING_NAME)
            .map(|p| p.value.clone()),
        spell_did: w
            .weenie_properties_did
            .iter()
            .find(|p| p.r#type == PROPERTY_DATA_ID_SPELL)
            .map(|p| p.value),
    }
}

/// Group rows by a key, keeping input order within each group.
fn grouped<T: Clone>(rows: &[T], key: impl Fn(&T) -> u64) -> BTreeMap<u64, Vec<T>> {
    let mut m: BTreeMap<u64, Vec<T>> = BTreeMap::new();
    for r in rows {
        m.entry(key(r)).or_default().push(r.clone());
    }
    m
}

impl WorldContent {
    /// Lay the content out as a pack in memory. The table layout is [`TableId`]'s documentation.
    pub fn to_pack(&self, dataset_id: [u8; 16]) -> Result<(Vec<u8>, BuildStats), ImportError> {
        let mut w = PackWriter::new(dataset_id, IMPORTER_VERSION);
        for &t in TableId::ALL {
            w.declare(t);
        }
        for x in &self.weenies {
            w.add(TableId::WEENIE, u64::from(x.class_id), x)?;
            w.add(TableId::WEENIE_INDEX, u64::from(x.class_id), &index_of(x))?;
        }
        let by_lb = grouped(&self.landblock_instances, |i| {
            u64::from(i.obj_cell_id >> 16)
        });
        for (k, v) in &by_lb {
            let v: Vec<LandblockInstance> = v
                .iter()
                .map(|i| {
                    let mut i = i.clone();
                    // The generated column: `obj_Cell_Id >> 16`, at most 0xFFFF, so it fits.
                    i.landblock = Some(i32::try_from(i.obj_cell_id >> 16).expect("16 bits"));
                    i
                })
                .collect();
            w.add(TableId::LANDBLOCK_INSTANCE, *k, &v)?;
        }
        #[allow(clippy::cast_sign_loss)]
        for (k, v) in &grouped(&self.encounters, |e| u64::from(e.landblock as u32)) {
            w.add(TableId::ENCOUNTER, *k, v)?;
        }
        for (k, v) in &grouped(&self.cook_books, |c| {
            (u64::from(c.source_wcid) << 32) | u64::from(c.target_wcid)
        }) {
            w.add(TableId::COOK_BOOK, *k, v)?;
        }
        macro_rules! by_id {
            ($rows:expr, $table:expr) => {
                for r in &$rows {
                    w.add($table, u64::from(r.id), r)?;
                }
            };
        }
        by_id!(self.recipes, TableId::RECIPE);
        by_id!(self.events, TableId::EVENT);
        by_id!(self.house_portals, TableId::HOUSE_PORTAL);
        by_id!(self.points_of_interest, TableId::POINTS_OF_INTEREST);
        by_id!(self.quests, TableId::QUEST);
        by_id!(self.spells, TableId::SPELL);
        by_id!(self.treasure_death, TableId::TREASURE_DEATH);
        by_id!(self.treasure_gem_count, TableId::TREASURE_GEM_COUNT);
        by_id!(self.treasure_material_base, TableId::TREASURE_MATERIAL_BASE);
        by_id!(
            self.treasure_material_color,
            TableId::TREASURE_MATERIAL_COLOR
        );
        by_id!(
            self.treasure_material_groups,
            TableId::TREASURE_MATERIAL_GROUPS
        );
        by_id!(self.treasure_wielded, TableId::TREASURE_WIELDED);
        by_id!(self.versions, TableId::VERSION);
        w.finish()
    }
}

/// A `Read` that hashes everything read through it: the dataset id is the dump's own hash, taken
/// in the same pass that parses it.
struct Hashing<R> {
    inner: R,
    hasher: blake3::Hasher,
    bytes: u64,
}

impl<R: Read> Read for Hashing<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        self.bytes += n as u64;
        Ok(n)
    }
}

/// What one import produced, besides the pack bytes.
#[derive(Debug, Clone)]
pub struct Imported {
    pub stats: BuildStats,
    /// Rows read, per dump table, in model order.
    pub rows: Vec<(&'static str, u64)>,
    pub orphans: Orphans,
    pub unknown_tables: BTreeMap<String, u64>,
    pub unread_columns: Vec<(String, String)>,
    /// World tables the dump never declared.
    pub missing_tables: Vec<&'static str>,
    /// Statements of the base dump the reader could not read, by kind.
    pub skipped_statements: BTreeMap<String, mysqldump::Skipped>,
    pub dump_bytes: u64,
    /// Every content file applied over the base, in order (empty for a plain import).
    pub applied: Vec<patch::Applied>,
    /// The clock that stood in for `CURRENT_TIMESTAMP` / `DateTime.UtcNow` (patched builds only).
    pub now: Option<String>,
}

/// Import a dump from any reader into pack bytes.
pub fn import<R: Read>(dump: R) -> Result<(Vec<u8>, Imported), ImportError> {
    let mut hashing = Hashing {
        inner: dump,
        hasher: blake3::Hasher::new(),
        bytes: 0,
    };
    let mut rows = WorldRows::default();
    let scanned = {
        let reader = std::io::BufReader::with_capacity(8 << 20, &mut hashing);
        mysqldump::scan(reader, &mut rows)?
    };
    // Drain anything BufReader left unread (nothing, after EOF), so the hash covers the file.
    std::io::copy(&mut hashing, &mut std::io::sink()).map_err(|e| ImportError::Sql {
        line: 0,
        what: e.to_string(),
    })?;
    let mut dataset_id = [0u8; 16];
    dataset_id.copy_from_slice(&hashing.hasher.finalize().as_bytes()[..16]);

    let missing_tables: Vec<&'static str> = world_rows::WORLD_TABLES
        .iter()
        .copied()
        .filter(|t| !rows.declared.iter().any(|d| d == t))
        .collect();
    let counts = rows.counts();
    let unknown_tables = std::mem::take(&mut rows.unknown_tables);
    let unread_columns = std::mem::take(&mut rows.unread_columns);
    let (content, orphans) = rows.assemble();
    let (bytes, stats) = content.to_pack(dataset_id)?;
    Ok((
        bytes,
        Imported {
            stats,
            rows: counts,
            orphans,
            unknown_tables,
            unread_columns,
            missing_tables,
            skipped_statements: scanned.skipped,
            dump_bytes: hashing.bytes,
            applied: Vec::new(),
            now: None,
        },
    ))
}

/// A build: the base dump, then content inputs in order.
#[derive(Debug, Clone)]
pub struct Build {
    pub sql: std::path::PathBuf,
    pub inputs: Vec<patch::Input>,
    /// Stands in for `CURRENT_TIMESTAMP` (a column default, `ON UPDATE`) and for the
    /// `DateTime.UtcNow` ACE's JSON import stamps, so a build is reproducible.
    pub now: DotNetDateTime,
}

/// The default clock for a patched build: 2000-01-01 00:00:00.
#[must_use]
pub fn default_now() -> DotNetDateTime {
    DotNetDateTime::new_hms(2000, 1, 1, 0, 0, 0)
}

/// Build pack bytes from a base dump and content inputs. Without inputs this is exactly
/// [`import`] of the dump.
pub fn build(b: &Build) -> Result<(Vec<u8>, Imported), ImportError> {
    let file = std::fs::File::open(&b.sql).map_err(|e| ImportError::io(&b.sql, e))?;
    if b.inputs.is_empty() {
        return import(file);
    }
    build_from(file, patch::sources(&b.inputs), b.now)
}

/// Build pack bytes from a base dump read from `dump` and content files applied in order.
pub fn build_from<R: Read>(
    dump: R,
    sources: impl IntoIterator<Item = Result<patch::Source, ImportError>>,
    now: DotNetDateTime,
) -> Result<(Vec<u8>, Imported), ImportError> {
    let Loaded {
        store,
        base_hash,
        dump_bytes,
        applied,
        now_text,
        skipped_statements,
    } = load_store(dump, sources, now)?;
    let dataset_id = patch::dataset_id(&base_hash, &now_text, &applied);
    let mut rows = WorldRows::default();
    store.emit(&mut rows)?;
    let missing_tables: Vec<&'static str> = world_rows::WORLD_TABLES
        .iter()
        .copied()
        .filter(|t| !rows.declared.iter().any(|d| d == t))
        .collect();
    let counts = rows.counts();
    let unknown_tables = std::mem::take(&mut rows.unknown_tables);
    let unread_columns = std::mem::take(&mut rows.unread_columns);
    let (content, orphans) = rows.assemble();
    let (bytes, stats) = content.to_pack(dataset_id)?;
    Ok((
        bytes,
        Imported {
            stats,
            rows: counts,
            orphans,
            unknown_tables,
            unread_columns,
            missing_tables,
            skipped_statements,
            dump_bytes,
            applied,
            now: Some(now_text),
        },
    ))
}

/// The SQL store a patched build runs on, before it is laid out as a pack.
#[derive(Debug)]
pub struct Loaded {
    pub store: sql::store::Store,
    /// BLAKE3 of the dump's bytes.
    pub base_hash: [u8; 32],
    pub dump_bytes: u64,
    /// What each content file did, in order.
    pub applied: Vec<patch::Applied>,
    /// `now` as `CURRENT_TIMESTAMP` text.
    pub now_text: String,
    /// Statements of the base dump the reader could not read, by kind.
    pub skipped_statements: BTreeMap<String, mysqldump::Skipped>,
}

/// Load a base dump into the SQL store and apply content files in order: [`build_from`] up to
/// the pack layout (the content overlay keeps the store to apply more files later).
pub fn load_store<R: Read>(
    dump: R,
    sources: impl IntoIterator<Item = Result<patch::Source, ImportError>>,
    now: DotNetDateTime,
) -> Result<Loaded, ImportError> {
    let now_text = patch::datetime_text(now);
    let mut store = sql::store::Store::new(now_text.clone());
    let mut hashing = Hashing {
        inner: dump,
        hasher: blake3::Hasher::new(),
        bytes: 0,
    };
    let scanned = {
        let reader = std::io::BufReader::with_capacity(8 << 20, &mut hashing);
        mysqldump::scan(reader, &mut sql::store::Loader::new(&mut store))?
    };
    std::io::copy(&mut hashing, &mut std::io::sink()).map_err(|e| ImportError::Sql {
        line: 0,
        what: e.to_string(),
    })?;
    let base_hash = *hashing.hasher.finalize().as_bytes();
    let mut applied = Vec::new();
    for src in sources {
        applied.push(patch::apply_source(&mut store, &src?, now)?);
    }
    Ok(Loaded {
        store,
        base_hash,
        dump_bytes: hashing.bytes,
        applied,
        now_text,
        skipped_statements: scanned.skipped,
    })
}

/// What an import may leave unread and still write a pack. Nothing, by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Allow {
    /// Statements the reader cannot read ([`Imported::skipped_statements`]).
    pub skipped_statements: bool,
    /// Tables that are not ACE world tables ([`Imported::unknown_tables`]).
    pub unknown_tables: bool,
    /// Columns of world tables that no model reads ([`Imported::unread_columns`]).
    pub unknown_columns: bool,
}

/// The world tables a pack is refused without rows in, whatever is allowed: every object's
/// definition, and the objects placed in the world.
pub const REQUIRED_TABLES: &[&str] = &["weenie", "landblock_instance"];

impl Imported {
    /// Why this import must not become a pack under `allow`, one line per reason; empty when it
    /// may. No rows at all, or none in a [`REQUIRED_TABLES`] table, is never allowed.
    #[must_use]
    pub fn problems(&self, allow: Allow) -> Vec<String> {
        let mut out = Vec::new();
        let rows = |t: &str| {
            self.rows
                .iter()
                .find(|(n, _)| *n == t)
                .map_or(0, |(_, c)| *c)
        };
        let total: u64 = self.rows.iter().map(|(_, n)| n).sum();
        if total == 0 {
            out.push(
                "the dump gave no rows for any world table (is it a MySQL dump of ACE's world database?)"
                    .to_owned(),
            );
        } else {
            for t in REQUIRED_TABLES {
                if rows(t) == 0 {
                    out.push(format!("table `{t}` has no rows"));
                }
            }
        }
        if !allow.skipped_statements && !self.skipped_statements.is_empty() {
            let n: u64 = self.skipped_statements.values().map(|k| k.count).sum();
            let kinds: Vec<String> = self
                .skipped_statements
                .iter()
                .map(|(k, s)| format!("{k} x{} (first at line {})", s.count, s.first_line))
                .collect();
            out.push(format!(
                "{n} statements could not be read: {}",
                kinds.join(", ")
            ));
        }
        if !allow.unknown_tables && !self.unknown_tables.is_empty() {
            let names: Vec<String> = self
                .unknown_tables
                .iter()
                .map(|(t, n)| format!("`{t}` ({n} rows)"))
                .collect();
            out.push(format!(
                "{} tables are not ACE world tables: {}",
                names.len(),
                names.join(", ")
            ));
        }
        if !allow.unknown_columns && !self.unread_columns.is_empty() {
            let names: Vec<String> = self
                .unread_columns
                .iter()
                .map(|(t, c)| format!("`{t}.{c}` ({} rows)", rows(t)))
                .collect();
            out.push(format!(
                "{} columns are not in ACE's world schema: {}",
                names.len(),
                names.join(", ")
            ));
        }
        out
    }

    /// [`Imported::problems`] as an error.
    pub fn verify(&self, allow: Allow) -> Result<(), ImportError> {
        let problems = self.problems(allow);
        if problems.is_empty() {
            Ok(())
        } else {
            Err(ImportError::Refused(problems))
        }
    }
}

/// [`build`], then write `out` and its report (see [`import_file`]).
pub fn build_file(
    b: &Build,
    allow: Allow,
    out: &std::path::Path,
    report: Option<&std::path::Path>,
) -> Result<Imported, ImportError> {
    let (bytes, imported) = build(b)?;
    imported.verify(allow)?;
    write_pack_and_report(&bytes, &imported, out, report)?;
    Ok(imported)
}

/// Import a dump file and write `out` and its report in one step. The report path defaults to
/// `<out>.report.json`; it is always written, so it can never describe a different pack. An import
/// [`Imported::verify`] refuses under `allow` writes neither.
pub fn import_file(
    sql: &std::path::Path,
    allow: Allow,
    out: &std::path::Path,
    report: Option<&std::path::Path>,
) -> Result<Imported, ImportError> {
    let file = std::fs::File::open(sql).map_err(|e| ImportError::io(sql, e))?;
    let (bytes, imported) = import(file)?;
    imported.verify(allow)?;
    write_pack_and_report(&bytes, &imported, out, report)?;
    Ok(imported)
}

pub fn write_pack_and_report(
    bytes: &[u8],
    imported: &Imported,
    out: &std::path::Path,
    report: Option<&std::path::Path>,
) -> Result<(), ImportError> {
    // Refuse to write a pack the reader would not open.
    crate::pack::Pack::from_bytes(bytes.to_vec())?.verify_hash()?;
    crate::pack::write_atomically(out, bytes)?;
    let default_report = {
        let mut p = out.as_os_str().to_owned();
        p.push(".report.json");
        std::path::PathBuf::from(p)
    };
    let report = report.unwrap_or(&default_report);
    let text = serde_json::to_string_pretty(&imported.report_json()).expect("json");
    crate::pack::write_atomically(
        report,
        format!(
            "{text}
"
        )
        .as_bytes(),
    )?;
    Ok(())
}

impl Imported {
    /// The report written next to the pack in the same step: rows per dump table, records per pack
    /// table, and the content hash.
    #[must_use]
    pub fn report_json(&self) -> serde_json::Value {
        use serde_json::json;
        let rows: serde_json::Map<String, serde_json::Value> = self
            .rows
            .iter()
            .map(|(t, n)| ((*t).to_owned(), json!(n)))
            .collect();
        let tables: Vec<serde_json::Value> = self
            .stats
            .tables
            .iter()
            .map(|(name, count, bytes)| json!({ "table": name, "records": count, "bytes": bytes }))
            .collect();
        let mut report = json!({
            "format": "world.pack report v1",
            "importer_version": IMPORTER_VERSION,
            "schema_version": crate::pack::format::SCHEMA_VERSION,
            "content_hash": crate::pack::hex(&self.stats.content_hash),
            "dataset_id": crate::pack::hex(&self.stats.dataset_id),
            "dump_bytes": self.dump_bytes,
            "pack_bytes": self.stats.file_len,
            "index_entries": self.stats.index_count,
            "deduplicated_records": self.stats.deduplicated_records,
            "dump_rows": rows,
            "pack_tables": tables,
            "orphans": self.orphans,
            "unknown_tables": self.unknown_tables,
            "unread_columns": self.unread_columns.iter().map(|(t, c)| format!("{t}.{c}")).collect::<Vec<_>>(),
            "missing_tables": self.missing_tables,
            "skipped_statements": self
                .skipped_statements
                .iter()
                .map(|(k, s)| (k.clone(), json!({ "count": s.count, "first_line": s.first_line })))
                .collect::<serde_json::Map<String, serde_json::Value>>(),
        });
        if let Some(now) = &self.now {
            // A patched build: the clock and what every input file did, in order.
            report["format"] = json!("world.pack report v2");
            report["now"] = json!(now);
            report["inputs"] = json!(self
                .applied
                .iter()
                .map(patch::Applied::to_json)
                .collect::<Vec<_>>());
        }
        report
    }
}
