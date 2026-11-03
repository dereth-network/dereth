//! Behaviour: none (tooling: Auberean's release files, which are the launcher's with the Auberean
//! client beside it: their names and contents, the defaults file and the update version the
//! launcher reads, the launcher built as Dereth's package builds it, the macOS signing, the index,
//! and the release workflow).

use base64::Engine as _;

use super::archive::{self, Member};
use super::auberean::{self, Asset};
use super::dereth;
use super::guard;
use super::sign::{self, Signer};
use super::targets::{self, Target};
use super::version::Product;
use crate::util::workspace_root;

const WINDOWS: &str = "x86_64-pc-windows-msvc";
const LINUX: &str = "x86_64-unknown-linux-gnu";
const MAC_ARM: &str = "aarch64-apple-darwin";
const V: &str = "0.1.0";

fn target(triple: &str) -> Target {
    targets::find_dereth(triple).expect("a release target")
}

/// Each release file is named for Auberean, its version and the platform, as the launcher's own
/// fetch of the Auberean client names it, and the folder's classification reads every name back;
/// Dereth's files and anything else are refused.
#[test]
fn release_files_are_named_as_the_launcher_fetches_them_and_read_back() {
    use dereth_launch::components::Product as Component;
    let names: Vec<String> = targets::DESKTOP_TRIPLES
        .iter()
        .map(|t| auberean::release_file_name(target(t), V))
        .collect();
    assert_eq!(
        names,
        [
            "auberean-0.1.0-windows-x86_64.zip",
            "Auberean-0.1.0-linux-x86_64.AppImage",
            "Auberean-0.1.0-macos-aarch64.app.tar.gz",
        ]
    );
    for (name, triple) in names.iter().zip(targets::DESKTOP_TRIPLES) {
        let t = target(triple);
        assert_eq!(
            Component::Auberean.archive_name(triple, V).as_deref(),
            Some(name.as_str())
        );
        // A launcher from Auberean's package fetches Dereth's client from Dereth's own file.
        assert_eq!(
            Component::Dereth.archive_name(triple, V),
            Some(dereth::release_file_name(t, V))
        );
        assert_eq!(auberean::classify(name, V), Ok(Asset::Release(t)));
        assert_eq!(
            auberean::classify(&format!("{name}.manifest"), V),
            Ok(Asset::ManifestPart)
        );
        assert_eq!(
            auberean::classify(&dereth::piece_name(t), V),
            Ok(Asset::Piece(t))
        );
    }
    for index in ["SHA256SUMS", "MANIFEST.txt", "release.json", "latest.json"] {
        assert_eq!(auberean::classify(index, V), Ok(Asset::Index));
    }
    for stray in [
        "dereth-0.1.0-windows-x86_64.zip",
        "auberean-0.2.0-windows-x86_64.zip",
        "dereth-plugin-0.1.0-windows-x86_64.zip",
        "portal.dat",
    ] {
        assert!(auberean::classify(stray, V).is_err(), "{stray}");
    }
}

/// The package holds the launcher, the Auberean client, the defaults file, the retail client's
/// friends library and the notices, and never Dereth's client or tool: those are what the launcher's build took in, and are taken out.
#[test]
fn the_package_holds_the_launcher_the_client_and_the_defaults_and_not_dereth_s_client() {
    let (windows, none) = auberean::contents(target(WINDOWS));
    assert_eq!(
        windows,
        [
            "LICENSE",
            "NOTICE.txt",
            "THIRD-PARTY-LICENSES.html",
            "auberean.exe",
            "auberean_friends.dll",
            "dereth.exe",
            "launcher-defaults.json",
        ]
    );
    assert!(none.is_empty());
    let (mac, optional) = auberean::contents(target(MAC_ARM));
    for needed in [
        "Contents/MacOS/dereth",
        "Contents/MacOS/auberean",
        "Contents/Resources/launcher-defaults.json",
        "Contents/Resources/NOTICE.txt",
        "Contents/Resources/auberean_friends.dll",
    ] {
        assert!(mac.iter().any(|m| m == needed), "{needed}");
    }
    for name in mac.iter().chain(&optional).chain(&windows) {
        assert!(
            !name.contains("dereth-client")
                && !name.contains(dereth::TOOL)
                && !name.contains("MoltenVK"),
            "{name}"
        );
        assert!(guard::name_finding(name).is_none(), "{name}");
    }
    assert!(
        auberean::contents(target(LINUX)).0.is_empty(),
        "an AppImage"
    );
    assert_eq!(
        auberean::taken_out(target(MAC_ARM)),
        [
            "Contents/MacOS/dereth-client",
            "Contents/MacOS/dereth-plugin",
            "Contents/Frameworks",
            "Contents/_CodeSignature",
        ]
    );
    assert_eq!(
        auberean::taken_out(target(LINUX)),
        ["usr/bin/dereth-client", "usr/bin/dereth-plugin"]
    );
    assert_eq!(
        auberean::programs(target(MAC_ARM)),
        ["Contents/MacOS/dereth", "Contents/MacOS/auberean"]
    );

    // A written zip of exactly that reads back clean; one carrying Dereth's client is refused.
    let t = target(WINDOWS);
    let dir = std::env::temp_dir().join(format!("xtask-auberean-zip-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    let member = |n: &str| Member {
        name: n.to_owned(),
        bytes: n.as_bytes().to_vec(),
        executable: n.ends_with(".exe"),
    };
    let root = auberean::archive_root(t, V).expect("a zip");
    assert_eq!(root, "auberean-0.1.0");
    let path = dir.join(auberean::release_file_name(t, V));
    let members: Vec<Member> = windows.iter().map(|n| member(n)).collect();
    std::fs::write(
        &path,
        archive::zip_bytes(&root, &members, 1_700_000_000).expect("zip"),
    )
    .expect("written");
    auberean::check_archive(&auberean::AUBEREAN, &path, t, V).expect("clean");
    let mut with_dereth = members.clone();
    with_dereth.push(member("dereth-client.exe"));
    std::fs::write(
        &path,
        archive::zip_bytes(&root, &with_dereth, 1_700_000_000).expect("zip"),
    )
    .expect("written");
    let refused =
        auberean::check_archive(&auberean::AUBEREAN, &path, t, V).expect_err("Dereth's client");
    assert!(refused.contains("dereth-client.exe"), "{refused}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The defaults file the package writes is what the launcher reads as Auberean's: a first run opens
/// in Auberean's realm, and the updates come from Auberean's releases. Dereth's package writes
/// none, which the launcher reads as Dereth's.
#[test]
fn the_defaults_file_opens_auberean_s_realm_and_names_auberean_s_updates() {
    use dereth_launch::defaults::{Defaults, FILE};
    use dereth_launch::state::Realm;
    use dereth_launch::world::Game;
    assert_eq!(auberean::DEFAULTS_FILE, FILE);
    let read = Defaults::parse(auberean::DEFAULTS);
    assert_eq!(read.realm, Realm::Auberean);
    assert_eq!(read.updates, Game::Auberean);
    assert_eq!(
        read.opening_realm(true, Realm::Dereth),
        Realm::Auberean,
        "a first run"
    );
    assert_eq!(read.updates, dereth_launch::world::Game::Auberean);
    let (dereth, _) = dereth::contents(target(WINDOWS));
    assert!(
        !dereth.iter().any(|n| n == FILE),
        "Dereth's package has none"
    );
}

/// A game's `latest.json` names the launcher it holds, by that launcher's version alone: what an
/// install from before the launcher's own releases compares against its own. A signature made for
/// it verifies for it and for no other version.
#[test]
fn a_games_update_version_is_the_launchers_and_the_signature_is_for_it() {
    let written = "0.5.0";
    let kp = minisign::KeyPair::generate_unencrypted_keypair().expect("a key pair");
    let b64 = |t: String| base64::engine::general_purpose::STANDARD.encode(t);
    let secret = b64(kp.sk.to_box(None).expect("secret box").to_string());
    let public = b64(kp.pk.to_box().expect("public box").to_string());
    let signer =
        Signer::from_key_text(&sign::key_text(&secret).expect("key text"), "").expect("signer");
    let data = b"the release zip";
    let sig = signer
        .sign(
            data,
            "auberean-0.1.0-windows-x86_64.zip",
            written,
            1_700_000_000,
        )
        .expect("signs");
    sign::verify(data, &sig, &public, written).expect("verifies for its version");
    assert!(sign::verify(data, &sig, &public, "0.4.9").is_err());
    assert!(sign::verify(data, &sig, &public, V).is_err());
    assert_eq!(
        auberean::download_url("https://github.com/dereth-network/dereth", V, "x.zip"),
        "https://github.com/dereth-network/dereth/releases/download/auberean-v0.1.0/x.zip"
    );
}

/// The package's launcher is the one its tree names, at the launcher's own version, and only the
/// launcher's own package builds one, by the function that builds it for that.
#[test]
fn the_launcher_is_built_at_its_own_version_as_dereth_s_package_builds_it() {
    let ws = workspace_root();
    let (client, launcher) = auberean::versions(&ws).expect("versions");
    assert_eq!(
        launcher,
        Product::Launcher.version(&ws).expect("the launcher's")
    );
    assert_eq!(client, Product::Auberean.version(&ws).expect("Auberean's"));
    let source = std::fs::read_to_string(ws.join("tools/xtask/src/package/auberean.rs"))
        .expect("the module");
    assert!(source.contains("dereth::build_launcher_as_dereth("));
    assert!(source.contains("dereth::launcher_facts(ws, allow_dirty)"));
    // In the same staging folder as Dereth's package: the launcher's configuration names the
    // files beside it relative to its folder, and that configuration is built into the program.
    assert!(
        source.contains("&dereth::launcher_stage(&target_dir().join(\"package\"), self.target)")
    );
    let base = std::path::Path::new("target/package");
    for triple in targets::DESKTOP_TRIPLES {
        assert_eq!(
            dereth::launcher_stage(base, target(triple)),
            base.join("stage").join(format!("dereth-{triple}"))
        );
    }
}

/// The macOS bundle is signed again inside out: the client, then the bundle; ad hoc without a
/// Developer ID, and with one under the hardened runtime and a timestamp.
#[test]
fn the_bundle_is_signed_again_client_first_ad_hoc_or_with_a_developer_id() {
    let ad_hoc = auberean::codesign_runs("D.app", None);
    assert_eq!(
        ad_hoc,
        [
            ["--force", "--sign", "-", "D.app/Contents/MacOS/auberean"],
            ["--force", "--sign", "-", "D.app"],
        ]
    );
    assert_eq!(auberean::codesign_runs("D.app", Some("-")), ad_hoc);
    let signed = auberean::codesign_runs("D.app", Some("Developer ID Application: X"));
    assert_eq!(signed.len(), 2);
    for run in &signed {
        assert_eq!(
            run[..6],
            [
                "--force",
                "--sign",
                "Developer ID Application: X",
                "--options",
                "runtime",
                "--timestamp"
            ]
        );
    }
    assert_eq!(signed[1].last().map(String::as_str), Some("D.app"));
}

/// The index over a folder of unsigned release files sums them, names Auberean and the launcher's
/// version in `release.json`, and writes no `latest.json`; a file of Dereth's release is refused.
#[test]
fn the_index_sums_the_release_files_and_names_the_launcher_s_version() {
    let dir = std::env::temp_dir().join(format!("xtask-auberean-index-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a folder");
    let t = target(WINDOWS);
    let (required, _) = auberean::contents(t);
    let members: Vec<Member> = required
        .iter()
        .map(|n| Member {
            name: n.clone(),
            bytes: n.as_bytes().to_vec(),
            executable: n.ends_with(".exe"),
        })
        .collect();
    let root = auberean::archive_root(t, V).expect("a zip");
    std::fs::write(
        dir.join(auberean::release_file_name(t, V)),
        archive::zip_bytes(&root, &members, 1_700_000_000).expect("zip"),
    )
    .expect("written");
    let facts = super::BuildFacts {
        commit: "a".repeat(40),
        branch: "auberean-v0.1.0".to_owned(),
        number: "1".to_owned(),
        epoch: 1_700_000_000,
        source_url: "https://github.com/dereth-network/dereth".to_owned(),
    };
    let ws = workspace_root();
    auberean::write_index(&auberean::AUBEREAN, &ws, &dir, V, "0.4.9", &facts).expect("indexed");
    let sums = std::fs::read_to_string(dir.join("SHA256SUMS")).expect("sums");
    assert!(
        sums.contains("  auberean-0.1.0-windows-x86_64.zip\n"),
        "{sums}"
    );
    assert!(!dir.join("latest.json").exists(), "unsigned");
    let release: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("release.json")).expect("facts"))
            .expect("json");
    assert_eq!(release["product"], "auberean");
    assert_eq!(release["launcher_version"], "0.4.9");
    assert_eq!(release["tag"], "auberean-v0.1.0");
    std::fs::write(dir.join("dereth-0.1.0-windows-x86_64.zip"), b"x").expect("written");
    assert!(auberean::write_index(&auberean::AUBEREAN, &ws, &dir, V, "0.4.9", &facts).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The release workflow starts on Auberean's tags, builds every target with the package command on
/// its own system with what the launcher's build needs (the Tauri CLI, MoltenVK, the signing key,
/// the same source as Dereth's release), smoke-runs both programs and the defaults file, gathers
/// the files, labels every one, never marks the release "Latest", and checks out nothing but this
/// repository.
#[test]
fn the_release_workflow_builds_the_launcher_package_and_labels_every_target() {
    let ws = workspace_root();
    let file = ".github/workflows/release-auberean.yml";
    let workflow = std::fs::read_to_string(ws.join(file)).expect(file);
    for needed in [
        "tags: ['auberean-v*']",
        "cargo xtask version auberean --tag \"$GITHUB_REF_NAME\"",
        "launcher=$(cargo xtask version launcher)",
        "cargo xtask release-notes auberean --version \"$VERSION\"",
        "cargo xtask package auberean --target ${{ matrix.target }} --out dist",
        "cargo xtask package auberean --gather dist",
        "APPIMAGETOOL_SHA256: ",
        "APPIMAGE_RUNTIME_SHA256: ",
        "TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}",
        "DERETH_BUILD_SOURCE_URL: https://github.com/${{ github.repository }}",
        "AUBEREAN_BUILD_SOURCE_URL: ",
        "grep -qx \"dereth $LAUNCHER\" version-launcher.txt",
        "grep -qx \"auberean $VERSION\" version-client.txt",
        "\"target: $TARGET\"",
        "grep -q '\"realm\": \"auberean\"' \"$defaults\"",
        "test ! -e \"$dereth_client\"",
        "test -s dist/latest.json",
        "--draft --verify-tag --latest=false",
        "persist-credentials: false",
        "dist/*.AppImage",
    ] {
        assert!(workflow.contains(needed), "{file}: {needed}");
    }
    for triple in targets::DESKTOP_TRIPLES {
        assert!(
            workflow.contains(&format!("target: {triple},")),
            "{file}: {triple}"
        );
        let release = auberean::release_file_name(target(triple), V);
        let suffix = release
            .strip_prefix("auberean-0.1.0-")
            .or_else(|| release.strip_prefix("Auberean-0.1.0-"))
            .expect("the product's name and version");
        assert!(
            workflow.contains(&format!("{suffix}) label=")),
            "{file}: {suffix}"
        );
    }
    // The same tools as Dereth's release.
    let dereth = std::fs::read_to_string(ws.join(".github/workflows/release-dereth.yml"))
        .expect("Dereth's workflow");
    for pin in ["CARGO_ABOUT_VERSION", "APPIMAGETOOL_SHA256"] {
        let line = |w: &str| {
            w.lines()
                .find(|l| l.trim_start().starts_with(&format!("{pin}:")))
                .map(str::trim)
                .map(str::to_owned)
        };
        assert_eq!(line(&workflow), line(&dereth), "{pin}");
    }
    assert!(
        !workflow.contains("repository:"),
        "only this repository is checked out"
    );
}
