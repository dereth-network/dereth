# Database upgrades

Empyrean's shard and authentication databases are SQLite files that upgrade themselves when the
server opens them (`src/upgrade.rs`). This page is for whoever changes their schemas or cuts a
release.

## The rules

- **Schema 1 is Empyrean 0.1.0.** `src/schema/shard_v001.sql` and `src/schema/auth_v001.sql` are
  the baselines. A released migration is never edited, reordered or removed: a database somewhere
  has recorded it as applied.
- **A change is a new migration.** Append `shard_v00N.sql` to `SHARD_MIGRATIONS` (or
  `auth_v00N.sql` to `AUTH_MIGRATIONS`). Each runs in its own transaction with its version
  recorded in the same transaction, so a crash leaves the file at the previous version.
- **Data changes are migrations too**, and must leave an operator's own values alone: guard each
  update by the old value.
- **A migration makes a minor release.** A patch release never migrates; the release packaging
  refuses one that does. The minor release's entry in `empyrean/releases.toml` says the oldest
  version that can upgrade straight to it (`RELEASING.md`, "the upgrade declaration"); the
  migration itself is read from this crate's migration lists into the release's `release.json`,
  which the server's updater reads.
- **Before a release is installed over them, the updater backs both files up** the same way
  (`upgrade::snapshot`, as `<file>.backup-update-v<old>-v<new>-<UTC timestamp>`), and restores
  them (`upgrade::restore`) when the new release fails its health check.
- **The server backs up before it migrates.** With a migration pending on an existing schema it
  first copies the file beside itself, as `<file>.backup-<schema>-v<from>-v<to>-<UTC timestamp>`,
  with SQLite's online backup; a failed backup stops the open, and nothing is migrated. The newest
  three backups of each file are kept (`empyrean_common::backups::KEEP_BACKUPS`).
- **An older build refuses a newer file.** A database whose version is above the migrations the
  build knows is not opened; the message names both versions and the newest backup.

## Every release adds its fixture

`tests/fixtures/upgrade/<version>/` holds a small synthetic database pair each release made
(`shard.sqlite`, `auth.sqlite`: a few accounts, characters and items, no retail data) and
`expected.json`, what they held as that release's store read them back. The harness
(`tests/all/persistence/upgrade_fixtures.rs`) opens a copy of every fixture with the current build
and checks that each upgrades to the current schemas (with a backup exactly when a migration ran)
and still holds what `expected.json` says.

`cargo xtask release empyrean <version>` makes it: after setting the version, it runs the
generator when the release has no fixture yet and commits the three files with the version (or on
their own, when the version was already set). By hand, after the release's last schema change:

```
cargo run -p empyrean-store --example upgrade_fixture -- empyrean/crates/store/tests/fixtures/upgrade/<version>
```

The folder is named by the numeric version (`0.2.0-rc.1` makes `0.2.0`). The generator refuses a
folder that exists, so a committed fixture is never rewritten. The harness fails when the current
version has no fixture.

Every migration must pass the harness over every earlier release's fixture. A migration that
deliberately changes stored values (so an older `expected.json` no longer matches) changes the
harness to expect the new values for the releases it affects, in the same commit; it never
rewrites an old fixture.
