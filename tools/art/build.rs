//! The land converter's version (`newdereth-world` says it first): a hash of the converter's own
//! code and tables, the files that decide what land it makes from the same installs. It changes
//! when they change; a change in a crate they build on may change the land without changing it.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The converter's own files, from this crate's folder: every file under each folder, the
/// creature tables (the files directly in `ac2-creatures`, not the worlds beneath it) and their
/// drawn icons.
const OWN: [(&str, bool); 6] = [
    ("src/ac2_world", true),
    ("ac2-world", true),
    ("../../core/ac2-convert/src", true),
    ("src/ac2_creature", true),
    ("ac2-creatures", false),
    (ICONS, false),
];

/// The creature families' drawn icons, `<family>.png` each.
const ICONS: &str = "ac2-creatures/icons";

fn files(dir: &Path, deep: bool, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for e in read.flatten() {
        let p = e.path();
        if p.is_dir() {
            if deep {
                files(&p, deep, out);
            }
        } else {
            out.push(p);
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo's folder"));
    let mut h = Sha256::new();
    for (own, deep) in OWN {
        let dir = root.join(own);
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut list = Vec::new();
        files(&dir, deep, &mut list);
        list.sort();
        for f in list {
            let name = f
                .strip_prefix(&dir)
                .unwrap_or(&f)
                .to_string_lossy()
                .replace('\\', "/");
            h.update(own.as_bytes());
            h.update(b"/");
            h.update(name.as_bytes());
            h.update([0]);
            h.update(std::fs::read(&f).unwrap_or_default());
            h.update([0]);
        }
    }
    drawn_icons(&root);
    let hex: String = h.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("cargo:rustc-env=DERETH_ART_CONVERTER={hex}");
}

/// `icons.rs` in cargo's output folder: each drawn icon there is, by family, its PNG's bytes, for
/// the converter to write ([`ICONS`]).
fn drawn_icons(root: &Path) {
    let mut list = Vec::new();
    files(&root.join(ICONS), false, &mut list);
    list.retain(|p| p.extension().is_some_and(|e| e == "png"));
    list.sort();
    let mut out = String::from("&[\n");
    for p in &list {
        let family = p
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        out.push_str(&format!(
            "    ({family:?}, include_bytes!({:?}).as_slice()),\n",
            p.to_string_lossy()
        ));
    }
    out.push(']');
    let dir = PathBuf::from(std::env::var("OUT_DIR").expect("cargo's output folder"));
    std::fs::write(dir.join("icons.rs"), out).expect("icons.rs written");
}
