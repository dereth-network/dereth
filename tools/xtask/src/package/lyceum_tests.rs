//! Behaviour: none (tooling: Lyceum's release files, their contents and deny scan, the macOS
//! bundle and the Linux desktop entry, the header checks of its programs, its notice, its index,
//! version and tag, and its release workflow).

use super::archive::{self, Member};
use super::guard::{self, Entry, Kind};
use super::headers;
use super::lyceum::{self, Asset};
use super::notice;
use super::targets::{self, Os, Target};
use super::tests::{elf, macho, pe};
use super::version::{self, Product};
use crate::util::workspace_root;

fn target(triple: &str) -> Target {
    targets::find_lyceum(triple).expect("a Lyceum release target")
}

const WINDOWS: &str = "x86_64-pc-windows-msvc";
const LINUX: &str = "x86_64-unknown-linux-gnu";
const MAC_ARM: &str = "aarch64-apple-darwin";
const MAC_X64: &str = "x86_64-apple-darwin";
const V: &str = "0.1.0";

/// A scratch folder in the temporary directory, made anew and removed when dropped.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("xtask-lyceum-{name}-{}", std::process::id()));
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

// ---------------------------------------------------------------- names and contents

/// Each release file is named for the version and the platform, and the folder's classification
/// reads every name back; anything else is refused, among them another version's file, Dereth's
/// and a signature file (Lyceum has no updater to sign for).
#[test]
fn release_files_are_named_for_version_and_platform_and_read_back() {
    assert_eq!(
        lyceum::release_file_name(target(WINDOWS), V),
        "lyceum-0.1.0-windows-x86_64.zip"
    );
    assert_eq!(
        lyceum::release_file_name(target(MAC_ARM), V),
        "Lyceum-0.1.0-macos-aarch64.app.tar.gz"
    );
    assert_eq!(
        lyceum::release_file_name(target(MAC_X64), V),
        "Lyceum-0.1.0-macos-x86_64.app.tar.gz"
    );
    assert_eq!(
        lyceum::release_file_name(target(LINUX), V),
        "lyceum-0.1.0-linux-x86_64.tar.gz"
    );
    for triple in targets::LYCEUM_TRIPLES {
        let t = target(triple);
        let name = lyceum::release_file_name(t, V);
        assert_eq!(lyceum::classify(&name, V), Ok(Asset::Release(t)));
        assert_eq!(
            lyceum::classify(&format!("{name}.manifest"), V),
            Ok(Asset::ManifestPart)
        );
        assert_eq!(guard::name_finding(&name), None, "{name}");
        assert!(lyceum::classify(&format!("{name}.sig"), V).is_err());
    }
    for index in ["SHA256SUMS", "MANIFEST.txt", "release.json"] {
        assert_eq!(lyceum::classify(index, V), Ok(Asset::Index));
    }
    for stray in [
        "lyceum-0.0.9-windows-x86_64.zip",
        "dereth-0.1.0-windows-x86_64.zip",
        "client_portal.dat",
        "latest.json",
        "lyceum-0.1.0-linux-aarch64.tar.gz",
        "notes.txt",
    ] {
        assert!(lyceum::classify(stray, V).is_err(), "{stray}");
    }
    assert!(targets::find_lyceum("aarch64-unknown-linux-gnu").is_err());
}

/// The Windows zip holds, in one folder, the editor (which carries the icon), the command, the
/// notices, the README and the change log, and nothing else.
#[test]
fn the_windows_zip_holds_all_four_programs_the_notices_readme_and_changes() {
    let w = target(WINDOWS);
    assert_eq!(lyceum::archive_root(w, V), "lyceum-0.1.0");
    assert_eq!(
        lyceum::contents(w),
        [
            "CHANGES.md",
            "EMPYREAN-LICENSE",
            "LICENSE",
            "LIFESTONED-LICENSE.txt",
            "NOTICE.txt",
            "README.md",
            "THIRD-PARTY-LICENSES.html",
            "empyrean-import.exe",
            "empyrean-server.exe",
            "lyceum-editor.exe",
            "lyceum.exe",
        ]
    );
    assert_eq!(
        lyceum::programs(w),
        [
            ("lyceum-editor.exe".to_owned(), true),
            ("lyceum.exe".to_owned(), false),
            ("empyrean-server.exe".to_owned(), false),
            ("empyrean-import.exe".to_owned(), false)
        ]
    );
}

/// The Linux tarball holds all four programs and the desktop entry with its icon, named by the
/// application id the editor's window gives, beside the notices, the README and the change log.
#[test]
fn the_linux_tarball_holds_all_four_programs_and_a_desktop_entry_with_its_icon() {
    let l = target(LINUX);
    assert_eq!(lyceum::archive_root(l, V), "lyceum-0.1.0");
    assert_eq!(
        lyceum::contents(l),
        [
            "CHANGES.md",
            "EMPYREAN-LICENSE",
            "LICENSE",
            "LIFESTONED-LICENSE.txt",
            "NOTICE.txt",
            "README.md",
            "THIRD-PARTY-LICENSES.html",
            "empyrean-import",
            "empyrean-server",
            "lyceum",
            "lyceum-editor",
            "network.dereth.lyceum.desktop",
            "network.dereth.lyceum.png",
        ]
    );
    assert_eq!(
        lyceum::programs(l),
        [
            ("lyceum-editor".to_owned(), true),
            ("lyceum".to_owned(), false),
            ("empyrean-server".to_owned(), false),
            ("empyrean-import".to_owned(), false)
        ]
    );
    let entry = lyceum::desktop_entry();
    assert!(entry.starts_with("[Desktop Entry]\n"), "{entry}");
    for line in [
        "Type=Application",
        "Name=Lyceum",
        "Exec=lyceum-editor %f",
        "Icon=network.dereth.lyceum",
        "StartupWMClass=network.dereth.lyceum",
        "Terminal=false",
    ] {
        assert!(entry.lines().any(|l| l == line), "{line}: {entry}");
    }
    // The window gives the same application id, which is how the desktop finds the entry.
    let lib = std::fs::read_to_string(workspace_root().join("lyceum/editor/src/lib.rs"))
        .expect("the editor's lib.rs");
    assert!(
        lib.contains(&format!("APP_ID: &'static str = \"{}\"", lyceum::APP_ID)),
        "the editor's window names another application id"
    );
}

/// The macOS bundle carries all four programs in `Contents/MacOS`, MoltenVK in `Contents/Frameworks`
/// where the editor looks for it, the icon its `Info.plist` names, the notices, the README and the
/// change log in `Contents/Resources`, and its code signature.
#[test]
fn the_macos_bundle_carries_all_four_programs_moltenvk_the_icon_and_the_notices() {
    for triple in [MAC_ARM, MAC_X64] {
        let m = target(triple);
        assert_eq!(lyceum::archive_root(m, V), "Lyceum.app");
        let contents = lyceum::contents(m);
        for need in [
            "Contents/Info.plist",
            "Contents/PkgInfo",
            "Contents/MacOS/lyceum-editor",
            "Contents/MacOS/lyceum",
            "Contents/MacOS/empyrean-server",
            "Contents/MacOS/empyrean-import",
            "Contents/Frameworks/libMoltenVK.dylib",
            "Contents/Resources/lyceum.icns",
            "Contents/Resources/NOTICE.txt",
            "Contents/Resources/THIRD-PARTY-LICENSES.html",
            "Contents/Resources/LICENSE",
            "Contents/Resources/EMPYREAN-LICENSE",
            "Contents/Resources/LIFESTONED-LICENSE.txt",
            "Contents/Resources/README.md",
            "Contents/Resources/CHANGES.md",
            "Contents/_CodeSignature/CodeResources",
        ] {
            assert!(contents.contains(&need.to_owned()), "{need}");
        }
        assert_eq!(contents.len(), 16, "{contents:?}");
        assert_eq!(
            lyceum::programs(m),
            [
                ("Contents/MacOS/lyceum-editor".to_owned(), true),
                ("Contents/MacOS/lyceum".to_owned(), false),
                ("Contents/MacOS/empyrean-server".to_owned(), false),
                ("Contents/MacOS/empyrean-import".to_owned(), false)
            ]
        );
    }
}

/// The bundle's `Info.plist` names the executable and the icon file the bundle holds, the
/// identifier, the release's version (its numeric part, for a pre-release) and the macOS floor.
#[test]
fn the_info_plist_names_what_the_bundle_holds_and_the_version() {
    let plist = lyceum::info_plist("0.2.0-rc.1");
    assert!(plist.starts_with("<?xml version=\"1.0\""), "{plist}");
    let after = |key: &str| {
        let at = plist.find(&format!("<key>{key}</key>")).expect(key);
        let rest = &plist[at..];
        let s = rest.find("<string>").expect("a string") + "<string>".len();
        let e = rest.find("</string>").expect("its end");
        rest[s..e].to_owned()
    };
    assert_eq!(after("CFBundleExecutable"), "lyceum-editor");
    assert_eq!(after("CFBundleIconFile"), "lyceum.icns");
    assert_eq!(after("CFBundleIdentifier"), "network.dereth.lyceum");
    assert_eq!(after("CFBundleShortVersionString"), "0.2.0");
    assert_eq!(after("CFBundleVersion"), "0.2.0");
    assert_eq!(after("LSMinimumSystemVersion"), "11.0");
    let m = target(MAC_ARM);
    let contents = lyceum::contents(m);
    assert!(contents.contains(&format!("Contents/MacOS/{}", after("CFBundleExecutable"))));
    assert!(contents.contains(&format!("Contents/Resources/{}", after("CFBundleIconFile"))));
}

/// No name a package holds, and no release file's name, is one the deny rules refuse.
#[test]
fn no_name_in_a_package_is_one_the_deny_rules_refuse() {
    for triple in targets::LYCEUM_TRIPLES {
        let t = target(triple);
        for name in lyceum::contents(t) {
            assert_eq!(guard::name_finding(&name), None, "{name}");
        }
    }
}

fn file(path: &str, bytes: &[u8]) -> Entry {
    Entry::file(path, bytes)
}

fn dir(path: &str) -> Entry {
    Entry {
        path: path.to_owned(),
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    }
}

/// A macOS bundle's entries as staged: its folders and every file it holds.
fn bundle_entries() -> Vec<Entry> {
    let mut entries = vec![
        dir(""),
        dir("Contents"),
        dir("Contents/MacOS"),
        dir("Contents/Frameworks"),
        dir("Contents/Resources"),
        dir("Contents/_CodeSignature"),
    ];
    entries.extend(
        lyceum::contents(target(MAC_ARM))
            .iter()
            .map(|p| file(p, b"x")),
    );
    entries
}

/// A package passes with exactly its files; a stray, a folder no file is in, a link, the game's
/// data header, an oversized file or a missing file is each refused.
#[test]
fn a_package_scan_passes_its_own_files_and_refuses_everything_else() {
    let allowed = lyceum::contents(target(MAC_ARM));
    let scan = |entries: &[Entry]| {
        guard::scan_tree(entries, &allowed, &[], lyceum::FILE_CAP, lyceum::TOTAL_CAP)
    };
    assert_eq!(scan(&bundle_entries()), Vec::<String>::new());

    let mut stray = bundle_entries();
    stray.push(file("Contents/Resources/extra.txt", b"x"));
    assert_eq!(scan(&stray).len(), 1, "{:?}", scan(&stray));

    let mut empty_dir = bundle_entries();
    empty_dir.push(dir("Contents/Plugins"));
    assert!(scan(&empty_dir)[0].contains("folder"));

    let mut dats = bundle_entries();
    let mut head = vec![0u8; 0x200];
    head[0x140..0x144].copy_from_slice(&0x5442u32.to_le_bytes());
    dats.push(file("Contents/Resources/client_portal.dat", &head));
    let findings = scan(&dats);
    assert!(findings.iter().any(|f| f.contains(".dat")), "{findings:?}");
    assert!(
        findings.iter().any(|f| f.contains("header")),
        "{findings:?}"
    );

    let mut linked = bundle_entries();
    linked.push(Entry {
        path: "Contents/MacOS/link".to_owned(),
        kind: Kind::Link("a symbolic link".to_owned()),
        size: 0,
        head: Vec::new(),
    });
    assert!(scan(&linked).iter().any(|f| f.contains("link")));

    let mut big = bundle_entries();
    big.retain(|e| e.path != "Contents/MacOS/lyceum-editor");
    big.push(Entry {
        size: lyceum::FILE_CAP + 1,
        ..file("Contents/MacOS/lyceum-editor", b"x")
    });
    assert!(scan(&big).iter().any(|f| f.contains("cap")));

    let mut missing = bundle_entries();
    missing.retain(|e| e.path != "Contents/Frameworks/libMoltenVK.dylib");
    assert!(scan(&missing).iter().any(|f| f.contains("missing")));
}

/// Members for every file a target's package holds, the programs executable.
fn members(t: Target) -> Vec<Member> {
    let programs: Vec<String> = lyceum::programs(t).into_iter().map(|(p, _)| p).collect();
    lyceum::contents(t)
        .into_iter()
        .map(|name| Member {
            bytes: name.as_bytes().to_vec(),
            executable: programs.contains(&name),
            name,
        })
        .collect()
}

/// Write `t`'s release file, from `members`, into `dir` as the package writes it.
fn write_release(dir: &std::path::Path, t: Target, members: &[Member]) -> std::path::PathBuf {
    let root = lyceum::archive_root(t, V);
    let bytes = if t.os == Os::Windows {
        archive::zip_bytes(&root, members, 1_700_000_000).expect("zip")
    } else {
        archive::tar_gz_bytes(&root, members, 1_700_000_000).expect("tar.gz")
    };
    let path = dir.join(lyceum::release_file_name(t, V));
    std::fs::write(&path, bytes).expect("write");
    path
}

/// Each target's release file, written as the package writes it, reads back and passes the scan
/// of what was written; one with a stray file, or without one of its files, is refused.
#[test]
fn a_written_release_file_reads_back_and_passes_the_scan() {
    let scratch = Scratch::new("written");
    for triple in targets::LYCEUM_TRIPLES {
        let t = target(triple);
        let path = write_release(&scratch.0, t, &members(t));
        lyceum::check_archive(&path, t, V).expect("passes");

        let mut stray = members(t);
        stray.push(Member {
            name: "notes.txt".to_owned(),
            bytes: b"x".to_vec(),
            executable: false,
        });
        let path = write_release(&scratch.0, t, &stray);
        assert!(lyceum::check_archive(&path, t, V).is_err(), "{triple}");

        let mut short = members(t);
        short.retain(|m| !m.name.ends_with("NOTICE.txt"));
        let path = write_release(&scratch.0, t, &short);
        let e = lyceum::check_archive(&path, t, V).expect_err("no notice");
        assert!(e.contains("NOTICE.txt"), "{e}");
    }
}

/// The index lists every release file with its checksum, names the release, and takes in every
/// target's manifest lines; a file that belongs to no release in the folder is refused.
#[test]
fn the_index_lists_every_release_file_with_its_checksum() {
    let scratch = Scratch::new("index");
    for triple in [WINDOWS, LINUX] {
        let t = target(triple);
        let path = write_release(&scratch.0, t, &members(t));
        std::fs::write(
            scratch
                .0
                .join(format!("{}.manifest", lyceum::release_file_name(t, V))),
            format!("{triple}\tline\n"),
        )
        .expect("manifest part");
        assert!(path.is_file());
    }
    let facts = super::BuildFacts {
        commit: "a".repeat(40),
        branch: "lyceum-v0.1.0".to_owned(),
        number: "1".to_owned(),
        epoch: 1_700_000_000,
        source_url: "https://github.com/someone/fork".to_owned(),
    };
    lyceum::write_index(&scratch.0, V, &facts).expect("writes the index");
    let read = |name: &str| std::fs::read_to_string(scratch.0.join(name)).expect(name);
    let release: serde_json::Value = serde_json::from_str(&read("release.json")).expect("json");
    assert_eq!(release["product"], "lyceum");
    assert_eq!(release["tag"], "lyceum-v0.1.0");
    assert_eq!(release["source_url"], "https://github.com/someone/fork");
    assert_eq!(release["assets"].as_array().map(Vec::len), Some(2));
    let manifest = read("MANIFEST.txt");
    assert!(manifest.starts_with(&format!("# lyceum {V}, commit {}", "a".repeat(40))));
    assert!(
        manifest.contains("x86_64-pc-windows-msvc\tline\n"),
        "{manifest}"
    );
    let sums = read("SHA256SUMS");
    for name in [
        "lyceum-0.1.0-windows-x86_64.zip",
        "lyceum-0.1.0-linux-x86_64.tar.gz",
        "MANIFEST.txt",
        "release.json",
    ] {
        let bytes = std::fs::read(scratch.0.join(name)).expect(name);
        assert!(
            sums.contains(&format!("{}  {name}\n", super::sha256_hex(&bytes))),
            "{name}: {sums}"
        );
    }
    std::fs::write(scratch.0.join("latest.json"), "{}").expect("stray");
    assert!(lyceum::write_index(&scratch.0, V, &facts).is_err());
}

// ---------------------------------------------------------------- signing

/// The bundle is signed inside out, MoltenVK and the command before the bundle; ad hoc without a
/// Developer ID, with the hardened runtime off, and with one under the hardened runtime.
#[test]
fn the_bundle_is_signed_inside_out_ad_hoc_or_with_a_developer_id() {
    for none in [None, Some(""), Some("-"), Some("  ")] {
        let runs = lyceum::codesign_runs("L.app", none);
        let paths: Vec<&str> = runs.iter().map(|r| r.last().unwrap().as_str()).collect();
        assert_eq!(
            paths,
            [
                "L.app/Contents/Frameworks/libMoltenVK.dylib",
                "L.app/Contents/MacOS/lyceum",
                "L.app/Contents/MacOS/empyrean-server",
                "L.app/Contents/MacOS/empyrean-import",
                "L.app"
            ]
        );
        for r in &runs {
            assert_eq!(r[..3], ["--force", "--sign", "-"], "{none:?}");
            assert!(!r.contains(&"runtime".to_owned()), "{none:?}");
        }
    }
    let id = "Developer ID Application: X (T)";
    for r in lyceum::codesign_runs("L.app", Some(id)) {
        assert_eq!(r[..3], ["--force", "--sign", id]);
        assert!(r.windows(2).any(|w| w == ["--options", "runtime"]), "{r:?}");
        assert!(r.contains(&"--timestamp".to_owned()), "{r:?}");
    }
}

// ---------------------------------------------------------------- the programs' headers

/// On Windows the editor is a windowed program that carries its icon, as Dereth's are, and the
/// command a console program; each links the C runtime in. A console editor, an editor without
/// its icon, a windowed command or a program importing the Visual C++ runtime is refused.
#[test]
fn the_windows_editor_is_windowed_with_its_icon_and_the_command_a_console_program() {
    let w = target(WINDOWS);
    let floor = targets::LYCEUM_GLIBC_FLOOR;
    assert_eq!(
        lyceum::programs(w),
        [
            ("lyceum-editor.exe".to_owned(), true),
            ("lyceum.exe".to_owned(), false),
            ("empyrean-server.exe".to_owned(), false),
            ("empyrean-import.exe".to_owned(), false)
        ]
    );
    let editor = pe(0x8664, 2, &["KERNEL32.dll", "USER32.dll"], &[], true);
    let d = headers::check_app_binary(&editor, w, floor).expect("passes");
    assert!(d.contains("windowed") && d.contains("resources"), "{d}");
    let console = pe(0x8664, 3, &["KERNEL32.dll"], &[], true);
    let e = headers::check_app_binary(&console, w, floor).expect_err("a console editor");
    assert!(e.iter().any(|p| p.contains("windowed")), "{e:?}");
    let bare = pe(0x8664, 2, &["KERNEL32.dll"], &[], false);
    let e = headers::check_app_binary(&bare, w, floor).expect_err("no icon");
    assert!(e.iter().any(|p| p.contains("icon")), "{e:?}");

    let command = pe(0x8664, 3, &["KERNEL32.dll"], &[], false);
    let d = headers::check_console_app(&command, w, floor).expect("passes");
    assert!(d.contains("console"), "{d}");
    let e = headers::check_console_app(&editor, w, floor).expect_err("a windowed command");
    assert!(e.iter().any(|p| p.contains("subsystem")), "{e:?}");
    let runtime = pe(0x8664, 3, &["VCRUNTIME140.dll"], &[], false);
    assert!(headers::check_console_app(&runtime, w, floor).is_err());
}

/// A Linux program may link the desktop's sound and window libraries, but no glibc symbol newer
/// than the Ubuntu 22.04 it is built on; a macOS program links only the system's libraries.
#[test]
fn the_linux_and_macos_programs_are_held_to_their_floors() {
    let l = target(LINUX);
    let floor = targets::LYCEUM_GLIBC_FLOOR;
    assert_eq!(floor, (2, 35));
    let interp = headers::linux_interp(targets::Arch::X86_64);
    let good = elf(
        62,
        interp,
        &["libasound.so.2", "libc.so.6"],
        &["GLIBC_2.34", "GLIBC_2.35"],
    );
    assert!(headers::check_app_binary(&good, l, floor).is_ok());
    assert!(headers::check_console_app(&good, l, floor).is_ok());
    let newer = elf(62, interp, &["libc.so.6"], &["GLIBC_2.39"]);
    let e = headers::check_console_app(&newer, l, floor).expect_err("too new");
    assert!(e.iter().any(|p| p.contains("2.35")), "{e:?}");

    let m = target(MAC_ARM);
    let system = ["/usr/lib/libSystem.B.dylib"];
    let signed = macho(headers::CPU_TYPE_ARM64, &system, Some((11, 0)), true);
    assert!(headers::check_app_binary(&signed, m, floor).is_ok());
    assert!(headers::check_console_app(&signed, m, floor).is_ok());
    let brewed = macho(
        headers::CPU_TYPE_ARM64,
        &["/opt/homebrew/lib/libvulkan.1.dylib"],
        Some((11, 0)),
        true,
    );
    assert!(headers::check_console_app(&brewed, m, floor).is_err());
}

/// Each Lyceum target builds on its own operating system only, and says when the target's
/// standard library is missing.
#[test]
fn lyceum_builds_natively_and_names_what_is_missing() {
    let host = |os| targets::Host {
        os: Some(os),
        zigbuild: true,
        zig: true,
        installed: None,
    };
    assert!(targets::plan_lyceum(target(WINDOWS), &host(Os::Windows)).is_ok());
    let e = targets::plan_lyceum(target(LINUX), &host(Os::Windows)).expect_err("cross");
    assert!(e.contains("Linux host only"), "{e}");
    assert!(targets::plan_lyceum(target(MAC_X64), &host(Os::Mac)).is_ok());
    let missing = targets::Host {
        installed: Some(vec![MAC_ARM.to_owned()]),
        ..host(Os::Mac)
    };
    let e = targets::plan_lyceum(target(MAC_X64), &missing).expect_err("not installed");
    assert!(e.contains("rustup target add"), "{e}");
}

/// A release build takes the commit's time and keeps this machine's folders out of the programs;
/// on Windows it links the C runtime in, and on macOS it claims the release's floor.
#[test]
fn a_release_build_links_statically_on_windows_and_claims_the_macos_floor() {
    let facts = super::BuildFacts {
        commit: "a".repeat(40),
        branch: "lyceum-v0.1.0".to_owned(),
        number: "1".to_owned(),
        epoch: 1_700_000_000,
        source_url: super::DEFAULT_SOURCE_URL.to_owned(),
    };
    let ws = workspace_root();
    let value = |env: &[(String, String)], key: &str| {
        env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    };
    let windows = lyceum::build_env(&ws, &facts, target(WINDOWS));
    let flags = value(&windows, "CARGO_ENCODED_RUSTFLAGS").expect("flags");
    assert!(
        flags.contains("+crt-static") && flags.contains("/Brepro"),
        "{flags}"
    );
    assert!(flags.contains("--remap-path-prefix="), "{flags}");
    assert_eq!(
        value(&windows, "SOURCE_DATE_EPOCH").as_deref(),
        Some("1700000000")
    );
    let mac = lyceum::build_env(&ws, &facts, target(MAC_ARM));
    assert_eq!(
        value(&mac, "MACOSX_DEPLOYMENT_TARGET").as_deref(),
        Some("11.0")
    );
    let linux = lyceum::build_env(&ws, &facts, target(LINUX));
    for env in [&windows, &mac, &linux] {
        assert_eq!(value(env, "LYCEUM_BUNDLED_SERVER").as_deref(), Some("1"));
    }
    assert!(!value(&linux, "CARGO_ENCODED_RUSTFLAGS")
        .expect("flags")
        .contains("crt-static"));
    // What all four programs name in `--version`, which the release workflow checks.
    assert_eq!(
        value(&linux, "LYCEUM_BUILD_COMMIT").as_deref(),
        Some("a".repeat(40).as_str())
    );
    assert_eq!(value(&linux, "LYCEUM_BUILD_TARGET").as_deref(), Some(LINUX));
    let editor = std::fs::read_to_string(workspace_root().join("lyceum/editor/src/start.rs"))
        .expect("the editor's start");
    let command = std::fs::read_to_string(workspace_root().join("lyceum/cli/src/main.rs"))
        .expect("the command");
    for (program, text) in [("editor", editor), ("command", command)] {
        for var in ["LYCEUM_BUILD_COMMIT", "LYCEUM_BUILD_TARGET"] {
            assert!(
                text.contains(&format!("option_env!(\"{var}\")")),
                "the {program} reads {var}"
            );
        }
    }
}

// ---------------------------------------------------------------- the notice

/// The notice names the source and the release, says there is no game data, carries the
/// typeface's license, and carries MoltenVK's only where the package carries MoltenVK; Lyceum's
/// crates are the project's own and the rest third parties'.
#[test]
fn the_notice_names_the_source_and_carries_the_licenses_the_package_needs() {
    let crates = [
        notice::Crate {
            name: "egui".to_owned(),
            version: "0.36.2".to_owned(),
            licence: "MIT OR Apache-2.0".to_owned(),
        },
        notice::Crate {
            name: "lyceum-foundry".to_owned(),
            version: "0.1.0".to_owned(),
            licence: "MIT".to_owned(),
        },
    ];
    let fonts = [(
        "Liberation",
        "The editor's text is drawn in Liberation",
        "Liberation OFL".to_owned(),
    )];
    let facts = |moltenvk| notice::LyceumFacts {
        version: V,
        target: MAC_ARM,
        commit: "abc123",
        source_url: "https://github.com/dereth-network/dereth",
        crates: &crates,
        mit_license: "MIT License",
        fonts: &fonts,
        moltenvk_license: moltenvk,
    };
    let text = notice::lyceum_notice(&facts(Some("Apache License")));
    assert!(
        text.starts_with("Lyceum 0.1.0 (aarch64-apple-darwin)\n"),
        "{text}"
    );
    assert!(text.contains("https://github.com/dereth-network/dereth/tree/abc123"));
    assert!(text.contains("releases/tag/lyceum-v0.1.0"));
    assert!(text.contains("NO GAME DATA"));
    assert!(text.contains("EMPYREAN-LICENSE"));
    assert!(text.contains("LIFESTONED-LICENSE.txt"));
    assert!(text.contains("/tree/abc123/empyrean"));
    assert!(text.contains("/archive/abc123.tar.gz"));
    assert!(text.contains("THE LIBERATION TYPEFACE") && text.contains("Liberation OFL"));
    assert!(text.contains("the editor draws through MoltenVK") && text.contains("Apache License"));
    let own = text.find("This project's own crates:").expect("own");
    let third = text.find("Third-party crates").expect("third");
    let foundry = text.find("lyceum-foundry 0.1.0").expect("the Foundry");
    let egui = text.find("egui 0.36.2").expect("egui");
    assert!(own < foundry && foundry < third && third < egui, "{text}");
    let windows = notice::lyceum_notice(&facts(None));
    assert!(!windows.contains("MOLTENVK"));
}

// ---------------------------------------------------------------- the tree

/// Every file the Lyceum package reads from the tree is there: the license, cargo-about's
/// configuration and a template with the section markers, the README and the change log, the
/// icon and the picture the desktop entry names, and the typeface's license.
#[test]
fn every_file_the_lyceum_package_reads_from_the_tree_is_there() {
    let ws = workspace_root();
    for path in [
        "LICENSE",
        "lyceum/about.toml",
        "lyceum/about.hbs",
        "lyceum/editor/Cargo.toml",
        "lyceum/cli/Cargo.toml",
        "empyrean/server/Cargo.toml",
        "empyrean/import/Cargo.toml",
        "empyrean/about.toml",
        "empyrean/about.hbs",
    ] {
        assert!(ws.join(path).is_file(), "{path}");
    }
    for (_, path) in lyceum::DOCUMENTS {
        assert!(ws.join(path).is_file(), "{path}");
    }
    for (_, _, path) in lyceum::FONTS {
        assert!(ws.join(path).is_file(), "{path}");
    }
    let icns = std::fs::read(ws.join(lyceum::ICNS)).expect("the icon");
    assert_eq!(&icns[..4], b"icns");
    let png = std::fs::read(ws.join(lyceum::ICON_PNG)).expect("the picture");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    let hbs = std::fs::read_to_string(ws.join("lyceum/about.hbs")).expect("template");
    assert!(hbs.contains(notice::SECTION_START) && hbs.contains(notice::SECTION_END));
    assert!(hbs.contains("<title>Lyceum"));
}

// ---------------------------------------------------------------- version and tag

/// The editor, the command and the Foundry carry one version, Lyceum's own rather than the
/// workspace's, which the tag must name exactly.
#[test]
fn the_editor_the_command_and_the_foundry_carry_one_version_which_the_tag_names() {
    let ws = workspace_root();
    let v = Product::Lyceum.version(&ws).expect("the three agree");
    version::parse_release_version(&v).expect("a release version");
    assert_eq!(version::lyceum_manifests(&ws).len(), 3);
    let tag = Product::Lyceum.tag_for(&v);
    assert_eq!(tag, format!("lyceum-v{v}"));
    assert!(version::check_product_tag(Product::Lyceum, &tag, &v).is_ok());
    assert!(version::check_product_tag(Product::Lyceum, &format!("refs/tags/{tag}"), &v).is_ok());
    let e = version::check_product_tag(Product::Lyceum, &format!("dereth-v{v}"), &v)
        .expect_err("another product's tag");
    assert!(e.contains("a Lyceum release tag"), "{e}");
    assert!(version::check_product_tag(Product::Lyceum, "lyceum-v9.9.9", &v).is_err());
    assert_eq!(Product::from_command_name("lyceum"), Some(Product::Lyceum));
}

/// The release command's edit moves a Lyceum manifest's own version and keeps its comment.
#[test]
fn the_version_edit_moves_a_lyceum_manifest_and_keeps_its_comment() {
    let ws = workspace_root();
    for manifest in version::lyceum_manifests(&ws) {
        let text = std::fs::read_to_string(&manifest).expect("manifest");
        let edited = version::set_package_version(&text, "0.2.0").expect("a literal version");
        assert_eq!(version::package_version(&edited).as_deref(), Some("0.2.0"));
        assert!(
            edited.contains("\"0.2.0\" # Lyceum's own version"),
            "{edited}"
        );
        assert_eq!(edited.lines().count(), text.lines().count());
    }
}

// ---------------------------------------------------------------- the workflow

/// The release workflow starts on Lyceum's tags, checks the tag names the version, builds every
/// Lyceum target with the package command on its own system, gathers them, gives every release
/// file a label, carries the same MoltenVK as Dereth's, and checks out nothing but this
/// repository.
#[test]
fn the_release_workflow_builds_and_labels_every_lyceum_target() {
    let ws = workspace_root();
    let read = |f: &str| std::fs::read_to_string(ws.join(f)).expect(f);
    let workflow = read(".github/workflows/release-lyceum.yml");
    for needed in [
        "tags: ['lyceum-v*']",
        "cargo xtask version lyceum --tag \"$GITHUB_REF_NAME\"",
        "cargo xtask release-notes lyceum --version \"$VERSION\"",
        "cargo xtask package lyceum --target ${{ matrix.target }} --out dist",
        "cargo xtask package lyceum --gather dist",
        "--draft --verify-tag",
        "persist-credentials: false",
        "grep -qx \"lyceum-editor $VERSION\" version-editor.txt",
        "grep -qx \"lyceum $VERSION\" version-command.txt",
        "grep -qx \"target: $TARGET\" version-editor.txt",
        "\"$editor\" --list-recent",
    ] {
        assert!(workflow.contains(needed), "{needed}");
    }
    // The start screen's lines the smoke run looks for are the editor's own words.
    let text = read("lyceum/editor/lyceum-text.toml");
    for line in ["Open a world…", "New world from the sample"] {
        assert!(
            workflow.contains(&format!("grep -qx \"{line}\" start.txt")),
            "the smoke run looks for {line}"
        );
        assert!(
            text.contains(&format!("\"{line}\"")),
            "the editor says {line}"
        );
    }
    for triple in targets::LYCEUM_TRIPLES {
        let t = target(triple);
        assert!(
            workflow.contains(&format!("target: {triple},")),
            "the matrix builds {triple}"
        );
        let file = lyceum::release_file_name(t, "$VERSION");
        let (lead, rest) = file.split_once("$VERSION-").expect("named by version");
        assert!(
            workflow.contains(&format!("\"{lead}$VERSION-\"*)")),
            "a label for {file}"
        );
        assert!(
            workflow.contains(&format!("{rest}) label=")),
            "a label for {file}"
        );
    }
    let dereth = read(".github/workflows/release-dereth.yml");
    for key in ["MOLTENVK_URL:", "MOLTENVK_SHA256:", "CARGO_ABOUT_VERSION:"] {
        let line = |text: &str| {
            text.lines()
                .find(|l| l.trim_start().starts_with(key))
                .map(str::trim)
                .map(str::to_owned)
        };
        assert_eq!(line(&workflow), line(&dereth), "{key}");
    }
    assert!(!workflow.contains("submodules"), "a submodule checked out");
    assert!(
        !workflow.contains("repository:"),
        "another repository checked out"
    );
}
