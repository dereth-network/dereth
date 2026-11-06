//! Behaviour: none (tooling: the release files of Asheron's Call 2's desktop products, Athenaeum's
//! and those of a product of one program: their names and contents, the deny scan, the macOS
//! bundle, the Linux AppImage's folder, the header checks, the notice, the index, the versions and
//! tags, and the release workflows).

use super::archive::{self, Member};
use super::athenaeum;
use super::desktop::{self, Asset, Spec};
use super::guard::{self, Entry, Kind};
use super::headers;
use super::notice;
use super::targets::{self, Os, Target};
use super::tests::{elf, pe};
use super::version::{self, Product};
use crate::util::workspace_root;

const WINDOWS: &str = "x86_64-pc-windows-msvc";
const LINUX: &str = "x86_64-unknown-linux-gnu";
const MAC_ARM: &str = "aarch64-apple-darwin";
const V: &str = "0.1.0";

/// A product of one program, as the desktop packaging takes one: Auberean's client on its own,
/// which is how its package was shaped before it became the launcher's. Athenaeum's is the other
/// shape, several programs in one package.
const ONE_PROGRAM: Spec = Spec {
    product: Product::Auberean,
    name: "auberean",
    title: "Auberean",
    app_id: "network.dereth.auberean",
    programs: &[desktop::Program {
        package: "auberean-client",
        manifest: "auberean/client/Cargo.toml",
        binary: "auberean",
        windowed: true,
    }],
    documents: &[
        ("README.md", "auberean/README.md"),
        ("CHANGES.md", "auberean/CHANGES.md"),
    ],
    about: ("auberean/about.toml", "auberean/about.hbs"),
    fonts: &[],
    icon_png: "auberean/client/assets/auberean-256.png",
    icon_layered: "auberean/client/assets/Auberean.icon",
    mac_category: "public.app-category.role-playing-games",
    linux_categories: "Game;RolePlaying;",
    comment: "The Asheron's Call 2 client",
    source_url_env: "AUBEREAN_BUILD_SOURCE_URL",
    file_cap: 96 << 20,
    total_cap: 192 << 20,
};

const SPECS: [&Spec; 2] = [&ONE_PROGRAM, &athenaeum::SPEC];

fn target(spec: &Spec, triple: &str) -> Target {
    spec.find(triple).expect("a release target")
}

/// A scratch folder in the temporary directory, made anew and removed when dropped.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("xtask-desktop-{name}-{}", std::process::id()));
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

/// Each release file is named for the product, the version and the platform, and the folder's
/// classification reads every name back; anything else is refused, among them another version's
/// file, the other product's and Lyceum's.
#[test]
fn release_files_are_named_for_product_version_and_platform_and_read_back() {
    let a = &ONE_PROGRAM;
    let names = |spec: &Spec| -> Vec<String> {
        targets::DESKTOP_TRIPLES
            .iter()
            .map(|t| spec.release_file_name(target(spec, t), V))
            .collect()
    };
    assert_eq!(
        names(a),
        [
            "auberean-0.1.0-windows-x86_64.zip",
            "Auberean-0.1.0-linux-x86_64.AppImage",
            "Auberean-0.1.0-macos-aarch64.app.tar.gz",
        ]
    );
    assert_eq!(
        names(&athenaeum::SPEC),
        [
            "athenaeum-0.1.0-windows-x86_64.zip",
            "Athenaeum-0.1.0-linux-x86_64.AppImage",
            "Athenaeum-0.1.0-macos-aarch64.app.tar.gz",
        ]
    );
    for spec in SPECS {
        for (name, triple) in names(spec).iter().zip(targets::DESKTOP_TRIPLES) {
            assert_eq!(
                spec.classify(name, V),
                Ok(Asset::Release(target(spec, triple)))
            );
            assert_eq!(
                spec.classify(&format!("{name}.manifest"), V),
                Ok(Asset::ManifestPart)
            );
        }
        for index in ["SHA256SUMS", "MANIFEST.txt", "release.json"] {
            assert_eq!(spec.classify(index, V), Ok(Asset::Index));
        }
        for stray in [
            "auberean-0.2.0-windows-x86_64.zip",
            "lyceum-0.1.0-windows-x86_64.zip",
            "latest.json",
            "portal.dat",
        ] {
            assert!(spec.classify(stray, V).is_err(), "{stray}");
        }
    }
    assert!(ONE_PROGRAM
        .classify("athenaeum-0.1.0-windows-x86_64.zip", V)
        .is_err());
    let e = ONE_PROGRAM
        .find("aarch64-unknown-linux-gnu")
        .expect_err("not a desktop target");
    assert!(e.contains("an Auberean release target"), "{e}");
}

/// The Windows zip holds the programs, the notices and the documents in one folder; Athenaeum's
/// carries the server and the client beside its editor and its command.
#[test]
fn the_windows_zip_holds_the_programs_the_notices_readme_and_changes() {
    let a = &ONE_PROGRAM;
    let w = target(a, WINDOWS);
    assert_eq!(a.archive_root(w, V), "auberean-0.1.0");
    assert_eq!(
        a.contents(w),
        [
            "CHANGES.md",
            "LICENSE",
            "NOTICE.txt",
            "README.md",
            "THIRD-PARTY-LICENSES.html",
            "auberean.exe",
        ]
    );
    let b = &athenaeum::SPEC;
    assert_eq!(
        b.program_paths(w),
        [
            ("athenaeum-editor.exe".to_owned(), true),
            ("athenaeum.exe".to_owned(), false),
            ("yalain.exe".to_owned(), false),
            ("auberean.exe".to_owned(), true),
        ]
    );
    let contents = b.contents(w);
    for file in [
        "athenaeum-editor.exe",
        "athenaeum.exe",
        "yalain.exe",
        "auberean.exe",
    ] {
        assert!(contents.contains(&file.to_owned()), "{file}");
    }
    assert_eq!(contents.len(), 9, "{contents:?}");
    assert!(b.optional_contents(w).is_empty());
}

/// The Linux AppImage is packed from a folder holding `AppRun`, the desktop entry and its icon
/// named by the application id, the programs in `usr/bin`, the notices and the documents.
#[test]
fn the_appimage_folder_holds_apprun_the_desktop_entry_the_programs_and_the_notices() {
    let b = &athenaeum::SPEC;
    let l = target(b, LINUX);
    assert_eq!(b.archive_root(l, V), "Athenaeum.AppDir");
    let contents = b.contents(l);
    for file in [
        "AppRun",
        "network.dereth.athenaeum.desktop",
        "network.dereth.athenaeum.png",
        "usr/bin/athenaeum-editor",
        "usr/bin/athenaeum",
        "usr/bin/yalain",
        "usr/bin/auberean",
        "LICENSE",
        "NOTICE.txt",
        "THIRD-PARTY-LICENSES.html",
        "README.md",
        "CHANGES.md",
    ] {
        assert!(contents.contains(&file.to_owned()), "{file}: {contents:?}");
    }
    assert_eq!(contents.len(), 12);
    let entry = b.desktop_entry();
    assert!(entry.starts_with("[Desktop Entry]\nType=Application\nName=Athenaeum\n"));
    assert!(entry.contains("\nExec=athenaeum-editor\n"), "{entry}");
    assert!(
        entry.contains("\nIcon=network.dereth.athenaeum\n"),
        "{entry}"
    );
    assert!(entry.contains("\nCategories=Development;\n"), "{entry}");
    let client = ONE_PROGRAM.desktop_entry();
    assert!(client.contains("\nExec=auberean\n") && client.contains("Categories=Game;"));
}

/// `AppRun` starts the first program with every argument, or another of the product's programs
/// when the first argument names it; a product of one program has no choice to make.
#[test]
fn apprun_starts_the_first_program_or_the_one_its_first_argument_names() {
    let run = athenaeum::SPEC.app_run();
    assert!(run.starts_with("#!/bin/sh\n"), "{run}");
    assert!(
        run.contains("case \"${1:-}\" in\n  athenaeum|yalain|auberean)\n"),
        "{run}"
    );
    assert!(
        run.contains("exec \"$here/usr/bin/$program\" \"$@\""),
        "{run}"
    );
    assert!(
        run.trim_end()
            .ends_with("exec \"$here/usr/bin/athenaeum-editor\" \"$@\""),
        "{run}"
    );
    let one = ONE_PROGRAM.app_run();
    assert!(!one.contains("case"), "{one}");
    assert!(
        one.contains("exec \"$here/usr/bin/auberean\" \"$@\""),
        "{one}"
    );
    // A script for any POSIX shell, every line ended plainly.
    assert!(!run.contains('\r') && !run.contains("[["), "{run}");
}

/// The macOS bundle carries the programs in `Contents/MacOS`, its plist, its flat icon, its
/// signature, the notices and the documents in `Contents/Resources`, and no MoltenVK: both
/// products draw through Metal.
#[test]
fn the_macos_bundle_carries_the_programs_the_icon_the_signature_and_the_notices() {
    for spec in SPECS {
        {
            let m = target(spec, MAC_ARM);
            assert_eq!(spec.archive_root(m, V), format!("{}.app", spec.title));
            let contents = spec.contents(m);
            for file in [
                "Contents/Info.plist".to_owned(),
                "Contents/PkgInfo".to_owned(),
                format!("Contents/Resources/{}.icns", spec.name),
                "Contents/_CodeSignature/CodeResources".to_owned(),
                "Contents/Resources/NOTICE.txt".to_owned(),
                "Contents/Resources/README.md".to_owned(),
                format!("Contents/MacOS/{}", spec.programs[0].binary),
            ] {
                assert!(contents.contains(&file), "{file}");
            }
            assert!(
                !contents.iter().any(|f| f.contains("MoltenVK")),
                "{contents:?}"
            );
            assert_eq!(spec.optional_contents(m), ["Contents/Resources/Assets.car"]);
        }
    }
}

/// The plist names the bundle, its identifier, its executable (the first program), its version's
/// numeric core and its icons; the layered icon only when it was compiled.
#[test]
fn the_info_plist_names_what_the_bundle_holds_and_the_version() {
    let a = &ONE_PROGRAM;
    let plist = a.info_plist("0.2.0-rc.1", false);
    let after = |key: &str| {
        let at = plist.find(&format!("<key>{key}</key>")).expect(key);
        let rest = &plist[at..];
        let s = rest.find("<string>").expect("a string") + "<string>".len();
        let e = rest.find("</string>").expect("its end");
        rest[s..e].to_owned()
    };
    assert_eq!(after("CFBundleName"), "Auberean");
    assert_eq!(after("CFBundleIdentifier"), "network.dereth.auberean");
    assert_eq!(after("CFBundleExecutable"), "auberean");
    assert_eq!(after("CFBundleIconFile"), "auberean.icns");
    assert_eq!(after("CFBundleShortVersionString"), "0.2.0");
    assert_eq!(after("LSMinimumSystemVersion"), "11.0");
    assert!(!plist.contains("CFBundleIconName"));
    let composed = athenaeum::SPEC.info_plist(V, true);
    assert!(composed.contains("<key>CFBundleIconName</key>\n\t<string>Athenaeum</string>"));
    assert!(composed.contains("<string>athenaeum-editor</string>"));
    assert!(composed.contains("public.app-category.developer-tools"));
}

/// The bundle is signed inside out, every program but the first before the bundle; ad hoc without a
/// Developer ID, and with one under the hardened runtime and a timestamp.
#[test]
fn the_bundle_is_signed_inside_out_ad_hoc_or_with_a_developer_id() {
    let runs = athenaeum::SPEC.codesign_runs("A.app", None);
    let paths: Vec<&str> = runs.iter().map(|r| r.last().unwrap().as_str()).collect();
    assert_eq!(
        paths,
        [
            "A.app/Contents/MacOS/athenaeum",
            "A.app/Contents/MacOS/yalain",
            "A.app/Contents/MacOS/auberean",
            "A.app",
        ]
    );
    for run in &runs {
        assert_eq!(&run[..3], ["--force", "--sign", "-"]);
        assert!(!run.contains(&"runtime".to_owned()));
    }
    let signed = ONE_PROGRAM.codesign_runs("C.app", Some("Developer ID Application: X"));
    assert_eq!(signed.len(), 1, "the client alone: the bundle signs it");
    assert!(
        signed[0].contains(&"runtime".to_owned()) && signed[0].contains(&"--timestamp".to_owned())
    );
}

// ---------------------------------------------------------------- the icon

/// A square PNG of a size macOS takes becomes an `.icns` of one picture, its type naming the size;
/// anything else is refused. Both products' pictures convert.
#[test]
fn a_square_png_becomes_an_icns_of_one_picture() {
    let png = |side: u32| {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend_from_slice(&side.to_be_bytes());
        b.extend_from_slice(&side.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b
    };
    let icns = desktop::icns_from_png(&png(256)).expect("256 pixels");
    assert_eq!(&icns[..4], b"icns");
    assert_eq!(
        u32::from_be_bytes([icns[4], icns[5], icns[6], icns[7]]) as usize,
        icns.len()
    );
    assert_eq!(&icns[8..12], b"ic08");
    assert_eq!(&icns[16..24], &png(256)[..8]);
    assert_eq!(
        &desktop::icns_from_png(&png(512)).expect("512")[8..12],
        b"ic09"
    );
    assert!(desktop::icns_from_png(&png(200)).is_err());
    assert!(desktop::icns_from_png(b"GIF89a").is_err());
    let ws = workspace_root();
    for spec in SPECS {
        let bytes = std::fs::read(ws.join(spec.icon_png)).expect("the picture");
        desktop::icns_from_png(&bytes).expect(spec.icon_png);
        assert!(ws.join(spec.icon_layered).is_dir(), "{}", spec.icon_layered);
        assert_eq!(spec.icon_name(), spec.title);
    }
}

// ---------------------------------------------------------------- the deny scan

/// No name a package holds is one the deny rules refuse, and the rules refuse Asheron's Call 2's
/// data files and its client's name.
#[test]
fn no_name_in_a_package_is_one_the_deny_rules_refuse() {
    for spec in SPECS {
        for triple in targets::DESKTOP_TRIPLES {
            let t = target(spec, triple);
            for name in spec.contents(t).iter().chain(&spec.optional_contents(t)) {
                assert_eq!(guard::name_finding(name), None, "{name}");
            }
        }
    }
    for refused in [
        "portal.dat",
        "cell_1.dat",
        "highres.dat",
        "local_English.dat",
        "ac2client.exe",
        "yalain.sqlite",
    ] {
        assert!(guard::name_finding(refused).is_some(), "{refused}");
    }
}

/// Either game's data-file header is found whatever the file is named: the first game's at its
/// offset, the second's at its own.
#[test]
fn either_games_data_file_header_is_found() {
    let at = |offset: usize| {
        let mut b = vec![0u8; guard::HEAD_LEN];
        b[offset..offset + 4].copy_from_slice(&0x5442u32.to_le_bytes());
        b
    };
    assert!(guard::has_dat_magic(&at(0x140)));
    assert!(guard::has_dat_magic(&at(0x12C)));
    assert!(!guard::has_dat_magic(&at(0x100)));
    let e = Entry::file("README.md", &at(0x12C));
    let findings = guard::scan_tree(&[e], &["README.md".to_owned()], &[], 1 << 20, 1 << 20);
    assert!(
        findings.iter().any(|f| f.contains("data-file header")),
        "{findings:?}"
    );
}

/// Members for every file a target's package holds, the programs executable.
fn members(spec: &Spec, t: Target) -> Vec<Member> {
    let programs: Vec<String> = spec.program_paths(t).into_iter().map(|(p, _)| p).collect();
    spec.contents(t)
        .into_iter()
        .map(|name| Member {
            bytes: name.as_bytes().to_vec(),
            executable: programs.contains(&name),
            name,
        })
        .collect()
}

/// Write `t`'s release file, from `members`, into `dir` as the package writes it.
fn write_release(
    dir: &std::path::Path,
    spec: &Spec,
    t: Target,
    members: &[Member],
) -> std::path::PathBuf {
    let root = spec.archive_root(t, V);
    let bytes = if t.os == Os::Windows {
        archive::zip_bytes(&root, members, 1_700_000_000).expect("zip")
    } else {
        archive::tar_gz_bytes(&root, members, 1_700_000_000).expect("tar.gz")
    };
    let path = dir.join(spec.release_file_name(t, V));
    std::fs::write(&path, bytes).expect("write");
    path
}

/// Each archive, written as the package writes it, reads back and passes the scan of what was
/// written; one with a stray file, or without one of its files, is refused.
#[test]
fn a_written_archive_reads_back_and_passes_the_scan() {
    let scratch = Scratch::new("written");
    for spec in SPECS {
        for triple in [WINDOWS, MAC_ARM] {
            let t = target(spec, triple);
            let path = write_release(&scratch.0, spec, t, &members(spec, t));
            desktop::check_archive(spec, &path, t, V).expect("passes");

            let mut stray = members(spec, t);
            stray.push(Member {
                name: "notes.txt".to_owned(),
                bytes: b"x".to_vec(),
                executable: false,
            });
            let path = write_release(&scratch.0, spec, t, &stray);
            assert!(desktop::check_archive(spec, &path, t, V).is_err());

            let mut short = members(spec, t);
            short.retain(|m| !m.name.ends_with("NOTICE.txt"));
            let path = write_release(&scratch.0, spec, t, &short);
            let e = desktop::check_archive(spec, &path, t, V).expect_err("no notice");
            assert!(e.contains("NOTICE.txt"), "{e}");
        }
    }
}

/// The AppImage is one file that must be an executable, under the cap, named as no data file is,
/// and free of either game's data-file header.
#[test]
fn an_appimage_is_an_executable_under_the_cap_with_no_data_file_header() {
    let mut good = b"\x7fELF".to_vec();
    good.resize(4096, 0);
    let name = "Auberean-0.1.0-linux-x86_64.AppImage";
    assert!(desktop::check_appimage(name, &good, 1 << 20).is_empty());
    let findings = desktop::check_appimage(name, b"PK\x03\x04", 1 << 20);
    assert!(
        findings.iter().any(|f| f.contains("no ELF header")),
        "{findings:?}"
    );
    assert!(!desktop::check_appimage(name, &good, 1024).is_empty());
    let mut dat = good.clone();
    dat[0x12C..0x130].copy_from_slice(&0x5442u32.to_le_bytes());
    assert!(!desktop::check_appimage(name, &dat, 1 << 20).is_empty());
    assert!(!desktop::check_appimage("portal.dat", &good, 1 << 20).is_empty());
}

/// The scan of a staged bundle passes its own files and refuses a link, a stray folder and a file
/// over the cap.
#[test]
fn a_bundle_scan_passes_its_own_files_and_refuses_everything_else() {
    let spec = &ONE_PROGRAM;
    let m = target(spec, MAC_ARM);
    let mut entries: Vec<Entry> = vec![Entry {
        path: String::new(),
        kind: Kind::Dir,
        size: 0,
        head: Vec::new(),
    }];
    for dir in [
        "Contents",
        "Contents/MacOS",
        "Contents/Resources",
        "Contents/_CodeSignature",
    ] {
        entries.push(Entry {
            path: dir.to_owned(),
            kind: Kind::Dir,
            size: 0,
            head: Vec::new(),
        });
    }
    for f in spec.contents(m) {
        entries.push(Entry::file(&f, b"x"));
    }
    let scan = |e: &[Entry]| {
        guard::scan_tree(
            e,
            &spec.contents(m),
            &spec.optional_contents(m),
            spec.file_cap,
            spec.total_cap,
        )
    };
    assert!(scan(&entries).is_empty(), "{:?}", scan(&entries));
    let mut linked = entries.clone();
    linked.push(Entry {
        path: "Contents/MacOS/data".to_owned(),
        kind: Kind::Link("a symbolic link".to_owned()),
        size: 0,
        head: Vec::new(),
    });
    assert!(!scan(&linked).is_empty());
    let mut big = entries.clone();
    big.last_mut().expect("a file").size = spec.file_cap + 1;
    assert!(!scan(&big).is_empty());
}

/// The index lists every release file with its checksum, names the product and its tag, and takes
/// in every target's manifest lines; a file that belongs to no release in the folder is refused.
#[test]
fn the_index_lists_every_release_file_with_its_checksum() {
    let scratch = Scratch::new("index");
    let spec = &athenaeum::SPEC;
    let w = target(spec, WINDOWS);
    write_release(&scratch.0, spec, w, &members(spec, w));
    std::fs::write(
        scratch
            .0
            .join(format!("{}.manifest", spec.release_file_name(w, V))),
        format!("{WINDOWS}\tline\n"),
    )
    .expect("manifest part");
    // An AppImage, as the Linux runner writes it.
    let mut appimage = b"\x7fELF".to_vec();
    appimage.resize(64, 0);
    let l = target(spec, LINUX);
    std::fs::write(scratch.0.join(spec.release_file_name(l, V)), &appimage).expect("AppImage");
    let facts = super::BuildFacts {
        commit: "a".repeat(40),
        branch: "athenaeum-v0.1.0".to_owned(),
        number: "1".to_owned(),
        epoch: 1_700_000_000,
        source_url: "https://github.com/someone/fork".to_owned(),
    };
    desktop::write_index(spec, &scratch.0, V, &facts).expect("writes the index");
    let read = |name: &str| std::fs::read_to_string(scratch.0.join(name)).expect(name);
    let release: serde_json::Value = serde_json::from_str(&read("release.json")).expect("json");
    assert_eq!(release["product"], "athenaeum");
    assert_eq!(release["tag"], "athenaeum-v0.1.0");
    assert_eq!(release["source_url"], "https://github.com/someone/fork");
    assert_eq!(release["assets"].as_array().map(Vec::len), Some(2));
    let manifest = read("MANIFEST.txt");
    assert!(manifest.starts_with(&format!("# athenaeum {V}, commit {}", "a".repeat(40))));
    assert!(
        manifest.contains(&format!("{WINDOWS}\tline\n")),
        "{manifest}"
    );
    let sums = read("SHA256SUMS");
    for name in [
        "athenaeum-0.1.0-windows-x86_64.zip",
        "Athenaeum-0.1.0-linux-x86_64.AppImage",
        "MANIFEST.txt",
        "release.json",
    ] {
        let bytes = std::fs::read(scratch.0.join(name)).expect(name);
        assert!(
            sums.contains(&format!("{}  {name}\n", super::sha256_hex(&bytes))),
            "{name}: {sums}"
        );
    }
    std::fs::write(scratch.0.join("auberean-0.1.0-windows-x86_64.zip"), "x").expect("stray");
    assert!(desktop::write_index(spec, &scratch.0, V, &facts).is_err());
}

// ---------------------------------------------------------------- the build

/// Each desktop target builds on its own operating system only.
#[test]
fn the_desktop_products_build_natively() {
    let host = |os| targets::Host {
        os: Some(os),
        zigbuild: true,
        zig: true,
        installed: None,
        glibc: None,
    };
    let a = &ONE_PROGRAM;
    assert!(targets::plan_desktop(target(a, WINDOWS), &host(Os::Windows)).is_ok());
    let e = targets::plan_desktop(target(a, LINUX), &host(Os::Windows)).expect_err("cross");
    assert!(e.contains("Linux host only"), "{e}");
    assert!(targets::plan_desktop(target(a, MAC_ARM), &host(Os::Mac)).is_ok());
}

/// A release build of any of Asheron's Call 2's programs names the commit and the target in every
/// product's variables, which each program's `--version` reads; on Windows it links the C runtime
/// in, and on macOS it claims the release's floor.
#[test]
fn a_release_build_names_its_commit_and_target_for_every_program() {
    let facts = super::BuildFacts {
        commit: "b".repeat(40),
        branch: "auberean-v0.1.0".to_owned(),
        number: "1".to_owned(),
        epoch: 1_700_000_000,
        source_url: super::DEFAULT_SOURCE_URL.to_owned(),
    };
    let ws = workspace_root();
    let value = |env: &[(String, String)], key: &str| {
        env.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    };
    let windows = desktop::build_env(&ws, &facts, target(&ONE_PROGRAM, WINDOWS));
    let flags = value(&windows, "CARGO_ENCODED_RUSTFLAGS").expect("flags");
    assert!(
        flags.contains("+crt-static") && flags.contains("/Brepro"),
        "{flags}"
    );
    let mac = desktop::build_env(&ws, &facts, target(&ONE_PROGRAM, MAC_ARM));
    assert_eq!(
        value(&mac, "MACOSX_DEPLOYMENT_TARGET").as_deref(),
        Some("11.0")
    );
    for (commit, triple) in desktop::BUILD_VARS {
        assert_eq!(
            value(&mac, commit).as_deref(),
            Some("b".repeat(40).as_str())
        );
        assert_eq!(value(&mac, triple).as_deref(), Some(MAC_ARM));
    }
    for (file, var) in [
        ("auberean/client/src/main.rs", "AUBEREAN"),
        ("yalain/server/src/main.rs", "YALAIN"),
        ("athenaeum/editor/src/main.rs", "ATHENAEUM"),
        ("athenaeum/cli/src/main.rs", "ATHENAEUM"),
    ] {
        let text = std::fs::read_to_string(ws.join(file)).expect(file);
        for which in ["COMMIT", "TARGET"] {
            let needle = format!("option_env!(\"{var}_BUILD_{which}\")");
            assert!(text.contains(&needle), "{file} reads {var}_BUILD_{which}");
        }
    }
}

/// Every windowed program is a windowed program with its icon on Windows, and every other a
/// console program; the client and the editor are built so in a release.
#[test]
fn the_windowed_programs_are_windowed_in_a_release_and_the_others_console_programs() {
    let a = &athenaeum::SPEC;
    let w = target(a, WINDOWS);
    let floor = targets::DESKTOP_GLIBC_FLOOR;
    let windowed = pe(0x8664, 2, &["KERNEL32.dll", "USER32.dll"], &[], true);
    let console = pe(0x8664, 3, &["KERNEL32.dll"], &[], true);
    for (name, is_windowed) in a.program_paths(w) {
        let (good, bad) = if is_windowed {
            (&windowed, &console)
        } else {
            (&console, &windowed)
        };
        let check = |b: &[u8]| {
            if is_windowed {
                headers::check_app_binary(b, w, floor)
            } else {
                headers::check_console_app(b, w, floor)
            }
        };
        assert!(check(good).is_ok(), "{name}");
        assert!(check(bad).is_err(), "{name}");
    }
    let ws = workspace_root();
    for file in [
        "auberean/client/src/main.rs",
        "athenaeum/editor/src/main.rs",
    ] {
        let text = std::fs::read_to_string(ws.join(file)).expect(file);
        assert!(
            text.contains("#![cfg_attr(not(debug_assertions), windows_subsystem = \"windows\")]"),
            "{file}"
        );
    }
    let l = target(a, LINUX);
    let interp = headers::linux_interp(targets::Arch::X86_64);
    let newer = elf(62, interp, &["libc.so.6"], &["GLIBC_2.39"]);
    assert!(headers::check_console_app(&newer, l, floor).is_err());
}

// ---------------------------------------------------------------- the notice

/// Each product's notice names it, its source and its release, says there is no game data, carries
/// the typefaces it carries, and lists the project's crates apart from the third parties'.
#[test]
fn each_products_notice_names_its_source_and_carries_what_it_needs() {
    let crates = [
        notice::Crate {
            name: "wgpu".to_owned(),
            version: "30.0.0".to_owned(),
            licence: "MIT OR Apache-2.0".to_owned(),
        },
        notice::Crate {
            name: "auberean-dat".to_owned(),
            version: "0.0.0".to_owned(),
            licence: "MIT".to_owned(),
        },
        notice::Crate {
            name: "yalain".to_owned(),
            version: "0.1.0".to_owned(),
            licence: "MIT".to_owned(),
        },
    ];
    let fonts = [(
        "Liberation",
        "The editor's panels",
        "Liberation OFL".to_owned(),
    )];
    for (product, f) in [
        (Product::Auberean, &[][..]),
        (Product::Yalain, &[][..]),
        (Product::Athenaeum, &fonts[..]),
    ] {
        let text = notice::game_notice(&notice::GameFacts {
            product,
            version: V,
            target: WINDOWS,
            commit: "abc123",
            source_url: "https://github.com/dereth-network/dereth",
            crates: &crates,
            mit_license: "MIT License",
            fonts: f,
        });
        let name = product.display_name();
        assert!(
            text.starts_with(&format!("{name} 0.1.0 (x86_64-pc-windows-msvc)\n")),
            "{text}"
        );
        assert!(text.contains("https://github.com/dereth-network/dereth/tree/abc123"));
        assert!(text.contains(&format!("releases/tag/{}", product.tag_for(V))));
        assert!(text.contains("NO GAME DATA") && text.contains("no Asheron's Call 2 files"));
        assert!(text.contains("MIT License"));
        assert_eq!(
            text.contains("THE LIBERATION TYPEFACE"),
            !f.is_empty(),
            "{name}"
        );
        let own = text.find("This project's own crates:").expect("own");
        let third = text.find("Third-party crates").expect("third");
        let dat = text.find("auberean-dat 0.0.0").expect("the data crate");
        let server = text.find("  yalain 0.1.0").expect("the server's crate");
        let wgpu = text.find("wgpu 30.0.0").expect("wgpu");
        assert!(own < dat && own < server && dat < third && server < third && third < wgpu);
    }
}

// ---------------------------------------------------------------- the tree

/// Every file the packages read from the tree is there: the license, cargo-about's configuration
/// and a template with the section markers, each program's manifest, the documents, the typefaces'
/// licenses and the icons.
#[test]
fn every_file_the_desktop_packages_read_from_the_tree_is_there() {
    let ws = workspace_root();
    for spec in SPECS {
        assert!(ws.join("LICENSE").is_file());
        assert!(ws.join(spec.about.0).is_file() && ws.join(spec.about.1).is_file());
        for p in spec.programs {
            assert!(ws.join(p.manifest).is_file(), "{}", p.manifest);
        }
        for (_, path) in spec.documents {
            assert!(ws.join(path).is_file(), "{path}");
        }
        for (_, _, path) in spec.fonts {
            assert!(ws.join(path).is_file(), "{path}");
        }
        let hbs = std::fs::read_to_string(ws.join(spec.about.1)).expect("template");
        assert!(hbs.contains(notice::SECTION_START) && hbs.contains(notice::SECTION_END));
    }
}

// ---------------------------------------------------------------- versions and tags

/// Each of Asheron's Call 2's products carries one version of its own across its crates, which its
/// tag must name exactly, and the release command's edit moves each manifest's version and keeps
/// its comment.
#[test]
fn each_product_carries_one_version_of_its_own_which_its_tag_names() {
    let ws = workspace_root();
    for (product, count) in [
        (Product::Auberean, 3),
        (Product::Yalain, 9),
        (Product::Athenaeum, 3),
    ] {
        let v = product.version(&ws).expect("its crates agree");
        version::parse_release_version(&v).expect("a release version");
        let manifests = product.manifests(&ws).expect("manifests");
        assert_eq!(manifests.len(), count, "{product:?}");
        let tag = product.tag_for(&v);
        assert_eq!(tag, format!("{}-v{v}", product.command_name()));
        assert!(version::check_product_tag(product, &tag, &v).is_ok());
        let e = version::check_product_tag(product, &format!("dereth-v{v}"), &v)
            .expect_err("another product's tag");
        assert!(
            e.contains(&format!("{} release tag", product.display_name())),
            "{e}"
        );
        assert_eq!(
            Product::from_command_name(product.command_name()),
            Some(product)
        );
        assert!(product.is_auberean_game());
        for manifest in manifests {
            let text = std::fs::read_to_string(&manifest).expect("manifest");
            let edited = version::set_package_version(&text, "0.2.0").expect("a literal version");
            assert_eq!(version::package_version(&edited).as_deref(), Some("0.2.0"));
            let own = format!("\"0.2.0\" # {}'s own version", product.display_name());
            assert!(edited.contains(&own), "{}", manifest.display());
        }
    }
    for p in [Product::Empyrean, Product::Dereth, Product::Lyceum] {
        assert!(!p.is_auberean_game());
    }
}

// ---------------------------------------------------------------- the workflows

/// Each release workflow starts on its product's tags, checks the tag names the version, builds
/// every target with the package command on its own system (Linux with the pinned appimagetool and
/// runtime), smoke-runs `--version`, gathers the files, labels every one, never marks the release
/// "Latest", and checks out nothing but this repository.
#[test]
fn each_release_workflow_builds_smoke_runs_and_labels_every_target() {
    let ws = workspace_root();
    // Auberean's package is the launcher's, built as Dereth's is ([`super::auberean`]).
    for spec in [&athenaeum::SPEC] {
        let name = spec.name;
        let file = format!(".github/workflows/release-{name}.yml");
        let workflow = std::fs::read_to_string(ws.join(&file)).expect(&file);
        for needed in [
            format!("tags: ['{name}-v*']"),
            format!("cargo xtask version {name} --tag \"$GITHUB_REF_NAME\""),
            format!("cargo xtask release-notes {name} --version \"$VERSION\""),
            format!("cargo xtask package {name} --target ${{{{ matrix.target }}}} --out dist"),
            format!("cargo xtask package {name} --gather dist"),
            "--draft --verify-tag --latest=false".to_owned(),
            "persist-credentials: false".to_owned(),
            "APPIMAGETOOL_SHA256: ".to_owned(),
            "APPIMAGE_RUNTIME_SHA256: ".to_owned(),
            "sha256sum -c -".to_owned(),
            "\"target: $TARGET\"".to_owned(),
            format!("{}_BUILD_SOURCE_URL", name.to_ascii_uppercase()),
            "dist/*.AppImage".to_owned(),
        ] {
            assert!(workflow.contains(&needed), "{file}: {needed}");
        }
        for triple in targets::DESKTOP_TRIPLES {
            assert!(
                workflow.contains(&format!("target: {triple},")),
                "{file}: {triple}"
            );
            let t = target(spec, triple);
            let release = spec.release_file_name(t, V);
            let suffix = release
                .strip_prefix(&format!("{}-0.1.0-", spec.name))
                .or_else(|| release.strip_prefix(&format!("{}-0.1.0-", spec.title)))
                .expect("the product's name and version");
            assert!(
                workflow.contains(&format!("{suffix}) label=")),
                "{file}: {suffix}"
            );
        }
        assert!(!workflow.contains("MOLTENVK"), "{file}");
    }
}
