//! The experimental rendering effects are part of the desktop client as built, and never of the
//! browser client. Fixture: the client's and the browser client's own manifests, and cargo's
//! resolution of the browser client's dependencies for its target.

use std::path::Path;

/// The `[features]` table of the manifest at `path`, as `name = [members]` lines.
fn features(path: &Path) -> Vec<(String, String)> {
    let manifest = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{} does not read: {e}", path.display()));
    let mut out = Vec::new();
    let mut inside = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[features]";
            continue;
        }
        if !inside || line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some((name, members)) = line.split_once('=') {
            out.push((name.trim().to_owned(), members.trim().to_owned()));
        }
    }
    out
}

/// The browser client's manifest.
fn web_manifest() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../web/Cargo.toml")
}

/// Behaviour: hifi.build.the-desktop-client-carries-the-effects-and-the-browser-client-never-does
#[test]
fn the_desktop_build_carries_the_effects_and_the_browser_build_never_does() {
    let client = features(&Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"));
    let default = &client
        .iter()
        .find(|(n, _)| n == "default")
        .expect("the client has default features")
        .1;
    assert!(default.contains("\"hifi\""), "default = {default}");
    let hifi = &client
        .iter()
        .find(|(n, _)| n == "hifi")
        .expect("the client has a hifi feature")
        .1;
    for member in [
        "dereth-scene/hifi",
        "dereth-client-shell/hifi",
        "dereth-render/hifi",
    ] {
        assert!(hifi.contains(member), "hifi = {hifi} lacks {member}");
    }

    // The browser client's manifest names neither the feature nor the crate.
    let web = std::fs::read_to_string(web_manifest()).expect("the browser client's manifest");
    let code: String = web
        .lines()
        .map(|l| l.split('#').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !code.contains("hifi"),
        "the browser client asks for the effects"
    );

    // And nothing it depends on brings them in for its target.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let out = std::process::Command::new(cargo)
        .args([
            "tree",
            "-p",
            "dereth-web",
            "--target",
            "wasm32-unknown-unknown",
            "-e",
            "features",
            "--prefix",
            "none",
            "--offline",
            "--manifest-path",
        ])
        .arg(web_manifest())
        .output()
        .expect("cargo runs");
    assert!(
        out.status.success(),
        "cargo tree: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let tree = String::from_utf8_lossy(&out.stdout);
    assert!(tree.contains("dereth-web"), "{tree}");
    for line in tree.lines() {
        assert!(
            !line.contains("dereth-render-hifi") && !line.contains("feature \"hifi\""),
            "the browser client's tree carries the effects: {line}"
        );
    }
}
