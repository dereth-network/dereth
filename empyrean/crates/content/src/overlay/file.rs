//! The overlay file: one SQLite database beside `world.pack`.
//!
//! Not ACE-derived. Three tables:
//!
//! | table | contents |
//! |---|---|
//! | `meta` | `format`, the base pack's `content_hash` and `dataset_id` (hex), and `now` (the clock that stands in for `CURRENT_TIMESTAMP`) |
//! | `journal` | every content file applied, in order: `seq`, the path it was applied from, its SQL text, its BLAKE3 |
//! | `records` | each pack record the journal changed: `(tbl, key)` and its record bytes, `NULL` when deleted |
//!
//! The journal is what makes the overlay reproducible: replayed over the base inputs it gives the
//! records back, and `publish` bakes base inputs plus journal into a new pack exactly as
//! `empyrean-import` would. The records table lets a restart read the overlay without replaying it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};

use super::layer::{Layer, Record};
use super::OverlayError;
use crate::pack::TableId;

/// The `format` meta value this build writes and reads.
pub const FORMAT: &str = "dereth world overlay v1";

/// One applied content file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    pub seq: i64,
    /// The file it was applied from, as shown in messages.
    pub path: String,
    /// The SQL text as applied (the file's bytes).
    pub sql: Vec<u8>,
    /// BLAKE3 of `sql`.
    pub hash: [u8; 32],
}

/// What the overlay was made over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub content_hash: String,
    pub dataset_id: String,
    pub now: String,
}

/// An open overlay file.
#[derive(Debug)]
pub struct OverlayFile {
    conn: Connection,
    path: PathBuf,
}

fn sql_err(e: rusqlite::Error) -> OverlayError {
    OverlayError::Sqlite(e.to_string())
}

/// SQLite stores `INTEGER` as `i64`; a pack key is a `u64`, kept bit for bit.
#[allow(clippy::cast_possible_wrap)]
fn key_in(k: u64) -> i64 {
    k as i64
}

#[allow(clippy::cast_sign_loss)]
fn key_out(k: i64) -> u64 {
    k as u64
}

impl OverlayFile {
    /// Open the file, creating it (and its tables) when it does not exist.
    pub fn open(path: &Path) -> Result<Self, OverlayError> {
        let conn = Connection::open(path).map_err(sql_err)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS journal (seq INTEGER PRIMARY KEY, path TEXT NOT NULL, sql BLOB NOT NULL, hash BLOB NOT NULL);
             CREATE TABLE IF NOT EXISTS records (tbl INTEGER NOT NULL, key INTEGER NOT NULL, data BLOB, PRIMARY KEY (tbl, key));",
        )
        .map_err(sql_err)?;
        Ok(Self {
            conn,
            path: path.to_owned(),
        })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn meta_value(&self, key: &str) -> Result<Option<String>, OverlayError> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(sql_err)
    }

    /// The meta values, or `None` for a new (empty) file.
    pub fn meta(&self) -> Result<Option<Meta>, OverlayError> {
        let Some(format) = self.meta_value("format")? else {
            return Ok(None);
        };
        if format != FORMAT {
            return Err(OverlayError::Mismatch(format!(
                "{} is a `{format}` file; this build reads `{FORMAT}`",
                self.path.display()
            )));
        }
        let get = |k: &str| -> Result<String, OverlayError> {
            self.meta_value(k)?.ok_or_else(|| {
                OverlayError::Mismatch(format!("{}: meta `{k}` is missing", self.path.display()))
            })
        };
        Ok(Some(Meta {
            content_hash: get("content_hash")?,
            dataset_id: get("dataset_id")?,
            now: get("now")?,
        }))
    }

    /// Write the meta values of a new file.
    pub fn init(&mut self, meta: &Meta) -> Result<(), OverlayError> {
        let tx = self.conn.transaction().map_err(sql_err)?;
        for (k, v) in [
            ("format", FORMAT),
            ("content_hash", meta.content_hash.as_str()),
            ("dataset_id", meta.dataset_id.as_str()),
            ("now", meta.now.as_str()),
        ] {
            tx.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
                params![k, v],
            )
            .map_err(sql_err)?;
        }
        tx.commit().map_err(sql_err)
    }

    /// Every journal entry, in order.
    pub fn journal(&self) -> Result<Vec<JournalEntry>, OverlayError> {
        let mut st = self
            .conn
            .prepare("SELECT seq, path, sql, hash FROM journal ORDER BY seq")
            .map_err(sql_err)?;
        let rows = st
            .query_map([], |r| {
                let hash: Vec<u8> = r.get(3)?;
                Ok(JournalEntry {
                    seq: r.get(0)?,
                    path: r.get(1)?,
                    sql: r.get(2)?,
                    hash: hash.try_into().unwrap_or([0; 32]),
                })
            })
            .map_err(sql_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(sql_err)
    }

    /// Every record override.
    pub fn layer(&self) -> Result<Layer, OverlayError> {
        let mut st = self
            .conn
            .prepare("SELECT tbl, key, data FROM records")
            .map_err(sql_err)?;
        let rows = st
            .query_map([], |r| {
                let t: u16 = r.get(0)?;
                let k: i64 = r.get(1)?;
                let d: Option<Vec<u8>> = r.get(2)?;
                Ok((t, k, d))
            })
            .map_err(sql_err)?;
        let mut layer = Layer::default();
        for row in rows {
            let (t, k, d) = row.map_err(sql_err)?;
            layer.put(TableId(t), key_out(k), d.map(Arc::from));
        }
        Ok(layer)
    }

    /// Append one journal entry and set its records, in one transaction.
    pub fn commit(
        &mut self,
        path: &str,
        sql: &[u8],
        hash: &[u8; 32],
        records: &[Record],
    ) -> Result<i64, OverlayError> {
        let tx = self.conn.transaction().map_err(sql_err)?;
        tx.execute(
            "INSERT INTO journal (path, sql, hash) VALUES (?1, ?2, ?3)",
            params![path, sql, &hash[..]],
        )
        .map_err(sql_err)?;
        let seq = tx.last_insert_rowid();
        {
            let mut st = tx
                .prepare("INSERT OR REPLACE INTO records (tbl, key, data) VALUES (?1, ?2, ?3)")
                .map_err(sql_err)?;
            for (t, k, d) in records {
                st.execute(params![t.0, key_in(*k), d.as_deref()])
                    .map_err(sql_err)?;
            }
        }
        tx.commit().map_err(sql_err)?;
        Ok(seq)
    }
}
