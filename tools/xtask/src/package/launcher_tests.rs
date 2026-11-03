//! Behaviour: none (tooling: the launcher's own release files, and the version and tag it reads).

use super::auberean::{self, Asset};
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
        auberean::codesign_runs_of("Dereth.app", None, None),
        [["--force", "--sign", "-", "Dereth.app"]]
    );
    assert_eq!(
        auberean::programs_of(target("aarch64-apple-darwin"), None),
        ["Contents/MacOS/dereth"]
    );
}

/// A launcher release is signed under its own version, with no client's after it, so a launcher
/// reads it as a newer launcher and nothing else.
#[test]
fn a_launcher_release_is_signed_under_its_own_version() {
    assert_eq!((launcher::FLAVOR.signed_as)("0.4.12", V), V);
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
