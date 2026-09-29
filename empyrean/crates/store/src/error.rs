//! Backend errors (not ACE: ACE surfaces Entity Framework and MySQL exceptions).

use std::path::PathBuf;

/// A backend failure: what ACE would see as a `DbUpdateException` / `MySqlException`.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// SQLite reported an error (constraint violation, I/O, ...).
    #[error("SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A constraint the in-memory store enforces (the same ones SQLite enforces).
    #[error("constraint failed: {0}")]
    Constraint(String),
    /// A fault injected by a test (in-memory store).
    #[error("injected fault: {0}")]
    Injected(&'static str),
    /// Not ACE: the database's schema is newer than this build knows: a newer release wrote it.
    #[error("{}", newer_schema_message(database, path.as_ref(), *found, *known, backup.as_ref()))]
    NewerSchema {
        /// Which database (`shard`, `authentication`).
        database: &'static str,
        /// Its file (`None` in memory).
        path: Option<PathBuf>,
        /// The version it records.
        found: i64,
        /// The newest version this build knows.
        known: i64,
        /// The newest backup of the file, when there is one.
        backup: Option<PathBuf>,
    },
    /// Not ACE: the backup made before an upgrade failed, so the upgrade did not run.
    #[error(
        "the {database} database {} could not be backed up to {} before upgrading its schema from version {from} to {to}: {reason}. Nothing was migrated; the database is unchanged at version {from}. Free some space or fix the folder's permissions, then start the server again.",
        path.display(),
        backup.display()
    )]
    Backup {
        /// Which database (`shard`, `authentication`).
        database: &'static str,
        /// Its file.
        path: PathBuf,
        /// Where the backup was being written.
        backup: PathBuf,
        /// The version it is at.
        from: i64,
        /// The version the upgrade would bring it to.
        to: i64,
        /// Why the backup failed.
        reason: String,
    },
}

fn newer_schema_message(
    database: &str,
    path: Option<&PathBuf>,
    found: i64,
    known: i64,
    backup: Option<&PathBuf>,
) -> String {
    let file = path.map_or_else(|| "(in memory)".to_owned(), |p| p.display().to_string());
    let restore = match backup {
        Some(b) => format!(
            "or stop the server and restore the backup made before that release upgraded it ({}), copying it over {file}",
            b.display()
        ),
        None => "or restore a copy of the file from before that release upgraded it (no backup of it was found beside it)".to_owned(),
    };
    format!(
        "the {database} database {file} is at schema version {found}, but this build of Empyrean ({}) knows schema versions up to {known}: it was written by a newer Empyrean release. Run that release (or a newer one) against it, {restore}. Nothing was changed.",
        env!("CARGO_PKG_VERSION")
    )
}
