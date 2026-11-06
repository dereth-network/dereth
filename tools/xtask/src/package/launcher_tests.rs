//! Behaviour: none (tooling: the launcher's own release files, and the version and tag it reads).

use super::archive::{self, Member};
use super::auberean::{self, Asset};
use super::dereth;
use super::launcher;
use super::targets::{self, Target};
use super::version::Product;
use crate::util::workspace_root;

const V: &str = "0.5.0";

fn target(triple: &str) -> Target {
    targets::find_dereth(triple).expect("a target")
}

/// The launcher's files are named for its version and each system, and read back as what they are.
#[test]
fn the_launchers_release_files_are_named_for_its_version_and_read_back() {
    let names: Vec<String> = targets::DESKTOP_TRIPLES
        .iter()
        .map(|t| launcher::release_file_name(target(t), V))
        .collect();
    assert_eq!(
        names,
        [
            "launcher-0.5.0-windows-x86_64.zip",
            "Launcher-0.5.0-linux-x86_64.AppImage",
            "Launcher-0.5.0-macos-aarch64.app.tar.gz",
        ]
    );
    for (name, triple) in names.iter().zip(targets::DESKTOP_TRIPLES) {
        assert!(matches!(
            launcher::classify(name, V),
            Ok(Asset::Release(t)) if t.triple == *triple
        ));
        assert!(matches!(
            launcher::classify(&format!("{name}.manifest"), V),
            Ok(Asset::ManifestPart)
        ));
    }
    assert!(matches!(
        launcher::classify("latest.json", V),
        Ok(Asset::Index)
    ));
    assert!(launcher::classify("Auberean-0.1.0-linux-x86_64.AppImage", V).is_err());
}

/// The launcher's package holds the launcher and its friends library, and no client of either
/// game, no defaults file and no MoltenVK; its bundle is signed with no client to sign first.
#[test]
fn the_launchers_package_holds_no_client() {
    for triple in targets::DESKTOP_TRIPLES {
        let t = target(triple);
        let (required, optional) = launcher::contents(t);
        for name in required.iter().chain(&optional) {
            assert!(
                !name.contains("dereth-client") && !name.contains(auberean::CLIENT)
                    || name.contains("auberean_friends"),
                "{name}"
            );
            assert!(!name.contains(auberean::DEFAULTS_FILE), "{name}");
            assert!(!name.contains("MoltenVK"), "{name}");
        }
    }
    let (mac, optional) = launcher::contents(target("aarch64-apple-darwin"));
    assert!(mac.contains(&"Contents/MacOS/dereth".to_owned()));
    assert!(mac.contains(&"Contents/Resources/auberean_friends.dll".to_owned()));
    assert!(optional.contains(&"Contents/Resources/Network.icns".to_owned()));
    let (windows, _) = launcher::contents(target("x86_64-pc-windows-msvc"));
    assert!(windows.contains(&"dereth.exe".to_owned()));
    assert_eq!(
        auberean::codesign_runs_of("Dereth.app", None, &[]),
        [["--force", "--sign", "-", "Dereth.app"]]
    );
    assert_eq!(
        auberean::programs_of(target("aarch64-apple-darwin"), None),
        ["Contents/MacOS/dereth"]
    );
}

/// A launcher release is downloaded from its own tag.
#[test]
fn a_launcher_release_downloads_from_its_own_tag() {
    assert_eq!(
        auberean::download_url_of(Product::Launcher, "https://github.com/a/b/", V, "x.zip"),
        "https://github.com/a/b/releases/download/launcher-v0.5.0/x.zip"
    );
}

/// The launcher is a product by name, with its own tag and its version in its own manifest; the
/// release commands that cut products together don't take it yet.
#[test]
fn the_launcher_is_a_product_of_its_own() {
    let p = Product::from_command_name("launcher").expect("named");
    assert_eq!(p.tag_for(V), "launcher-v0.5.0");
    assert!(!p.is_auberean_game());
    assert!(!Product::ALL.contains(&p));
    assert!(p.version(&workspace_root()).is_ok());
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-launcher-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temp directory");
    dir
}

fn member(name: &str, bytes: &[u8], executable: bool) -> Member {
    Member {
        name: name.to_owned(),
        bytes: bytes.to_vec(),
        executable,
    }
}

/// A folder as `--fetch` leaves it: the release file for `target` and the checksums over it.
fn fetched(dir: &std::path::Path, target: Target, version: &str, bytes: &[u8]) {
    let file = launcher::release_file_name(target, version);
    std::fs::write(dir.join(&file), bytes).unwrap();
    std::fs::write(
        dir.join("SHA256SUMS"),
        format!("{}  {file}\n", super::sha256_hex(bytes)),
    )
    .unwrap();
}

/// A macOS launcher release unpacks to a bundle whose program can still run, ready for a client.
#[test]
fn a_macos_launcher_release_is_taken_as_a_bundle_that_keeps_its_program_runnable() {
    let temp = temp("take-1");
    let t = target("aarch64-apple-darwin");
    let tar = archive::tar_gz_bytes(
        "Dereth.app",
        &[
            member("Contents/MacOS/dereth", b"launcher", true),
            member("Contents/Resources/auberean_friends.dll", b"library", false),
        ],
        1_700_000_000,
    )
    .unwrap();
    fetched(temp.as_path(), t, V, &tar);

    let taken = launcher::take(temp.as_path(), t, V, &temp.as_path().join("stage")).unwrap();
    assert!(taken.bundle.ends_with("Dereth.app"));
    let program = taken.bundle.join("Contents/MacOS/dereth");
    assert_eq!(std::fs::read(&program).unwrap(), b"launcher");
    assert_eq!(std::fs::read(&taken.library).unwrap(), b"library");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode();
        assert_ne!(mode(&program) & 0o111, 0);
        assert_eq!(mode(&taken.library) & 0o111, 0);
    }
}

/// On Windows the program and the friends library are files in the zip's one folder.
#[test]
fn a_windows_launcher_release_is_taken_with_its_friends_library_beside_it() {
    let temp = temp("take-2");
    let t = target("x86_64-pc-windows-msvc");
    let zip = archive::zip_bytes(
        &format!("launcher-{V}"),
        &[
            member("dereth.exe", b"launcher", true),
            member("auberean_friends.dll", b"library", false),
        ],
        1_700_000_000,
    )
    .unwrap();
    fetched(temp.as_path(), t, V, &zip);
    let taken = launcher::take(temp.as_path(), t, V, &temp.as_path().join("stage")).unwrap();
    assert_eq!(std::fs::read(taken.bundle).unwrap(), b"launcher");
    assert_eq!(std::fs::read(taken.library).unwrap(), b"library");
}

/// The tree names one version, and only that version's file is ever used: an older one in the same
/// folder isn't taken in its place, and the failure says how to get the right one.
#[test]
fn a_package_never_takes_another_version_of_the_launcher_in_its_place() {
    let temp = temp("take-3");
    let t = target("aarch64-apple-darwin");
    fetched(temp.as_path(), t, "0.4.9", b"an older launcher");
    let Err(why) = launcher::take(temp.as_path(), t, V, &temp.as_path().join("stage")) else {
        panic!("took another version");
    };
    assert!(why.contains(V), "{why}");
    assert!(why.contains("--fetch"), "{why}");
    assert!(why.contains("--launcher"), "{why}");
}

/// A file that isn't what its release's checksums say is refused before it's unpacked.
#[test]
fn a_launcher_file_that_does_not_match_its_checksums_is_refused() {
    let temp = temp("take-4");
    let t = target("aarch64-apple-darwin");
    fetched(temp.as_path(), t, V, b"what the release holds");
    let file = launcher::release_file_name(t, V);
    std::fs::write(temp.as_path().join(&file), b"something else").unwrap();
    let stage = temp.as_path().join("stage");
    let Err(why) = launcher::take(temp.as_path(), t, V, &stage) else {
        panic!("took a file that doesn't match");
    };
    assert!(why.contains("SHA256SUMS"), "{why}");
    assert!(!stage.exists());
}

/// `--fetch` takes its folder first, and waiting and the repository are asked for by name.
#[test]
fn fetching_needs_a_folder_and_waits_only_when_asked() {
    let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let o = launcher::parse_fetch(&args(&["dir"])).unwrap();
    assert_eq!(o.dir, std::path::PathBuf::from("dir"));
    assert!(!o.wait);
    assert_eq!(o.repo, None);
    let o = launcher::parse_fetch(&args(&["dir", "--wait", "--repo", "a/b"])).unwrap();
    assert!(o.wait);
    assert_eq!(o.repo.as_deref(), Some("a/b"));
    assert!(launcher::parse_fetch(&args(&["--wait"])).is_err());
    assert!(launcher::parse_fetch(&args(&["dir", "--other"])).is_err());
}

/// Dereth's release no longer moves the launcher's version: the two are apart in the tree.
#[test]
fn the_launcher_carries_its_own_version_apart_from_dereth() {
    assert!(!super::version::DERETH_MANIFESTS.contains(&"launcher/Cargo.toml"));
    assert!(!super::version::DERETH_LOCKS
        .iter()
        .any(|(_, lock)| *lock == "launcher/Cargo.lock"));
    assert!(Product::Dereth.holds_launcher());
    assert!(Product::Auberean.holds_launcher());
    assert!(!Product::Launcher.holds_launcher());
    assert!(!Product::Empyrean.holds_launcher());
}

/// Each game's workflow waits for the named release in a job of its own, with the only write
/// access a build has, gives its files to the package jobs, and has nothing to build a launcher
/// with: there's no way for it to fall back to one.
#[test]
fn a_games_release_workflow_takes_the_named_launcher_and_cannot_build_one() {
    let ws = workspace_root();
    for (file, product) in [
        (".github/workflows/release-dereth.yml", "dereth"),
        (".github/workflows/release-auberean.yml", "auberean"),
    ] {
        let w = std::fs::read_to_string(ws.join(file)).expect(file);
        for needed in [
            "needs: [version, launcher]".to_owned(),
            "launcher=$(cargo xtask version launcher)".to_owned(),
            "cargo xtask package launcher --fetch launcher-release --wait".to_owned(),
            "if [ \"$GITHUB_REF_TYPE\" = tag ]; then\n            cargo xtask package launcher --fetch"
                .to_owned(),
            format!("name: launcher-release-{product}"),
            format!(
                "cargo xtask package {product} --target ${{{{ matrix.target }}}} --out dist --launcher launcher-release"
            ),
            "grep -qx \"dereth $LAUNCHER\" version-launcher.txt".to_owned(),
            "APPIMAGETOOL_SHA256: ".to_owned(),
            "test -s \"$library\"".to_owned(),
        ] {
            assert!(w.contains(&needed), "{file}: {needed}");
        }
        for gone in [
            "tauri",
            "friends-library",
            "cargo xtask package launcher --target",
        ] {
            assert!(!w.contains(gone), "{file}: {gone}");
        }
    }
}

/// The launcher carries each game client's icon, from that client's own layered source, and every
/// package with the launcher in it allows them.
#[test]
fn the_launcher_carries_each_clients_icon_in_every_package() {
    let icons = dereth::client_icons();
    for wanted in [
        "Contents/Resources/clients/Dereth/Assets.car",
        "Contents/Resources/clients/Dereth/Dereth.icns",
        "Contents/Resources/clients/Auberean/Assets.car",
        "Contents/Resources/clients/Auberean/Auberean.icns",
    ] {
        assert!(icons.iter().any(|i| i == wanted), "{wanted}: {icons:?}");
    }
    let mac = target("aarch64-apple-darwin");
    for (package, (_, optional)) in [
        ("launcher", launcher::contents(mac)),
        ("dereth", dereth::contents(mac)),
        ("auberean", auberean::contents(mac)),
    ] {
        for icon in &icons {
            assert!(optional.contains(icon), "{package}: {icon}");
        }
    }
    let ws = workspace_root();
    for source in [
        "dereth/client/assets/Dereth.icon",
        "auberean/client/assets/Auberean.icon",
    ] {
        assert!(ws.join(source).is_dir(), "{source}");
    }
}
