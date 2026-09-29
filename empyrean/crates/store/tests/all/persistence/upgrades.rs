//! Divergence: V3
//! Opening a database file upgrades its schema: a backup first, and only when a migration is
//! pending on an existing schema; a failed backup migrates nothing; a file a newer release wrote
//! is refused untouched; only the newest backups are kept; the backup taken before a release is
//! installed restores the file as it was.
//! Fixture: temporary SQLite files with synthetic two-step schemas and the store's own schemas.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::backups;
use empyrean_common::clock::SystemClock;
use empyrean_store::upgrade::{self, Schema, UpgradePolicy, Versioning};
use empyrean_store::{SqliteAuth, SqliteShard, StoreError};

const STEP_1: &str = "CREATE TABLE hero (name TEXT NOT NULL);";
const STEP_2: &str = "ALTER TABLE hero ADD COLUMN level INTEGER NOT NULL DEFAULT 1;";

const V1: Schema = Schema {
    name: "test",
    label: "test",
    migrations: &[STEP_1],
    versioning: Versioning::UserVersion,
};

const V2: Schema = Schema {
    name: "test",
    label: "test",
    migrations: &[STEP_1, STEP_2],
    versioning: Versioning::UserVersion,
};

/// A fresh folder for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("store-upgrades")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Everything in `dir` whose name starts with `prefix`, sorted.
fn named(dir: &Path, prefix: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(prefix))
        .collect();
    v.sort();
    v
}

/// A database at schema 1 of [`V1`] holding one hero.
fn at_v1(file: &Path) {
    let (conn, up) = upgrade::open(file, &V1, &UpgradePolicy::default()).unwrap();
    assert_eq!((up.from, up.to, up.backup), (0, 1, None));
    conn.execute("INSERT INTO hero (name) VALUES ('Asheron')", [])
        .unwrap();
}

fn user_version(file: &Path) -> i64 {
    rusqlite::Connection::open(file)
        .unwrap()
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap()
}

#[test]
fn a_pending_migration_backs_the_file_up_before_it_runs_and_only_then() {
    let dir = scratch("pending");
    let file = dir.join("game.db");
    at_v1(&file);
    // A new schema is created without a backup: there was nothing to keep.
    assert!(named(&dir, "game.db.backup-").is_empty());

    let (conn, up) = upgrade::open(&file, &V2, &UpgradePolicy::default()).unwrap();
    let backup = up.backup.clone().expect("a backup before the upgrade");
    assert_eq!((up.from, up.to), (1, 2));
    let name = backup.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with("game.db.backup-test-v1-v2-") && name.ends_with('Z'),
        "{name}"
    );
    assert_eq!(backup.parent(), Some(dir.as_path()));
    let level: i64 = conn
        .query_row("SELECT level FROM hero WHERE name = 'Asheron'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(level, 1, "the migration ran over the kept rows");
    drop(conn);

    // The backup is the database as it was: schema 1, its row, no level column.
    assert_eq!(user_version(&backup), 1);
    let old = rusqlite::Connection::open(&backup).unwrap();
    let names: Vec<String> = old
        .prepare("SELECT name FROM hero")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(names, ["Asheron"]);
    assert!(old.prepare("SELECT level FROM hero").is_err());
    drop(old);

    // Nothing pending: no second backup.
    let (_, again) = upgrade::open(&file, &V2, &UpgradePolicy::default()).unwrap();
    assert_eq!((again.from, again.to, again.backup), (2, 2, None));
    assert_eq!(named(&dir, "game.db.backup-").len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_or_current_store_database_is_never_backed_up() {
    let dir = scratch("fresh");
    let shard = dir.join("shard.db");
    let auth = dir.join("auth.db");
    for _ in 0..2 {
        drop(SqliteShard::open(&shard).unwrap());
        drop(
            SqliteAuth::open(
                &auth,
                AccountDefaults::default(),
                Arc::new(SystemClock::new()),
            )
            .unwrap(),
        );
    }
    // One file holding both schemas: the second schema's creation is not an upgrade either.
    let shared = dir.join("both.db");
    drop(SqliteShard::open(&shared).unwrap());
    drop(
        SqliteAuth::open(
            &shared,
            AccountDefaults::default(),
            Arc::new(SystemClock::new()),
        )
        .unwrap(),
    );
    assert!(
        named(&dir, "").iter().all(|n| !n.contains(".backup-")),
        "{:?}",
        named(&dir, "")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_backup_migrates_nothing_and_says_why() {
    let dir = scratch("failed-backup");
    let file = dir.join("game.db");
    at_v1(&file);
    // The backup folder is a file, so the backup cannot be written.
    let not_a_folder = dir.join("not-a-folder");
    std::fs::write(&not_a_folder, b"x").unwrap();
    let policy = UpgradePolicy {
        backup_dir: Some(not_a_folder),
        ..UpgradePolicy::default()
    };

    let err = upgrade::open(&file, &V2, &policy).unwrap_err();
    let StoreError::Backup { from, to, .. } = &err else {
        panic!("expected a backup failure, got {err}");
    };
    assert_eq!((*from, *to), (1, 2));
    let message = err.to_string();
    assert!(
        message.contains("could not be backed up") && message.contains("Nothing was migrated"),
        "{message}"
    );

    assert_eq!(user_version(&file), 1, "no migration ran");
    let conn = rusqlite::Connection::open(&file).unwrap();
    assert!(conn.prepare("SELECT level FROM hero").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_database_a_newer_release_wrote_is_refused_untouched() {
    let dir = scratch("newer");
    let file = dir.join("game.db");
    at_v1(&file);
    drop(upgrade::open(&file, &V2, &UpgradePolicy::default()).unwrap());
    let backup = backups::newest(&file, None).expect("the upgrade's backup");
    let before = std::fs::read(&file).unwrap();

    // The older build (one migration) meets the file the newer one (two) upgraded.
    let err = upgrade::open(&file, &V1, &UpgradePolicy::default()).unwrap_err();
    let StoreError::NewerSchema { found, known, .. } = &err else {
        panic!("expected a refusal, got {err}");
    };
    assert_eq!((*found, *known), (2, 1));
    let message = err.to_string();
    assert!(
        message.contains("schema version 2")
            && message.contains("up to 1")
            && message.contains("newer Empyrean release")
            && message.contains(&backup.display().to_string()),
        "{message}"
    );
    assert_eq!(
        std::fs::read(&file).unwrap(),
        before,
        "the file is untouched"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_store_refuses_shard_and_authentication_files_from_a_newer_release() {
    let dir = scratch("newer-store");
    let shard = dir.join("shard.db");
    drop(SqliteShard::open(&shard).unwrap());
    rusqlite::Connection::open(&shard)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    let err = SqliteShard::open(&shard).unwrap_err();
    assert!(
        matches!(err, StoreError::NewerSchema { database: "shard", found: 99, known, .. } if known == empyrean_store::sqlite_shard::SHARD_SCHEMA.current()),
        "{err}"
    );
    assert_eq!(user_version(&shard), 99);

    let auth = dir.join("auth.db");
    let open_auth = || {
        SqliteAuth::open(
            &auth,
            AccountDefaults::default(),
            Arc::new(SystemClock::new()),
        )
    };
    drop(open_auth().unwrap());
    rusqlite::Connection::open(&auth)
        .unwrap()
        .execute(
            "INSERT INTO serv_store_auth_version (version) VALUES (42)",
            [],
        )
        .unwrap();
    let err = open_auth().unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::NewerSchema {
                database: "authentication",
                found: 42,
                ..
            }
        ),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_the_newest_backups_are_kept() {
    let dir = scratch("retention");
    let file = dir.join("game.db");
    at_v1(&file);
    // Four older backups, the oldest first.
    let old = [
        "game.db.backup-test-v0-v1-20240101T000000Z",
        "game.db.backup-test-v0-v1-20250101T000000Z",
        "game.db.backup-20250601T000000Z",
        "game.db.backup-test-v0-v1-20250601T000000Z.2",
    ];
    for n in old {
        std::fs::write(dir.join(n), b"old").unwrap();
    }
    // Not backups of this file: never removed.
    std::fs::write(dir.join("other.db.backup-test-v0-v1-20200101T000000Z"), b"").unwrap();
    std::fs::write(dir.join("game.db.backup-notes.txt"), b"").unwrap();

    let (_, up) = upgrade::open(&file, &V2, &UpgradePolicy::default()).unwrap();
    let new = up.backup.unwrap();
    assert_eq!(backups::KEEP_BACKUPS, 3);
    let kept = backups::list(&file, None);
    assert_eq!(
        kept,
        [dir.join(old[2]), dir.join(old[3]), new],
        "the newest three, oldest first"
    );
    assert_eq!(named(&dir, "other.db.backup-").len(), 1);
    assert_eq!(named(&dir, "game.db.backup-notes").len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A release installed over the databases backs each one up first, whatever its schema, and a
/// failed install puts the backup back: the file reads as it did before, even after the new
/// build migrated and wrote it.
#[test]
fn a_snapshot_taken_before_an_install_restores_the_file_as_it_was() {
    let dir = scratch("snapshot");
    let file = dir.join("game.db");
    at_v1(&file);
    let backup = upgrade::snapshot(&file, "update-v1-v2", &UpgradePolicy::default()).unwrap();
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("game.db.backup-update-v1-v2-"));

    // The new build migrates and writes.
    let (conn, up) = upgrade::open(&file, &V2, &UpgradePolicy::default()).unwrap();
    assert_eq!((up.from, up.to), (1, 2));
    conn.execute("INSERT INTO hero (name, level) VALUES ('Bael', 9)", [])
        .unwrap();
    drop(conn);
    assert_eq!(user_version(&file), 2);

    upgrade::restore(&backup, &file).unwrap();
    assert_eq!(user_version(&file), 1);
    let names: Vec<String> = {
        let conn = rusqlite::Connection::open(&file).unwrap();
        let mut stmt = conn.prepare("SELECT name FROM hero").unwrap();
        let rows = stmt.query_map([], |r| r.get(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    assert_eq!(names, vec!["Asheron".to_owned()]);
    assert!(upgrade::snapshot(&dir.join("absent.db"), "x", &UpgradePolicy::default()).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
