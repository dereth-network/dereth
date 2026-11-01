//! Behaviour: none (tooling: Yalain's release archives: their names, their allowlist and deny scan,
//! the program's header check, the build's targets, the index and the release workflow).

use super::archive::{self, Member};
use super::headers::{self, Role};
use super::targets::{self, Os, Target};
use super::tests::pe;
use super::yalain::{self, Asset};
use crate::util::workspace_root;

const V: &str = "0.1.0";

fn target(triple: &str) -> Target {
    targets::find(triple).expect("a server release target")
}

/// A scratch folder in the temporary directory, made anew and removed when dropped.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("xtask-yalain-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        Self(dir)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Each archive is named for the version and the target, as Empyrean's are, for every target
/// Empyrean is released for; the folder's classification reads every name back and refuses the
/// rest.
#[test]
fn archives_are_named_for_version_and_target_and_read_back() {
    let w = target("x86_64-pc-windows-msvc");
    assert_eq!(
        yalain::archive_name(w, V),
        "yalain-0.1.0-x86_64-pc-windows-msvc.zip"
    );
    assert_eq!(
        yalain::archive_root(w, V),
        "yalain-0.1.0-x86_64-pc-windows-msvc"
    );
    let arm = target("aarch64-unknown-linux-gnu");
    assert_eq!(
        yalain::archive_name(arm, V),
        "yalain-0.1.0-aarch64-unknown-linux-gnu.tar.gz"
    );
    for t in targets::TARGETS {
        let name = yalain::archive_name(*t, V);
        assert_eq!(yalain::classify(&name, V), Ok(Asset::Archive(*t)));
        assert_eq!(
            yalain::classify(&format!("{name}.manifest"), V),
            Ok(Asset::ManifestPart)
        );
    }
    for index in ["SHA256SUMS", "MANIFEST.txt", "release.json"] {
        assert_eq!(yalain::classify(index, V), Ok(Asset::Index));
    }
    for stray in [
        "yalain-0.2.0-x86_64-pc-windows-msvc.zip",
        "empyrean-0.1.0-x86_64-pc-windows-msvc.zip",
        "yalain.sqlite",
    ] {
        assert!(yalain::classify(stray, V).is_err(), "{stray}");
    }
}

/// An archive holds the program, its notices, README and CHANGES, and nothing else.
#[test]
fn an_archive_holds_the_program_the_notices_readme_and_changes() {
    assert_eq!(
        yalain::allowlist(target("x86_64-pc-windows-msvc")),
        [
            "CHANGES.md",
            "LICENSE",
            "NOTICE.txt",
            "README.md",
            "THIRD-PARTY-LICENSES.html",
            "yalain.exe",
        ]
    );
    assert!(yalain::allowlist(target("x86_64-unknown-linux-gnu")).contains(&"yalain".to_owned()));
    let ws = workspace_root();
    for (_, path) in yalain::DOCUMENTS {
        assert!(ws.join(path).is_file(), "{path}");
    }
}

fn members(t: Target) -> Vec<Member> {
    yalain::allowlist(t)
        .into_iter()
        .map(|name| Member {
            bytes: name.as_bytes().to_vec(),
            executable: name.starts_with("yalain"),
            name,
        })
        .collect()
}

fn write_archive(dir: &std::path::Path, t: Target, members: &[Member]) -> std::path::PathBuf {
    let root = yalain::archive_root(t, V);
    let bytes = if t.os == Os::Windows {
        archive::zip_bytes(&root, members, 1_700_000_000).expect("zip")
    } else {
        archive::tar_gz_bytes(&root, members, 1_700_000_000).expect("tar.gz")
    };
    let path = dir.join(yalain::archive_name(t, V));
    std::fs::write(&path, bytes).expect("write");
    path
}

/// A written archive reads back and passes the scan; one with a stray file, a database or either
/// game's data file among its files, or without one of its files, is refused.
#[test]
fn a_written_archive_passes_the_scan_and_a_stray_or_missing_file_is_refused() {
    let scratch = Scratch::new("written");
    for triple in ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"] {
        let t = target(triple);
        let path = write_archive(&scratch.0, t, &members(t));
        yalain::check_archive(&path, t, V).expect("passes");
        for stray in ["yalain.sqlite", "cell_1.dat", "notes.txt"] {
            let mut more = members(t);
            more.push(Member {
                name: stray.to_owned(),
                bytes: b"x".to_vec(),
                executable: false,
            });
            let path = write_archive(&scratch.0, t, &more);
            assert!(yalain::check_archive(&path, t, V).is_err(), "{stray}");
        }
        let mut short = members(t);
        short.retain(|m| m.name != "NOTICE.txt");
        let path = write_archive(&scratch.0, t, &short);
        assert!(yalain::check_archive(&path, t, V).is_err());
    }
}

/// The index names the product and its tag, lists each archive with its checksum, and refuses a
/// file that belongs to no release in the folder.
#[test]
fn the_index_lists_every_archive_with_its_checksum() {
    let scratch = Scratch::new("index");
    let t = target("x86_64-unknown-linux-gnu");
    write_archive(&scratch.0, t, &members(t));
    let facts = super::BuildFacts {
        commit: "c".repeat(40),
        branch: "yalain-v0.1.0".to_owned(),
        number: "1".to_owned(),
        epoch: 1_700_000_000,
        source_url: super::DEFAULT_SOURCE_URL.to_owned(),
    };
    yalain::write_index(&scratch.0, V, &facts).expect("writes the index");
    let read = |name: &str| std::fs::read_to_string(scratch.0.join(name)).expect(name);
    let release: serde_json::Value = serde_json::from_str(&read("release.json")).expect("json");
    assert_eq!(release["product"], "yalain");
    assert_eq!(release["tag"], "yalain-v0.1.0");
    assert_eq!(release["assets"].as_array().map(Vec::len), Some(1));
    let sums = read("SHA256SUMS");
    assert!(
        sums.contains("  yalain-0.1.0-x86_64-unknown-linux-gnu.tar.gz\n"),
        "{sums}"
    );
    std::fs::write(scratch.0.join("stray.zip"), "x").expect("stray");
    assert!(yalain::write_index(&scratch.0, V, &facts).is_err());
}

/// The program is held to the server's header rules: on Windows a console program with its icon,
/// linking the C runtime in.
#[test]
fn the_program_is_checked_as_a_server() {
    let w = target("x86_64-pc-windows-msvc");
    let good = pe(0x8664, 3, &["KERNEL32.dll", "WS2_32.dll"], &[], true);
    assert!(headers::check_binary(&good, w, Role::Server).is_ok());
    let bare = pe(0x8664, 3, &["KERNEL32.dll"], &[], false);
    assert!(headers::check_binary(&bare, w, Role::Server).is_err());
    let runtime = pe(0x8664, 3, &["VCRUNTIME140.dll"], &[], true);
    assert!(headers::check_binary(&runtime, w, Role::Server).is_err());
}

/// The release workflow starts on Yalain's tags, checks the tag names the version, builds every
/// target with the package command (Linux with zigbuild), smoke-runs `--version` on the glibc
/// floor's distribution, labels every archive, and never marks the release "Latest".
#[test]
fn the_release_workflow_builds_and_labels_every_yalain_target() {
    let ws = workspace_root();
    let workflow = std::fs::read_to_string(ws.join(".github/workflows/release-yalain.yml"))
        .expect("the workflow");
    for needed in [
        "tags: ['yalain-v*']",
        "cargo xtask version yalain --tag \"$GITHUB_REF_NAME\"",
        "cargo xtask release-notes yalain --version \"$VERSION\"",
        "cargo xtask package yalain --target ${{ matrix.target }} --out dist",
        "cargo xtask package yalain --gather dist",
        "cargo-zigbuild",
        "almalinux:8",
        "grep -qx \"yalain $VERSION\" version.txt",
        "--draft --verify-tag --latest=false",
        "persist-credentials: false",
        "YALAIN_BUILD_SOURCE_URL",
    ] {
        assert!(workflow.contains(needed), "{needed}");
    }
    for t in targets::TARGETS {
        assert!(
            workflow.contains(&format!("target: {},", t.triple)),
            "{}",
            t.triple
        );
        let ext = t.archive_ext();
        assert!(
            workflow.contains(&format!("{}.{ext}) label=", t.triple)),
            "{}",
            t.triple
        );
    }
}
