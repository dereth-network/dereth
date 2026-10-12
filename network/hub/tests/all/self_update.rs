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
