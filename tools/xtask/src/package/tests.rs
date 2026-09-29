//! Behaviour: none (tooling: the release packaging's allowlist, deny scan, header checks,
//! archives, version and tag rules).

use super::archive::{self, Member};
use super::guard::{self, Entry, Kind};
use super::headers::{self, Role};
use super::notice;
use super::targets::{self, Arch, Builder, Host, Os};
use super::version;
use super::*;

fn target(triple: &str) -> Target {
    targets::find(triple).expect("a release target")
}

const WINDOWS: &str = "x86_64-pc-windows-msvc";
const LINUX_X64: &str = "x86_64-unknown-linux-gnu";
const LINUX_ARM: &str = "aarch64-unknown-linux-gnu";
const MAC_ARM: &str = "aarch64-apple-darwin";
const MAC_X64: &str = "x86_64-apple-darwin";

// ---------------------------------------------------------------- the allowlist

/// A package holds the two binaries, the six documents and the two generated notices, and on
/// Windows the binaries carry `.exe`.
#[test]
fn the_allowlist_is_the_binaries_the_documents_and_the_notices() {
    let linux = allowlist(target(LINUX_X64));
    assert_eq!(
        linux,
        [
            "ACE-BUGS.md",
            "DIVERGENCES.md",
            "LICENSE",
            "NOTICE.txt",
            "README.md",
            "SETUP.md",
            "THIRD-PARTY-LICENSES.html",
            "empyrean-import",
            "empyrean-server",
            "empyrean.toml.example",
        ]
    );
    let windows = allowlist(target(WINDOWS));
    assert!(windows.contains(&"empyrean-server.exe".to_owned()));
    assert!(windows.contains(&"empyrean-import.exe".to_owned()));
    assert!(!windows.contains(&"empyrean-server".to_owned()));
}

/// No allowlisted name is one the deny rules refuse, so the two can never fight over a file.
#[test]
fn no_allowlisted_name_is_one_the_deny_rules_refuse() {
    for t in targets::TARGETS {
        for name in allowlist(*t) {
            assert_eq!(guard::name_finding(&name), None, "{name}");
        }
    }
}

/// Every document the allowlist copies, the Lifestoned notice and cargo-about's inputs are in the
/// tree, and the template marks the section the per-binary pages are joined on.
#[test]
fn every_file_the_package_reads_from_the_tree_is_there() {
    let ws = workspace_root();
    for (_, path) in DOCUMENTS {
        assert!(ws.join(path).is_file(), "{path}");
    }
    for (_, manifest, _) in BINARIES {
        assert!(ws.join(manifest).is_file(), "{manifest}");
    }
    let lifestoned = std::fs::read_to_string(ws.join(LIFESTONED_NOTICE)).expect("the notice");
    assert!(lifestoned.contains("MIT License"), "{lifestoned}");
    assert!(ws.join(ABOUT_CONFIG).is_file());
    let template = std::fs::read_to_string(ws.join(ABOUT_TEMPLATE)).expect("the template");
    assert!(template.contains(notice::SECTION_START));
    assert!(template.contains(notice::SECTION_END));
}

// ---------------------------------------------------------------- the deny scan

fn clean_entries(t: Target) -> Vec<Entry> {
    let mut e = vec![Entry {
        path: String::new(),
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    }];
    e.extend(allowlist(t).iter().map(|n| Entry::file(n, b"contents")));
    e
}

/// The complete allowlist of plain files passes, and so the scan is not refusing everything.
#[test]
fn the_complete_allowlist_of_plain_files_passes_the_scan() {
    let t = target(LINUX_X64);
    assert_eq!(
        guard::scan(&clean_entries(t), &allowlist(t), guard::ARCHIVE_CAP),
        Vec::<String>::new()
    );
}

/// A file carrying the game's data-container header is refused whatever it is called, even on the
/// allowlist.
#[test]
fn a_file_with_the_data_header_is_refused_whatever_its_name() {
    let t = target(LINUX_X64);
    let mut bytes = vec![0u8; 0x400];
    bytes[0x140..0x144].copy_from_slice(&0x5442u32.to_le_bytes());
    let mut entries = clean_entries(t);
    let readme = entries
        .iter_mut()
        .find(|e| e.path == "README.md")
        .expect("on the allowlist");
    *readme = Entry::file("README.md", &bytes);
    let findings = guard::scan(&entries, &allowlist(t), guard::ARCHIVE_CAP);
    assert_eq!(findings.len(), 1, "{findings:?}");
    assert!(findings[0].contains("data-file header"), "{findings:?}");
    // The same magic one byte away is not the header.
    let mut shifted = vec![0u8; 0x400];
    shifted[0x141..0x145].copy_from_slice(&0x5442u32.to_le_bytes());
    assert!(!guard::has_dat_magic(&shifted));
    assert!(guard::has_dat_magic(&bytes));
}

/// The game's files, a world pack, the databases and a private configuration are refused by name,
/// in any case; the configuration's `.example` is not.
#[test]
fn data_state_and_private_names_are_refused_in_any_case() {
    for name in [
        "client_portal.dat",
        "CLIENT_CELL_1.DAT",
        "Client_HighRes.dat",
        "something.DAT",
        "acclient.exe",
        "AcClient.map",
        "00.00.11.6096.what",
        "world.pack",
        "world.pack.report.json",
        "shard.db",
        "auth.db",
        "auth.db-wal",
        "overlay.sqlite",
        "empyrean.toml",
        "EMPYREAN.TOML",
        "session.pcap",
        "fixtures/anything.txt",
        "packet-captures/raw/a.bin",
        "x/scrub-map/names.json",
    ] {
        assert!(guard::name_finding(name).is_some(), "{name} passed");
    }
    for name in [
        "empyrean.toml.example",
        "README.md",
        "empyrean-server.exe",
        "database.md",
    ] {
        assert_eq!(guard::name_finding(name), None, "{name} refused");
    }
}

/// A link, a folder, an extra file, a missing file and an oversized file are each refused.
#[test]
fn links_folders_strays_gaps_and_oversized_files_are_each_refused() {
    let t = target(LINUX_X64);
    let allow = allowlist(t);
    let with = |extra: Entry| {
        let mut e = clean_entries(t);
        e.push(extra);
        guard::scan(&e, &allow, guard::ARCHIVE_CAP)
    };
    let link = with(Entry {
        path: "dats".to_owned(),
        kind: Kind::Link("a symbolic link".to_owned()),
        size: 0,
        head: Vec::new(),
    });
    assert!(link.iter().any(|f| f.contains("no links")), "{link:?}");
    let dir = with(Entry {
        path: "docs".to_owned(),
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    });
    assert!(dir.iter().any(|f| f.contains("flat")), "{dir:?}");
    let stray = with(Entry::file("notes.txt", b"x"));
    assert!(
        stray.iter().any(|f| f.contains("not on the allowlist")),
        "{stray:?}"
    );

    let mut missing = clean_entries(t);
    missing.retain(|e| e.path != "NOTICE.txt");
    let gap = guard::scan(&missing, &allow, guard::ARCHIVE_CAP);
    assert!(
        gap.iter()
            .any(|f| f.contains("NOTICE.txt") && f.contains("missing")),
        "{gap:?}"
    );

    let mut big = clean_entries(t);
    big.iter_mut()
        .find(|e| e.path == "empyrean-server")
        .expect("listed")
        .size = guard::FILE_CAP + 1;
    let over = guard::scan(&big, &allow, guard::ARCHIVE_CAP);
    assert!(
        over.iter().any(|f| f.contains("cap for one file")),
        "{over:?}"
    );
    let total = guard::scan(&clean_entries(t), &allow, 10);
    assert!(
        total.iter().any(|f| f.contains("cap for one package")),
        "{total:?}"
    );
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-package-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temp directory");
    dir
}

/// The staging walk reports what is really on disk: files with their heads, and a folder inside.
#[test]
fn the_staging_walk_reports_nested_folders_and_file_heads() {
    let dir = temp("walk");
    std::fs::write(dir.join("README.md"), b"hello").expect("write");
    std::fs::create_dir(dir.join("inner")).expect("mkdir");
    std::fs::write(dir.join("inner").join("portal.dat"), b"x").expect("write");
    let entries = guard::staged_entries(&dir).expect("walk");
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["", "README.md", "inner", "inner/portal.dat"]);
    assert_eq!(entries[1].head, b"hello");
    let findings = guard::scan(&entries, &["README.md".to_owned()], guard::ARCHIVE_CAP);
    assert!(
        findings.iter().any(|f| f.starts_with("inner:")),
        "{findings:?}"
    );
    assert!(
        findings
            .iter()
            .any(|f| f.starts_with("inner/portal.dat: a .dat")),
        "{findings:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// A symbolic link in the staging folder is reported as a link, not followed.
#[cfg(unix)]
#[test]
fn a_symbolic_link_in_the_staging_folder_is_reported_as_a_link() {
    let dir = temp("symlink");
    std::fs::write(dir.join("target.bin"), b"x").expect("write");
    std::os::unix::fs::symlink(dir.join("target.bin"), dir.join("README.md")).expect("link");
    let entries = guard::staged_entries(&dir).expect("walk");
    let readme = entries
        .iter()
        .find(|e| e.path == "README.md")
        .expect("listed");
    assert!(matches!(readme.kind, Kind::Link(_)), "{readme:?}");
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------- archives

fn members() -> Vec<Member> {
    vec![
        Member {
            name: "empyrean-server".to_owned(),
            bytes: b"\x7fELF binary".to_vec(),
            executable: true,
        },
        Member {
            name: "LICENSE".to_owned(),
            bytes: b"licence text".to_vec(),
            executable: false,
        },
    ]
}

/// The same files and commit give byte-identical archives, the gzip header carries no time, and
/// every entry is under the one top-level folder in name order.
#[test]
fn archives_are_byte_identical_for_the_same_files_and_commit() {
    let epoch = 1_700_000_000;
    let a = archive::tar_gz_bytes("root", &members(), epoch).expect("tar.gz");
    let b = archive::tar_gz_bytes("root", &members(), epoch).expect("tar.gz");
    assert_eq!(a, b);
    assert_eq!(&a[4..8], &[0, 0, 0, 0], "the gzip header's time is zero");
    let z1 = archive::zip_bytes("root", &members(), epoch).expect("zip");
    let z2 = archive::zip_bytes("root", &members(), epoch).expect("zip");
    assert_eq!(z1, z2);
    for entries in [
        archive::tar_gz_entries(&a).expect("read"),
        archive::zip_entries(&z1).expect("read"),
    ] {
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["root/", "root/LICENSE", "root/empyrean-server"]);
        assert_eq!(entries[2].head, b"\x7fELF binary");
    }
    // A different commit time is a different archive.
    assert_ne!(
        a,
        archive::tar_gz_bytes("root", &members(), epoch + 1).expect("tar.gz")
    );
}

/// A tar entry carries the commit's time, mode 0755 for a binary and 0644 otherwise, and no owner.
#[test]
fn tar_entries_carry_the_commit_time_fixed_modes_and_no_owner() {
    let epoch = 1_700_000_000i64;
    let tar = archive::tar_bytes("root", &members(), epoch).expect("tar");
    let header = |i: usize| &tar[i * 512..i * 512 + 512];
    let text = |h: &[u8], r: std::ops::Range<usize>| {
        String::from_utf8_lossy(&h[r])
            .trim_end_matches('\0')
            .to_owned()
    };
    // Entry 0 the folder, 1 LICENSE (one data block follows), then the binary.
    let license = header(1);
    let binary = header(3);
    assert_eq!(text(license, 0..100), "root/LICENSE");
    assert_eq!(text(license, 100..108), "0000644");
    assert_eq!(text(binary, 100..108), "0000755");
    assert_eq!(text(binary, 108..116), "0000000");
    assert_eq!(text(binary, 265..297), "", "no owner name");
    assert_eq!(
        u64::from_str_radix(&text(binary, 136..148), 8).expect("octal"),
        1_700_000_000
    );
}

/// The archive scan reads what was written: a stray member, a symbolic link and a path outside the
/// top-level folder are refused from the archive itself.
#[test]
fn the_archive_scan_refuses_strays_links_and_escapes_in_the_written_archive() {
    let t = target(LINUX_X64);
    let root = t.archive_root("0.1.0");
    let good: Vec<Member> = allowlist(t)
        .into_iter()
        .map(|name| Member {
            name,
            bytes: b"x".to_vec(),
            executable: false,
        })
        .collect();
    let scan = |bytes: &[u8], zip: bool| {
        let raw = if zip {
            archive::zip_entries(bytes).expect("read")
        } else {
            archive::tar_gz_entries(bytes).expect("read")
        };
        guard::scan(
            &archive::relative_to_root(raw, &root),
            &allowlist(t),
            guard::ARCHIVE_CAP,
        )
    };
    let clean = archive::tar_gz_bytes(&root, &good, 0).expect("tar.gz");
    assert_eq!(scan(&clean, false), Vec::<String>::new());
    let clean_zip = archive::zip_bytes(&root, &good, 0).expect("zip");
    assert_eq!(scan(&clean_zip, true), Vec::<String>::new());

    let mut stray = good.clone();
    stray.push(Member {
        name: "client_portal.dat".to_owned(),
        bytes: b"x".to_vec(),
        executable: false,
    });
    let found = scan(&archive::zip_bytes(&root, &stray, 0).expect("zip"), true);
    assert!(
        found
            .iter()
            .any(|f| f.starts_with("client_portal.dat: a .dat")),
        "{found:?}"
    );

    // A tar with a symbolic link entry: the clean tar with one more header of type `2`.
    let mut tar = archive::tar_bytes(&root, &good, 0).expect("tar");
    tar.truncate(tar.len() - 1024);
    let mut link = [0u8; 512];
    let name = format!("{root}/dats");
    link[..name.len()].copy_from_slice(name.as_bytes());
    link[124..135].copy_from_slice(b"00000000000");
    link[156] = b'2';
    tar.extend_from_slice(&link);
    tar.resize(tar.len() + 1024, 0);
    let entries = archive::relative_to_root(archive::tar_entries(&tar).expect("read"), &root);
    let found = guard::scan(&entries, &allowlist(t), guard::ARCHIVE_CAP);
    assert!(
        found.iter().any(|f| f.starts_with("dats: a symbolic link")),
        "{found:?}"
    );

    let outside = archive::tar_gz_bytes("elsewhere", &good, 0).expect("tar.gz");
    let found = scan(&outside, false);
    assert!(
        found.iter().any(|f| f.starts_with("../elsewhere")),
        "{found:?}"
    );
}

/// Archives are named `empyrean-<version>-<target>`, a `.zip` for Windows and a `.tar.gz`
/// elsewhere, and the release folder holds nothing else but the manifest parts and the index.
#[test]
fn archives_are_named_for_version_and_target_and_the_release_folder_holds_nothing_else() {
    assert_eq!(
        target(WINDOWS).archive_name("0.1.0"),
        "empyrean-0.1.0-x86_64-pc-windows-msvc.zip"
    );
    assert_eq!(
        target(LINUX_ARM).archive_name("1.2.3"),
        "empyrean-1.2.3-aarch64-unknown-linux-gnu.tar.gz"
    );
    assert_eq!(
        target(MAC_ARM).archive_root("0.1.0"),
        "empyrean-0.1.0-aarch64-apple-darwin"
    );
    assert_eq!(
        classify("empyrean-0.1.0-x86_64-apple-darwin.tar.gz", "0.1.0"),
        Ok(Asset::Archive(target(MAC_X64)))
    );
    assert_eq!(
        classify(
            "empyrean-0.1.0-x86_64-pc-windows-msvc.zip.manifest",
            "0.1.0"
        ),
        Ok(Asset::ManifestPart)
    );
    assert_eq!(classify("SHA256SUMS", "0.1.0"), Ok(Asset::Index));
    for stray in [
        "empyrean-0.2.0-x86_64-pc-windows-msvc.zip",
        "empyrean-0.1.0-x86_64-pc-windows-msvc.tar.gz",
        "empyrean-0.1.0-x86_64-unknown-linux-gnu.zip",
        "client_portal.dat",
        "world.pack",
        // GitHub attaches the tag's source archives itself; the release folder carries none.
        "empyrean-0.1.0-source.tar.gz",
    ] {
        assert!(classify(stray, "0.1.0").is_err(), "{stray} was accepted");
    }
}

// ---------------------------------------------------------------- versions and tags

/// Every empyrean-* crate must carry one literal version; one that differs is named.
#[test]
fn every_empyrean_crate_must_carry_the_same_version() {
    let pair = |n: &str, v: Option<&str>| (n.to_owned(), v.map(str::to_owned));
    assert_eq!(
        version::shared_version(&[pair("a", Some("0.1.0")), pair("b", Some("0.1.0"))]),
        Ok("0.1.0".to_owned())
    );
    let err = version::shared_version(&[pair("a", Some("0.1.0")), pair("b", Some("0.2.0"))])
        .expect_err("they differ");
    assert!(err.contains("b: 0.2.0"), "{err}");
    let err = version::shared_version(&[pair("a", Some("0.1.0")), pair("c", None)])
        .expect_err("one inherits");
    assert!(err.contains("c: (none)"), "{err}");
    assert_eq!(
        version::package_version("[package]\nname = \"x\"\nversion.workspace = true\n"),
        None
    );
}

/// The tree's own empyrean-* crates agree on a release version today.
#[test]
fn the_trees_empyrean_crates_agree_on_a_release_version() {
    let v = version::empyrean_version(&workspace_root()).expect("one shared version");
    version::parse_release_version(&v).expect("a release version");
    assert!(
        version::empyrean_manifests(&workspace_root())
            .expect("manifests")
            .len()
            >= 10
    );
}

/// A release version is three plain numbers.
#[test]
fn a_release_version_is_three_plain_numbers() {
    assert_eq!(version::parse_release_version("0.1.0"), Ok([0, 1, 0]));
    assert_eq!(version::parse_release_version("10.20.30"), Ok([10, 20, 30]));
    assert_eq!(version::parse_release_version("0.1.0-rc.1"), Ok([0, 1, 0]));
    assert!(version::is_prerelease("0.1.0-rc.1"));
    assert!(!version::is_prerelease("0.1.0"));
    for bad in [
        "0.1",
        "0.1.0.0",
        "0.1.0+b",
        "0.1.0-",
        "0.1.0-rc..1",
        "0.1.0-rc_1",
        "01.2.3",
        "v0.1.0",
        "0..1",
        "",
    ] {
        assert!(
            version::parse_release_version(bad).is_err(),
            "{bad} accepted"
        );
    }
}

/// The tag must name exactly the version the tree carries.
#[test]
fn the_tag_must_name_exactly_the_trees_version() {
    assert_eq!(version::tag_for("0.1.0"), "empyrean-v0.1.0");
    assert_eq!(version::check_tag("empyrean-v0.1.0", "0.1.0"), Ok(()));
    assert_eq!(
        version::check_tag("refs/tags/empyrean-v0.1.0", "0.1.0"),
        Ok(())
    );
    let err = version::check_tag("empyrean-v0.1.1", "0.1.0").expect_err("mismatch");
    assert!(err.contains("0.1.1") && err.contains("0.1.0"), "{err}");
    assert!(version::check_tag("dereth-v0.1.0", "0.1.0").is_err());
    assert!(version::check_tag("v0.1.0", "0.1.0").is_err());
    assert_eq!(
        version::check_tag("empyrean-v0.1.0-rc.1", "0.1.0-rc.1"),
        Ok(())
    );
    assert!(version::check_tag("empyrean-v0.1.0-rc.1", "0.1.0").is_err());
}

/// The version edit changes the package's version line only, keeping its comment, and leaves a
/// dependency's version alone.
#[test]
fn the_version_edit_changes_the_package_line_only() {
    let before = "[package]\nname = \"empyrean-x\"\nversion = \"0.1.0\" # shared\nedition = \"2021\"\n\n[dependencies]\nlog = { version = \"0.4\" }\n[dev-dependencies]\nversion = \"9\"\n";
    let after = version::set_package_version(before, "0.2.0").expect("edited");
    assert_eq!(
        after,
        before.replace(
            "version = \"0.1.0\" # shared",
            "version = \"0.2.0\" # shared"
        )
    );
    assert_eq!(version::package_version(&after).as_deref(), Some("0.2.0"));
    assert!(
        version::set_package_version("[package]\nversion.workspace = true\n", "1.0.0").is_err()
    );
}

// ---------------------------------------------------------------- targets and hosts

fn host(os: Os, tools: bool) -> Host {
    Host {
        os: Some(os),
        zigbuild: tools,
        zig: tools,
        installed: None,
    }
}

/// Windows builds on Windows, macOS on macOS, and Linux anywhere zigbuild and zig are; a host that
/// cannot says why, naming what to install.
#[test]
fn each_host_builds_what_it_can_and_says_why_it_cannot() {
    assert_eq!(
        targets::plan(target(WINDOWS), &host(Os::Windows, false)),
        Ok(Builder::Cargo)
    );
    assert_eq!(
        targets::plan(target(MAC_X64), &host(Os::Mac, false)),
        Ok(Builder::Cargo)
    );
    assert_eq!(
        targets::plan(target(LINUX_ARM), &host(Os::Windows, true)),
        Ok(Builder::Zigbuild)
    );
    assert_eq!(
        targets::plan(target(LINUX_X64), &host(Os::Linux, true)),
        Ok(Builder::Zigbuild)
    );
    let e = targets::plan(target(WINDOWS), &host(Os::Linux, true)).expect_err("not here");
    assert!(e.contains("Windows host"), "{e}");
    let e = targets::plan(target(MAC_ARM), &host(Os::Windows, true)).expect_err("not here");
    assert!(e.contains("macOS host"), "{e}");
    let e = targets::plan(target(LINUX_X64), &host(Os::Linux, false)).expect_err("no zig");
    assert!(
        e.contains("cargo-zigbuild") && e.contains("zig on PATH"),
        "{e}"
    );
    let mut missing_std = host(Os::Windows, true);
    missing_std.installed = Some(vec![WINDOWS.to_owned()]);
    let e = targets::plan(target(LINUX_X64), &missing_std).expect_err("no std");
    assert!(
        e.contains("rustup target add x86_64-unknown-linux-gnu"),
        "{e}"
    );
    let e = targets::find("riscv64gc-unknown-linux-gnu").expect_err("not a target");
    assert!(e.contains(WINDOWS), "{e}");
}

/// A Linux build names the glibc floor to zigbuild; the others name the plain triple.
#[test]
fn a_linux_build_names_the_glibc_floor() {
    assert_eq!(
        target(LINUX_X64).build_target_arg(),
        "x86_64-unknown-linux-gnu.2.28"
    );
    assert_eq!(
        target(LINUX_ARM).build_target_arg(),
        "aarch64-unknown-linux-gnu.2.28"
    );
    assert_eq!(target(WINDOWS).build_target_arg(), WINDOWS);
    assert_eq!(target(MAC_ARM).build_target_arg(), MAC_ARM);
}

/// The build stamp comes from the commit's time, not the clock; Windows links the C runtime
/// statically without a link time; macOS names its floor.
#[test]
fn the_build_environment_stamps_the_commit_time_and_links_statically_on_windows() {
    let facts = BuildFacts {
        commit: "a".repeat(40),
        branch: "empyrean-v0.1.0".to_owned(),
        number: "12".to_owned(),
        epoch: 1_700_000_000,
        source_url: DEFAULT_SOURCE_URL.to_owned(),
    };
    assert_eq!(facts.utc_stamp(), "20231114221320");
    assert_eq!(facts.iso_time(), "2023-11-14T22:13:20Z");
    let get = |env: &[(String, String)], k: &str| {
        env.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
    };
    let remaps = [
        (PathBuf::from("/w"), "/dereth"),
        (PathBuf::from("/c"), "/cargo"),
    ];
    let win = build_env(&facts, target(WINDOWS), &remaps);
    let flags = get(&win, "CARGO_ENCODED_RUSTFLAGS").expect("flags");
    let flags: Vec<&str> = flags.split('\u{1f}').collect();
    assert!(flags.contains(&"-Ctarget-feature=+crt-static"), "{flags:?}");
    assert!(flags.contains(&"-Clink-arg=/Brepro"), "{flags:?}");
    assert!(flags
        .iter()
        .any(|f| f.starts_with("--remap-path-prefix=") && f.ends_with("=/cargo")));
    assert_eq!(
        get(&win, "EMPYREAN_BUILD_UTC").as_deref(),
        Some("20231114221320")
    );
    assert_eq!(
        get(&win, "SOURCE_DATE_EPOCH").as_deref(),
        Some("1700000000")
    );
    assert_eq!(get(&win, "EMPYREAN_BUILD_TARGET").as_deref(), Some(WINDOWS));
    assert_eq!(
        get(&win, "EMPYREAN_BUILD_BRANCH").as_deref(),
        Some("empyrean-v0.1.0")
    );
    let linux = build_env(&facts, target(LINUX_X64), &remaps);
    assert!(!get(&linux, "CARGO_ENCODED_RUSTFLAGS")
        .expect("flags")
        .contains("crt-static"));
    assert_eq!(get(&linux, "MACOSX_DEPLOYMENT_TARGET"), None);
    let mac = build_env(&facts, target(MAC_ARM), &remaps);
    assert_eq!(
        get(&mac, "MACOSX_DEPLOYMENT_TARGET").as_deref(),
        Some("11.0")
    );
}

/// The calendar conversion is right at the epoch, across a leap day and before 1970.
#[test]
fn the_calendar_conversion_is_right_at_the_edges() {
    assert_eq!(archive::civil(0), (1970, 1, 1, 0, 0, 0));
    assert_eq!(archive::civil(951_782_400), (2000, 2, 29, 0, 0, 0));
    assert_eq!(archive::civil(951_868_799), (2000, 2, 29, 23, 59, 59));
    assert_eq!(archive::civil(951_868_800), (2000, 3, 1, 0, 0, 0));
    assert_eq!(archive::civil(-1), (1969, 12, 31, 23, 59, 59));
}

// ---------------------------------------------------------------- header checks

/// A PE32+ image with one section holding the import and delay-import tables.
pub(super) fn pe(
    machine: u16,
    subsystem: u16,
    imports: &[&str],
    delayed: &[&str],
    resources: bool,
) -> Vec<u8> {
    let mut b = vec![0u8; 0x800];
    let put16 =
        |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let put32 =
        |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    b[..2].copy_from_slice(b"MZ");
    put32(&mut b, 0x3C, 0x80);
    b[0x80..0x84].copy_from_slice(b"PE\0\0");
    put16(&mut b, 0x84, machine);
    put16(&mut b, 0x86, 1);
    put16(&mut b, 0x94, 240);
    let opt = 0x98;
    put16(&mut b, opt, 0x20B);
    put16(&mut b, opt + 68, subsystem);
    put32(&mut b, opt + 108, 16);
    let (va, raw) = (0x1000u32, 0x200usize);
    let delay_off = (imports.len() + 1) * 20;
    let mut name_off = delay_off + (delayed.len() + 1) * 32;
    let mut name_rva = |b: &mut Vec<u8>, name: &str| {
        let at = raw + name_off;
        b[at..at + name.len()].copy_from_slice(name.as_bytes());
        let rva = va + u32::try_from(name_off).expect("small");
        name_off += name.len() + 1;
        rva
    };
    for (i, name) in imports.iter().enumerate() {
        let rva = name_rva(&mut b, name);
        put32(&mut b, raw + i * 20, 1);
        put32(&mut b, raw + i * 20 + 12, rva);
    }
    for (i, name) in delayed.iter().enumerate() {
        let rva = name_rva(&mut b, name);
        put32(&mut b, raw + delay_off + i * 32, 1);
        put32(&mut b, raw + delay_off + i * 32 + 4, rva);
    }
    put32(&mut b, opt + 112 + 8, va);
    put32(&mut b, opt + 112 + 12, 20);
    if resources {
        put32(&mut b, opt + 112 + 16, va + 0x500);
        put32(&mut b, opt + 112 + 20, 0x10);
    }
    if !delayed.is_empty() {
        put32(
            &mut b,
            opt + 112 + 13 * 8,
            va + u32::try_from(delay_off).expect("small"),
        );
        put32(&mut b, opt + 112 + 13 * 8 + 4, 32);
    }
    let s = opt + 240;
    b[s..s + 6].copy_from_slice(b".idata");
    put32(&mut b, s + 8, 0x600);
    put32(&mut b, s + 12, va);
    put32(&mut b, s + 16, 0x600);
    put32(&mut b, s + 20, u32::try_from(raw).expect("small"));
    b
}

/// A statically linked console x86-64 server with its icon passes, and the parse reads the
/// imports it was given.
#[test]
fn a_static_console_x64_pe_passes_and_its_imports_are_read() {
    let bytes = pe(
        0x8664,
        3,
        &["KERNEL32.dll", "WS2_32.dll"],
        &["bcrypt.dll"],
        true,
    );
    let parsed = headers::parse_pe(&bytes).expect("parses");
    assert_eq!(parsed.imports, ["KERNEL32.dll", "WS2_32.dll", "bcrypt.dll"]);
    let d = headers::check_binary(&bytes, target(WINDOWS), Role::Server).expect("passes");
    assert!(d.contains("KERNEL32.dll"), "{d}");
}

/// The Visual C++ runtime, imported or delay-loaded, fails the check and names the fix.
#[test]
fn a_pe_importing_the_visual_cpp_runtime_is_refused() {
    for (imports, delayed) in [
        (&["KERNEL32.dll", "VCRUNTIME140.dll"][..], &[][..]),
        (
            &["KERNEL32.dll"][..],
            &["api-ms-win-crt-runtime-l1-1-0.dll"][..],
        ),
        (&["MSVCP140.dll"][..], &[][..]),
    ] {
        let bytes = pe(0x8664, 3, imports, delayed, true);
        let problems = headers::check_binary(&bytes, target(WINDOWS), Role::Import)
            .expect_err("the runtime is imported");
        assert!(
            problems.iter().any(|p| p.contains("crt-static")),
            "{problems:?}"
        );
    }
}

/// The wrong machine, a GUI subsystem and a server without its icon are each refused; the
/// importer carries no icon and needs none.
#[test]
fn a_pe_with_the_wrong_machine_subsystem_or_missing_icon_is_refused() {
    let arm = pe(0xAA64, 3, &["KERNEL32.dll"], &[], true);
    assert!(headers::check_binary(&arm, target(WINDOWS), Role::Server).is_err());
    let gui = pe(0x8664, 2, &["KERNEL32.dll"], &[], true);
    let problems = headers::check_binary(&gui, target(WINDOWS), Role::Server).expect_err("GUI");
    assert!(
        problems.iter().any(|p| p.contains("console")),
        "{problems:?}"
    );
    let bare = pe(0x8664, 3, &["KERNEL32.dll"], &[], false);
    let problems =
        headers::check_binary(&bare, target(WINDOWS), Role::Server).expect_err("no icon");
    assert!(problems.iter().any(|p| p.contains("icon")), "{problems:?}");
    assert!(headers::check_binary(&bare, target(WINDOWS), Role::Import).is_ok());
    assert!(headers::parse_pe(b"not an executable").is_err());
}

/// This test binary, a real PE on Windows, parses as x86-64 importing KERNEL32.
#[cfg(all(windows, target_arch = "x86_64"))]
#[test]
fn a_real_windows_binary_parses() {
    let bytes = std::fs::read(std::env::current_exe().expect("exe")).expect("read");
    let parsed = headers::parse_pe(&bytes).expect("parses");
    assert_eq!(parsed.machine, headers::IMAGE_FILE_MACHINE_AMD64);
    assert!(parsed.pe32_plus);
    assert!(
        parsed
            .imports
            .iter()
            .any(|i| i.eq_ignore_ascii_case("kernel32.dll")),
        "{:?}",
        parsed.imports
    );
}

/// A 64-bit little-endian ELF: one load segment mapping the file, the loader, and a dynamic
/// section with `DT_NEEDED` entries and one version-needs record.
pub(super) fn elf(machine: u16, interp: &str, needed: &[&str], glibc: &[&str]) -> Vec<u8> {
    let mut b = vec![0u8; 0x800];
    let p16 = |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let p32 = |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let p64 = |b: &mut Vec<u8>, at: usize, v: u64| b[at..at + 8].copy_from_slice(&v.to_le_bytes());
    b[..4].copy_from_slice(b"\x7fELF");
    b[4] = 2;
    b[5] = 1;
    b[6] = 1;
    p16(&mut b, 16, 3);
    p16(&mut b, 18, machine);
    p64(&mut b, 32, 64);
    p16(&mut b, 54, 56);
    p16(&mut b, 56, 3);
    let (interp_at, strtab, verneed, dynamic) = (0x100usize, 0x200usize, 0x300usize, 0x400usize);
    b[interp_at..interp_at + interp.len()].copy_from_slice(interp.as_bytes());
    let mut str_end = 1usize;
    let mut add = |b: &mut Vec<u8>, s: &str| {
        let off = str_end;
        b[strtab + off..strtab + off + s.len()].copy_from_slice(s.as_bytes());
        str_end += s.len() + 1;
        u64::try_from(off).expect("small")
    };
    let needed_offs: Vec<u64> = needed.iter().map(|n| add(&mut b, n)).collect();
    let file = add(&mut b, "libc.so.6");
    let glibc_offs: Vec<u64> = glibc.iter().map(|g| add(&mut b, g)).collect();
    // Program headers: LOAD (the whole file), INTERP, DYNAMIC.
    let ph = |b: &mut Vec<u8>, i: usize, kind: u32, off: u64, size: u64| {
        let at = 64 + i * 56;
        p32(b, at, kind);
        p64(b, at + 8, off);
        p64(b, at + 16, off);
        p64(b, at + 32, size);
        p64(b, at + 40, size);
    };
    let mut entries: Vec<(u64, u64)> = needed_offs.iter().map(|&o| (1, o)).collect();
    entries.push((5, strtab as u64));
    entries.push((10, str_end as u64));
    if !glibc.is_empty() {
        entries.push((0x6fff_fffe, verneed as u64));
        entries.push((0x6fff_ffff, 1));
    }
    entries.push((0, 0));
    ph(&mut b, 0, 1, 0, 0x800);
    ph(&mut b, 1, 3, interp_at as u64, interp.len() as u64 + 1);
    ph(&mut b, 2, 2, dynamic as u64, entries.len() as u64 * 16);
    for (i, (tag, val)) in entries.iter().enumerate() {
        p64(&mut b, dynamic + i * 16, *tag);
        p64(&mut b, dynamic + i * 16 + 8, *val);
    }
    p16(&mut b, verneed, 1);
    p16(
        &mut b,
        verneed + 2,
        u16::try_from(glibc.len()).expect("small"),
    );
    p32(&mut b, verneed + 4, u32::try_from(file).expect("small"));
    p32(&mut b, verneed + 8, 16);
    for (i, off) in glibc_offs.iter().enumerate() {
        let aux = verneed + 16 + i * 16;
        p32(&mut b, aux + 8, u32::try_from(*off).expect("small"));
        p32(&mut b, aux + 12, if i + 1 < glibc.len() { 16 } else { 0 });
    }
    b
}

const X64_LOADER: &str = "/lib64/ld-linux-x86-64.so.2";

/// An x86-64 binary needing only the C library at glibc 2.28 passes; the parse reads its needs
/// and versions.
#[test]
fn an_elf_at_the_glibc_floor_needing_only_the_c_library_passes() {
    let bytes = elf(
        62,
        X64_LOADER,
        &["libc.so.6", "libm.so.6", "libgcc_s.so.1"],
        &["GLIBC_2.2.5", "GLIBC_2.28", "GLIBC_2.14"],
    );
    let parsed = headers::parse_elf(&bytes).expect("parses");
    assert_eq!(parsed.needed, ["libc.so.6", "libm.so.6", "libgcc_s.so.1"]);
    assert_eq!(parsed.glibc.iter().max(), Some(&(2, 28, 0)));
    let d = headers::check_binary(&bytes, target(LINUX_X64), Role::Server).expect("passes");
    assert!(d.contains("GLIBC_2.28"), "{d}");
}

/// A glibc symbol above the floor, a shared object beyond the C library's, the wrong loader and
/// the wrong machine are each refused.
#[test]
fn an_elf_above_the_floor_or_with_foreign_needs_is_refused() {
    let t = target(LINUX_X64);
    let newer = elf(
        62,
        X64_LOADER,
        &["libc.so.6"],
        &["GLIBC_2.28", "GLIBC_2.29"],
    );
    let p = headers::check_binary(&newer, t, Role::Server).expect_err("2.29");
    assert!(p.iter().any(|m| m.contains("GLIBC_2.29")), "{p:?}");
    let ssl = elf(
        62,
        X64_LOADER,
        &["libc.so.6", "libssl.so.3"],
        &["GLIBC_2.28"],
    );
    let p = headers::check_binary(&ssl, t, Role::Server).expect_err("libssl");
    assert!(p.iter().any(|m| m.contains("libssl.so.3")), "{p:?}");
    let loader = elf(
        62,
        "/lib/ld-musl-x86_64.so.1",
        &["libc.so.6"],
        &["GLIBC_2.28"],
    );
    assert!(headers::check_binary(&loader, t, Role::Server).is_err());
    let arm = elf(
        183,
        "/lib/ld-linux-aarch64.so.1",
        &["libc.so.6"],
        &["GLIBC_2.28"],
    );
    assert!(headers::check_binary(&arm, t, Role::Server).is_err());
    assert!(headers::check_binary(&arm, target(LINUX_ARM), Role::Server).is_ok());
    assert_eq!(headers::parse_glibc("GLIBC_2.2.5"), Some((2, 2, 5)));
    assert_eq!(headers::parse_glibc("GLIBC_PRIVATE"), None);
    assert_eq!(
        headers::linux_interp(Arch::Aarch64),
        "/lib/ld-linux-aarch64.so.1"
    );
}

/// A 64-bit Mach-O with the given libraries, build version and signature.
fn macho(cputype: u32, dylibs: &[&str], min: Option<(u32, u32)>, signed: bool) -> Vec<u8> {
    let mut cmds: Vec<u8> = Vec::new();
    let mut n = 0u32;
    let u32le = |v: u32| v.to_le_bytes();
    for d in dylibs {
        let mut name = d.as_bytes().to_vec();
        name.push(0);
        name.resize(name.len().div_ceil(8) * 8, 0);
        let size = 24 + u32::try_from(name.len()).expect("small");
        for v in [0xC, size, 24, 0, 0, 0] {
            cmds.extend_from_slice(&u32le(v));
        }
        cmds.extend_from_slice(&name);
        n += 1;
    }
    if let Some((major, minor)) = min {
        for v in [0x32, 24, 1, (major << 16) | (minor << 8), 0, 0] {
            cmds.extend_from_slice(&u32le(v));
        }
        n += 1;
    }
    if signed {
        for v in [0x1D, 16, 0, 0] {
            cmds.extend_from_slice(&u32le(v));
        }
        n += 1;
    }
    let mut b = Vec::new();
    for v in [
        0xFEED_FACF,
        cputype,
        0,
        2,
        n,
        u32::try_from(cmds.len()).expect("small"),
        0,
        0,
    ] {
        b.extend_from_slice(&u32le(v));
    }
    b.extend_from_slice(&cmds);
    b
}

const SYSTEM: &[&str] = &[
    "/usr/lib/libSystem.B.dylib",
    "/System/Library/Frameworks/Security.framework/Versions/A/Security",
];

/// A signed arm64 binary linking only system libraries at macOS 11 passes; an unsigned x86-64
/// one passes too (only Apple silicon demands the signature).
#[test]
fn a_macho_linking_only_the_system_at_the_floor_passes() {
    let arm = macho(headers::CPU_TYPE_ARM64, SYSTEM, Some((11, 0)), true);
    let parsed = headers::parse_macho(&arm).expect("parses");
    assert_eq!(parsed.dylibs, SYSTEM);
    assert_eq!(parsed.min_os, Some((11, 0, 0)));
    assert!(headers::check_binary(&arm, target(MAC_ARM), Role::Server).is_ok());
    let intel = macho(headers::CPU_TYPE_X86_64, SYSTEM, Some((10, 12)), false);
    assert!(headers::check_binary(&intel, target(MAC_X64), Role::Server).is_ok());
}

/// A Homebrew library, an unsigned arm64 binary, a minimum above the floor and the wrong CPU are
/// each refused.
#[test]
fn a_macho_with_foreign_libraries_no_signature_or_a_newer_minimum_is_refused() {
    let t = target(MAC_ARM);
    let brew = macho(
        headers::CPU_TYPE_ARM64,
        &[
            "/usr/lib/libSystem.B.dylib",
            "/opt/homebrew/lib/libsqlite3.dylib",
        ],
        Some((11, 0)),
        true,
    );
    let p = headers::check_binary(&brew, t, Role::Server).expect_err("homebrew");
    assert!(p.iter().any(|m| m.contains("/opt/homebrew")), "{p:?}");
    let unsigned = macho(headers::CPU_TYPE_ARM64, SYSTEM, Some((11, 0)), false);
    let p = headers::check_binary(&unsigned, t, Role::Server).expect_err("unsigned");
    assert!(p.iter().any(|m| m.contains("signature")), "{p:?}");
    let newer = macho(headers::CPU_TYPE_ARM64, SYSTEM, Some((14, 0)), true);
    let p = headers::check_binary(&newer, t, Role::Server).expect_err("14.0");
    assert!(p.iter().any(|m| m.contains("14.0")), "{p:?}");
    let intel = macho(headers::CPU_TYPE_X86_64, SYSTEM, Some((11, 0)), true);
    assert!(headers::check_binary(&intel, t, Role::Server).is_err());
}

// ---------------------------------------------------------------- notices

/// The crate list comes from cargo tree once per crate, with path, proc-macro and repeat markers
/// dropped.
#[test]
fn the_crate_list_is_read_from_cargo_tree_once_per_crate() {
    let out = "empyrean-server v0.1.0 (C:\\w\\empyrean\\server)|GPL-3.0-only\n\
               log v0.4.22|MIT OR Apache-2.0\n\
               serde_derive v1.0.200 (proc-macro)|MIT OR Apache-2.0\n\
               log v0.4.22|MIT OR Apache-2.0 (*)\n\
               dereth-dat v0.0.0 (C:\\w\\core\\dat)|MIT\n";
    let crates = notice::parse_cargo_tree(out);
    let names: Vec<(&str, &str)> = crates
        .iter()
        .map(|c| (c.name.as_str(), c.version.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("dereth-dat", "0.0.0"),
            ("empyrean-server", "0.1.0"),
            ("log", "0.4.22"),
            ("serde_derive", "1.0.200")
        ]
    );
    assert_eq!(crates[2].licence, "MIT OR Apache-2.0");
    assert!(
        notice::is_own(&crates[0]) && notice::is_own(&crates[1]) && !notice::is_own(&crates[2])
    );
}

/// NOTICE.txt offers the exact commit's source and the release's, says no game data is inside,
/// and carries the ACE attribution, the Lifestoned notice and the crate list.
#[test]
fn the_notice_offers_the_commits_source_and_carries_every_attribution() {
    let crates = notice::parse_cargo_tree(
        "log v0.4.22|MIT OR Apache-2.0\nempyrean-net v0.1.0 (p)|GPL-3.0-only\n",
    );
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let text = notice::notice(&notice::Facts {
        version: "0.1.0",
        target: WINDOWS,
        commit,
        source_url: "https://github.com/dereth-network/dereth/",
        crates: &crates,
        mit_licence: "MIT License\n\nCopyright (c) the contributors\n",
        lifestoned_notice: "MIT License\n\nCopyright (c) 2018 Scribble\n",
    });
    for needle in [
        &format!("https://github.com/dereth-network/dereth/tree/{commit}") as &str,
        "https://github.com/dereth-network/dereth/releases/tag/empyrean-v0.1.0",
        "https://github.com/dereth-network/dereth/archive/refs/tags/empyrean-v0.1.0.tar.gz",
        "https://github.com/dereth-network/dereth/archive/refs/tags/empyrean-v0.1.0.zip",
        "no Asheron's Call client files",
        "https://github.com/ACEmulator/ACE",
        "Copyright (c) 2018 Scribble",
        "public domain",
        "  log 0.4.22 (MIT OR Apache-2.0)",
        "  empyrean-net 0.1.0 (GPL-3.0-only)",
        notice::SERVER_LICENCE,
    ] {
        assert!(text.contains(needle), "missing {needle:?}:\n{text}");
    }
    assert!(
        !text.contains("dereth//tree"),
        "the trailing slash is not doubled"
    );
}

/// The per-binary licence pages join into one document: the first page's frame, then each
/// binary's section under its name.
#[test]
fn the_licence_pages_join_into_one_document_with_a_section_per_binary() {
    let page = |body: &str| {
        format!(
            "<html><head>h</head><body>{}{body}{}</body></html>",
            notice::SECTION_START,
            notice::SECTION_END
        )
    };
    let joined = notice::splice_licence_pages(&[
        ("empyrean-server", page("server licences")),
        ("empyrean-import", page("import licences")),
    ])
    .expect("joined");
    assert!(joined.starts_with("<html><head>h</head><body>"), "{joined}");
    assert!(joined.ends_with("</body></html>"), "{joined}");
    let server = joined
        .find("Linked into empyrean-server")
        .expect("server section");
    let import = joined
        .find("Linked into empyrean-import")
        .expect("import section");
    assert!(server < joined.find("server licences").expect("body") && import > server);
    assert!(joined.contains("import licences"));
    assert!(notice::splice_licence_pages(&[("x", "<html>no markers</html>".to_owned())]).is_err());
}

// ---------------------------------------------------------------- the upgrade declaration

use super::declaration::{self, Facts as UpgradeFacts, Renamed};

fn upgrade_facts(shard: i64, pack_format: u32) -> UpgradeFacts {
    UpgradeFacts {
        shard,
        auth: 1,
        pack_format,
        pack_schema: 100,
        renamed: Vec::new(),
    }
}

fn entries(text: &str) -> Vec<declaration::Entry> {
    declaration::parse_entries(text).expect("entries")
}

const FIRST_AND_MINOR: &str = "[[release]]\nversion = \"0.1.0\"\nupgrades_from = \"0.1.0\"\n\n\
     [[release]]\nversion = \"0.2.0\"\nupgrades_from = \"0.1.0\"\n\
     config = [{ key = \"server.gone\", change = \"removed\", note = \"no longer read\" }]\n";

/// The facts are read from the tree: every migration file the store holds is one it lists, and
/// the pack's versions are the constants the content crate reads packs with.
#[test]
fn the_trees_facts_are_its_migrations_and_pack_versions() {
    let ws = workspace_root();
    let read = |p: &str| std::fs::read_to_string(ws.join(p)).map_err(|e| e.to_string());
    let f = declaration::facts(&read).expect("the tree's facts");
    let files = |prefix: &str| {
        std::fs::read_dir(ws.join("empyrean/crates/store/src/schema"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
            .count() as i64
    };
    assert_eq!(
        f.shard,
        files("shard_v"),
        "every shard migration file is listed"
    );
    assert_eq!(
        f.auth,
        files("auth_v"),
        "every authentication migration file is listed"
    );
    assert!(f.pack_format >= 1 && f.pack_schema >= 100, "{f:?}");
}

/// The tree's own version declares cleanly against the tree's `releases.toml`.
#[test]
fn the_trees_version_has_a_declaration() {
    let ws = workspace_root();
    let v = version::empyrean_version(&ws).expect("the version");
    let d = declaration::for_release(&ws, &v).expect("declared");
    assert!(
        d["upgrades_from"].as_str().is_some_and(|s| !s.is_empty()),
        "{d}"
    );
    assert_eq!(d["databases"]["shard"].as_i64().map(|n| n >= 1), Some(true));
}

/// The code's facts: migrations counted, the pack's constants and the renamed keys read.
#[test]
fn facts_are_read_from_the_code_as_it_is_written() {
    let shard = "pub const SHARD_MIGRATIONS: &[&str] = &[\n    include_str!(\"schema/shard_v001.sql\"),\n    include_str!(\"schema/shard_v002.sql\"),\n];";
    let auth = "const AUTH_MIGRATIONS: &[&str] = &[include_str!(\"schema/auth_v001.sql\")];";
    let pack = "pub const FORMAT_VERSION: u32 = 2;\npub const SCHEMA_VERSION: u32 = 1_01;\n";
    let config = "pub const RENAMED: &[Renamed] = &[\n    Renamed { old: \"server.a\", new: \"server.b\", since: \"0.3.0\" },\n];\n";
    let read = |p: &str| -> Result<String, String> {
        Ok(match p {
            declaration::SHARD_SOURCE => shard,
            declaration::AUTH_SOURCE => auth,
            declaration::PACK_SOURCE => pack,
            declaration::CONFIG_SOURCE => config,
            other => return Err(format!("unexpected {other}")),
        }
        .to_owned())
    };
    let f = declaration::facts(&read).expect("facts");
    assert_eq!(
        (f.shard, f.auth, f.pack_format, f.pack_schema),
        (2, 1, 2, 101)
    );
    assert_eq!(
        f.renamed,
        [Renamed {
            old: "server.a".into(),
            new: "server.b".into(),
            since: "0.3.0".into()
        }]
    );
    let empty = |p: &str| -> Result<String, String> {
        if p == declaration::CONFIG_SOURCE {
            Ok("pub const RENAMED: &[Renamed] = &[];".to_owned())
        } else {
            read(p)
        }
    };
    assert!(declaration::facts(&empty).unwrap().renamed.is_empty());
}

/// A patch release declares nothing: no entry, no migration, no pack change, no renamed key; its
/// oldest upgradable version is inherited from its minor release.
#[test]
fn a_patch_release_must_declare_nothing() {
    let list = entries(FIRST_AND_MINOR);
    let same = upgrade_facts(2, 1);
    let d = declaration::declare("0.2.1", Some(("0.2.0", &same)), &same, &list).expect("clean");
    assert_eq!(d["kind"], "patch");
    assert_eq!(d["upgrades_from"], "0.1.0", "inherited from 0.2.0");
    assert_eq!(d["migrations"].as_array().map(Vec::len), Some(0));
    assert_eq!(d["world_pack_rebuild"], false);
    assert_eq!(d["config"].as_array().map(Vec::len), Some(0));

    // An entry of its own.
    let with_entry = entries(&format!(
        "{FIRST_AND_MINOR}\n[[release]]\nversion = \"0.2.1\"\nupgrades_from = \"0.2.0\"\n"
    ));
    let e = declaration::declare("0.2.1", Some(("0.2.0", &same)), &same, &with_entry).unwrap_err();
    assert!(e[0].contains("declares nothing"), "{e:?}");
    // A migration, a pack change and a rename, each refused.
    let mut changed = upgrade_facts(3, 2);
    changed.renamed.push(Renamed {
        old: "server.a".into(),
        new: "server.b".into(),
        since: "0.2.1".into(),
    });
    let e =
        declaration::declare("0.2.1-rc.1", Some(("0.2.0", &same)), &changed, &list).unwrap_err();
    assert_eq!(e.len(), 3, "{e:?}");
    assert!(e[0].contains("shard 2 -> 3"), "{e:?}");
    assert!(e[1].contains("world.pack changes"), "{e:?}");
    assert!(e[2].contains("renames"), "{e:?}");
}

/// A minor release needs an entry naming the oldest version that upgrades straight to it (the
/// previous release or older), and carries its migrations, pack rebuild and configuration changes.
#[test]
fn a_minor_release_declares_the_oldest_version_that_upgrades_to_it() {
    let list = entries(FIRST_AND_MINOR);
    let old = upgrade_facts(1, 1);
    let mut new = upgrade_facts(2, 2);
    new.renamed.push(Renamed {
        old: "server.x".into(),
        new: "server.y".into(),
        since: "0.2.0".into(),
    });
    new.renamed.push(Renamed {
        old: "server.older".into(),
        new: "server.newer".into(),
        since: "0.1.0".into(),
    });
    let d = declaration::declare("0.2.0", Some(("0.1.0", &old)), &new, &list).expect("declared");
    assert_eq!(d["kind"], "minor");
    assert_eq!(d["previous"], "0.1.0");
    assert_eq!(d["upgrades_from"], "0.1.0");
    assert_eq!(
        d["migrations"],
        serde_json::json!([{"database": "shard", "from": 1, "to": 2}])
    );
    assert_eq!(d["world_pack_rebuild"], true);
    assert_eq!(
        d["config"],
        serde_json::json!([
            {"key": "server.x", "change": "renamed", "to": "server.y"},
            {"key": "server.gone", "change": "removed", "note": "no longer read"}
        ])
    );

    // No entry.
    let e = declaration::declare("0.3.0", Some(("0.2.0", &new)), &new, &list).unwrap_err();
    assert!(e[0].contains("no [[release]] for 0.3.0"), "{e:?}");
    // Newer than the previous release: the previous release could not upgrade to it.
    let skip = entries("[[release]]\nversion = \"0.4.0\"\nupgrades_from = \"0.3.0\"\n");
    let e = declaration::declare("0.4.0", Some(("0.2.0", &new)), &new, &skip).unwrap_err();
    assert!(e[0].contains("newer than the previous release"), "{e:?}");
    // A schema going backwards.
    let e = declaration::declare("0.2.0", Some(("0.1.0", &new)), &old, &list).unwrap_err();
    assert!(e.iter().any(|p| p.contains("went back")), "{e:?}");
}

/// The first release has nothing before it: it upgrades from itself only.
#[test]
fn the_first_release_upgrades_from_itself() {
    let f = upgrade_facts(1, 1);
    let d = declaration::declare("0.1.0", None, &f, &entries(FIRST_AND_MINOR)).expect("declared");
    assert_eq!(d["kind"], "initial");
    assert_eq!(d["previous"], serde_json::Value::Null);
    let wrong = entries("[[release]]\nversion = \"0.1.0\"\nupgrades_from = \"0.0.9\"\n");
    assert!(declaration::declare("0.1.0", None, &f, &wrong).is_err());
}

/// The previous release is the newest release tag older than the version, pre-releases not
/// counted.
#[test]
fn the_previous_release_is_the_newest_older_release_tag() {
    let tags: Vec<String> = [
        "empyrean-v0.1.0",
        "empyrean-v0.1.1",
        "empyrean-v0.2.0-rc.1",
        "empyrean-v0.2.0",
        "dereth-v9.0.0",
    ]
    .map(str::to_owned)
    .to_vec();
    assert_eq!(
        declaration::previous_tag(&tags, "0.2.0").unwrap(),
        Some("0.1.1".into())
    );
    assert_eq!(
        declaration::previous_tag(&tags, "0.2.0-rc.2").unwrap(),
        Some("0.1.1".into())
    );
    assert_eq!(
        declaration::previous_tag(&tags, "0.2.1").unwrap(),
        Some("0.2.0".into())
    );
    assert_eq!(declaration::previous_tag(&tags, "0.1.0").unwrap(), None);
}

/// `releases.toml` is read strictly: unknown keys, pre-release entries, an `upgrades_from` newer
/// than the release, renames declared by hand and duplicates are refused.
#[test]
fn the_declarations_file_is_read_strictly() {
    let bad = [
        "[[release]]\nversion = \"0.2.0\"\nupgrades_from = \"0.1.0\"\nupgrade_from = \"x\"\n",
        "[[release]]\nversion = \"0.2.0-rc.1\"\nupgrades_from = \"0.1.0\"\n",
        "[[release]]\nversion = \"0.2.0\"\nupgrades_from = \"0.3.0\"\n",
        "[[release]]\nversion = \"0.2.0\"\nupgrades_from = \"0.1.0\"\nconfig = [{ key = \"a\", change = \"renamed\" }]\n",
        "[[release]]\nversion = \"0.2.0\"\nupgrades_from = \"0.1.0\"\n[[release]]\nversion = \"0.2.0\"\nupgrades_from = \"0.1.0\"\n",
        "[[release]]\nversion = \"0.2.0\"\n",
    ];
    for text in bad {
        assert!(declaration::parse_entries(text).is_err(), "{text}");
    }
    assert_eq!(entries(FIRST_AND_MINOR).len(), 2);
    assert!(entries("").is_empty());
}
