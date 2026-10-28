//! Put the launcher's artwork where the web front end can load it, then let Tauri build.
//!
//! The paintings, the emblems the page shows and the two typefaces live in
//! `dereth/launcher/assets`, and the app's icon once, with the Dereth client. They are copied
//! into `ui/assets` (which git ignores) before Tauri embeds `ui/`. The chrome is not
//! among them: this front end draws its frames, buttons and beads in CSS and SVG rather than
//! cutting them out of the sprite atlas.

use std::path::Path;

fn copy_tree(from: &Path, to: &Path) {
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    let _ = std::fs::create_dir_all(to);
    for e in entries.flatten() {
        let p = e.path();
        let dest = to.join(e.file_name());
        if p.is_dir() {
            copy_tree(&p, &dest);
        } else {
            let fresh = std::fs::metadata(&dest).map(|d| d.len()).ok()
                == std::fs::metadata(&p).map(|m| m.len()).ok();
            if !fresh {
                let _ = std::fs::copy(&p, &dest);
            }
        }
    }
}

fn main() {
    println!(
        "cargo:rustc-env=DERETH_COMPONENT_TARGET={}",
        std::env::var("TARGET").expect("Cargo target")
    );
    let assets = Path::new("assets");
    println!("cargo:rerun-if-changed=assets");
    // The paintings come in sets, one folder each; only the set shown is copied, so the others do
    // not grow the download. Copied afresh, so nothing removed or renamed is left behind in the
    // build: Tauri embeds whatever is there.
    let set = "dereth";
    let _ = std::fs::remove_dir_all("ui/assets");
    copy_tree(
        &assets.join("background").join(set),
        &Path::new("ui/assets/background").join(set),
    );
    copy_tree(&assets.join("fonts"), Path::new("ui/assets/fonts"));
    // The emblems the page shows, small: Dereth's at the top, and Empyrean's and Lyceum's beside
    // the titles of Host a world and Build a world.
    copy_tree(&assets.join("emblem"), Path::new("ui/assets/emblem"));
    // The list of paintings, so the page does not have to guess file names.
    let mut names: Vec<String> = std::fs::read_dir(assets.join("background").join(set))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".webp"))
        .map(|n| format!("{set}/{n}"))
        .collect();
    names.sort();
    // The app's icon is the Dereth client's, kept once beside the client.
    let client = Path::new("../client/assets");
    println!("cargo:rerun-if-changed=../client/assets");
    let _ = std::fs::create_dir_all("icons");
    let _ = std::fs::copy(client.join("dereth-256.png"), "icons/icon.png");
    // Windows embeds the client's own multi-size icon in the executable.
    let _ = std::fs::copy(client.join("dereth.ico"), "icons/icon.ico");
    let _ = std::fs::write(
        "ui/assets/backgrounds.json",
        serde_json::to_string(&names).unwrap_or_default(),
    );
    compile_icon(&client.join("Dereth.icon"));
    tauri_build::build();
}

/// The client's layered `Dereth.icon`, compiled the way the client's own bundle compiles it: Xcode's
/// `actool` writes `Assets.car` (what macOS 26 draws, with its glass and shading) and a flat
/// `Dereth.icns` beside it for every earlier macOS. The bundle carries both (`tauri.macos.conf.json`,
/// which only a macOS build reads: no other system has the files), and
/// `Info.plist` names the one inside `Assets.car`.
///
/// Without Xcode there is no `actool`; `cargo run` needs neither file, so this only warns, and
/// packaging is what fails, naming the missing file.
fn compile_icon(source: &Path) {
    let out = Path::new("icons");
    if !cfg!(target_os = "macos") || !source.is_dir() {
        return;
    }
    let partial = out.join("partial-info.plist");
    let status = std::process::Command::new("xcrun")
        .args([
            "actool",
            "--output-format",
            "human-readable-text",
            "--notices",
            "--warnings",
        ])
        .args(["--app-icon", "Dereth", "--output-partial-info-plist"])
        .arg(&partial)
        .args([
            "--target-device",
            "mac",
            "--platform",
            "macosx",
            "--minimum-deployment-target",
            "11.0",
            "--compile",
        ])
        .arg(out)
        .arg(source)
        .stdout(std::process::Stdio::null())
        .status();
    let _ = std::fs::remove_file(&partial);
    if !matches!(status, Ok(s) if s.success()) {
        println!(
            "cargo:warning=actool could not compile {}; an .app bundle will lack the layered icon",
            source.display()
        );
    }
}
