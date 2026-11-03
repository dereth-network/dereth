//! Yalain's updater, as Empyrean's: a release served from a stand-in repository on loopback is
//! found, checked, installed over a stopped installation in a temporary folder with its database
//! backed up first, and health-checked; what Empyrean's refuses is refused; the world database is
//! fetched first; and the running server's thread counts down before anything is installed. Fixture:
//! temporary installations, synthetic release archives on 127.0.0.1, programs stood in for by files
//! saying what they are, and a real SQLite file for the database.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use dereth_server_update::install::{Layout, State};
use dereth_server_update::policy::Policy;
use dereth_server_update::release::{target_triple, ReleaseIndex};
use dereth_server_update::running::{self, AutoUpdate, Cycle, World};
use dereth_server_update::source::{self, Source};
use dereth_server_update::testing::{Release, Repository};
use dereth_server_update::Check;
use serde_json::json;

use super::{Databases, Declaration, Facts, Programs, Settings, Updater, Yalain, SOURCE_URL};

const EXE: &str = std::env::consts::EXE_SUFFIX;
const PIN: &str = "v0.23.0";

/// A folder of its own under the system's temporary folder, emptied. Each test names its own.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("yalain-update-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

fn facts(version: &str, store: i64, world_database: &str) -> Facts {
    Facts {
        version: version.to_owned(),
        target: target_triple(None),
        databases: Databases { store },
        world_database: world_database.to_owned(),
    }
}

/// Stands in for running a release's programs: a "program" is a JSON file saying what it is and
/// whether it comes up. A program that does not come up migrates the database first, as a real one
/// would before failing later in its start-up. Every run is recorded, in turn.
#[derive(Debug)]
struct Stand {
    database: PathBuf,
    runs: Arc<Mutex<Vec<String>>>,
    fetch_fails: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Program {
    facts: Facts,
    healthy: bool,
}

fn program(facts: Facts, healthy: bool) -> Vec<u8> {
    serde_json::to_vec(&Program { facts, healthy }).expect("JSON")
}

fn read_program(exe: &Path) -> Result<Program, String> {
    serde_json::from_slice(&std::fs::read(exe).map_err(|e| e.to_string())?)
        .map_err(|e| format!("{} does not run here: {e}", exe.display()))
}

impl Stand {
    /// Where `exe` is, as the record says: in the staging folder or installed.
    fn record(&self, what: &str, exe: &Path) {
        let place = if exe.to_string_lossy().contains(".yalain-update") {
            "staged"
        } else {
            "installed"
        };
        self.runs
            .lock()
            .expect("the record")
            .push(format!("{what} {place}"));
    }
}

impl Programs for Stand {
    fn facts(&self, exe: &Path) -> Result<Facts, String> {
        self.record("facts", exe);
        Ok(read_program(exe)?.facts)
    }

    fn trial(&self, exe: &Path) -> Result<Facts, String> {
        self.record("trial", exe);
        let p = read_program(exe)?;
        let db = rusqlite::Connection::open(&self.database).map_err(|e| e.to_string())?;
        db.pragma_update(None, "user_version", p.facts.databases.store)
            .map_err(|e| e.to_string())?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS added_by_the_new_program (x INTEGER)")
            .map_err(|e| e.to_string())?;
        drop(db);
        if p.healthy {
            Ok(p.facts)
        } else {
            Err("the new server stopped before it was ready".to_owned())
        }
    }

    fn fetch_world_database(&self, exe: &Path) -> Result<(), String> {
        self.record("fetch", exe);
        if self.fetch_fails {
            Err("the new program could not fetch its world database".to_owned())
        } else {
            Ok(())
        }
    }
}

/// An installation of 0.1.0 in a temporary folder: the program, a guide and a database at schema 2
/// holding a marker row.
struct Installation {
    dir: PathBuf,
    layout: Layout<Yalain>,
    runs: Arc<Mutex<Vec<String>>>,
    repository: Repository,
}

impl Installation {
    fn new(name: &str) -> Self {
        let dir = scratch(name);
        std::fs::write(
            dir.join(format!("yalain{EXE}")),
            program(facts("0.1.0", 2, PIN), true),
        )
        .expect("the program");
        std::fs::write(dir.join("README.md"), b"old guide").expect("the guide");
        let db = rusqlite::Connection::open(dir.join("yalain.sqlite")).expect("the database");
        db.execute_batch(
            "PRAGMA user_version = 2; CREATE TABLE marker (x INTEGER); INSERT INTO marker VALUES (7);",
        )
        .expect("its rows");
        drop(db);
        let layout = Layout::new(&dir, vec![dir.join("yalain.sqlite")], PathBuf::new());
        Self {
            dir,
            layout,
            runs: Arc::default(),
            repository: Repository::start("yalain", "yalain-v"),
        }
    }

    fn updater(&self) -> Updater {
        self.updater_with(false)
    }

    fn updater_with(&self, fetch_fails: bool) -> Updater {
        Updater {
            source: Source::configured(&self.repository.api(), SOURCE_URL).expect("a mirror"),
            agent: source::agent("yalain-test"),
            mine: facts("0.1.0", 2, PIN),
            layout: self.layout.clone(),
            build: Box::new(Stand {
                database: self.dir.join("yalain.sqlite"),
                runs: Arc::clone(&self.runs),
                fetch_fails,
            }),
            pack_inputs: None,
        }
    }

    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.dir.join(name)).expect("a file of the installation")
    }

    fn database_version(&self) -> i64 {
        let db = rusqlite::Connection::open(self.dir.join("yalain.sqlite")).expect("the database");
        db.pragma_query_value(None, "user_version", |r| r.get(0))
            .expect("its version")
    }

    fn runs(&self) -> Vec<String> {
        self.runs.lock().expect("the record").clone()
    }

    /// Publishes `version`, whose program reports `reports` and which declares `upgrade`.
    fn publish(
        &self,
        version: &str,
        reports: Facts,
        healthy: bool,
        upgrade: serde_json::Value,
        sums_edit: impl Fn(String) -> String,
    ) {
        self.repository.publish(
            &Release {
                version,
                files: vec![
                    (format!("yalain{EXE}"), program(reports, healthy)),
                    ("README.md".to_owned(), b"new guide".to_vec()),
                ],
                target: &target_triple(None),
                other_targets: &["some-other-target"],
                upgrade,
                prerelease: false,
            },
            sums_edit,
        );
    }

    /// Publishes a patch release that changes nothing but the program.
    fn publish_patch(&self, version: &str) {
        self.publish(
            version,
            facts(version, 2, PIN),
            true,
            declared("patch", "0.1.0", 2, PIN),
            |s| s,
        );
    }
}

impl Drop for Installation {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// `release.json`'s `upgrade`, as the release packaging writes it.
fn declared(
    kind: &str,
    upgrades_from: &str,
    store: i64,
    world_database: &str,
) -> serde_json::Value {
    json!({
        "kind": kind,
        "previous": "0.1.0",
        "upgrades_from": upgrades_from,
        "databases": {"store": store},
        "migrations": [],
        "world_database": world_database,
        "world_database_fetch": world_database != PIN,
        "config": [],
        "notes": "",
    })
}

fn taken(check: &Check<Declaration>) -> Option<String> {
    check.selection.take.as_ref().map(|(v, _)| v.to_string())
}

fn why(check: &Check<Declaration>, version: &str) -> String {
    check
        .selection
        .reported
        .iter()
        .find(|(v, _)| v.to_string() == version)
        .map_or_else(String::new, |(_, w)| w.clone())
}

/// A newer patch release is found on the source, its archive for this target checked against
/// `SHA256SUMS` and `release.json`, staged, its program run once, and installed: the old files moved
/// aside, the database backed up first, the new program started for its health check.
/// Behaviour: server.update.newer-release-installed
#[test]
fn a_newer_release_is_found_checked_installed_and_health_checked() {
    let inst = Installation::new("found");
    inst.publish_patch("0.1.1");
    let updater = inst.updater();
    let check = updater.check(Policy::Patch).expect("the check");
    assert_eq!(taken(&check).as_deref(), Some("0.1.1"));
    let staged = updater.stage(&check).expect("staged").expect("a release");
    assert_eq!(
        inst.read("README.md"),
        b"old guide",
        "nothing installed yet"
    );
    let installed = updater.install(&staged).expect("installed");

    assert_eq!(installed.facts.version, "0.1.1");
    assert_eq!(inst.read("README.md"), b"new guide");
    assert_eq!(
        read_program(&inst.dir.join(format!("yalain{EXE}")))
            .expect("the program")
            .facts
            .version,
        "0.1.1"
    );
    assert_eq!(
        inst.read(&format!("yalain{EXE}.previous-v0.1.0")),
        program(facts("0.1.0", 2, PIN), true),
        "the old program kept aside"
    );
    let (db, backup) = &installed.backups[0];
    assert_eq!(db, &inst.dir.join("yalain.sqlite"));
    assert!(
        backup
            .to_string_lossy()
            .contains("yalain.sqlite.backup-update-v0.1.0-v0.1.1-"),
        "{}",
        backup.display()
    );
    assert_eq!(
        inst.runs(),
        ["facts staged", "trial installed"],
        "the staged program once, then the installed one for its health check"
    );
}

/// `release.json` lists an archive for each target; the one taken is the one for the target this
/// build was made for, whichever comes first.
/// Behaviour: server.update.archive-for-this-target
#[test]
fn the_archive_taken_is_the_one_for_this_builds_target() {
    let inst = Installation::new("target");
    inst.publish_patch("0.1.1");
    let updater = inst.updater();
    let check = updater.check(Policy::Patch).expect("the check");
    let index = check.found[0].index.as_ref().expect("its release.json");
    assert_eq!(index.assets[0].target, "some-other-target");
    assert!(updater.stage(&check).expect("staged").is_some());

    let mut elsewhere = inst.updater();
    elsewhere.mine.target = "a-target-with-no-archive".to_owned();
    let check = elsewhere.check(Policy::Patch).expect("the check");
    let refused = elsewhere.stage(&check).expect_err("no archive for it");
    assert!(
        refused.contains("0.1.1 has no archive for a-target-with-no-archive"),
        "{refused}"
    );
    assert_eq!(inst.read("README.md"), b"old guide");
}

/// One repository holds the releases of many programs: only Yalain's tags are read.
/// Behaviour: server.update.reads-its-own-tags
#[test]
fn only_yalains_tags_are_read_from_a_repository_of_many_programs() {
    let inst = Installation::new("tags");
    inst.publish_patch("0.1.1");
    inst.repository.list_other("empyrean-v9.0.0");
    inst.repository.list_other("dereth-v8.0.0");
    let check = inst.updater().check(Policy::Minor).expect("the check");
    let found: Vec<String> = check
        .found
        .iter()
        .map(|f| f.published.version.to_string())
        .collect();
    assert_eq!(found, ["0.1.1"]);
}

/// A download whose SHA-256 is not the one `SHA256SUMS` lists is refused, with nothing installed
/// and the new program never run.
/// Behaviour: server.update.bad-checksum-refused
#[test]
fn a_checksum_that_does_not_match_refuses_the_release_and_installs_nothing() {
    let inst = Installation::new("checksum");
    inst.publish(
        "0.1.1",
        facts("0.1.1", 2, PIN),
        true,
        declared("patch", "0.1.0", 2, PIN),
        |sums| {
            let (first, rest) = sums.split_at(1);
            let other = if first == "0" { "1" } else { "0" };
            format!("{other}{rest}")
        },
    );
    let updater = inst.updater();
    let check = updater.check(Policy::Patch).expect("the check");
    let refused = updater.stage(&check).expect_err("refused");
    assert!(
        refused.contains("refused (a damaged or altered download)"),
        "{refused}"
    );
    assert_eq!(inst.read("README.md"), b"old guide");
    assert!(inst.runs().is_empty(), "{:?}", inst.runs());
}

/// A release whose own program reports another database schema or world database than its release
/// declares is refused.
/// Behaviour: server.update.not-what-it-declares-refused
#[test]
fn a_release_that_is_not_what_it_declares_is_refused() {
    let inst = Installation::new("declares");
    inst.publish(
        "0.1.1",
        facts("0.1.1", 3, PIN),
        true,
        declared("patch", "0.1.0", 2, PIN),
        |s| s,
    );
    let updater = inst.updater();
    let check = updater.check(Policy::Patch).expect("the check");
    let refused = updater.stage(&check).expect_err("refused");
    assert!(
        refused.contains("server reports database schema 3") && refused.ends_with(": refused"),
        "{refused}"
    );
    assert_eq!(inst.read("README.md"), b"old guide");
}

/// A release that cannot be upgraded to straight from this version is reported and waits; so do one
/// that needs a configuration key set by hand and one whose database is older than this build's.
/// Behaviour: server.update.needs-a-step-or-a-hand-refused
#[test]
fn a_release_that_cannot_upgrade_straight_or_needs_a_key_set_by_hand_is_not_taken() {
    let inst = Installation::new("refused");
    inst.publish(
        "0.2.0",
        facts("0.2.0", 2, PIN),
        true,
        declared("minor", "0.1.5", 2, PIN),
        |s| s,
    );
    let check = inst.updater().check(Policy::Minor).expect("the check");
    assert_eq!(taken(&check), None);
    assert_eq!(
        why(&check, "0.2.0"),
        "upgrading straight to it needs 0.1.5 or newer installed first"
    );

    let inst = Installation::new("by-hand");
    let mut upgrade = declared("minor", "0.1.0", 2, PIN);
    upgrade["config"] = json!([{
        "key": "server.some_key",
        "change": "required",
        "note": "say which",
    }]);
    inst.publish("0.2.0", facts("0.2.0", 2, PIN), true, upgrade, |s| s);
    let check = inst.updater().check(Policy::Minor).expect("the check");
    assert_eq!(taken(&check), None);
    assert_eq!(
        why(&check, "0.2.0"),
        "it needs the configuration key server.some_key set (say which); set it and install by hand"
    );

    let inst = Installation::new("older-database");
    inst.publish(
        "0.2.0",
        facts("0.2.0", 1, PIN),
        true,
        declared("minor", "0.1.0", 1, PIN),
        |s| s,
    );
    let check = inst.updater().check(Policy::Minor).expect("the check");
    assert_eq!(taken(&check), None);
    assert_eq!(
        why(&check, "0.2.0"),
        "its database schemas are older than this build's"
    );
}

/// A release that migrates the database backs it up first; when its health check fails, the old
/// program comes back, the database comes back from its backup with the rows it held, and the
/// release is recorded as failed and not tried again.
/// Behaviour: server.update.backup-and-way-back
#[test]
fn a_release_that_fails_its_health_check_is_rolled_back_with_its_database_and_not_retried() {
    let inst = Installation::new("rollback");
    inst.publish(
        "0.2.0",
        facts("0.2.0", 3, PIN),
        false,
        declared("minor", "0.1.0", 3, PIN),
        |s| s,
    );
    let updater = inst.updater();
    let check = updater.check(Policy::Minor).expect("the check");
    let (_, needs) = check.selection.take.as_ref().expect("taken");
    assert_eq!(
        needs.describe(),
        ["the store database is migrated from schema 2 to 3 (backed up first)"]
    );
    let staged = updater.stage(&check).expect("staged").expect("a release");
    let failed = updater
        .install(&staged)
        .expect_err("its health check fails");
    assert!(failed.contains("rolled back to 0.1.0"), "{failed}");

    assert_eq!(inst.read("README.md"), b"old guide");
    assert_eq!(
        inst.read(&format!("yalain{EXE}")),
        program(facts("0.1.0", 2, PIN), true)
    );
    assert_eq!(inst.database_version(), 2, "the database as it was");
    let db = rusqlite::Connection::open(inst.dir.join("yalain.sqlite")).expect("the database");
    let marker: i64 = db
        .query_row("SELECT x FROM marker", [], |r| r.get(0))
        .expect("its rows");
    assert_eq!(marker, 7);
    let added: i64 = db
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name = 'added_by_the_new_program'",
            [],
            |r| r.get(0),
        )
        .expect("the tables");
    assert_eq!(added, 0, "what the new program did is gone");
    drop(db);

    assert_eq!(
        State::load(&inst.layout.work_dir)
            .failed_versions()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["0.2.0"]
    );
    let again = updater.check(Policy::Minor).expect("the check");
    assert_eq!(taken(&again), None, "not tried again");
    assert!(
        why(&again, "0.2.0").starts_with("failed its health check on this server"),
        "{}",
        why(&again, "0.2.0")
    );
}

/// A release that serves a newer world database has it fetched by its own program while it is
/// staged, before anything is installed and before the new program starts; a fetch that fails
/// refuses the release with nothing installed. A release serving the same one fetches nothing.
/// Behaviour: server.update.world-database-fetched-first
#[test]
fn a_newer_world_database_is_fetched_by_the_new_program_before_anything_is_installed() {
    let newer = "v0.24.0";
    let inst = Installation::new("fetch");
    inst.publish(
        "0.2.0",
        facts("0.2.0", 2, newer),
        true,
        declared("minor", "0.1.0", 2, newer),
        |s| s,
    );
    let updater = inst.updater();
    let check = updater.check(Policy::Minor).expect("the check");
    let (_, needs) = check.selection.take.as_ref().expect("taken");
    assert_eq!(needs.world_database_fetch.as_deref(), Some(newer));
    let staged = updater.stage(&check).expect("staged").expect("a release");
    assert_eq!(inst.runs(), ["facts staged", "fetch staged"]);
    assert_eq!(
        inst.read("README.md"),
        b"old guide",
        "nothing installed yet"
    );
    updater.install(&staged).expect("installed");
    assert_eq!(
        inst.runs(),
        ["facts staged", "fetch staged", "trial installed"],
        "fetched before the new program starts"
    );

    let inst = Installation::new("fetch-fails");
    inst.publish(
        "0.2.0",
        facts("0.2.0", 2, newer),
        true,
        declared("minor", "0.1.0", 2, newer),
        |s| s,
    );
    let updater = inst.updater_with(true);
    let check = updater.check(Policy::Minor).expect("the check");
    let refused = updater.stage(&check).expect_err("refused");
    assert!(refused.contains("could not fetch"), "{refused}");
    assert_eq!(inst.read("README.md"), b"old guide");

    let inst = Installation::new("no-fetch");
    inst.publish_patch("0.1.1");
    let updater = inst.updater();
    let check = updater.check(Policy::Patch).expect("the check");
    updater.stage(&check).expect("staged").expect("a release");
    assert_eq!(inst.runs(), ["facts staged"], "the same world database");
}

/// `off` installs nothing, and says so of each newer release. `patch` takes the patch release that
/// changes nothing but the program, fetching a patch of the world database, and leaves a minor one;
/// and a patch release that migrates the database. `minor` takes the minor release, with its
/// migration. Without a setting, by hand means `minor`.
/// Behaviour: server.update.off-patch-minor
#[test]
fn off_patch_and_minor_take_what_they_take_on_empyrean() {
    let inst = Installation::new("policies");
    inst.publish_patch("0.1.1");
    inst.publish(
        "0.2.0",
        facts("0.2.0", 3, PIN),
        true,
        declared("minor", "0.1.0", 3, PIN),
        |s| s,
    );
    let updater = inst.updater();

    let off = updater.check(Policy::Off).expect("the check");
    assert_eq!(taken(&off), None);
    assert_eq!(why(&off, "0.1.1"), "server.update is \"off\"");
    assert_eq!(why(&off, "0.2.0"), "server.update is \"off\"");

    let patch = updater.check(Policy::Patch).expect("the check");
    assert_eq!(taken(&patch).as_deref(), Some("0.1.1"));
    assert!(
        why(&patch, "0.2.0").starts_with("a minor release: server.update = \"patch\" takes only"),
        "{}",
        why(&patch, "0.2.0")
    );

    let minor = updater.check(Policy::Minor).expect("the check");
    assert_eq!(taken(&minor).as_deref(), Some("0.2.0"));
    assert_eq!(why(&minor, "0.1.1"), "0.2.0 is newer and is taken instead");

    let request = dereth_server_update::command::Request {
        apply: false,
        config: None,
        policy: None,
    };
    assert_eq!(request.policy("off"), Policy::Minor);
    assert_eq!(request.policy("patch"), Policy::Patch);
    assert_eq!(request.policy("sometimes"), Policy::Minor);

    assert_eq!(running::policy_to_run("off"), None);
    assert_eq!(running::policy_to_run("sometimes"), None);
    assert_eq!(running::policy_to_run("Patch "), Some(Policy::Patch));
    assert_eq!(running::policy_to_run("minor"), Some(Policy::Minor));

    // A patch of the world database is a download: `patch` takes it, and says so.
    let inst = Installation::new("world-patch");
    inst.publish(
        "0.1.1",
        facts("0.1.1", 2, "v0.23.1"),
        true,
        declared("patch", "0.1.0", 2, "v0.23.1"),
        |s| s,
    );
    let check = inst.updater().check(Policy::Patch).expect("the check");
    let (_, needs) = check.selection.take.as_ref().expect("taken");
    assert_eq!(
        needs.describe(),
        ["the world database v0.23.1 is fetched first"]
    );

    // A patch that migrates the database is more than `patch` takes.
    let inst = Installation::new("patch-migrates");
    inst.publish(
        "0.1.1",
        facts("0.1.1", 3, PIN),
        true,
        declared("patch", "0.1.0", 3, PIN),
        |s| s,
    );
    let check = inst.updater().check(Policy::Patch).expect("the check");
    assert_eq!(taken(&check), None);
    assert!(
        why(&check, "0.1.1")
            .contains("server.update = \"patch\" takes only releases that change nothing else"),
        "{}",
        why(&check, "0.1.1")
    );
}

/// What `update --check` and the running server's log say, in Empyrean's words with Yalain's name.
/// Behaviour: server.update.says-what-it-would-do
#[test]
fn the_check_says_what_it_would_install_and_why_it_would_not() {
    let inst = Installation::new("describe");
    inst.publish_patch("0.1.1");
    inst.publish(
        "0.2.0",
        facts("0.2.0", 2, PIN),
        true,
        declared("minor", "0.1.0", 2, PIN),
        |s| s,
    );
    let updater = inst.updater();
    let check = updater.check(Policy::Patch).expect("the check");
    let target = target_triple(None);
    assert_eq!(
        dereth_server_update::describe::<Yalain>(&updater.mine, &check),
        [
            format!("This server: Yalain 0.1.0 for {target}; server.update = \"patch\""),
            "0.2.0: not installed: a minor release: server.update = \"patch\" takes only 0.1.x"
                .to_owned(),
            "0.1.1: would be installed:".to_owned(),
            "  - nothing but the binaries changes".to_owned(),
        ]
    );
    let none = Installation::new("describe-none");
    let check = none.updater().check(Policy::Patch).expect("the check");
    assert_eq!(
        dereth_server_update::describe::<Yalain>(&none.updater().mine, &check)[1..],
        [
            "No newer release is published.",
            "Nothing would be installed."
        ]
    );
}

/// The world, as the running server's thread reaches it: its countdown, which the test ends or
/// cancels.
#[derive(Debug, Default)]
struct StandWorld {
    /// The seconds of each countdown asked for.
    asked: Mutex<Vec<u32>>,
    /// What each look at the countdown answers, in turn; then that the world has gone.
    looks: Mutex<Vec<Option<bool>>>,
    looked: AtomicUsize,
}

impl World for StandWorld {
    fn begin_countdown(&self, seconds: u32) -> Option<bool> {
        self.asked.lock().expect("the record").push(seconds);
        Some(true)
    }

    fn counting_down(&self) -> Option<bool> {
        self.looked.fetch_add(1, Ordering::SeqCst);
        let mut looks = self.looks.lock().expect("the record");
        if looks.is_empty() {
            None
        } else {
            looks.remove(0)
        }
    }
}

/// The running server stages a release with the world still running, then warns players with a
/// countdown of `server.update_warning_seconds`: nothing is installed until it has run out and the
/// world has stopped by itself. Stopped from outside, it installs nothing. No clock: the thread's
/// waits are the test's.
/// Behaviour: server.update.players-warned-first
#[test]
fn the_running_server_warns_players_first_and_installs_only_after_the_world_ends_by_itself() {
    let inst = Installation::new("running");
    inst.publish_patch("0.1.1");
    let auto = AutoUpdate::new(inst.updater());
    let world = StandWorld {
        looks: Mutex::new(vec![Some(true), Some(true)]),
        ..StandWorld::default()
    };
    let mut waits = Vec::new();
    let cycle = running::cycle(&auto, Policy::Patch, 90, &world, &mut |d| waits.push(d));
    assert_eq!(cycle, Cycle::WorldGone, "the world ended");
    assert_eq!(*world.asked.lock().expect("the record"), [90]);
    assert_eq!(waits, [running::COUNTDOWN_POLL; 3]);
    assert_eq!(auto.staged_version().as_deref(), Some("0.1.1"));
    assert_eq!(inst.read("README.md"), b"old guide", "nothing yet");

    let exe = auto.install_staged(true, || {}).expect("installed");
    assert_eq!(exe, inst.dir.join(format!("yalain{EXE}")));
    assert_eq!(inst.read("README.md"), b"new guide");

    // Stopped from outside: staged, and not installed.
    let inst = Installation::new("running-outside");
    inst.publish_patch("0.1.1");
    let auto = AutoUpdate::new(inst.updater());
    let world = StandWorld::default();
    running::cycle(&auto, Policy::Patch, 90, &world, &mut |_| {});
    assert_eq!(auto.staged_version().as_deref(), Some("0.1.1"));
    assert_eq!(auto.install_staged(false, || {}), None);
    assert_eq!(inst.read("README.md"), b"old guide");
}

/// An operator who cancels the shutdown cancels the update: nothing is staged any more, and the
/// next check stages it again.
/// Behaviour: server.update.cancelled-with-the-shutdown
#[test]
fn a_cancelled_countdown_cancels_the_update_and_the_next_check_stages_it_again() {
    let inst = Installation::new("cancel");
    inst.publish_patch("0.1.1");
    let auto = AutoUpdate::new(inst.updater());
    let world = StandWorld {
        looks: Mutex::new(vec![Some(true), Some(false)]),
        ..StandWorld::default()
    };
    let cycle = running::cycle(&auto, Policy::Patch, 90, &world, &mut |_| {});
    assert_eq!(cycle, Cycle::Again);
    assert_eq!(auto.staged_version(), None);
    assert_eq!(auto.install_staged(true, || {}), None);
    assert_eq!(inst.read("README.md"), b"old guide");

    let world = StandWorld::default();
    running::cycle(&auto, Policy::Patch, 90, &world, &mut |_| {});
    assert_eq!(auto.staged_version().as_deref(), Some("0.1.1"));
}

/// The source is the repository the build came from, a GitHub repository named instead, or a mirror
/// of the releases API; anything else is refused.
/// Behaviour: server.update.source
#[test]
fn the_release_source_is_the_builds_repository_or_a_configured_mirror() {
    let own = Source::configured("", SOURCE_URL).expect("the build's");
    assert_eq!(own, Source::github(SOURCE_URL).expect("a repository"));
    assert!(own.api.starts_with("https://api.github.com/repos/"));
    let settings = Settings::default();
    let updater = super::for_this_installation(&settings, None, Vec::new()).expect("the updater");
    assert_eq!(updater.source, own);
    let github = Source::configured("https://github.com/someone/fork/", SOURCE_URL).expect("one");
    assert_eq!(github.repo, "someone/fork");
    let mirror = Source::configured("http://127.0.0.1:8080/api/repos/someone/fork", SOURCE_URL)
        .expect("one");
    assert_eq!(mirror.api, "http://127.0.0.1:8080/api/repos/someone/fork");
    for bad in [
        "someone/fork",
        "https://example.org/someone/fork",
        "ftp://x/repos/a/b",
    ] {
        assert!(Source::configured(bad, SOURCE_URL).is_err(), "{bad}");
    }
}

/// `server.update`, `server.update_check_hours`, `server.update_warning_seconds` and
/// `server.update_source` are Yalain's own settings, with Empyrean's defaults, read without a
/// warning.
/// Behaviour: server.update.settings
#[test]
fn the_update_keys_are_read_with_empyreans_defaults() {
    let defaults = crate::parse_from(Vec::<String>::new()).expect("the defaults");
    assert_eq!(
        defaults.update,
        Settings {
            update: "off".to_owned(),
            check_hours: 6,
            warning_seconds: 300,
            source: String::new(),
            own_world: false,
        }
    );
    let text = "[server]\nupdate = \"patch\"\nupdate_check_hours = 2\nupdate_warning_seconds = 60\nupdate_source = \"https://github.com/someone/fork\"\n";
    let (args, warnings) = crate::config::arguments(text, Path::new("/worlds")).expect("it reads");
    assert_eq!(warnings, Vec::<String>::new());
    let o = crate::parse_from(args).expect("the options");
    assert_eq!(
        o.update,
        Settings {
            update: "patch".to_owned(),
            check_hours: 2,
            warning_seconds: 60,
            source: "https://github.com/someone/fork".to_owned(),
            own_world: false,
        }
    );
}

/// The command takes what Empyrean's does, and refuses what it refuses, with the same words.
/// Behaviour: server.update.command-line
#[test]
fn the_update_command_takes_check_or_apply_and_a_config_and_a_policy() {
    use dereth_server_update::command::Request;
    let parse = |args: &[&str]| Request::parse(args.iter().map(|a| (*a).to_owned()));
    assert_eq!(parse(&[]).unwrap_err(), "give --check or --apply");
    assert_eq!(
        parse(&["--policy", "soon", "--check"]).unwrap_err(),
        "--policy takes off, patch or minor"
    );
    assert_eq!(
        parse(&["--check", "--apply"]).unwrap_err(),
        "unexpected argument `--apply`"
    );
    assert_eq!(parse(&["--config"]).unwrap_err(), "--config needs a path");
    let r = parse(&["--apply", "--config", "w.toml", "--policy", "patch"]).expect("reads");
    assert!(r.apply);
    assert_eq!(r.config.as_deref(), Some("w.toml"));
    assert_eq!(r.policy, Some(Policy::Patch));
    assert_eq!(
        super::USAGE,
        "usage: yalain update --check|--apply [--config <yalain.toml>] [--policy <off|patch|minor>]"
    );
}

/// What the release packaging writes for a release is what the updater reads, and what
/// `--release-facts` prints is the facts it compares it with.
/// Behaviour: server.update.reads-what-packaging-writes
#[test]
fn the_release_json_the_packaging_writes_and_the_facts_the_program_prints_are_read() {
    let text = json!({
        "schema": 2,
        "product": "yalain",
        "version": "0.2.0",
        "tag": "yalain-v0.2.0",
        "commit": "abc",
        "commit_time": "2026-11-03T00:00:00Z",
        "source_url": SOURCE_URL,
        "prerelease": false,
        "assets": [{
            "target": "x86_64-unknown-linux-gnu",
            "file": "yalain-0.2.0-x86_64-unknown-linux-gnu.tar.gz",
            "size": 10,
            "sha256": "00",
        }],
        "upgrade": {
            "kind": "minor",
            "previous": "0.1.1",
            "upgrades_from": "0.1.1",
            "databases": {"store": 3},
            "migrations": [{"database": "store", "from": 2, "to": 3}],
            "world_database": "v0.24.0",
            "world_database_fetch": true,
            "config": [],
            "notes": "",
        },
    })
    .to_string();
    let index = ReleaseIndex::<Declaration>::parse(&text).expect("it reads");
    let upgrade = index.upgrade.expect("its declaration");
    assert_eq!(upgrade.databases.store, 3);
    assert_eq!(upgrade.world_database, "v0.24.0");
    let mine = Facts::this_build();
    let back: Facts =
        dereth_server_update::install::facts_from_json(&mine.to_json()).expect("it reads back");
    assert_eq!(back, mine);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&mine.to_json()).expect("JSON")["databases"]
            ["store"],
        yalain_store::SCHEMA_VERSION
    );
}

/// A world serving its own folder does not use the release a build pins, so an update needs no fetch
/// of it, and one that cannot be fetched refuses nothing: the new program is not even run for it. A
/// world that lies over a base naming no version does use it, as `pinned` does.
/// Behaviour: server.update.own-world-needs-no-fetch
#[test]
fn a_world_serving_its_own_folder_needs_no_fetch_of_the_pinned_world_database() {
    let run = |own_world: bool| {
        super::Processes {
            args: Vec::new(),
            current_dir: std::env::temp_dir(),
            timeout: std::time::Duration::from_secs(1),
            work_dir: std::env::temp_dir(),
            own_world,
        }
        .fetch_world_database(Path::new("/no/such/program"))
    };
    assert!(run(false).is_err(), "the fetch runs the new program");
    assert_eq!(run(true), Ok(()));

    let world = |dir: &Path, base: &str| {
        std::fs::write(
            dir.join("world.toml"),
            format!(
                "format_version = 1\nkey = \"w\"\ngame = \"ac2\"\nera = \"eor\"\npacks = []\n{base}"
            ),
        )
        .expect("written");
    };
    let dir = scratch("own-world");
    let (text, base) = (dir.to_string_lossy().into_owned(), Path::new("/some/base"));
    let uses = crate::world::uses_the_pin;
    assert!(uses("pinned", None));
    assert!(uses(&format!("{text}/nothing-here"), None), "does not read");
    world(&dir, "");
    assert!(!uses(&text, None), "a folder alone");
    let o = crate::parse_from(vec!["--world-db".into(), text.clone()]).expect("the options");
    assert!(o.update.own_world, "as the options say");
    world(&dir, "[base]\ndb = \"db\"\n");
    assert!(uses(&text, None), "over the pinned release");
    assert!(!uses(&text, Some(base)), "over a folder of its own");
    world(&dir, "[base]\ndb = \"db\"\nversion = \"v0.1.0\"\n");
    assert!(!uses(&text, None), "over a release it names");
    std::fs::remove_dir_all(&dir).ok();
}
