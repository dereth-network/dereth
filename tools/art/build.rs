//! The land converter's version (`newdereth-world` says it first): a hash of the converter's own
//! code and tables, the files that decide what land it makes from the same installs. It changes
//! when they change; a change in a crate they build on may change the land without changing it.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The converter's own files, from this crate's folder.
const OWN: [&str; 3] = ["src/ac2_world", "ac2-world", "../../core/ac2-convert/src"];

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for e in read.flatten() {
        let p = e.path();
        if p.is_dir() {
            files(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn main() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo's folder"));
    let mut h = Sha256::new();
    for own in OWN {
        let dir = root.join(own);
        println!("cargo:rerun-if-changed={}", dir.display());
        let mut list = Vec::new();
        files(&dir, &mut list);
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
    let hex: String = h.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    println!("cargo:rustc-env=DERETH_ART_CONVERTER={hex}");
}
