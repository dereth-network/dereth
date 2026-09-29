//! Divergence: V3
//! Every release's fixture databases open with this build: upgraded to the current schemas (with
//! a backup when a migration ran), then their accounts, characters and items read back as the
//! release that made them wrote them.
//! Fixture: tests/fixtures/upgrade/<version>/ (shard.sqlite, auth.sqlite, expected.json), written
//! by the upgrade_fixture example; synthetic, no retail data.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::backups;
use empyrean_common::clock::VirtualClock;
use empyrean_store::sqlite_auth::AUTH_SCHEMA;
use empyrean_store::sqlite_shard::SHARD_SCHEMA;
use empyrean_store::upgrade::{self, Schema};
use empyrean_store::{SqliteAuth, SqliteShard};

use crate::support::upgrade_snapshot::snapshot;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/upgrade")
}

/// Every release's fixture folder, oldest first by version.
fn releases() -> Vec<(Vec<u64>, PathBuf)> {
    let mut v: Vec<(Vec<u64>, PathBuf)> = std::fs::read_dir(fixtures())
        .expect("tests/fixtures/upgrade")
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let version = name
                .split('.')
                .map(|p| {
                    p.parse::<u64>()
                        .unwrap_or_else(|_| panic!("{name} is not a version"))
                })
                .collect();
            (version, e.path())
        })
        .collect();
    v.sort();
    v
}

fn version_of(file: &Path, schema: &Schema) -> i64 {
    let conn = rusqlite::Connection::open(file).unwrap();
    upgrade::version(&conn, schema).unwrap()
}

#[test]
fn every_release_fixture_upgrades_and_keeps_its_characters_and_items() {
    let releases = releases();
    assert!(
        !releases.is_empty(),
        "no fixture in {}",
        fixtures().display()
    );
    // The release number without a pre-release suffix (`0.2.0-rc.1` is 0.2.0).
    let own: Vec<u64> = env!("CARGO_PKG_VERSION")
        .split('-')
        .next()
        .unwrap()
        .split('.')
        .map(|p| p.parse().unwrap())
        .collect();
    assert!(
        releases.iter().any(|(v, _)| *v == own),
        "no fixture for this release ({}): run the upgrade_fixture example (UPGRADES.md)",
        env!("CARGO_PKG_VERSION")
    );

    for (_, fixture) in releases {
        let release = fixture.file_name().unwrap().to_string_lossy().into_owned();
        // Upgrade copies, never the committed files.
        let work = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("store-upgrade-fixtures")
            .join(format!("{release}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work).unwrap();
        let shard_path = work.join("shard.sqlite");
        let auth_path = work.join("auth.sqlite");
        std::fs::copy(fixture.join("shard.sqlite"), &shard_path).unwrap();
        std::fs::copy(fixture.join("auth.sqlite"), &auth_path).unwrap();
        let expected: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(fixture.join("expected.json")).unwrap())
                .unwrap();

        let shard_from = version_of(&shard_path, &SHARD_SCHEMA);
        let auth_from = version_of(&auth_path, &AUTH_SCHEMA);
        assert!(
            shard_from >= 1 && auth_from >= 1,
            "{release}: a release's fixture has its schemas"
        );

        let mut shard = SqliteShard::open(&shard_path)
            .unwrap_or_else(|e| panic!("{release}: the shard does not open: {e}"));
        let mut auth = SqliteAuth::open(
            &auth_path,
            AccountDefaults::default(),
            Arc::new(VirtualClock::default()),
        )
        .unwrap_or_else(|e| panic!("{release}: the authentication database does not open: {e}"));

        assert_eq!(
            upgrade::version(shard.connection(), &SHARD_SCHEMA).unwrap(),
            SHARD_SCHEMA.current(),
            "{release}: the shard is at this build's schema"
        );
        assert_eq!(
            version_of(&auth_path, &AUTH_SCHEMA),
            AUTH_SCHEMA.current(),
            "{release}: the authentication database is at this build's schema"
        );
        // A backup exactly when a migration ran.
        for (path, from, schema) in [
            (&shard_path, shard_from, &SHARD_SCHEMA),
            (&auth_path, auth_from, &AUTH_SCHEMA),
        ] {
            assert_eq!(
                backups::list(path, None).len(),
                usize::from(from < schema.current()),
                "{release}: {} backups",
                schema.name
            );
        }

        let found = snapshot(&mut auth, &mut shard);
        assert_eq!(
            found, expected,
            "{release}: the upgraded databases hold what the release wrote"
        );
        drop(shard);
        drop(auth);
        let _ = std::fs::remove_dir_all(&work);
    }
}
