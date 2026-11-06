//! The store in an embedded SQLite file.
//!
//! The file is kept as Empyrean keeps its databases: with a write-ahead log (`-wal` and `-shm`
//! files beside it while it is open) and `synchronous = NORMAL`, which loses no committed save
//! when the server crashes, and with its foreign keys enforced, so no character's rows outlive it
//! (as the SQLite built in has always enforced them; set here so that holds of any build).
//!
//! **The schema's versions.** The file records how many of the schema's migrations it has had
//! (`PRAGMA user_version`; a file kept before versions reads 0). Opening it:
//!
//! 1. refuses a file a newer Yalain wrote (a version above this build's), touching nothing;
//! 2. with a migration to run on a file that holds anything, first copies the file beside itself
//!    with SQLite's online backup, as `<file>.backup-v<from>-v<to>-<UTC timestamp>`, as Empyrean
//!    does; a copy that fails stops the open before anything is migrated (and leaves no partial
//!    copy);
//! 3. runs each migration in a transaction of its own, recording the new version in it;
//! 4. keeps the newest [`KEEP_BACKUPS`] backups of the file and removes older ones.
//!
//! The migrations so far:
//!
//! 1. makes the tables: a file kept before versions gains those it lacks (the Dereth Network's,
//!    the friends service's), empty;
//! 2. the operator's moderation: each account's access level (every account a player's) and its
//!    ban (none), and the gagged characters (none);
//! 3. the admin flag in place of the levels: an account at the highest level (5) with a secret is an
//!    admin; one between (1 to 4), or at the highest with no secret (an open world gives such an
//!    account to the first ticket it sees), is a player, each named in the log, for the operator to
//!    raise if it should be an admin; the levels are gone from the file (the backup keeps them);
//! 4. the named groups an account's admin commands are checked against (none).

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, DatabaseName, OptionalExtension, Row};

use crate::{
    physique_bytes, physique_from, position_bytes, position_from, Account, Ban, Character, Listed,
    NetworkList, Store, StoreError, StoreResult, WorldState,
};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS account (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    secret BLOB,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS character (
    id INTEGER PRIMARY KEY,
    account INTEGER NOT NULL REFERENCES account (id),
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    race INTEGER NOT NULL,
    sex INTEGER NOT NULL,
    class INTEGER NOT NULL,
    implement INTEGER NOT NULL,
    start_area INTEGER NOT NULL,
    entity_did INTEGER NOT NULL,
    physique BLOB NOT NULL,
    look BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    deleted_at INTEGER,
    last_played INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS counter (name TEXT PRIMARY KEY, value INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS world_state (
    id INTEGER PRIMARY KEY REFERENCES character (id),
    position BLOB,
    instances INTEGER NOT NULL,
    qualities BLOB NOT NULL,
    options BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS play_state (
    id INTEGER PRIMARY KEY REFERENCES character (id),
    play BLOB NOT NULL
);
CREATE TABLE IF NOT EXISTS kept_place (
    id INTEGER NOT NULL REFERENCES character (id),
    kind INTEGER NOT NULL,
    position BLOB NOT NULL,
    PRIMARY KEY (id, kind)
);
CREATE TABLE IF NOT EXISTS world_record (name TEXT PRIMARY KEY, value BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS network_character (id INTEGER PRIMARY KEY REFERENCES character (id));
CREATE TABLE IF NOT EXISTS network_block (block TEXT PRIMARY KEY);
CREATE TABLE IF NOT EXISTS network_list (
    id INTEGER NOT NULL REFERENCES character (id),
    list INTEGER NOT NULL,
    entry TEXT NOT NULL COLLATE NOCASE,
    PRIMARY KEY (id, list, entry)
);
";

/// The operator's moderation: each account's access level and ban, the gags.
const MODERATION_COLUMNS: [(&str, &str); 5] = [
    ("access", "INTEGER NOT NULL DEFAULT 0"),
    ("banned_at", "INTEGER"),
    ("banned_until", "INTEGER"),
    ("banned_by", "TEXT"),
    ("ban_reason", "TEXT"),
];
const GAGS: &str = "
CREATE TABLE IF NOT EXISTS gag (
    id INTEGER PRIMARY KEY REFERENCES character (id),
    seconds REAL NOT NULL
);
";

/// A migration: what it does to a file. Each may meet a file that had it before (a version put
/// back by hand), and leaves that as it was.
type Migration = fn(&Connection) -> rusqlite::Result<()>;

fn tables(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(SCHEMA)
}

fn moderation(db: &Connection) -> rusqlite::Result<()> {
    for (column, kind) in MODERATION_COLUMNS {
        let has: i64 = db.query_row(
            "SELECT count(*) FROM pragma_table_info('account') WHERE name = ?1",
            [column],
            |r| r.get(0),
        )?;
        if has == 0 {
            db.execute_batch(&format!("ALTER TABLE account ADD COLUMN {column} {kind}"))?;
        }
    }
    db.execute_batch(GAGS)
}

/// The highest of the levels migration 3 replaces: an admin's.
const LEVEL_ADMIN: i64 = 5;

fn admin_flag(db: &Connection) -> rusqlite::Result<()> {
    let columns = |c: &str| -> rusqlite::Result<i64> {
        db.query_row(
            "SELECT count(*) FROM pragma_table_info('account') WHERE name = ?1",
            [c],
            |r| r.get(0),
        )
    };
    if columns("admin")? == 0 {
        db.execute_batch("ALTER TABLE account ADD COLUMN admin INTEGER NOT NULL DEFAULT 0")?;
    }
    if columns("access")? == 0 {
        return Ok(());
    }
    {
        let mut between = db.prepare(
            "SELECT name, access FROM account WHERE access > 0 \
             AND (access < ?1 OR secret IS NULL) ORDER BY name",
        )?;
        let rows = between.query_map([LEVEL_ADMIN], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (name, level) = row?;
            tracing::warn!(
                account = %name,
                level,
                "access levels are gone: this account is now a player; set-accountadmin makes it                  an admin"
            );
        }
    }
    db.execute(
        "UPDATE account SET admin = 1 WHERE access >= ?1 AND secret IS NOT NULL",
        [LEVEL_ADMIN],
    )?;
    db.execute_batch("ALTER TABLE account DROP COLUMN access")
}

fn admin_groups(db: &Connection) -> rusqlite::Result<()> {
    let has: i64 = db.query_row(
        "SELECT count(*) FROM pragma_table_info('account') WHERE name = 'admin_groups'",
        [],
        |r| r.get(0),
    )?;
    if has == 0 {
        db.execute_batch("ALTER TABLE account ADD COLUMN admin_groups TEXT NOT NULL DEFAULT ''")?;
    }
    Ok(())
}

/// The schema's migrations, in order: a file at version `n` has had the first `n`.
const MIGRATIONS: &[Migration] = &[tables, moderation, admin_flag, admin_groups];

/// The schema version this build keeps its files at.
pub const SCHEMA_VERSION: i64 = 4;

/// How long a call waits for a lock another program holds on the file.
const BUSY_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// How many backups of a file are kept (the newest), as Empyrean keeps.
pub const KEEP_BACKUPS: usize = 3;

/// What follows a file's name in its backups' names.
const BACKUP_MARKER: &str = ".backup-";

const ACCOUNT_COLUMNS: &str =
    "id, name, secret, created_at, admin, banned_at, banned_until, banned_by, ban_reason, admin_groups";

const CHARACTER_COLUMNS: &str =
    "id, account, name, race, sex, class, implement, start_area, entity_did, physique, look, created_at, deleted_at, last_played";

#[allow(clippy::needless_pass_by_value)]
fn err(e: rusqlite::Error) -> StoreError {
    StoreError(e.to_string())
}

/// An update meant for one account: an error when it found none.
fn one_account(changed: usize, account: u64) -> StoreResult<()> {
    if changed == 0 {
        return Err(StoreError(format!("no account {account}")));
    }
    Ok(())
}

/// The SQLite store.
#[derive(Debug)]
pub struct SqliteStore {
    db: Connection,
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn account(r: &Row<'_>) -> rusqlite::Result<Account> {
    let at: Option<i64> = r.get(5)?;
    let until: Option<i64> = r.get(6)?;
    Ok(Account {
        id: r.get::<_, i64>(0)? as u64,
        name: r.get(1)?,
        secret: r.get(2)?,
        created_at: r.get(3)?,
        admin: r.get::<_, i64>(4)? != 0,
        groups: r
            .get::<_, String>(9)?
            .split(',')
            .filter(|g| !g.is_empty())
            .map(str::to_owned)
            .collect(),
        ban: match (at, until) {
            (Some(at), Some(until)) => Some(Ban {
                at,
                until,
                by: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                reason: r.get(8)?,
            }),
            _ => None,
        },
    })
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn character(r: &Row<'_>) -> rusqlite::Result<Character> {
    Ok(Character {
        id: r.get::<_, i64>(0)? as u64,
        account: r.get::<_, i64>(1)? as u64,
        name: r.get(2)?,
        race: r.get::<_, i64>(3)? as u32,
        sex: r.get::<_, i64>(4)? as u32,
        class: r.get::<_, i64>(5)? as u32,
        implement: r.get::<_, i64>(6)? as u32,
        start_area: r.get::<_, i64>(7)? as u32,
        entity_did: r.get::<_, i64>(8)? as u32,
        physique: physique_from(&r.get::<_, Vec<u8>>(9)?),
        look: r.get(10)?,
        created_at: r.get(11)?,
        deleted_at: r.get(12)?,
        last_played: r.get(13)?,
    })
}

impl SqliteStore {
    /// Open (or make) the store in a file, its schema brought up to date (after a backup when
    /// there is something to migrate; see the module's documentation).
    ///
    /// # Errors
    /// The file cannot be opened, a newer Yalain wrote it, its backup failed, or a migration
    /// failed (that migration is rolled back).
    pub fn open(path: &Path) -> StoreResult<Self> {
        let db = Connection::open(path).map_err(err)?;
        db.pragma_update(None, "foreign_keys", "ON").map_err(err)?;
        db.pragma_update(None, "journal_mode", "WAL").map_err(err)?;
        db.pragma_update(None, "synchronous", "NORMAL")
            .map_err(err)?;
        // Saves are tried again, so one that finds the file locked gives way soon: the world's loop
        // does not stand still behind it.
        db.busy_timeout(BUSY_WAIT).map_err(err)?;
        upgrade(&db, Some(path))?;
        Ok(Self { db })
    }

    /// A store in memory, gone when dropped.
    ///
    /// # Errors
    /// The tables cannot be made.
    pub fn open_in_memory() -> StoreResult<Self> {
        let db = Connection::open_in_memory().map_err(err)?;
        db.pragma_update(None, "foreign_keys", "ON").map_err(err)?;
        upgrade(&db, None)?;
        Ok(Self { db })
    }
}

/// The schema version a file records.
fn version(db: &Connection) -> StoreResult<i64> {
    db.pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(err)
}

/// Bring the file to [`SCHEMA_VERSION`], as the module's documentation says. `file` is `None` for
/// a database in memory, which is never backed up.
fn upgrade(db: &Connection, file: Option<&Path>) -> StoreResult<()> {
    let from = version(db)?;
    if from > SCHEMA_VERSION {
        let newest = file.and_then(|f| backups(f).pop());
        return Err(StoreError(format!(
            "the database is at schema version {from}, newer than this Yalain's \
             ({SCHEMA_VERSION}): a newer Yalain kept it; run that, or put back a backup kept \
             before its upgrade{}",
            newest.map_or(String::new(), |b| format!(" (the newest: {})", b.display()))
        )));
    }
    if from == SCHEMA_VERSION {
        return Ok(());
    }
    let holds_anything: i64 = db
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |r| r.get(0),
        )
        .map_err(err)?;
    let backup = match file {
        Some(file) if holds_anything > 0 => Some(back_up(db, file, from)?),
        _ => None,
    };
    let applied = usize::try_from(from).unwrap_or(0);
    for (i, migrate) in MIGRATIONS.iter().enumerate().skip(applied) {
        db.execute_batch("BEGIN IMMEDIATE").map_err(err)?;
        let to = i64::try_from(i + 1).unwrap_or(i64::MAX);
        let done = migrate(db).and_then(|()| db.pragma_update(None, "user_version", to));
        match done {
            Ok(()) => db.execute_batch("COMMIT").map_err(err)?,
            Err(e) => {
                let _ = db.execute_batch("ROLLBACK");
                return Err(err(e));
            }
        }
    }
    if let (Some(file), Some(_)) = (file, backup) {
        let all = backups(file);
        for old in &all[..all.len().saturating_sub(KEEP_BACKUPS)] {
            let _ = std::fs::remove_file(old);
        }
    }
    Ok(())
}

/// Copy the file beside itself with SQLite's online backup before migrating it from `from`; a
/// copy that fails leaves nothing behind.
fn back_up(db: &Connection, file: &Path, from: i64) -> StoreResult<PathBuf> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let target = backup_path(
        file,
        &format!("v{from}-v{SCHEMA_VERSION}-{}", utc_stamp(now)),
    );
    db.backup(DatabaseName::Main, &target, None).map_err(|e| {
        let _ = std::fs::remove_file(&target);
        StoreError(format!(
            "the backup before upgrading the schema from version {from} to {SCHEMA_VERSION} \
             ({}) failed, so nothing was upgraded: {e}",
            target.display()
        ))
    })?;
    Ok(target)
}

/// Back up the database file at `file` as the schema upgrade does (SQLite's online backup, a sibling
/// named `<file>.backup-<label>-<UTC timestamp>`), whatever its schema, and keep the newest
/// [`KEEP_BACKUPS`] backups of it. Used before a release is installed over the file. The backup's
/// path.
///
/// # Errors
/// The file cannot be opened or copied; no backup is left behind.
pub fn snapshot(file: &Path, label: &str) -> StoreResult<PathBuf> {
    if !file.is_file() {
        return Err(StoreError(format!(
            "{}: no such database file",
            file.display()
        )));
    }
    let db = Connection::open(file).map_err(err)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    let target = backup_path(file, &format!("{label}-{}", utc_stamp(now)));
    db.backup(DatabaseName::Main, &target, None).map_err(|e| {
        let _ = std::fs::remove_file(&target);
        StoreError(format!("{}: {e}", target.display()))
    })?;
    drop(db);
    let all = backups(file);
    for old in &all[..all.len().saturating_sub(KEEP_BACKUPS)] {
        let _ = std::fs::remove_file(old);
    }
    Ok(target)
}

/// Put the backup at `backup` back as the database file `file`: the file's `-wal` and `-shm`
/// companions are removed first (they belong to the file being replaced), then the backup is copied
/// over it. The database must not be open.
///
/// # Errors
/// A file cannot be removed or copied.
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

/// Where a backup of `file` named for `label` goes: beside it, and not the name of anything
/// already there (a second backup in the same second gets `.2`, `.3` ...).
fn backup_path(file: &Path, label: &str) -> PathBuf {
    let dir = file.parent().unwrap_or_else(|| Path::new(""));
    let base = format!("{}{BACKUP_MARKER}{label}", file_name(file));
    let mut candidate = dir.join(&base);
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base}.{n}"));
        n += 1;
    }
    candidate
}

fn file_name(file: &Path) -> String {
    file.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The backups of `file` beside it, oldest first: by their timestamp, then their same-second
/// number.
#[must_use]
pub fn backups(file: &Path) -> Vec<PathBuf> {
    let dir = file.parent().unwrap_or_else(|| Path::new(""));
    let prefix = format!("{}{BACKUP_MARKER}", file_name(file));
    let read = if dir.as_os_str().is_empty() {
        std::fs::read_dir(".")
    } else {
        std::fs::read_dir(dir)
    };
    let Ok(entries) = read else {
        return Vec::new();
    };
    let mut found: Vec<((String, u32), PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let rest = name.strip_prefix(&prefix)?;
            let last = rest.rsplit('-').next()?;
            let (stamp, n) = match last.split_once('.') {
                Some((s, n)) => (s, n.parse().ok()?),
                None => (last, 1),
            };
            let ok = stamp.len() == 16
                && stamp.as_bytes()[8] == b'T'
                && stamp.ends_with('Z')
                && stamp[..8].bytes().all(|b| b.is_ascii_digit())
                && stamp[9..15].bytes().all(|b| b.is_ascii_digit());
            ok.then(|| ((stamp.to_owned(), n), dir.join(&name)))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

/// Seconds since 1970 as a UTC timestamp to the second: `YYYYMMDDTHHMMSSZ`.
fn utc_stamp(unix: i64) -> String {
    let (days, secs) = (unix.div_euclid(86_400), unix.rem_euclid(86_400));
    // Days since 1970 to a civil date (the proleptic Gregorian calendar).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// Keep a new character's row.
#[allow(clippy::cast_possible_wrap)]
fn insert_character(db: &Connection, c: &Character) -> StoreResult<()> {
    let sql = format!(
        "INSERT INTO character ({CHARACTER_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
    );
    db.execute(
        &sql,
        params![
            c.id as i64,
            c.account as i64,
            c.name,
            i64::from(c.race),
            i64::from(c.sex),
            i64::from(c.class),
            i64::from(c.implement),
            i64::from(c.start_area),
            i64::from(c.entity_did),
            physique_bytes(&c.physique),
            c.look,
            c.created_at,
            c.deleted_at,
            c.last_played
        ],
    )
    .map(|_| ())
    .map_err(err)
}

/// Write what a character is in the world, inside a transaction the caller commits.
#[allow(clippy::cast_possible_wrap)]
fn write_state(tx: &rusqlite::Transaction<'_>, id: u64, s: &WorldState) -> StoreResult<()> {
    tx.execute(
        "INSERT INTO world_state (id, position, instances, qualities, options) VALUES (?1, ?2, ?3, ?4, ?5)                  ON CONFLICT (id) DO UPDATE SET position = ?2, instances = ?3, qualities = ?4, options = ?5",
        params![
            id as i64,
            s.position.as_ref().map(position_bytes),
            i64::from(s.instances),
            s.qualities,
            s.options
        ],
    )
    .map_err(err)?;
    tx.execute(
        "INSERT INTO play_state (id, play) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET play = ?2",
        params![id as i64, s.play],
    )
    .map_err(err)?;
    tx.execute("DELETE FROM kept_place WHERE id = ?1", [id as i64])
        .map_err(err)?;
    for (kind, p) in &s.places {
        tx.execute(
            "INSERT INTO kept_place (id, kind, position) VALUES (?1, ?2, ?3)",
            params![id as i64, i64::from(*kind), position_bytes(p)],
        )
        .map_err(err)?;
    }
    Ok(())
}

#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
impl Store for SqliteStore {
    fn account_by_name(&mut self, name: &str) -> StoreResult<Option<Account>> {
        self.db
            .query_row(
                &format!("SELECT {ACCOUNT_COLUMNS} FROM account WHERE name = ?1"),
                [name],
                account,
            )
            .optional()
            .map_err(err)
    }

    fn create_account(
        &mut self,
        name: &str,
        secret: Option<Vec<u8>>,
        now: i64,
    ) -> StoreResult<Account> {
        self.db
            .execute(
                "INSERT INTO account (name, secret, created_at) VALUES (?1, ?2, ?3)",
                params![name, secret, now],
            )
            .map_err(err)?;
        let id = self.db.last_insert_rowid() as u64;
        Ok(Account {
            id,
            name: name.to_string(),
            secret,
            created_at: now,
            admin: false,
            groups: Vec::new(),
            ban: None,
        })
    }

    fn set_account_secret(&mut self, account: u64, secret: Vec<u8>) -> StoreResult<()> {
        let n = self
            .db
            .execute(
                "UPDATE account SET secret = ?2 WHERE id = ?1",
                params![account as i64, secret],
            )
            .map_err(err)?;
        one_account(n, account)
    }

    fn account(&mut self, id: u64) -> StoreResult<Option<Account>> {
        self.db
            .query_row(
                &format!("SELECT {ACCOUNT_COLUMNS} FROM account WHERE id = ?1"),
                [id as i64],
                account,
            )
            .optional()
            .map_err(err)
    }

    fn accounts(&mut self) -> StoreResult<Vec<Account>> {
        let mut st = self
            .db
            .prepare(&format!(
                "SELECT {ACCOUNT_COLUMNS} FROM account ORDER BY name COLLATE NOCASE, id"
            ))
            .map_err(err)?;
        let rows = st.query_map([], account).map_err(err)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(err)
    }

    fn set_account_admin(&mut self, account: u64, admin: bool) -> StoreResult<()> {
        let n = self
            .db
            .execute(
                "UPDATE account SET admin = ?2 WHERE id = ?1",
                params![account as i64, i64::from(admin)],
            )
            .map_err(err)?;
        one_account(n, account)
    }

    fn set_account_groups(&mut self, account: u64, groups: &[String]) -> StoreResult<()> {
        let n = self
            .db
            .execute(
                "UPDATE account SET admin_groups = ?2 WHERE id = ?1",
                params![account as i64, groups.join(",")],
            )
            .map_err(err)?;
        one_account(n, account)
    }

    fn set_character_entity(&mut self, character: u64, entity: u32) -> StoreResult<()> {
        let n = self
            .db
            .execute(
                "UPDATE character SET entity_did = ?2 WHERE id = ?1",
                params![character as i64, i64::from(entity)],
            )
            .map_err(err)?;
        if n == 0 {
            return Err(StoreError(format!("no character {character}")));
        }
        Ok(())
    }

    fn rename_character(&mut self, character: u64, name: &str) -> StoreResult<()> {
        let tx = self.db.transaction().map_err(err)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT name FROM character WHERE id = ?1",
                [character as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        let Some(old) = old else {
            return Err(StoreError(format!("no character {character}")));
        };
        tx.execute(
            "UPDATE character SET name = ?2 WHERE id = ?1",
            params![character as i64, name],
        )
        .map_err(err)?;
        // A list that holds both names holds the new one once. The column ignores case.
        tx.execute(
            "UPDATE OR REPLACE network_list SET entry = ?2 WHERE entry = ?1",
            params![old, name],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
    }

    fn set_account_ban(&mut self, account: u64, ban: Option<&Ban>) -> StoreResult<()> {
        let n = self
            .db
            .execute(
                "UPDATE account SET banned_at = ?2, banned_until = ?3, banned_by = ?4, \
                 ban_reason = ?5 WHERE id = ?1",
                params![
                    account as i64,
                    ban.map(|b| b.at),
                    ban.map(|b| b.until),
                    ban.map(|b| b.by.as_str()),
                    ban.and_then(|b| b.reason.as_deref())
                ],
            )
            .map_err(err)?;
        one_account(n, account)
    }

    fn gag(&mut self, character: u64) -> StoreResult<Option<f64>> {
        self.db
            .query_row(
                "SELECT seconds FROM gag WHERE id = ?1",
                [character as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)
    }

    fn set_gag(&mut self, character: u64, seconds: Option<f64>) -> StoreResult<()> {
        match seconds {
            Some(s) => self.db.execute(
                "INSERT INTO gag (id, seconds) VALUES (?1, ?2) \
                 ON CONFLICT (id) DO UPDATE SET seconds = ?2",
                params![character as i64, s],
            ),
            None => self
                .db
                .execute("DELETE FROM gag WHERE id = ?1", [character as i64]),
        }
        .map(|_| ())
        .map_err(err)
    }

    fn characters(&mut self, account: u64) -> StoreResult<Vec<Character>> {
        let mut s = self
            .db
            .prepare(&format!("SELECT {CHARACTER_COLUMNS} FROM character WHERE account = ?1 ORDER BY last_played, rowid"))
            .map_err(err)?;
        let rows = s.query_map([account as i64], character).map_err(err)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(err)
    }

    fn character(&mut self, id: u64) -> StoreResult<Option<Character>> {
        self.db
            .query_row(
                &format!("SELECT {CHARACTER_COLUMNS} FROM character WHERE id = ?1"),
                [id as i64],
                character,
            )
            .optional()
            .map_err(err)
    }

    fn character_named(&mut self, name: &str) -> StoreResult<Option<Character>> {
        self.db
            .query_row(
                &format!("SELECT {CHARACTER_COLUMNS} FROM character WHERE name = ?1"),
                [name],
                character,
            )
            .optional()
            .map_err(err)
    }

    fn name_taken(&mut self, name: &str) -> StoreResult<bool> {
        self.db
            .query_row(
                "SELECT count(*) FROM character WHERE name = ?1",
                [name],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .map_err(err)
    }

    fn reserve_thing_numbers(&mut self, n: u64) -> StoreResult<u64> {
        let tx = self.db.transaction().map_err(err)?;
        tx.execute(
            "INSERT INTO counter (name, value) VALUES ('thing', ?1) ON CONFLICT (name) DO UPDATE SET value = value + ?1",
            [n as i64],
        )
        .map_err(err)?;
        let top: i64 = tx
            .query_row("SELECT value FROM counter WHERE name = 'thing'", [], |r| {
                r.get(0)
            })
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(top as u64 - n + 1)
    }

    fn next_character_number(&mut self) -> StoreResult<u64> {
        let tx = self.db.transaction().map_err(err)?;
        tx.execute(
            "INSERT INTO counter (name, value) VALUES (?1, 1) ON CONFLICT (name) DO UPDATE SET value = value + 1",
            ["character"],
        )
        .map_err(err)?;
        let n: i64 = tx
            .query_row(
                "SELECT value FROM counter WHERE name = ?1",
                ["character"],
                |r| r.get(0),
            )
            .map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(n as u64)
    }

    fn insert_character(&mut self, c: &Character) -> StoreResult<()> {
        insert_character(&self.db, c)
    }

    // All or nothing: a character is never kept without the state it was made with.
    fn insert_character_with_state(
        &mut self,
        c: &Character,
        state: &WorldState,
    ) -> StoreResult<()> {
        let tx = self.db.transaction().map_err(err)?;
        insert_character(&tx, c)?;
        write_state(&tx, c.id, state)?;
        tx.commit().map_err(err)
    }

    fn set_deleted(&mut self, id: u64, at: Option<i64>) -> StoreResult<()> {
        self.db
            .execute(
                "UPDATE character SET deleted_at = ?1 WHERE id = ?2",
                params![at, id as i64],
            )
            .map(|_| ())
            .map_err(err)
    }

    fn set_last_played(&mut self, id: u64, at: i64) -> StoreResult<()> {
        self.db
            .execute(
                "UPDATE character SET last_played = ?1 WHERE id = ?2",
                params![at, id as i64],
            )
            .map(|_| ())
            .map_err(err)
    }

    // All or nothing: a character is never left half gone.
    fn purge_character(&mut self, id: u64) -> StoreResult<()> {
        let tx = self.db.transaction().map_err(err)?;
        for table in [
            "world_state",
            "kept_place",
            "play_state",
            "network_character",
            "network_list",
            "gag",
            "character",
        ] {
            tx.execute(&format!("DELETE FROM {table} WHERE id = ?1"), [id as i64])
                .map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    fn world_state(&mut self, id: u64) -> StoreResult<Option<WorldState>> {
        let mut places = Vec::new();
        {
            let mut st = self
                .db
                .prepare("SELECT kind, position FROM kept_place WHERE id = ?1 ORDER BY kind")
                .map_err(err)?;
            let rows = st
                .query_map([id as i64], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
                })
                .map_err(err)?;
            for row in rows {
                let (kind, bytes) = row.map_err(err)?;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                if let Some(p) = position_from(&bytes) {
                    places.push((kind as u32, p));
                }
            }
        }
        let play: Vec<u8> = self
            .db
            .query_row(
                "SELECT play FROM play_state WHERE id = ?1",
                [id as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?
            .unwrap_or_default();
        self.db
            .query_row(
                "SELECT position, instances, qualities, options FROM world_state WHERE id = ?1",
                [id as i64],
                |r| {
                    let position: Option<Vec<u8>> = r.get(0)?;
                    #[allow(clippy::cast_possible_truncation)]
                    Ok(WorldState {
                        position: position.as_deref().and_then(position_from),
                        instances: r.get::<_, i64>(1)? as u16,
                        qualities: r.get(2)?,
                        options: r.get(3)?,
                        places: places.clone(),
                        play: play.clone(),
                    })
                },
            )
            .optional()
            .map_err(err)
    }

    // All or nothing: a save that fails part way leaves the one before it whole.
    fn set_world_state(&mut self, id: u64, s: &WorldState) -> StoreResult<()> {
        let tx = self.db.transaction().map_err(err)?;
        write_state(&tx, id, s)?;
        tx.commit().map_err(err)
    }

    fn save_all(
        &mut self,
        states: &[(u64, &WorldState)],
        records: &[(&str, &[u8])],
    ) -> StoreResult<()> {
        // Taken at once, so that a lock another program holds a moment is waited out.
        let tx = self
            .db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(err)?;
        for (id, state) in states {
            let held: i64 = tx
                .query_row(
                    "SELECT count(*) FROM character WHERE id = ?1",
                    [*id as i64],
                    |r| r.get(0),
                )
                .map_err(err)?;
            if held > 0 {
                write_state(&tx, *id, state)?;
            }
        }
        for (name, value) in records {
            tx.execute(
                "INSERT INTO world_record (name, value) VALUES (?1, ?2) ON CONFLICT (name) DO UPDATE SET value = ?2",
                params![name, value],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    fn world_record(&mut self, name: &str) -> StoreResult<Option<Vec<u8>>> {
        self.db
            .query_row(
                "SELECT value FROM world_record WHERE name = ?1",
                [name],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)
    }

    fn set_world_record(&mut self, name: &str, value: &[u8]) -> StoreResult<()> {
        self.db
            .execute(
                "INSERT INTO world_record (name, value) VALUES (?1, ?2) ON CONFLICT (name) DO UPDATE SET value = ?2",
                params![name, value],
            )
            .map(|_| ())
            .map_err(err)
    }

    fn network_characters(&mut self) -> StoreResult<Vec<u64>> {
        let mut st = self
            .db
            .prepare("SELECT id FROM network_character ORDER BY id")
            .map_err(err)?;
        let rows = st.query_map([], |r| r.get::<_, i64>(0)).map_err(err)?;
        rows.map(|r| r.map(|id| id as u64).map_err(err)).collect()
    }

    fn set_network_on(&mut self, id: u64, on: bool) -> StoreResult<()> {
        let sql = if on {
            "INSERT OR IGNORE INTO network_character (id) VALUES (?1)"
        } else {
            "DELETE FROM network_character WHERE id = ?1"
        };
        self.db.execute(sql, [id as i64]).map(|_| ()).map_err(err)
    }

    fn network_blocks(&mut self) -> StoreResult<Vec<String>> {
        let mut st = self
            .db
            .prepare("SELECT block FROM network_block ORDER BY block")
            .map_err(err)?;
        let rows = st.query_map([], |r| r.get(0)).map_err(err)?;
        rows.map(|r| r.map_err(err)).collect()
    }

    fn set_network_block(&mut self, block: &str, on: bool) -> StoreResult<()> {
        let sql = if on {
            "INSERT OR IGNORE INTO network_block (block) VALUES (?1)"
        } else {
            "DELETE FROM network_block WHERE block = ?1"
        };
        self.db.execute(sql, [block]).map(|_| ()).map_err(err)
    }

    fn network_lists(&mut self) -> StoreResult<Vec<Listed>> {
        let mut st = self
            .db
            .prepare("SELECT id, list, entry FROM network_list ORDER BY id, list, entry")
            .map_err(err)?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)? as u64,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(err)?;
        let mut out = Vec::new();
        for row in rows {
            let (character, list, entry) = row.map_err(err)?;
            // A list a later version keeps is left alone.
            if let Some(list) = u32::try_from(list).ok().and_then(NetworkList::from_number) {
                out.push(Listed {
                    character,
                    list,
                    entry,
                });
            }
        }
        out.sort();
        Ok(out)
    }

    fn set_listed(
        &mut self,
        character: u64,
        list: NetworkList,
        entry: &str,
        on: bool,
    ) -> StoreResult<()> {
        let sql = if on {
            "INSERT OR IGNORE INTO network_list (id, list, entry) VALUES (?1, ?2, ?3)"
        } else {
            "DELETE FROM network_list WHERE id = ?1 AND list = ?2 AND entry = ?3"
        };
        self.db
            .execute(
                sql,
                params![character as i64, i64::from(list.number()), entry],
            )
            .map(|_| ())
            .map_err(err)
    }
}
