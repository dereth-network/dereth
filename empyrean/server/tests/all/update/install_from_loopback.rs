//! Divergence: V385
//! ACE: Program.CheckForServerUpdate (ACE only reports a newer ACE release; Empyrean installs its
//! own), ServerManager.BeginShutdown (the countdown before the restart)
//! Behaviour: none (a server tooling claim: how a release is fetched, checked, installed and
//! rolled back, not game behaviour).
//! A release served from a loopback web server is found, checked against SHA256SUMS and
//! release.json, unpacked, installed over a stopped installation with its databases backed up
//! first, and health-checked; a checksum mismatch refuses it with nothing installed; a release
//! that fails its health check is rolled back (the old files, pack and databases back) and not
//! retried; the release source is the build's repository or a configured mirror; the built
//! server reports its own facts and fails its health check without a world.
//! Fixture: temporary installations, synthetic release archives served on 127.0.0.1, the built
//! server binary; real content (retail dats and world.pack) only for the passing health check.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use empyrean_server::update::install::{self, Build, Layout, PackInputs, Processes};
use empyrean_server::update::policy::Policy;
use empyrean_server::update::release::{Databases, Facts, WorldPack};
use empyrean_server::update::source::Source;
use empyrean_server::update::verify;
use empyrean_server::update::{source, Updater};

use super::Loopback;

const EXE: &str = std::env::consts::EXE_SUFFIX;

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("update")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn facts(version: &str, shard: i64, pack_format: u32) -> Facts {
    Facts {
        version: version.to_owned(),
        target: empyrean_server::update::release::target_triple(),
        databases: Databases { shard, auth: 1 },
        world_pack: WorldPack {
            format: pack_format,
            schema: 100,
        },
    }
}

/// Stands in for running a release's programs: a "binary" is a JSON file saying what it is and
/// whether it comes up. A binary that does not come up migrates the shard database first, as a
/// real one would before failing later in its start-up.
#[derive(Debug)]
struct FakeBuild {
    shard: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FakeBinary {
    facts: Facts,
    healthy: bool,
}

fn fake_binary(facts: Facts, healthy: bool) -> Vec<u8> {
    serde_json::to_vec(&FakeBinary { facts, healthy }).unwrap()
}

impl Build for FakeBuild {
    fn facts(&self, exe: &Path) -> Result<Facts, String> {
        let b: FakeBinary = serde_json::from_slice(&std::fs::read(exe).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{} does not run here: {e}", exe.display()))?;
        Ok(b.facts)
    }

    fn trial(&self, exe: &Path) -> Result<Facts, String> {
        let b: FakeBinary = serde_json::from_slice(&std::fs::read(exe).map_err(|e| e.to_string())?)
            .map_err(|e| format!("{} does not run: {e}", exe.display()))?;
        let conn = rusqlite::Connection::open(&self.shard).unwrap();
        conn.pragma_update(None, "user_version", b.facts.databases.shard)
            .unwrap();
        conn.execute_batch("CREATE TABLE IF NOT EXISTS added_by_new_build (x INTEGER)")
            .unwrap();
        drop(conn);
        if b.healthy {
            Ok(b.facts)
        } else {
            Err("the new server stopped before it was ready".to_owned())
        }
    }

    fn rebuild_pack(
        &self,
        _importer: &Path,
        inputs: &PackInputs,
        out: &Path,
    ) -> Result<(), String> {
        std::fs::write(out, format!("pack from {}", inputs.sql.display()))
            .map_err(|e| e.to_string())
    }
}

/// An installation of 0.1.0: the two binaries, a guide, a shard database at schema 1, an
/// authentication database and a world pack.
struct Installation {
    dir: PathBuf,
    layout: Layout,
}

impl Installation {
    fn new(name: &str) -> Self {
        let dir = scratch(name);
        std::fs::write(
            dir.join(format!("empyrean-server{EXE}")),
            fake_binary(facts("0.1.0", 1, 1), true),
        )
        .unwrap();
        std::fs::write(dir.join(format!("empyrean-import{EXE}")), b"old importer").unwrap();
        std::fs::write(dir.join("README.md"), b"old guide").unwrap();
        std::fs::write(dir.join("world.pack"), b"old pack").unwrap();
        let shard = dir.join("shard.db");
        let conn = rusqlite::Connection::open(&shard).unwrap();
        conn.execute_batch("CREATE TABLE hero (name TEXT); INSERT INTO hero VALUES ('Asheron');")
            .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        drop(conn);
        let auth = dir.join("auth.db");
        rusqlite::Connection::open(&auth)
            .unwrap()
            .execute_batch("CREATE TABLE account (name TEXT);")
            .unwrap();
        let layout = Layout::new(&dir, vec![shard, auth], dir.join("world.pack"));
        Self { dir, layout }
    }

    fn read(&self, name: &str) -> Vec<u8> {
        std::fs::read(self.dir.join(name)).unwrap()
    }

    fn shard_version(&self) -> i64 {
        rusqlite::Connection::open(self.dir.join("shard.db"))
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap()
    }

    fn updater(&self, web: &Loopback) -> Updater {
        Updater {
            source: Source {
                api: format!("{}/repos/example/empyrean", web.base),
                repo: "example/empyrean".to_owned(),
            },
            agent: source::agent(),
            mine: facts("0.1.0", 1, 1),
            layout: self.layout.clone(),
            build: Box::new(FakeBuild {
                shard: self.dir.join("shard.db"),
            }),
            pack_inputs: Some(PackInputs {
                sql: self.dir.join("dump.sql"),
                patches: Vec::new(),
            }),
        }
    }
}

/// A release of `version` whose server binary reports `binary_facts`, published on `web` with
/// its archive, `release.json` and `SHA256SUMS` (`sums_edit` may alter the sums).
fn publish(
    web: &Loopback,
    version: &str,
    kind: &str,
    declared: &Facts,
    binary_facts: Facts,
    healthy: bool,
    sums_edit: impl Fn(String) -> String,
) {
    let target = empyrean_server::update::release::target_triple();
    let root = format!("empyrean-{version}-{target}");
    let file = format!("{root}.zip");
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().unix_permissions(0o755);
    zip.add_directory(format!("{root}/"), opts).unwrap();
    for (name, bytes) in [
        (
            format!("empyrean-server{EXE}"),
            fake_binary(binary_facts, healthy),
        ),
        (format!("empyrean-import{EXE}"), b"new importer".to_vec()),
        ("README.md".to_owned(), b"new guide".to_vec()),
    ] {
        zip.start_file(format!("{root}/{name}"), opts).unwrap();
        zip.write_all(&bytes).unwrap();
    }
    let archive = zip.finish().unwrap().into_inner();
    let index = serde_json::json!({
        "schema": 2,
        "product": "empyrean",
        "version": version,
        "prerelease": false,
        "assets": [{"target": target, "file": file, "size": archive.len(), "sha256": verify::sha256_hex(&archive)}],
        "upgrade": {
            "kind": kind,
            "previous": "0.1.0",
            "upgrades_from": "0.1.0",
            "databases": declared.databases,
            "world_pack": declared.world_pack,
            "migrations": [],
            "world_pack_rebuild": declared.world_pack.format != 1,
            "config": []
        }
    })
    .to_string();
    let sums = sums_edit(format!(
        "{}  {file}\n{}  release.json\n",
        verify::sha256_hex(&archive),
        verify::sha256_hex(index.as_bytes())
    ));
    let tag = format!("empyrean-v{version}");
    let dl = |name: &str| format!("{}/download/{tag}/{name}", web.base);
    web.put(&format!("/download/{tag}/{file}"), archive);
    web.put(&format!("/download/{tag}/release.json"), index);
    web.put(&format!("/download/{tag}/SHA256SUMS"), sums);
    let list = serde_json::json!([
        {"tag_name": tag, "draft": false, "prerelease": false, "assets": [
            {"name": file, "browser_download_url": dl(&file)},
            {"name": "release.json", "browser_download_url": dl("release.json")},
            {"name": "SHA256SUMS", "browser_download_url": dl("SHA256SUMS")}
        ]},
        {"tag_name": "empyrean-v0.1.0", "draft": false, "prerelease": false, "assets": []},
        {"tag_name": "empyrean-v9.0.0", "draft": true, "prerelease": false, "assets": []}
    ]);
    web.put(
        "/repos/example/empyrean/releases?per_page=100",
        list.to_string(),
    );
}

#[test]
fn a_patch_release_is_found_checked_installed_and_health_checked() {
    let inst = Installation::new("patch");
    let web = Loopback::start();
    let new = facts("0.1.1", 1, 1);
    publish(&web, "0.1.1", "patch", &new, new.clone(), true, |s| s);
    let updater = inst.updater(&web);

    let check = updater.check(Policy::Patch).unwrap();
    let (version, needs) = check.selection.take.clone().expect("0.1.1 is taken");
    assert_eq!(version.to_string(), "0.1.1");
    assert!(needs.is_empty());
    assert_eq!(
        check.found.len(),
        1,
        "0.1.0 and the draft are not newer releases"
    );

    let staged = updater.stage(&check).unwrap().expect("staged");
    assert_eq!(staged.world_pack, None);
    // Staging touches nothing installed.
    assert_eq!(inst.read("README.md"), b"old guide");

    let installed = updater.install(&staged).unwrap();
    assert_eq!(installed.facts.version, "0.1.1");
    assert_eq!(inst.read("README.md"), b"new guide");
    assert_eq!(inst.read(&format!("empyrean-import{EXE}")), b"new importer");
    assert_eq!(
        inst.read(&format!("empyrean-server{EXE}.previous-v0.1.0")),
        fake_binary(facts("0.1.0", 1, 1), true),
        "the old server is kept aside"
    );
    assert_eq!(inst.read("world.pack"), b"old pack");
    // Each database was backed up first, as the store backs up before a migration.
    assert_eq!(installed.backups.len(), 2);
    for (db, backup) in &installed.backups {
        let name = backup.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            name.starts_with(&format!(
                "{}.backup-update-v0.1.0-v0.1.1-",
                db.file_name().unwrap().to_string_lossy()
            )),
            "{name}"
        );
    }
    assert!(install::State::load(&inst.layout.work_dir)
        .failed
        .is_empty());
}

#[test]
fn a_minor_release_rebuilds_the_world_pack_before_the_server_stops() {
    let inst = Installation::new("minor");
    let web = Loopback::start();
    let new = facts("0.2.0", 2, 2);
    publish(&web, "0.2.0", "minor", &new, new.clone(), true, |s| s);
    let updater = inst.updater(&web);

    assert!(
        updater
            .check(Policy::Patch)
            .unwrap()
            .selection
            .take
            .is_none(),
        "patch does not take a minor release"
    );
    let check = updater.check(Policy::Minor).unwrap();
    let staged = updater.stage(&check).unwrap().expect("staged");
    let pack = staged.world_pack.clone().expect("a rebuilt pack");
    assert!(pack.is_file());
    assert_eq!(inst.read("world.pack"), b"old pack", "not installed yet");
    assert!(staged
        .needs
        .iter()
        .any(|l| l.contains("from schema 1 to 2")));

    updater.install(&staged).unwrap();
    assert!(String::from_utf8(inst.read("world.pack"))
        .unwrap()
        .starts_with("pack from"));
    assert_eq!(inst.read("world.pack.previous-v0.1.0"), b"old pack");
    assert_eq!(inst.shard_version(), 2);
}

#[test]
fn a_checksum_that_does_not_match_refuses_the_release_and_installs_nothing() {
    let inst = Installation::new("checksum");
    let web = Loopback::start();
    let new = facts("0.1.1", 1, 1);
    publish(&web, "0.1.1", "patch", &new, new.clone(), true, |sums| {
        let (first, rest) = sums.split_at(1);
        let flipped = if first == "0" { "1" } else { "0" };
        format!("{flipped}{rest}")
    });
    let updater = inst.updater(&web);
    let check = updater.check(Policy::Patch).unwrap();
    let err = updater.stage(&check).unwrap_err();
    assert!(err.contains("SHA-256") && err.contains("refused"), "{err}");
    assert!(!inst.layout.work_dir.join("staged").exists());
    assert_eq!(inst.read("README.md"), b"old guide");
}

#[test]
fn a_release_that_is_not_what_it_declares_is_refused() {
    let inst = Installation::new("undeclared");
    let web = Loopback::start();
    // Declared as changing nothing, but its server migrates the shard.
    let declared = facts("0.1.1", 1, 1);
    publish(
        &web,
        "0.1.1",
        "patch",
        &declared,
        facts("0.1.1", 2, 1),
        true,
        |s| s,
    );
    let updater = inst.updater(&web);
    let check = updater.check(Policy::Patch).unwrap();
    let err = updater.stage(&check).unwrap_err();
    assert!(err.contains("declares"), "{err}");
}

/// `server.update_source`: empty is the build's own GitHub repository; a GitHub repository or a
/// mirror of the releases API names the repository by its last two path segments.
#[test]
fn the_release_source_is_the_builds_repository_or_a_configured_mirror() {
    let own = Source::configured("").unwrap();
    assert_eq!(own, Source::this_build().unwrap());
    assert!(own.api.starts_with("https://api.github.com/repos/"));
    let github = Source::configured("https://github.com/someone/fork/").unwrap();
    assert_eq!(github.api, "https://api.github.com/repos/someone/fork");
    assert_eq!(github.repo, "someone/fork");
    let mirror = Source::configured("http://127.0.0.1:8080/api/repos/someone/fork").unwrap();
    assert_eq!(mirror.api, "http://127.0.0.1:8080/api/repos/someone/fork");
    assert_eq!(mirror.repo, "someone/fork");
    for bad in [
        "someone/fork",
        "https://example.org/someone/fork",
        "ftp://x/repos/a/b",
    ] {
        assert!(Source::configured(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_release_that_fails_its_health_check_is_rolled_back_and_not_retried() {
    let inst = Installation::new("rollback");
    let web = Loopback::start();
    let new = facts("0.2.0", 2, 2);
    publish(&web, "0.2.0", "minor", &new, new.clone(), false, |s| s);
    let updater = inst.updater(&web);
    let old_server = inst.read(&format!("empyrean-server{EXE}"));

    let check = updater.check(Policy::Minor).unwrap();
    let staged = updater.stage(&check).unwrap().expect("staged");
    let err = updater.install(&staged).unwrap_err();
    assert!(err.contains("rolled back to 0.1.0"), "{err}");

    // The old installation is back, and its database as it was before the new build migrated it.
    assert_eq!(inst.read(&format!("empyrean-server{EXE}")), old_server);
    assert_eq!(inst.read(&format!("empyrean-import{EXE}")), b"old importer");
    assert_eq!(inst.read("README.md"), b"old guide");
    assert_eq!(inst.read("world.pack"), b"old pack");
    assert_eq!(inst.shard_version(), 1);
    let tables: i64 = rusqlite::Connection::open(inst.dir.join("shard.db"))
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'added_by_new_build'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 0);
    // The old server still is the one that runs.
    assert_eq!(
        updater
            .build
            .facts(&inst.layout.server_exe())
            .unwrap()
            .version,
        "0.1.0"
    );

    // Recorded, and not taken again.
    let state = install::State::load(&inst.layout.work_dir);
    assert_eq!(state.failed.len(), 1);
    assert_eq!(state.failed[0].version, "0.2.0");
    let again = updater.check(Policy::Minor).unwrap();
    assert!(again.selection.take.is_none());
    assert!(again.selection.reported[0]
        .1
        .contains("failed its health check"));
}

/// `--release-facts` is what the updater runs on a staged release: the built server reports the
/// schema and pack versions this build uses.
#[test]
fn the_built_server_reports_its_facts() {
    let dir = scratch("facts");
    let build = Processes {
        config: None,
        current_dir: dir.clone(),
        timeout: Duration::from_secs(60),
        work_dir: dir.join("work"),
    };
    let reported = build
        .facts(Path::new(env!("CARGO_BIN_EXE_empyrean-server")))
        .unwrap();
    assert_eq!(reported, Facts::this_build());
    assert_eq!(reported.version, env!("CARGO_PKG_VERSION"));
    // The release packaging counts the store's migration files for the same numbers.
    let schema = Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/store/src/schema");
    let files = |prefix: &str| {
        std::fs::read_dir(&schema)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
            .count()
    };
    assert_eq!(
        usize::try_from(reported.databases.shard),
        Ok(files("shard_v"))
    );
    assert_eq!(
        usize::try_from(reported.databases.auth),
        Ok(files("auth_v"))
    );
}

/// The real health check of a server that cannot come up (its world pack is missing) fails.
#[test]
fn the_built_server_fails_its_health_check_without_a_world() {
    let dir = scratch("unhealthy");
    let config = dir.join("empyrean.toml");
    std::fs::write(
        &config,
        "[server]\nworld_pack_path = \"missing.pack\"\ninteractive_console = false\n\
         [server.network]\nhost = \"127.0.0.1\"\nport = 19470\n",
    )
    .unwrap();
    let build = Processes {
        config: Some(config),
        current_dir: dir.clone(),
        timeout: Duration::from_secs(120),
        work_dir: dir.join("work"),
    };
    let err = build
        .trial(Path::new(env!("CARGO_BIN_EXE_empyrean-server")))
        .unwrap_err();
    assert!(err.contains("before it was ready"), "{err}");
}

/// With the retail dats and a world pack, the real health check passes: the server opens and
/// migrates its databases, loads its world, binds its listeners, reports ready and stops.
#[test]
fn the_built_server_passes_its_health_check_on_real_content() {
    if !cfg!(feature = "real-content") {
        return;
    }
    let client = dereth_dat::testing::dat_dir();
    let pack = empyrean_common::test_paths::world_pack();
    if !dereth_dat::testing::have_dats() || !pack.is_file() {
        eprintln!("(no retail dats or no world.pack: the health check did not run)");
        return;
    }
    let dir = scratch("healthy");
    let config = dir.join("empyrean.toml");
    std::fs::write(
        &config,
        format!(
            "[server]\ndat_files_directory = '{}'\nworld_pack_path = '{}'\nlandblock_preloading = false\n\
             interactive_console = false\n[server.network]\nhost = \"127.0.0.1\"\nport = 19474\n",
            client.display(),
            pack.display()
        ),
    )
    .unwrap();
    let build = Processes {
        config: Some(config),
        current_dir: dir.clone(),
        timeout: Duration::from_secs(600),
        work_dir: dir.join("work"),
    };
    let facts = build
        .trial(Path::new(env!("CARGO_BIN_EXE_empyrean-server")))
        .unwrap();
    assert_eq!(facts, Facts::this_build());
    assert!(dir.join("shard.db").is_file(), "the databases were opened");
}

/// The command line: `update` needs `--check` or `--apply`, and names the three policies.
#[test]
fn the_update_command_needs_check_or_apply() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_empyrean-server"))
        .args(["update", "--policy", "sometimes"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("off, patch or minor"), "{text}");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_empyrean-server"))
        .args(["update"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
