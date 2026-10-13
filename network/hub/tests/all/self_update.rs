//! `self-update` installs a release over a binary only when every check passes: the archive's
//! SHA-256 is the one `SHA256SUMS` lists, the new binary names the release's version and reads the
//! registry, the binary's folder is writable, and the registry is copied first; the old binary is
//! kept as `.previous-v<old>`. A release that fails here is recorded and not taken again.
//! Fixture: temporary installations with a real registry; releases served by a stand-in fetcher
//! whose "binaries" are text files a stand-in runner reads, and once by a loopback web server
//! holding the built hub itself.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use dereth_network_hub::registry::{self, Registry, SCHEMA_VERSION};
use dereth_network_hub::self_update::{
    archive_name, binary_name, describe, sha256_hex, this_target, Fetcher, Fetching, Https,
    Installed, Policy, Processes, Runner, Source, State, Updater, Version, STATE_FILE,
    WRITE_CHECK_FILE,
};

const LINUX: &str = "x86_64-unknown-linux-gnu";
const LIST: &str = "https://api.github.com/repos/someone/hub/releases?per_page=100";

fn v(s: &str) -> Version {
    Version::parse(s).expect("a version")
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("self-update")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    dir
}

/// A stand-in binary: what it answers to `--version` and `--registry-schema`.
fn fake_binary(version: &str, schema: i64) -> Vec<u8> {
    format!("--version=dereth-network-hub {version}\n--registry-schema={schema}\n").into_bytes()
}

/// Runs a stand-in binary by reading what it would answer.
#[derive(Debug)]
struct FakeRunner;

impl Runner for FakeRunner {
    fn output(&self, exe: &Path, arg: &str) -> Result<String, String> {
        let text = std::fs::read_to_string(exe).map_err(|e| format!("does not run: {e}"))?;
        text.lines()
            .find_map(|l| l.strip_prefix(&format!("{arg}=")))
            .map(str::to_owned)
            .ok_or_else(|| format!("{} {arg} failed", exe.display()))
    }
}

/// Serves fixed answers by URL, and remembers what was asked.
#[derive(Debug, Default)]
struct FakeFetcher {
    files: HashMap<String, Vec<u8>>,
    asked: Rc<RefCell<Vec<String>>>,
}

impl Fetcher for FakeFetcher {
    fn get(&self, url: &str, _: Fetching) -> Result<Vec<u8>, String> {
        self.asked.borrow_mut().push(url.to_owned());
        self.files
            .get(url)
            .cloned()
            .ok_or_else(|| format!("{url} answered HTTP 404"))
    }
}

/// A gzipped tar of `entries` (path, contents), each a plain file.
fn tar_gz(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut tar = Vec::new();
    for (path, data) in entries {
        let mut h = [0u8; 512];
        h[..path.len()].copy_from_slice(path.as_bytes());
        h[100..107].copy_from_slice(b"0000755");
        h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        tar.extend_from_slice(&h);
        tar.extend_from_slice(data);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.resize(tar.len() + 1024, 0);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&tar).expect("written");
    gz.finish().expect("finished")
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, data) in entries {
        z.start_file(*path, zip::write::SimpleFileOptions::default())
            .expect("a file");
        z.write_all(data).expect("written");
    }
    z.finish().expect("finished").into_inner()
}

/// The release archive of `version` for `target`, holding `binary` and a licence.
fn archive(version: &str, target: &str, binary: &[u8]) -> (String, Vec<u8>) {
    let name = archive_name(&v(version), target);
    let root = name
        .strip_suffix(".zip")
        .or_else(|| name.strip_suffix(".tar.gz"))
        .expect("an archive");
    let bin = format!("{root}/{}", binary_name(target));
    let licence = format!("{root}/LICENSE");
    let entries: [(&str, &[u8]); 2] = [(&licence, b"licence"), (&bin, binary)];
    let bytes = if name.ends_with(".zip") {
        zip_of(&entries)
    } else {
        tar_gz(&entries)
    };
    (name, bytes)
}

/// The releases of `published` (version, binary) for `target`, as the releases API and the
/// downloads answer them at `base`: the list (at `list`) and each release's archive and sums.
fn releases(
    list: &str,
    base: &str,
    target: &str,
    published: &[(&str, Vec<u8>)],
) -> HashMap<String, Vec<u8>> {
    let mut files = HashMap::new();
    let mut json = Vec::new();
    for (version, binary) in published {
        let (name, bytes) = archive(version, target, binary);
        let sums = format!(
            "{}  {name}\n{}  dereth-network-hub-{version}-image.oci.tar\n",
            sha256_hex(&bytes),
            sha256_hex(b"image")
        );
        let archive_url = format!("{base}/{version}/{name}");
        let sums_url = format!("{base}/{version}/SHA256SUMS");
        files.insert(archive_url.clone(), bytes);
        files.insert(sums_url.clone(), sums.into_bytes());
        json.push(serde_json::json!({
            "tag_name": format!("network-hub-v{version}"),
            "prerelease": false,
            "assets": [
                { "name": "SHA256SUMS", "browser_download_url": sums_url },
                { "name": name, "browser_download_url": archive_url },
            ],
        }));
    }
    files.insert(
        list.to_owned(),
        serde_json::to_vec(&json).expect("the list"),
    );
    files
}

/// An installed hub: its binary (a stand-in of 0.1.1) and a real registry beside it.
struct Install {
    dir: PathBuf,
    bin: PathBuf,
    binary: PathBuf,
    registry: PathBuf,
}

impl Install {
    fn new(name: &str) -> Self {
        let dir = scratch(name);
        let bin = dir.join("bin");
        std::fs::create_dir_all(&bin).expect("a folder");
        let binary = bin.join(format!(
            "dereth-network-hub{}",
            std::env::consts::EXE_SUFFIX
        ));
        std::fs::write(&binary, fake_binary("0.1.1", SCHEMA_VERSION)).expect("written");
        let registry = dir.join("data").join("hub.sqlite");
        let r = Registry::open(
            registry.to_str().expect("a path"),
            "dereth.network",
            Box::new(std::time::SystemTime::now),
        )
        .expect("a registry");
        r.signed_in("100", "Someone").expect("an operator");
        drop(r);
        Self {
            dir,
            bin,
            binary,
            registry,
        }
    }

    fn updater(&self, current: &str, files: HashMap<String, Vec<u8>>) -> (Updater, Asked) {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let updater = Updater {
            source: Source::parse("https://github.com/someone/hub").expect("a source"),
            fetcher: Box::new(FakeFetcher {
                files,
                asked: Rc::clone(&asked),
            }),
            runner: Box::new(FakeRunner),
            current: v(current),
            target: Some(LINUX.to_owned()),
            schema: SCHEMA_VERSION,
            binary: self.binary.clone(),
            registry: self.registry.clone(),
        };
        (updater, asked)
    }

    /// The files in the binary's folder.
    fn bin_files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.bin)
            .expect("the folder")
            .map(|e| {
                e.expect("an entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    /// The registry's update copies.
    fn backups(&self) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(self.registry.parent().expect("a folder"))
            .expect("the folder")
            .map(|e| e.expect("an entry").path())
            .filter(|p| p.to_string_lossy().contains("hub.sqlite.backup-update-"))
            .collect();
        found.sort();
        found
    }
}

type Asked = Rc<RefCell<Vec<String>>>;

fn apply(updater: &Updater, policy: Policy) -> Result<Option<Installed>, String> {
    let check = updater.check(policy).expect("checked");
    updater.apply(&check, &mut |_| {})
}

/// Spec: hub: `self-update --apply` moves the installed binary aside as `.previous-v<old>` and the
/// release's into its place, after copying the registry beside it; the next update removes the
/// binary an earlier one moved aside, and only the newest three of the registry's update copies are
/// kept; a new binary an interrupted update left beside the binary is removed.
#[test]
fn an_update_keeps_the_previous_binary_and_a_copy_of_the_registry() {
    let install = Install::new("swap");
    std::fs::write(
        install.bin.join(".dereth-network-hub.new-v0.1.0"),
        b"left by an interrupted update",
    )
    .expect("written");
    let new = fake_binary("0.1.2", SCHEMA_VERSION);
    let (updater, _) = install.updater(
        "0.1.1",
        releases(LIST, "https://dl.test", LINUX, &[("0.1.2", new.clone())]),
    );
    let installed = apply(&updater, Policy::Patch)
        .expect("installed")
        .expect("a release");
    assert_eq!(installed.version, v("0.1.2"));
    assert_eq!(std::fs::read(&install.binary).expect("the binary"), new);
    let previous = install.bin.join(format!(
        "dereth-network-hub{}.previous-v0.1.1",
        std::env::consts::EXE_SUFFIX
    ));
    assert_eq!(installed.previous, previous);
    assert_eq!(
        std::fs::read(&previous).expect("kept"),
        fake_binary("0.1.1", SCHEMA_VERSION)
    );
    assert_eq!(
        install.bin_files().len(),
        2,
        "the binary and the previous one, nothing staged left: {:?}",
        install.bin_files()
    );
    assert!(installed
        .backup
        .to_string_lossy()
        .contains("hub.sqlite.backup-update-v0.1.1-v0.1.2-"));
    assert_eq!(
        registry::schema_of_file(&installed.backup).expect("a registry"),
        SCHEMA_VERSION
    );
    let copy = Registry::open(
        installed.backup.to_str().expect("a path"),
        "dereth.network",
        Box::new(std::time::SystemTime::now),
    )
    .expect("the copy opens");
    assert_eq!(
        copy.operators().expect("read").as_array().map(Vec::len),
        Some(1),
        "the copy holds the registry"
    );
    drop(copy);

    // Older update copies past the newest three go; the binary moved aside before goes too.
    for stamp in ["20200101T000000Z", "20200102T000000Z", "20200103T000000Z"] {
        std::fs::write(
            install
                .registry
                .with_file_name(format!("hub.sqlite.backup-update-v0.0.1-v0.0.2-{stamp}")),
            b"old copy",
        )
        .expect("written");
    }
    let newer = fake_binary("0.1.3", SCHEMA_VERSION);
    let (updater, _) = install.updater(
        "0.1.2",
        releases(LIST, "https://dl.test", LINUX, &[("0.1.3", newer.clone())]),
    );
    let again = apply(&updater, Policy::Patch)
        .expect("installed")
        .expect("a release");
    assert_eq!(std::fs::read(&install.binary).expect("the binary"), newer);
    assert_eq!(
        install.bin_files(),
        [
            format!("dereth-network-hub{}", std::env::consts::EXE_SUFFIX),
            format!(
                "dereth-network-hub{}.previous-v0.1.2",
                std::env::consts::EXE_SUFFIX
            )
        ]
    );
    let backups = install.backups();
    assert_eq!(backups.len(), 3, "{backups:?}");
    assert!(backups.contains(&installed.backup) && backups.contains(&again.backup));
    let _ = std::fs::remove_dir_all(&install.dir);
}

/// Spec: hub: an archive whose SHA-256 is not the one `SHA256SUMS` lists is refused, saying the
/// check is by SHA-256 only, and nothing is installed or copied.
#[test]
fn an_archive_that_does_not_match_its_checksum_installs_nothing() {
    let install = Install::new("mismatch");
    let mut files = releases(
        LIST,
        "https://dl.test",
        LINUX,
        &[("0.1.2", fake_binary("0.1.2", SCHEMA_VERSION))],
    );
    let url = format!("https://dl.test/0.1.2/{}", archive_name(&v("0.1.2"), LINUX));
    files.get_mut(&url).expect("the archive").push(0);
    let (updater, _) = install.updater("0.1.1", files);
    let e = apply(&updater, Policy::Patch).expect_err("refused");
    assert!(
        e.contains("is not the") && e.contains("checked by SHA-256 only"),
        "{e}"
    );
    assert_eq!(
        std::fs::read(&install.binary).expect("the binary"),
        fake_binary("0.1.1", SCHEMA_VERSION)
    );
    assert_eq!(install.bin_files().len(), 1, "{:?}", install.bin_files());
    assert!(install.backups().is_empty());
    assert!(
        State::load(&install.bin.join(STATE_FILE)).failed.is_empty(),
        "a bad download is tried again"
    );
    let _ = std::fs::remove_dir_all(&install.dir);
}

/// Spec: hub: a new binary whose `--version` does not name the release is refused, nothing is
/// installed, and the release is recorded as failed and not taken again.
#[test]
fn a_binary_that_names_another_version_is_refused_and_not_retried() {
    let install = Install::new("version");
    let files = releases(
        LIST,
        "https://dl.test",
        LINUX,
        &[("0.1.2", fake_binary("0.1.9", SCHEMA_VERSION))],
    );
    let (updater, _) = install.updater("0.1.1", files);
    let e = apply(&updater, Policy::Patch).expect_err("refused");
    assert!(
        e.contains("names itself `dereth-network-hub 0.1.9`, not `dereth-network-hub 0.1.2`"),
        "{e}"
    );
    assert_eq!(
        std::fs::read(&install.binary).expect("the binary"),
        fake_binary("0.1.1", SCHEMA_VERSION)
    );
    assert_eq!(install.bin_files().len(), 2, "the binary and the record");
    assert!(install.backups().is_empty());
    let check = updater.check(Policy::Patch).expect("checked");
    assert_eq!(check.selection.take, None, "not taken again");
    let lines = describe(&updater, &check).join("\n");
    assert!(
        lines.contains("0.1.2: not installed: it failed here before"),
        "{lines}"
    );
    assert!(lines.contains(STATE_FILE), "{lines}");
    let _ = std::fs::remove_dir_all(&install.dir);
}

/// Spec: hub: a release whose binary reads an older registry schema than the registry's is refused
/// (it would refuse to start), and `--policy patch` installs only a release whose binary reads this
/// build's schema, which `--policy minor` does not ask.
#[test]
fn a_release_must_read_the_registry_and_patch_must_not_change_its_schema() {
    let reads_older = Install::new("schema-older");
    let files = releases(
        LIST,
        "https://dl.test",
        LINUX,
        &[("0.1.2", fake_binary("0.1.2", SCHEMA_VERSION - 1))],
    );
    let (updater, _) = reads_older.updater("0.1.1", files);
    let e = apply(&updater, Policy::Minor).expect_err("refused");
    assert!(e.contains("it would refuse to start"), "{e}");
    assert_eq!(
        reads_older.bin_files().len(),
        2,
        "the binary and the record"
    );

    let reads_newer = Install::new("schema-newer");
    let files = || {
        releases(
            LIST,
            "https://dl.test",
            LINUX,
            &[("0.1.2", fake_binary("0.1.2", SCHEMA_VERSION + 1))],
        )
    };
    let (updater, _) = reads_newer.updater("0.1.1", files());
    let e = apply(&updater, Policy::Patch).expect_err("refused");
    assert!(e.contains("--policy patch installs only"), "{e}");
    assert_eq!(
        reads_newer.bin_files().len(),
        1,
        "not recorded: minor may take it"
    );
    let (updater, _) = reads_newer.updater("0.1.1", files());
    assert!(apply(&updater, Policy::Minor).expect("installed").is_some());
    let _ = std::fs::remove_dir_all(&reads_older.dir);
    let _ = std::fs::remove_dir_all(&reads_newer.dir);
}

/// Spec: hub: `self-update --apply` refuses before downloading anything when this account cannot
/// write in the binary's folder, naming the folder, and `--check` says it would.
#[test]
fn a_folder_this_account_cannot_write_is_refused_by_name() {
    let install = Install::new("unwritable");
    let files = releases(
        LIST,
        "https://dl.test",
        LINUX,
        &[("0.1.2", fake_binary("0.1.2", SCHEMA_VERSION))],
    );
    // Where the updater would write, a folder: making the file there fails, as it does in a
    // folder this account may not write.
    std::fs::create_dir(install.bin.join(WRITE_CHECK_FILE)).expect("a folder");
    let (updater, asked) = install.updater("0.1.1", files);
    let check = updater.check(Policy::Patch).expect("checked");
    let folder = install.bin.display().to_string();
    let lines = describe(&updater, &check).join("\n");
    assert!(
        lines.contains(&format!(
            "--apply would refuse: this account cannot write in {folder}"
        )),
        "{lines}"
    );
    let e = updater.apply(&check, &mut |_| {}).expect_err("refused");
    assert!(
        e.contains(&folder) && e.contains("nothing was installed"),
        "{e}"
    );
    assert_eq!(*asked.borrow(), [LIST], "nothing downloaded");
    assert_eq!(
        std::fs::read(&install.binary).expect("the binary"),
        fake_binary("0.1.1", SCHEMA_VERSION)
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::remove_dir(install.bin.join(WRITE_CHECK_FILE)).expect("removed");
        std::fs::set_permissions(&install.bin, std::fs::Permissions::from_mode(0o555))
            .expect("read-only");
        // Root writes anyway, so the folder's permissions say nothing there.
        if std::fs::write(install.bin.join("probe"), b"").is_err() {
            let e = updater.apply(&check, &mut |_| {}).expect_err("refused");
            assert!(e.contains(&folder), "{e}");
        }
        std::fs::set_permissions(&install.bin, std::fs::Permissions::from_mode(0o755))
            .expect("writable again");
    }
    // No registry where it is named: refused, naming it.
    let (mut updater, _) = install.updater("0.1.1", HashMap::new());
    updater.registry = install.dir.join("elsewhere").join("hub.sqlite");
    std::fs::remove_dir(install.bin.join(WRITE_CHECK_FILE)).ok();
    assert!(updater
        .blocked()
        .expect("refused")
        .contains("there is no registry at"));
    let _ = std::fs::remove_dir_all(&install.dir);
}

/// A web server on `listener` answering `routes` (path to status, extra header lines, body), one
/// request to a connection.
fn serve(listener: std::net::TcpListener, routes: HashMap<String, (u16, String, Vec<u8>)>) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut head = Vec::new();
            let mut b = [0u8; 1024];
            while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                match s.read(&mut b) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => head.extend_from_slice(&b[..n]),
                }
            }
            let line = String::from_utf8_lossy(&head);
            let path = line.split(' ').nth(1).unwrap_or("").to_owned();
            let (status, header, body) =
                routes
                    .get(&path)
                    .cloned()
                    .unwrap_or((404, String::new(), Vec::new()));
            let _ = write!(
                s,
                "HTTP/1.1 {status} X\r\n{header}Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(&body);
        }
    });
}

/// The release before this build's, which the updater is told it is, and the policy that takes
/// this build from it.
fn one_before_this_build() -> (Version, Policy) {
    let this = Version::this_build();
    if this.patch > 0 {
        (
            Version {
                patch: this.patch - 1,
                ..this
            },
            Policy::Patch,
        )
    } else {
        (
            Version {
                minor: this.minor.saturating_sub(1),
                ..this
            },
            Policy::Minor,
        )
    }
}

/// Spec: hub: the built hub, published as a release on a web server and reached through a
/// redirect, is downloaded, checked, run (naming its own version and its registry schema) and
/// installed in place of the binary; published under another version, it is refused.
#[test]
fn the_built_hub_installs_from_a_release_served_over_http() {
    let target = this_target().expect("this build's target");
    let built = std::fs::read(env!("CARGO_BIN_EXE_dereth-network-hub")).expect("the built hub");
    let this = Version::this_build().to_string();
    let wrong = {
        let t = Version::this_build();
        Version {
            patch: t.patch + 7,
            ..t
        }
        .to_string()
    };
    let install = Install::new("built");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("an address"));
    let mut routes = HashMap::new();
    let mut list = Vec::new();
    for version in [this.as_str(), wrong.as_str()] {
        let (name, bytes) = archive(version, target, &built);
        let sums = format!("{}  {name}\n", sha256_hex(&bytes));
        // The download address answers with a redirect to where the file is, as GitHub's does.
        routes.insert(
            format!("/download/{version}/{name}"),
            (
                302,
                format!("Location: /files/{version}/{name}\r\n"),
                Vec::new(),
            ),
        );
        routes.insert(
            format!("/files/{version}/{name}"),
            (200, String::new(), bytes),
        );
        routes.insert(
            format!("/files/{version}/SHA256SUMS"),
            (200, String::new(), sums.into_bytes()),
        );
        list.push(serde_json::json!({
            "tag_name": format!("network-hub-v{version}"),
            "assets": [
                { "name": "SHA256SUMS", "browser_download_url": format!("{base}/files/{version}/SHA256SUMS") },
                { "name": name, "browser_download_url": format!("{base}/download/{version}/{name}") },
            ],
        }));
    }
    routes.insert(
        "/repos/someone/hub/releases?per_page=100".to_owned(),
        (200, String::new(), serde_json::to_vec(&list).expect("json")),
    );
    serve(listener, routes);
    let (from, policy) = one_before_this_build();
    let make = |current: Version| Updater {
        source: Source::parse(&format!("{base}/repos/someone/hub")).expect("a mirror"),
        fetcher: Box::new(Https),
        runner: Box::new(Processes),
        current,
        target: Some(target.to_owned()),
        schema: SCHEMA_VERSION,
        binary: install.binary.clone(),
        registry: install.registry.clone(),
    };

    // Taken for the release just before this one, it installs this one (the newest it takes is
    // the one published under the wrong version, which is refused first).
    let updater = make(from.clone());
    let check = updater.check(policy).expect("checked");
    assert_eq!(check.selection.take, Some(v(&wrong)));
    let e = updater.apply(&check, &mut |_| {}).expect_err("refused");
    assert!(
        e.contains(&format!("names itself `dereth-network-hub {this}`")),
        "{e}"
    );
    let check = updater.check(policy).expect("checked again");
    assert_eq!(
        check.selection.take,
        Some(v(&this)),
        "the failed one skipped"
    );
    let installed = updater
        .apply(&check, &mut |_| {})
        .expect("installed")
        .expect("a release");
    assert_eq!(installed.version, v(&this));
    assert_eq!(std::fs::read(&install.binary).expect("the binary"), built);
    let answer = Processes
        .output(&install.binary, "--version")
        .expect("the installed hub runs");
    assert_eq!(answer, format!("dereth-network-hub {this}"));
    assert!(installed.previous.is_file());
    let _ = std::fs::remove_dir_all(&install.dir);
}

mod registries {
    //! A registry an earlier hub kept, opened by this build: its schema moves forward to this
    //! build's, as a new registry's is, and every row comes through, column for column, but what an
    //! upgrade takes away on purpose. Two earlier hubs keep registries in use: release 0.1.x, at
    //! schema 7, and the network's own hub, a build before 0.2, at schema 8.
    //! Fixture: each earlier hub's schema frozen as its own code makes it (every statement its
    //! `sqlite_master` holds, in order), filled through `ATTACH` with the rows this build writes
    //! for worlds, persons, links, the directory, mirrors, operators, sessions and settings, and
    //! with the rows only an earlier hub wrote; registry files in a temp folder.

    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    use dereth_network_hub::bridge::Placement;
    use dereth_network_hub::registry::{schema_of_file, Registration, Registry, SCHEMA_VERSION};
    use dereth_network_hub::settings::Setting;
    use dereth_network_protocol::directory::Allegiance;
    use dereth_network_protocol::{Address, Federation, Profile};
    use rusqlite::types::Value;
    use rusqlite::Connection;
    use serde_json::json;

    /// The schema hub 0.1.x makes (the registry's version 7): each statement as its `sqlite_master`
    /// holds it, in the order it was made.
    const SCHEMA_7: &str = r#"
CREATE TABLE servers (
                   id TEXT PRIMARY KEY,
                   slug TEXT NOT NULL UNIQUE,
                   metadata TEXT NOT NULL,
                   key_hash TEXT,
                   created_at TEXT NOT NULL,
                   last_seen_at TEXT,
                   heartbeat TEXT,
                   registry TEXT NOT NULL DEFAULT '{}',
                   federation TEXT NOT NULL DEFAULT 'unset'
                 , operator_id TEXT);
CREATE UNIQUE INDEX community_identity
                   ON servers(json_extract(registry, '$.provenance.externalId'));
CREATE TABLE registry_events (
                   id INTEGER PRIMARY KEY,
                   server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                   kind TEXT NOT NULL, received_at TEXT NOT NULL, details TEXT NOT NULL, actor TEXT);
CREATE INDEX server_events ON registry_events(server_id, id);
CREATE TABLE hub_state (name TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE hub_channels (name TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE operators (
                   id TEXT PRIMARY KEY,
                   discord_id TEXT NOT NULL UNIQUE,
                   name TEXT NOT NULL,
                   created_at TEXT NOT NULL,
                   signed_in_at TEXT,
                   disabled_at TEXT, alts TEXT NOT NULL DEFAULT 'hidden');
CREATE TABLE operator_sessions (
                   id_hash TEXT PRIMARY KEY,
                   operator_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
                   created_at TEXT NOT NULL,
                   expires_at TEXT NOT NULL);
CREATE TABLE operator_events (
                   id INTEGER PRIMARY KEY,
                   operator_id TEXT NOT NULL,
                   kind TEXT NOT NULL, at TEXT NOT NULL, details TEXT NOT NULL);
CREATE INDEX operator_event_kinds ON operator_events(kind, at);
CREATE TABLE verifications (
                   id TEXT PRIMARY KEY,
                   server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                   operator_id TEXT NOT NULL REFERENCES operators(id),
                   key_hash TEXT,
                   token_hash TEXT NOT NULL,
                   token_expires_at TEXT NOT NULL,
                   created_at TEXT NOT NULL,
                   state TEXT NOT NULL);
CREATE INDEX server_verifications ON verifications(server_id, state);
CREATE TABLE reports (
                   id TEXT PRIMARY KEY,
                   server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                   reporter_id TEXT NOT NULL REFERENCES operators(id),
                   reason TEXT NOT NULL, details TEXT NOT NULL,
                   created_at TEXT NOT NULL, state TEXT NOT NULL,
                   resolved_at TEXT, resolution TEXT, note TEXT);
CREATE TABLE person_characters (
      address_key TEXT PRIMARY KEY,
      address TEXT NOT NULL,
      world TEXT NOT NULL,
      person_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
      linked_at TEXT NOT NULL);
CREATE INDEX person_characters_by_person ON person_characters(person_id);
CREATE TABLE person_codes (
      address_key TEXT PRIMARY KEY,
      address TEXT NOT NULL,
      world TEXT NOT NULL,
      code_hash TEXT NOT NULL UNIQUE,
      expires_at TEXT NOT NULL);
CREATE INDEX person_codes_by_world ON person_codes(world, expires_at);
CREATE TABLE person_code_log (address_key TEXT NOT NULL, at TEXT NOT NULL);
CREATE INDEX person_code_log_by_key ON person_code_log(address_key, at);
CREATE TABLE person_friends (
      person_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
      handle_key TEXT NOT NULL,
      handle TEXT NOT NULL,
      added_from TEXT NOT NULL,
      added_from_address TEXT NOT NULL,
      added_at TEXT NOT NULL,
      PRIMARY KEY (person_id, handle_key, added_from));
CREATE INDEX person_code_log_by_time ON person_code_log(at);
CREATE TABLE person_returns (
      address_key TEXT NOT NULL,
      world TEXT NOT NULL,
      handle_key TEXT NOT NULL,
      handle TEXT NOT NULL,
      at TEXT NOT NULL,
      PRIMARY KEY (address_key, handle_key));
CREATE INDEX person_returns_by_world ON person_returns(world);
CREATE INDEX person_returns_by_time ON person_returns(at);
"#;

    /// The schema the build before 0.2 makes (the registry's version 8), which the network's own
    /// hub runs: each statement as its `sqlite_master` holds it, in the order it was made.
    const SCHEMA_8: &str = r#"
CREATE TABLE servers (
                   id TEXT PRIMARY KEY,
                   slug TEXT NOT NULL UNIQUE,
                   metadata TEXT NOT NULL,
                   key_hash TEXT,
                   created_at TEXT NOT NULL,
                   last_seen_at TEXT,
                   heartbeat TEXT,
                   registry TEXT NOT NULL DEFAULT '{}',
                   federation TEXT NOT NULL DEFAULT 'unset'
                 , operator_id TEXT);
CREATE UNIQUE INDEX community_identity
                   ON servers(json_extract(registry, '$.provenance.externalId'));
CREATE TABLE registry_events (
                   id INTEGER PRIMARY KEY,
                   server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                   kind TEXT NOT NULL, received_at TEXT NOT NULL, details TEXT NOT NULL, actor TEXT);
CREATE INDEX server_events ON registry_events(server_id, id);
CREATE TABLE hub_state (name TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE hub_channels (name TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE operators (
                   id TEXT PRIMARY KEY,
                   discord_id TEXT NOT NULL UNIQUE,
                   name TEXT NOT NULL,
                   created_at TEXT NOT NULL,
                   signed_in_at TEXT,
                   disabled_at TEXT, alts TEXT NOT NULL DEFAULT 'hidden', discord_tells TEXT NOT NULL DEFAULT 'off');
CREATE TABLE operator_sessions (
                   id_hash TEXT PRIMARY KEY,
                   operator_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
                   created_at TEXT NOT NULL,
                   expires_at TEXT NOT NULL);
CREATE TABLE operator_events (
                   id INTEGER PRIMARY KEY,
                   operator_id TEXT NOT NULL,
                   kind TEXT NOT NULL, at TEXT NOT NULL, details TEXT NOT NULL);
CREATE INDEX operator_event_kinds ON operator_events(kind, at);
CREATE TABLE verifications (
                   id TEXT PRIMARY KEY,
                   server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                   operator_id TEXT NOT NULL REFERENCES operators(id),
                   key_hash TEXT,
                   token_hash TEXT NOT NULL,
                   token_expires_at TEXT NOT NULL,
                   created_at TEXT NOT NULL,
                   state TEXT NOT NULL);
CREATE INDEX server_verifications ON verifications(server_id, state);
CREATE TABLE reports (
                   id TEXT PRIMARY KEY,
                   server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
                   reporter_id TEXT NOT NULL REFERENCES operators(id),
                   reason TEXT NOT NULL, details TEXT NOT NULL,
                   created_at TEXT NOT NULL, state TEXT NOT NULL,
                   resolved_at TEXT, resolution TEXT, note TEXT);
CREATE TABLE person_characters (
      address_key TEXT PRIMARY KEY,
      address TEXT NOT NULL,
      world TEXT NOT NULL,
      person_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
      linked_at TEXT NOT NULL);
CREATE INDEX person_characters_by_person ON person_characters(person_id);
CREATE TABLE person_codes (
      address_key TEXT PRIMARY KEY,
      address TEXT NOT NULL,
      world TEXT NOT NULL,
      code_hash TEXT NOT NULL UNIQUE,
      expires_at TEXT NOT NULL);
CREATE INDEX person_codes_by_world ON person_codes(world, expires_at);
CREATE TABLE person_code_log (address_key TEXT NOT NULL, at TEXT NOT NULL);
CREATE INDEX person_code_log_by_key ON person_code_log(address_key, at);
CREATE TABLE person_friends (
      person_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
      handle_key TEXT NOT NULL,
      handle TEXT NOT NULL,
      added_from TEXT NOT NULL,
      added_from_address TEXT NOT NULL,
      added_at TEXT NOT NULL,
      PRIMARY KEY (person_id, handle_key, added_from));
CREATE INDEX person_code_log_by_time ON person_code_log(at);
CREATE TABLE person_returns (
      address_key TEXT NOT NULL,
      world TEXT NOT NULL,
      handle_key TEXT NOT NULL,
      handle TEXT NOT NULL,
      at TEXT NOT NULL,
      PRIMARY KEY (address_key, handle_key));
CREATE INDEX person_returns_by_world ON person_returns(world);
CREATE INDEX person_returns_by_time ON person_returns(at);
CREATE TABLE directory_listings (
      address_key TEXT PRIMARY KEY,
      address TEXT NOT NULL,
      name_key TEXT NOT NULL,
      world TEXT NOT NULL,
      level INTEGER NOT NULL,
      heritage INTEGER NOT NULL,
      allegiance_key TEXT,
      allegiance_name_key TEXT,
      allegiance_rank INTEGER,
      patron_key TEXT,
      monarch_key TEXT,
      profile TEXT NOT NULL,
      updated TEXT NOT NULL,
      sent_day TEXT NOT NULL);
CREATE INDEX directory_by_world ON directory_listings(world);
CREATE INDEX directory_by_updated ON directory_listings(updated DESC, name_key, address_key);
CREATE INDEX directory_by_level ON directory_listings(level DESC, name_key, address_key);
CREATE INDEX directory_by_allegiance
      ON directory_listings(world, allegiance_key, allegiance_rank DESC, name_key);
CREATE INDEX directory_by_patron ON directory_listings(patron_key);
CREATE INDEX directory_by_monarch ON directory_listings(monarch_key);
CREATE INDEX directory_by_sent ON directory_listings(sent_day);
CREATE TABLE directory_holds (
      address_key TEXT PRIMARY KEY,
      address TEXT NOT NULL,
      world TEXT NOT NULL,
      by TEXT NOT NULL,
      owed INTEGER NOT NULL,
      until TEXT NOT NULL);
CREATE INDEX directory_holds_by_world ON directory_holds(world, owed);
CREATE TABLE discord_mirrors (
      server_id TEXT REFERENCES servers(id) ON DELETE CASCADE,
      room TEXT NOT NULL,
      channel_id TEXT NOT NULL UNIQUE,
      guild_id TEXT NOT NULL,
      webhook_id TEXT,
      webhook_token TEXT,
      set_at TEXT NOT NULL);
CREATE UNIQUE INDEX discord_mirrors_of_worlds ON discord_mirrors(server_id, room)
      WHERE server_id IS NOT NULL;
CREATE UNIQUE INDEX discord_mirrors_of_the_hub ON discord_mirrors(room)
      WHERE server_id IS NULL;
CREATE TABLE discord_events (
      id INTEGER PRIMARY KEY,
      kind TEXT NOT NULL, at TEXT NOT NULL, actor TEXT, details TEXT NOT NULL);
CREATE TABLE person_identities (
      person_id TEXT NOT NULL REFERENCES operators(id) ON DELETE CASCADE,
      world TEXT NOT NULL,
      address_key TEXT NOT NULL REFERENCES person_characters(address_key) ON DELETE CASCADE,
      set_at TEXT NOT NULL,
      PRIMARY KEY (person_id, world));
CREATE INDEX person_identities_by_character ON person_identities(address_key);
CREATE TABLE hub_settings (name TEXT PRIMARY KEY, value INTEGER NOT NULL);
CREATE TABLE settings_events (
      id INTEGER PRIMARY KEY,
      kind TEXT NOT NULL, at TEXT NOT NULL, actor TEXT, details TEXT NOT NULL);
"#;

    /// The Discord channels and server the mirrors are in.
    const CHANNEL: &str = "1558376951000000101";
    const ADMINS_CHANNEL: &str = "1558376951000000102";
    const SERVER: &str = "1558376951000000000";

    fn a(s: &str) -> Address {
        Address::parse(s).expect("an address")
    }

    fn open(path: &Path) -> Registry {
        Registry::open(
            path.to_str().expect("a path"),
            "dereth.network",
            Box::new(SystemTime::now),
        )
        .expect("opened")
    }

    /// An empty folder of the test's own.
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hub-upgrade-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temp folder");
        dir
    }

    fn profile(level: u32) -> Profile {
        Profile {
            level,
            heritage: 1,
            gender: 2,
            title: Some("the Bloodletter".into()),
            allegiance: Some(Allegiance {
                key: "dawn-key-1".into(),
                name: Some("Order of the Dawn".into()),
                members: 41,
                rank: 3,
                patron: None,
                monarch: None,
            }),
            ..Profile::default()
        }
    }

    /// What a hub in use holds, written by this build at `path`: three worlds, two on the network
    /// and one pending; a channel; two persons, one with a session; characters on the network and
    /// linked, their person friends, a character unlinked with its friends handed back, a code not
    /// yet used; the directory's listings and the operator's hold; a person's chosen character; a
    /// world's mirror and the admins'; a setting; a report. The session's token.
    fn write(path: &Path) -> String {
        let r = open(path);
        for slug in ["coldeve", "frost", "pending"] {
            let reg = Registration::from_json(&json!({
                "slug": slug, "name": format!("World {slug}"), "ruleset": "PvE",
                "endpoint": { "mode": "manual", "address": "192.0.2.1", "port": 9000 },
                "emulator": { "family": "ACE" },
            }))
            .expect("a registration");
            r.register(&reg).expect("registered");
            if slug != "pending" {
                r.set_federation(slug, Federation::On).expect("on");
            }
        }
        r.set_channel("oldguard", Some("{}")).expect("a channel");
        let ann = r.signed_in("100", "Ann").expect("signed in");
        let bob = r.signed_in("200", "Bob").expect("signed in");
        let token = r.new_session(&ann).expect("a session");
        for (op, who, level) in [
            (&ann, "Ann@coldeve", 20),
            (&ann, "Ann@frost", 30),
            (&bob, "Bob@frost", 40),
            (&ann, "Cat@coldeve", 50),
        ] {
            r.listing(&a(who), Some(&profile(level))).expect("listed");
            let (code, _) = r.person_code(&a(who)).expect("a code");
            r.redeem_link_code(op, &code).expect("linked");
        }
        r.person_befriend(&a("Ann@coldeve"), &a("Bob@frost"))
            .expect("befriended");
        r.person_befriend(&a("Bob@frost"), &a("Ann@coldeve"))
            .expect("befriended");
        r.person_unlink(&a("Cat@coldeve")).expect("unlinked");
        r.person_code(&a("Dan@frost")).expect("a code");
        r.take_down("Eve@frost").expect("taken down");
        r.set_identity(&ann.id, "coldeve", Some(&a("Ann@coldeve")))
            .expect("chosen");
        r.set_mirror(
            Some("coldeve"),
            "general",
            &Placement::bot(CHANNEL, SERVER),
            Some(&ann.id),
        )
        .expect("a world's mirror");
        r.set_mirror(
            None,
            "global",
            &Placement::bot(ADMINS_CHANNEL, SERVER),
            Some(&bob.id),
        )
        .expect("the admins' mirror");
        r.set_settings(&[(Setting::PendingDays, 21)], Some(&bob.id))
            .expect("a setting");
        r.report(&bob, "frost", "other", "Lines.")
            .expect("a report");
        token
    }

    /// The tables of `schema` (`main` or an attached one), in the order they were made.
    fn tables(db: &Connection, schema: &str) -> Vec<String> {
        db.prepare(&format!(
            "SELECT name FROM {schema}.sqlite_master WHERE type = 'table' ORDER BY rowid"
        ))
        .expect("prepared")
        .query_map([], |r| r.get(0))
        .expect("read")
        .collect::<Result<_, _>>()
        .expect("read")
    }

    /// The columns of `table` in `schema`, in order.
    fn columns(db: &Connection, schema: &str, table: &str) -> Vec<String> {
        db.prepare(&format!(
            "SELECT name FROM pragma_table_info('{table}', '{schema}')"
        ))
        .expect("prepared")
        .query_map([], |r| r.get(0))
        .expect("read")
        .collect::<Result<_, _>>()
        .expect("read")
    }

    /// Every row of `table` that `filter` keeps, `cols` alone, in the order they were written.
    fn rows(db: &Connection, table: &str, cols: &[String], filter: &str) -> Vec<Vec<Value>> {
        let mut q = db
            .prepare(&format!(
                "SELECT {} FROM {table} WHERE {filter} ORDER BY rowid",
                cols.join(", ")
            ))
            .expect("prepared");
        let n = q.column_count();
        q.query_map([], |r| (0..n).map(|i| r.get(i)).collect())
            .expect("read")
            .collect::<Result<_, _>>()
            .expect("read")
    }

    /// Each table's and index's definition, white space aside, by name.
    fn schema_of(db: &Connection) -> Vec<(String, String)> {
        db.prepare("SELECT name, sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name")
            .expect("prepared")
            .query_map([], |r| {
                let sql: String = r.get(1)?;
                Ok((r.get(0)?, sql.split_whitespace().collect()))
            })
            .expect("read")
            .collect::<Result<_, _>>()
            .expect("read")
    }

    /// The rows an upgrade takes away on purpose, by table: the holds persons put on their
    /// characters' listings, and the events of the setting that hid a person's characters from each
    /// other.
    const TAKEN: &[(&str, &str)] = &[
        ("directory_holds", "by = 'person'"),
        ("operator_events", "kind = 'alts'"),
    ];

    fn taken(table: &str) -> Option<&'static str> {
        TAKEN.iter().find(|(t, _)| *t == table).map(|(_, f)| *f)
    }

    /// Builds a registry at `version` from `schema`, as the earlier hub made it, with this build's
    /// rows copied in column for column and `extra` (the rows only an earlier hub wrote); opens it
    /// with this build; and checks that every row came through but the ones [`TAKEN`] names, that
    /// the columns gone are `dropped` alone, that the schema is a new registry's, and that what it
    /// held reads as before.
    fn moves_forward(
        name: &str,
        schema: &str,
        version: i64,
        extra: &str,
        dropped: &[(&str, &str)],
    ) {
        let dir = folder(name);
        let (written, earlier, fresh) = (
            dir.join("written.sqlite"),
            dir.join("earlier.sqlite"),
            dir.join("fresh.sqlite"),
        );
        let token = write(&written);
        drop(open(&fresh));
        let db = Connection::open(&earlier).expect("open");
        db.execute_batch(&format!("{schema} PRAGMA user_version = {version};"))
            .expect("the earlier schema");
        db.execute(
            "ATTACH DATABASE ? AS written",
            [written.to_str().expect("a path")],
        )
        .expect("attached");
        db.execute(
            "ATTACH DATABASE ? AS fresh",
            [fresh.to_str().expect("a path")],
        )
        .expect("attached");
        let mut gone = Vec::new();
        let mut kept = Vec::new();
        for t in tables(&db, "main") {
            let shared: Vec<String> = columns(&db, "main", &t)
                .into_iter()
                .filter(|c| columns(&db, "written", &t).contains(c))
                .collect();
            db.execute_batch(&format!(
                "INSERT INTO {t}({0}) SELECT {0} FROM written.{t};",
                shared.join(", ")
            ))
            .expect("copied");
            for c in columns(&db, "main", &t) {
                if !columns(&db, "fresh", &t).contains(&c) {
                    gone.push((t.clone(), c));
                }
            }
            let after: Vec<String> = columns(&db, "main", &t)
                .into_iter()
                .filter(|c| columns(&db, "fresh", &t).contains(c))
                .collect();
            kept.push((t, after));
        }
        db.execute_batch(&format!(
            "DETACH DATABASE written; DETACH DATABASE fresh; {extra}"
        ))
        .expect("the earlier hub's own rows");
        let dropped: Vec<(String, String)> = dropped
            .iter()
            .map(|(t, c)| ((*t).to_owned(), (*c).to_owned()))
            .collect();
        assert_eq!(gone, dropped, "the columns an upgrade takes away");
        let mut before = Vec::new();
        for (t, cols) in &kept {
            let all = rows(&db, t, cols, "1");
            assert!(!all.is_empty(), "{name}: {t} holds a row");
            let filter = taken(t).map_or_else(|| "1".to_owned(), |f| format!("NOT ({f})"));
            before.push(rows(&db, t, cols, &filter));
            if let Some(f) = taken(t) {
                assert!(
                    !rows(&db, t, cols, f).is_empty(),
                    "{name}: {t} holds a row the upgrade takes away"
                );
            }
        }
        drop(db);

        let r = open(&earlier);
        assert_eq!(schema_of_file(&earlier).expect("read"), SCHEMA_VERSION);
        let db = Connection::open(&earlier).expect("open");
        for ((t, cols), rows_before) in kept.iter().zip(&before) {
            assert_eq!(&rows(&db, t, cols, "1"), rows_before, "{name}: {t}");
        }
        let new = Connection::open(&fresh).expect("open");
        assert_eq!(
            schema_of(&db),
            schema_of(&new),
            "{name}: a new registry's schema"
        );

        // What it held reads as before.
        assert!(r.server("coldeve").expect("read").is_some());
        assert_eq!(
            r.session(&token).expect("read").map(|op| op.name),
            Some("Ann".to_owned()),
            "the session holds"
        );
        let view = r.person_view(&a("Ann@coldeve")).expect("a view");
        assert!(view.linked, "the link is kept");
        assert_eq!(view.persons.len(), 1);
        assert_eq!(view.persons[0].handle, a("Bob@frost"));
        let back = r.person_view(&a("Cat@coldeve")).expect("a view");
        assert_eq!(back.persons.len(), 1, "the friends handed back");
        assert!(r
            .channels()
            .expect("read")
            .iter()
            .any(|(n, _)| n == "oldguard"));
        if version >= 8 {
            assert!(
                r.directory_character("Bob@frost").expect("read").is_some(),
                "the directory's listing"
            );
            assert!(
                r.mirror_in_channel(CHANNEL).expect("read").is_some(),
                "the world's mirror"
            );
            let ann = r.session(&token).expect("read").expect("Ann");
            assert_eq!(
                r.identity(&ann.id, "coldeve").expect("read"),
                Some(a("Ann@coldeve")),
                "the character chosen to speak as"
            );
        }
        drop(r);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The rows hub 0.1.x wrote that this build no longer writes: a person's setting that showed
    /// its characters as one person, its change's event, and a value the hub kept of its own.
    const ROWS_7: &str = "
    UPDATE operators SET alts = 'shown' WHERE discord_id = '100';
    INSERT INTO operator_events(operator_id, kind, at, details)
      VALUES ((SELECT id FROM operators WHERE discord_id = '100'), 'alts',
              '2026-09-30T00:00:00.000Z', '{\"shown\":true}');
    INSERT INTO hub_state(name, value) VALUES ('kept', 'yes');
    INSERT INTO verifications VALUES ('v1', (SELECT id FROM servers WHERE slug = 'pending'),
      (SELECT id FROM operators WHERE discord_id = '200'), NULL, 'aa', '2026-09-01T00:00:00.000Z',
      '2026-08-31T00:00:00.000Z', 'open');
";

    /// Spec: registry: a registry hub 0.1.x kept (version 7), with rows in every table, opens on
    /// this build at its schema: every row comes through, column for column, but the person's
    /// setting that showed its characters as one person and that setting's events, which go; the
    /// schema it ends with is a new registry's, and what it held reads as before.
    #[test]
    fn a_registry_of_hub_0_1_moves_forward_keeping_every_row() {
        moves_forward("7", SCHEMA_7, 7, ROWS_7, &[("operators", "alts")]);
    }

    /// Spec: registry: a registry the build before 0.2 kept (version 8), the network's own hub's,
    /// with rows in every table (worlds, persons, links, the directory, mirrors, operators and
    /// sessions), opens on this build at its schema: every row comes through, column for column,
    /// but each person's two settings, the holds persons put on their characters' listings and the
    /// first setting's events, which go; the schema it ends with is a new registry's, and what it
    /// held reads as before.
    #[test]
    fn a_registry_of_the_build_before_0_2_moves_forward_keeping_every_row() {
        let rows_8 = format!(
            "{ROWS_7}
         UPDATE operators SET discord_tells = 'on' WHERE discord_id = '100';
         INSERT INTO directory_holds(address_key, address, world, by, owed, until)
           VALUES ('fay@frost', 'Fay@frost', 'frost', 'person', 0, '2099-01-01T00:00:00.000Z');"
        );
        moves_forward(
            "8",
            SCHEMA_8,
            8,
            &rows_8,
            &[("operators", "alts"), ("operators", "discord_tells")],
        );
    }
}
