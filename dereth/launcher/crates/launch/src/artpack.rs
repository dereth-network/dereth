//! Art packs the player keeps in the Library, as they keep data sets: a packed file or a pack's
//! folder, kept where it is, one of them the default. A world's page chooses which the Dereth client
//! draws, by `--art-pack`, or none. Nothing a server sees follows from it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The manifest format the Dereth client reads (`dereth_assets::art_pack::FORMAT`).
pub const FORMAT: i64 = 2;

/// The folder form's manifest (`dereth_assets::art_pack::MANIFEST_FILE`).
const MANIFEST: &str = "manifest.toml";

/// How far into a packed file the container's header lies, as bytes to read.
const HEADER: usize = 1024;

/// An art pack in the Library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtPack {
    pub id: String,
    /// What the pack is called: its manifest's name, or its file's.
    pub name: String,
    pub path: PathBuf,
    /// Whether a world nothing was chosen for draws it.
    #[serde(default)]
    pub is_default: bool,
}

/// The name of the art pack at `path`, a packed file or a pack's folder. A packed file's format
/// is the client's to read, as it opens it.
///
/// # Errors
/// Why it is not an art pack this client can draw, as a sentence.
pub fn read_art_pack(path: &Path) -> Result<String, String> {
    let stem = path.file_stem().map_or_else(
        || "Art pack".to_owned(),
        |s| s.to_string_lossy().into_owned(),
    );
    if path.is_dir() {
        let manifest = std::fs::read_to_string(path.join(MANIFEST))
            .map_err(|_| format!("{} holds no {MANIFEST}.", path.display()))?;
        let value = |key: &str| {
            manifest.lines().find_map(|line| {
                let (k, v) = line.split_once('=')?;
                (k.trim() == key).then(|| v.trim().trim_matches('"').to_owned())
            })
        };
        return match value("format").and_then(|f| f.parse::<i64>().ok()) {
            Some(FORMAT) => Ok(value("name").filter(|n| !n.is_empty()).unwrap_or(stem)),
            Some(f) => Err(format!(
                "That art pack is format {f}, and this client reads format {FORMAT}."
            )),
            None => Err(format!("{MANIFEST} names no format.")),
        };
    }
    use std::io::Read;
    let mut head = Vec::with_capacity(HEADER);
    std::fs::File::open(path)
        .and_then(|f| f.take(HEADER as u64).read_to_end(&mut head))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    // Its container's header carries the data subset `Pack`.
    if head.windows(4).any(|w| w == b"Pack") {
        Ok(stem)
    } else {
        Err(format!("{} is not a packed art pack.", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_is_a_packed_file_or_a_folder_of_the_format_the_client_reads() {
        let root = std::env::temp_dir().join(format!("dereth-artpack-{}", std::process::id()));
        let folder = root.join("folder-pack");
        std::fs::create_dir_all(&folder).unwrap();
        let manifest = |f: i64| format!("[pack]\nname = \"Woods\"\nformat = {f}\n");
        std::fs::write(folder.join(MANIFEST), manifest(2)).unwrap();
        assert_eq!(read_art_pack(&folder).as_deref(), Ok("Woods"));
        std::fs::write(folder.join(MANIFEST), manifest(1)).unwrap();
        assert!(read_art_pack(&folder).unwrap_err().contains("format 1"));
        let packed = root.join("woods.dat");
        std::fs::write(&packed, b"....Pack....").unwrap();
        assert_eq!(read_art_pack(&packed).as_deref(), Ok("woods"));
        std::fs::write(&packed, b"....cell....").unwrap();
        assert!(read_art_pack(&packed).is_err());
        assert!(read_art_pack(&root.join("missing.dat")).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
