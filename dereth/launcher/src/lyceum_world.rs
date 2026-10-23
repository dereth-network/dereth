//! A Lyceum world as the launcher reads it without Lyceum: its key and era, and its sources, the
//! world file and every file in the folders its `packs` and `imports` name. When any of those
//! files last changed tells whether the world was saved since; a digest of their contents tells
//! whether a publication was made from the sources as they are.

use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

/// When each of a world's sources last changed, and its size, in path order.
pub type Stamp = Vec<(PathBuf, Option<SystemTime>, u64)>;

/// What `world.toml` says of the world, read as plain TOML.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub key: Option<String>,
    /// `eor` or `infiltration`; a world that names none is built on the end of retail's files.
    pub era: String,
    /// The folders its packs and imports are in.
    pub folders: Vec<PathBuf>,
}

/// The world file `world`, read.
#[must_use]
pub fn header(world: &Path) -> Header {
    let table: toml::Table = std::fs::read_to_string(world)
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default();
    let dir = world.parent().unwrap_or(Path::new("."));
    let list = |key: &str| {
        table
            .get(key)
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(toml::Value::as_str)
            .map(|p| dir.join(p))
            .collect::<Vec<_>>()
    };
    let mut folders = list("packs");
    folders.extend(list("imports"));
    Header {
        key: table
            .get("key")
            .and_then(toml::Value::as_str)
            .map(str::to_owned),
        era: table
            .get("era")
            .and_then(toml::Value::as_str)
            .unwrap_or("eor")
            .to_owned(),
        folders,
    }
}

/// Every file under `dir`, links passed over.
fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            files_under(&e.path(), out);
        } else if kind.is_file() {
            out.push(e.path());
        }
    }
}

/// The world file and every file of its packs and imports, in path order.
#[must_use]
pub fn source_files(world: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for folder in header(world).folders {
        files_under(&folder, &mut files);
    }
    files.sort();
    files.dedup();
    files.insert(0, world.to_path_buf());
    files
}

/// When each of the world's sources last changed, and its size.
#[must_use]
pub fn stamp(world: &Path) -> Stamp {
    source_files(world)
        .into_iter()
        .map(|f| {
            let meta = std::fs::metadata(&f).ok();
            let modified = meta.as_ref().and_then(|m| m.modified().ok());
            let len = meta.map_or(0, |m| m.len());
            (f, modified, len)
        })
        .collect()
}

/// When the world was last saved, in seconds since 1970: when its newest source last changed.
#[must_use]
pub fn saved(stamp: &Stamp) -> Option<u64> {
    stamp
        .iter()
        .filter_map(|(_, modified, _)| *modified)
        .max()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// A digest of the world's sources: each file's contents under its path from the world's folder,
/// so the same sources give the same digest wherever the world is.
///
/// # Errors
/// A source that cannot be read.
pub fn digest(world: &Path) -> Result<String, String> {
    use std::io::Read;
    let root = world.parent().unwrap_or(Path::new("."));
    let mut all = Sha256::new();
    for file in source_files(world) {
        let name = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        let mut input =
            std::fs::File::open(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let mut one = Sha256::new();
        let mut buf = vec![0; 65536];
        loop {
            let n = input.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            one.update(&buf[..n]);
        }
        all.update(name.as_bytes());
        all.update([0]);
        all.update(one.finalize());
    }
    Ok(format!("{:x}", all.finalize()))
}

/// The digests worked out so far, each with the stamp of the sources it was worked out from, and
/// the worlds being worked out now.
#[derive(Default)]
struct Digests {
    known: HashMap<PathBuf, (Stamp, String)>,
    working: HashSet<PathBuf>,
}

static DIGESTS: Mutex<Option<Digests>> = Mutex::new(None);

/// The digest of the world's sources as they are now, when it is known: worked out once for each
/// change of their stamps, in the background, so asking often costs only the stamp. `None` while
/// it is being worked out, or when the sources cannot be read.
#[must_use]
pub fn current_digest(world: &Path) -> Option<String> {
    let now = stamp(world);
    let mut digests = DIGESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let digests = digests.get_or_insert_with(Digests::default);
    if let Some((then, digest)) = digests.known.get(world) {
        if *then == now {
            return Some(digest.clone());
        }
    }
    if digests.working.insert(world.to_path_buf()) {
        let world = world.to_path_buf();
        std::thread::spawn(move || {
            let result = digest(&world);
            let mut digests = DIGESTS
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let digests = digests.get_or_insert_with(Digests::default);
            digests.working.remove(&world);
            if let Ok(digest) = result {
                digests.known.insert(world, (now, digest));
            }
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world with one pack, `sample`, holding a landblock's edits.
    fn world() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let world = temp.path().join("world.toml");
        std::fs::write(
            &world,
            "key = 'hills'\nera = 'infiltration'\npacks = ['sample']\nimports = ['made']\n",
        )
        .unwrap();
        std::fs::create_dir_all(temp.path().join("sample/edits/landblocks")).unwrap();
        std::fs::write(temp.path().join("sample/pack.toml"), "name = 'sample'\n").unwrap();
        std::fs::write(
            temp.path().join("sample/edits/landblocks/A9B4.toml"),
            "[[vertex]]\nat = [0, 1]\nheight = 50\n",
        )
        .unwrap();
        (temp, world)
    }

    /// A world's sources are its world file and its packs' and imports' files: its lockfile, the
    /// editor's own files beside it and what it builds are not; the world file names its key and
    /// era, the end of retail when it names none.
    #[test]
    fn a_worlds_sources_are_its_world_file_and_its_packs_files() {
        let (temp, world) = world();
        std::fs::write(temp.path().join("world.lock"), "[wcid]\n").unwrap();
        std::fs::write(temp.path().join(".lyceum-place.toml"), "x = 1\n").unwrap();
        std::fs::create_dir_all(temp.path().join("build/overlay")).unwrap();
        std::fs::write(temp.path().join("build/overlay/a.dat"), b"built").unwrap();
        std::fs::create_dir_all(temp.path().join("made")).unwrap();
        std::fs::write(temp.path().join("made/overlay_portal.dat"), b"imported").unwrap();
        let names: Vec<String> = source_files(&world)
            .iter()
            .map(|f| {
                f.strip_prefix(temp.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(
            names,
            [
                "world.toml",
                "made/overlay_portal.dat",
                "sample/edits/landblocks/A9B4.toml",
                "sample/pack.toml"
            ]
        );
        let h = header(&world);
        assert_eq!(
            (h.key.as_deref(), h.era.as_str()),
            (Some("hills"), "infiltration")
        );
        std::fs::write(&world, "key = 'plain'\n").unwrap();
        assert_eq!(header(&world).era, "eor");
    }

    /// The digest is the sources' contents, wherever the world is: a pack file changed changes
    /// it, the same files in another folder do not, and a file beside the world that is no source
    /// leaves it as it was.
    #[test]
    fn the_digest_follows_the_sources_contents_and_nothing_else() {
        let (temp, world) = world();
        let first = digest(&world).unwrap();
        assert_eq!(
            digest(&world).unwrap(),
            first,
            "the same sources, the same digest"
        );
        std::fs::write(temp.path().join("world.lock"), "[wcid]\n").unwrap();
        assert_eq!(digest(&world).unwrap(), first, "the lockfile is no source");
        let moved = tempfile::tempdir().unwrap();
        crate::hosts::copy_tree(temp.path(), moved.path()).unwrap();
        assert_eq!(digest(&moved.path().join("world.toml")).unwrap(), first);
        std::fs::write(
            temp.path().join("sample/edits/landblocks/A9B4.toml"),
            "[[vertex]]\nat = [0, 1]\nheight = 51\n",
        )
        .unwrap();
        assert_ne!(digest(&world).unwrap(), first);
    }

    /// When the world was last saved is when its newest source last changed; a world whose files
    /// are gone has no such time.
    #[test]
    fn a_world_was_last_saved_when_its_newest_source_changed() {
        let (_temp, world) = world();
        let s = stamp(&world);
        let newest = s.iter().filter_map(|(_, m, _)| *m).max().unwrap();
        assert_eq!(
            saved(&s),
            Some(
                newest
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            )
        );
        assert_eq!(saved(&Vec::new()), None);
    }
}
