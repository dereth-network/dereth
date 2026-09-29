//! Not ACE: how a database file reaches the schema this build writes. ACE's MySQL schemas are
//! updated by SQL scripts the operator runs (`AutoApplyDatabaseUpdates`); Empyrean's SQLite files
//! upgrade themselves when they are opened.
//!
//! A schema is an ordered list of migrations; the database records how many have run (the shard in
//! `PRAGMA user_version`, the authentication tables in a table of their own, so both can share a
//! file). Opening a database:
//!
//! 1. reads its version. A version above this build's migration count means a newer release wrote
//!    the file: the open is refused ([`StoreError::NewerSchema`]), naming both versions, the newest
//!    backup and the release to use, and nothing is touched;
//! 2. with nothing pending, does nothing more;
//! 3. with migrations pending on a schema that already exists (version above 0), first copies the
//!    file beside itself with SQLite's online backup, as
//!    `<file>.backup-<schema>-v<from>-v<to>-<UTC timestamp>` ([`empyrean_common::backups`]). A backup
//!    that fails stops the open before any migration runs ([`StoreError::Backup`]). A schema at
//!    version 0 has nothing to preserve (its first migration creates its tables), so it is not
//!    backed up;
//! 4. applies each pending migration in a transaction of its own, recording the new version in the
//!    same transaction;
//! 5. keeps the newest [`KEEP_BACKUPS`] backups of the file and removes older ones.

use empyrean_common::backups;
use empyrean_common::clock::{Clock, SystemClock};
use rusqlite::{Connection, DatabaseName, OptionalExtension};
use std::path::{Path, PathBuf};

use crate::error::StoreError;

pub use empyrean_common::backups::KEEP_BACKUPS;

/// Where a schema records how many of its migrations have run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Versioning {
    /// `PRAGMA user_version` (the file's own version field).
    UserVersion,
    /// The largest `version` in the named table (created with the first migration).
    Table(&'static str),
}

/// A schema: its migrations, in order, and where its version is kept.
#[derive(Debug, Clone, Copy)]
pub struct Schema {
    /// What messages call it (`shard`, `authentication`).
    pub name: &'static str,
    /// Its part of a backup's name (`shard`, `auth`).
    pub label: &'static str,
    /// The migrations; schema version `n` is the first `n` applied.
    pub migrations: &'static [&'static str],
    /// Where the version is recorded.
    pub versioning: Versioning,
}

impl Schema {
    /// The version this build brings a database to.
    #[must_use]
    pub fn current(&self) -> i64 {
        i64::try_from(self.migrations.len()).unwrap_or(i64::MAX)
    }
}

/// How an upgrade keeps its backups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradePolicy {
    /// The folder backups go in; `None` puts them beside the database file.
    pub backup_dir: Option<PathBuf>,
    /// How many backups of the file are kept (the newest).
    pub keep_backups: usize,
}

impl Default for UpgradePolicy {
    fn default() -> Self {
        Self {
            backup_dir: None,
            keep_backups: KEEP_BACKUPS,
        }
    }
}

/// What opening a database did to its schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upgraded {
    /// The version found.
    pub from: i64,
    /// The version it is at now.
    pub to: i64,
    /// The backup made first, when one was.
    pub backup: Option<PathBuf>,
}

/// The version `schema` records in `conn`.
///
/// # Errors
/// When SQLite cannot read it.
pub fn version(conn: &Connection, schema: &Schema) -> Result<i64, StoreError> {
    Ok(match schema.versioning {
        Versioning::UserVersion => conn.pragma_query_value(None, "user_version", |r| r.get(0))?,
        Versioning::Table(table) => {
            let exists = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |_| Ok(()),
                )
                .optional()?
                .is_some();
            if exists {
                conn.query_row(&format!("SELECT MAX(version) FROM {table}"), [], |r| {
                    r.get::<_, Option<i64>>(0)
                })?
                .unwrap_or(0)
            } else {
                0
            }
        }
    })
}

/// The version `schema` records in `conn`, refused when it is newer than this build's.
fn upgrade_check(
    conn: &Connection,
    file: Option<&Path>,
    schema: &Schema,
    policy: &UpgradePolicy,
) -> Result<i64, StoreError> {
    let found = version(conn, schema)?;
    let known = schema.current();
    if found > known {
        return Err(StoreError::NewerSchema {
            database: schema.name,
            path: file.map(Path::to_path_buf),
            found,
            known,
            backup: file.and_then(|f| backups::newest(f, policy.backup_dir.as_deref())),
        });
    }
    Ok(found)
}

fn record_version(conn: &Connection, schema: &Schema, v: i64) -> rusqlite::Result<()> {
    match schema.versioning {
        Versioning::UserVersion => conn.pragma_update(None, "user_version", v),
        Versioning::Table(table) => {
            conn.execute_batch(&format!(
                "CREATE TABLE IF NOT EXISTS {table} (version INTEGER NOT NULL)"
            ))?;
            conn.execute(&format!("INSERT INTO {table} (version) VALUES (?1)"), [v])
                .map(|_| ())
        }
    }
}

/// Brings `conn` to `schema`'s current version, as the module documentation describes. `file` is
/// the database's path (`None` for an in-memory database, which is never backed up).
///
/// # Errors
/// [`StoreError::NewerSchema`] for a database a newer release wrote, [`StoreError::Backup`] when
/// the backup failed (nothing was migrated), or the SQLite error of a failed migration (that
/// migration is rolled back; earlier ones stay).
pub fn upgrade(
    conn: &Connection,
    file: Option<&Path>,
    schema: &Schema,
    policy: &UpgradePolicy,
) -> Result<Upgraded, StoreError> {
    let from = upgrade_check(conn, file, schema, policy)?;
    let to = schema.current();
    if from == to {
        return Ok(Upgraded {
            from,
            to,
            backup: None,
        });
    }

    let backup = match file {
        Some(file) if from > 0 => Some(back_up(conn, file, schema, from, to, policy)?),
        _ => None,
    };

    let applied = usize::try_from(from).unwrap_or(0);
    for (i, sql) in schema.migrations.iter().enumerate().skip(applied) {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = conn
            .execute_batch(sql)
            .and_then(|()| record_version(conn, schema, i64::try_from(i + 1).unwrap_or(i64::MAX)));
        match result {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(e.into());
            }
        }
    }

    if let Some(file) = file {
        if backup.is_some() {
            log::info!(
                "{} database {}: schema upgraded from version {from} to {to}",
                capitalised(schema.name),
                file.display()
            );
            for old in backups::prune(file, policy.backup_dir.as_deref(), policy.keep_backups) {
                log::info!("Removed the old backup {}", old.display());
            }
        }
    }
    Ok(Upgraded { from, to, backup })
}

/// Copies the database to its backup with SQLite's online backup.
fn back_up(
    conn: &Connection,
    file: &Path,
    schema: &Schema,
    from: i64,
    to: i64,
    policy: &UpgradePolicy,
) -> Result<PathBuf, StoreError> {
    let label = format!("{}-v{from}-v{to}", schema.label);
    let target = backups::backup_path(
        file,
        policy.backup_dir.as_deref(),
        Some(&label),
        SystemClock::new().utc_now(),
    );
    if let Err(e) = copy_online(conn, &target) {
        return Err(StoreError::Backup {
            database: schema.name,
            path: file.to_path_buf(),
            backup: target.clone(),
            from,
            to,
            reason: e,
        });
    }
    log::info!(
        "{} database {}: backed up to {} before upgrading its schema from version {from} to {to}",
        capitalised(schema.name),
        file.display(),
        target.display()
    );
    Ok(target)
}

/// Copies `conn`'s main database to `target` with SQLite's online backup; a failed copy leaves no
/// file behind (a partial copy is not a backup).
fn copy_online(conn: &Connection, target: &Path) -> Result<(), String> {
    conn.backup(DatabaseName::Main, target, None).map_err(|e| {
        let _ = std::fs::remove_file(target);
        e.to_string()
    })
}

/// Backs up the database file at `file` as the schema upgrade does (SQLite's online backup, a
/// sibling named `<file>.backup-<label>-<UTC timestamp>`, or in `policy`'s folder), whatever its
/// schema, and keeps the newest `policy.keep_backups` backups of it. Used before a release is
/// installed over the file. The backup's path.
///
/// # Errors
/// When the file cannot be opened or copied; no backup is left behind.
pub fn snapshot(file: &Path, label: &str, policy: &UpgradePolicy) -> Result<PathBuf, String> {
    if !file.is_file() {
        return Err(format!("{}: no such database file", file.display()));
    }
    let conn = Connection::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let target = backups::backup_path(
        file,
        policy.backup_dir.as_deref(),
        Some(label),
        SystemClock::new().utc_now(),
    );
    copy_online(&conn, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    drop(conn);
    for old in backups::prune(file, policy.backup_dir.as_deref(), policy.keep_backups) {
        if old != target {
            log::info!("Removed the old backup {}", old.display());
        }
    }
    Ok(target)
}

/// Puts the backup at `backup` back as the database file `file`: the file's `-wal` and `-shm`
/// companions are removed first (they belong to the file being replaced), then the backup is
/// copied over it. The database must not be open.
///
/// # Errors
/// When a file cannot be removed or copied.
pub fn restore(backup: &Path, file: &Path) -> std::io::Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut companion = file.as_os_str().to_owned();
        companion.push(suffix);
        match std::fs::remove_file(PathBuf::from(companion)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    std::fs::copy(backup, file).map(|_| ())
}

fn capitalised(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_uppercase().collect::<String>() + c.as_str()
    })
}

/// Opens (creating when absent) the database file at `path` for `schema`: the connection prepared
/// as every empyrean-store connection is (collation, foreign keys, WAL), then [`upgrade`]d.
///
/// # Errors
/// As [`upgrade`], or when the file cannot be opened.
pub fn open(
    path: &Path,
    schema: &Schema,
    policy: &UpgradePolicy,
) -> Result<(Connection, Upgraded), StoreError> {
    let conn = Connection::open(path)?;
    // Refused before the connection is set up, so a newer release's file is left as it was.
    upgrade_check(&conn, Some(path), schema, policy)?;
    crate::sqlite_shard::prepare_connection(&conn, true)?;
    let upgraded = upgrade(&conn, Some(path), schema, policy)?;
    Ok((conn, upgraded))
}
