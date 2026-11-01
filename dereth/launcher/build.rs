//! Put the launcher's artwork where the web front end can load it, then let Tauri build.
//!
//! The paintings (a folder for each game), the portals, the emblems the page shows, the two
//! typefaces and the app's own icon live in `dereth/launcher/assets`. The page's are copied into
//! `ui/assets` (which git ignores) before Tauri embeds `ui/`, and the icon's files into `icons/`. The chrome is not
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
    // Copied afresh, so nothing removed or renamed is left behind in the build: Tauri embeds
    // whatever is there.
    let _ = std::fs::remove_dir_all("ui/assets");
    copy_tree(
        &assets.join("background"),
        Path::new("ui/assets/background"),
    );
    copy_tree(&assets.join("fonts"), Path::new("ui/assets/fonts"));
    // The realm picker's portals: each realm's ring, and the vortex that turns under it.
    copy_tree(&assets.join("portal"), Path::new("ui/assets/portal"));
    // The emblems the page shows, small: Dereth's at the top, or the network's where the launcher
    // offers both games, and each game's server and world builder beside their pages' titles.
    copy_tree(&assets.join("emblem"), Path::new("ui/assets/emblem"));
    // The list of paintings, a folder for each game, so the page does not have to guess file names.
    let mut sets = std::collections::BTreeMap::new();
    for realm in ["dereth", "auberean"] {
        let mut names: Vec<String> = std::fs::read_dir(assets.join("background").join(realm))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| n.ends_with(".webp"))
            .map(|n| format!("{realm}/{n}"))
            .collect();
        names.sort();
        sets.insert(realm, names);
    }
    // The app's icon is the network's emblem, the one the page's crest shows: a 256-pixel picture,
    // the Windows icon with every size the shell asks for, and the macOS one.
    let icon = assets.join("icon");
    let _ = std::fs::create_dir_all("icons");
    let _ = std::fs::copy(icon.join("network.png"), "icons/icon.png");
    let _ = std::fs::copy(icon.join("network.ico"), "icons/icon.ico");
    let _ = std::fs::copy(icon.join("Network.icns"), "icons/Network.icns");
    let _ = std::fs::write(
        "ui/assets/backgrounds.json",
        serde_json::to_string(&sets).unwrap_or_default(),
    );
    tauri_build::build();
}
