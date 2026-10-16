//! Packing an art pack's folder into one file, which the client opens without making a picture:
//! [`pack`] writes it, and [`check`] says whether a file is what its folder packs into now.
//!
//! The packing is the client's own ([`dereth_scene::art_pack::packed`]): the folder is opened as
//! the client opens it, and every file read and every picture made on the way is kept, so the
//! packed pictures are exactly those the client makes from the folder.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use dereth_assets::art_pack::MANIFEST_FILE;
use dereth_scene::art_pack::packed::{self, Contents, Kind, PackedFile};

/// What [`pack`] wrote.
#[derive(Debug)]
pub struct Report {
    /// The packed file.
    pub out: PathBuf,
    /// The pack's name.
    pub name: String,
    /// Its records, the contents among them.
    pub records: usize,
    /// Its length.
    pub bytes: u64,
    /// What the pack could not read or make, left out of it as the client leaves it out of the
    /// folder, a line each.
    pub problems: Vec<String>,
}

/// The file the folder `dir` packs into unless another is named: beside it, its name and `.dat`.
#[must_use]
pub fn default_out(dir: &Path) -> PathBuf {
    let name = dir
        .file_name()
        .map_or_else(|| "pack".into(), |n| n.to_string_lossy().into_owned());
    dir.with_file_name(format!("{name}.dat"))
}

/// The folder `dir` packed, in memory.
fn build(dir: &Path) -> Result<packed::Packed, String> {
    let at = dir.join(MANIFEST_FILE);
    let manifest = std::fs::read_to_string(&at).map_err(|e| format!("{}: {e}", at.display()))?;
    packed::pack(&manifest, &mut |path| {
        std::fs::read(dir.join(path)).map_err(|e| format!("{path}: {e}"))
    })
}

/// Pack the folder `dir` into the file `out`, in place of any there. It is written beside `out`
/// first and then moved over it, so a pack that fails leaves the file that was there.
///
/// # Errors
/// The pack will not open or is too large for one file, or the file will not write.
pub fn pack(dir: &Path, out: &Path) -> Result<Report, String> {
    let made = build(dir)?;
    let partial = out.with_file_name(format!(
        "{}.partial",
        out.file_name()
            .map_or_else(|| "pack".into(), |n| n.to_string_lossy().into_owned())
    ));
    let written = std::fs::write(&partial, &made.bytes)
        .and_then(|()| std::fs::rename(&partial, out))
        .map_err(|e| format!("{}: {e}", out.display()));
    if written.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    written?;
    Ok(Report {
        out: out.to_path_buf(),
        name: made.pack.name,
        records: made.records,
        bytes: made.bytes.len() as u64,
        problems: made.pack.problems,
    })
}

/// Whether a packed file is what its folder packs into now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    /// Byte for byte.
    Current,
    /// It differs: a line for each record that changed, is new or is gone, by its key (none
    /// when only the container around them differs).
    Stale(Vec<String>),
    /// There is no file.
    Missing,
}

/// Whether the file `out` is what the folder `dir` packs into now: packed again in memory and
/// compared byte for byte.
///
/// # Errors
/// The pack will not open or is too large for one file, or the file will not read.
pub fn check(dir: &Path, out: &Path) -> Result<Freshness, String> {
    let made = build(dir)?;
    let have = match std::fs::read(out) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Freshness::Missing),
        Err(e) => return Err(format!("{}: {e}", out.display())),
    };
    if have == made.bytes {
        return Ok(Freshness::Current);
    }
    Ok(Freshness::Stale(differences(have, made.bytes)))
}

/// The records of the packed file `have` that are not those of `made`, a line each.
fn differences(have: Vec<u8>, made: Vec<u8>) -> Vec<String> {
    let contents = |bytes: Vec<u8>| -> Result<Contents, String> {
        PackedFile::from_storage("the file", Box::new(bytes)).map(|f| f.contents().clone())
    };
    let (have, made) = match (contents(have), contents(made)) {
        (Ok(have), Ok(made)) => (have, made),
        (Err(e), _) | (_, Err(e)) => return vec![e],
    };
    let by_key = |c: &Contents| -> BTreeMap<(Kind, String), [u8; 32]> {
        c.records
            .iter()
            .map(|r| ((r.kind, r.key.clone()), r.sha256))
            .collect()
    };
    let (old, new) = (by_key(&have), by_key(&made));
    let mut lines = Vec::new();
    if have.name != made.name {
        lines.push(format!("renamed: {:?} to {:?}", have.name, made.name));
    }
    for (key, sha) in &new {
        match old.get(key) {
            None => lines.push(format!("new: {}", key.1)),
            Some(was) if was != sha => lines.push(format!("changed: {}", key.1)),
            Some(_) => {}
        }
    }
    lines.extend(
        old.keys()
            .filter(|key| !new.contains_key(*key))
            .map(|key| format!("gone: {}", key.1)),
    );
    lines
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: packing an art pack)
    use super::*;

    fn png_of(side: u32, value: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut bytes, side, side);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut w = enc.write_header().expect("header");
            let texels = [value, 100, 50, 255].repeat((side * side) as usize);
            w.write_image_data(&texels).expect("data");
        }
        bytes
    }

    const MANIFEST: &str = r#"
[pack]
name = "Small"
format = 1
[[material]]
texture = "0x0500287C"
base_color = "m/wall.png"
author = "a"
tool = "t"
license = "CC0-1.0"
[[material]]
texture = "0x05002752"
base_color = "m/floor.png"
author = "a"
tool = "t"
license = "CC0-1.0"
"#;

    #[test]
    fn a_packed_file_is_current_until_its_folder_changes() {
        let root = std::env::temp_dir().join(format!("dereth-art-pack-{}", std::process::id()));
        let dir = root.join("small");
        std::fs::create_dir_all(dir.join("m")).expect("the folder");
        std::fs::write(dir.join(MANIFEST_FILE), MANIFEST).expect("the manifest");
        std::fs::write(dir.join("m/wall.png"), png_of(8, 120)).expect("a picture");
        std::fs::write(dir.join("m/floor.png"), png_of(8, 40)).expect("a picture");
        let out = default_out(&dir);
        let run = || -> Vec<Freshness> {
            let mut seen = vec![check(&dir, &out).expect("checks")];
            let report = pack(&dir, &out).expect("packs");
            assert_eq!((report.name.as_str(), report.records), ("Small", 4));
            assert!(report.problems.is_empty(), "{:?}", report.problems);
            seen.push(check(&dir, &out).expect("checks"));
            std::fs::write(dir.join("m/wall.png"), png_of(8, 121)).expect("a picture");
            seen.push(check(&dir, &out).expect("checks"));
            seen
        };
        let seen = run();
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(out.file_name().and_then(|n| n.to_str()), Some("small.dat"));
        assert_eq!(seen[0], Freshness::Missing);
        assert_eq!(seen[1], Freshness::Current);
        // One picture changed: the file is stale, and says which.
        assert_eq!(
            seen[2],
            Freshness::Stale(vec!["changed: material:m/wall.png::::1".to_owned()])
        );
    }
}
