//! The sample world Lyceum carries, and a new world made from it or empty: what the editor's
//! start screen and `lyceum new` both make.
//!
//! The folder's name is the world's key, and an empty world's one pack's name too: lowercase
//! letters, digits and hyphens. The sample is statics west of Holtburg, a hill, a new texture and
//! an NPC wearing it, with the sample's `world.lock` so its ids are the sample's. An empty world
//! has one pack with nothing in it yet, for the editor to write into.
//!
//! A world is built on the files of one era, which its `world.toml` names. The sample is the
//! same in both eras, but for its NPC's motion table, which the end of retail's files number
//! otherwise than Infiltration's ([`file_for`]).

use std::borrow::Cow;
use std::fmt;
use std::path::{Path, PathBuf};

use dereth_primitives::EraId;

/// The sample world's files, by their place in a world's folder. Its `world.toml` is written for
/// each new world ([`world_toml`]).
pub const FILES: &[(&str, &[u8])] = &[
    ("world.lock", include_bytes!("../../../sample/world.lock")),
    (
        "holtburg/pack.toml",
        include_bytes!("../../../sample/holtburg/pack.toml"),
    ),
    (
        "holtburg/art/stripes.png",
        include_bytes!("../../../sample/holtburg/art/stripes.png"),
    ),
    (
        "holtburg/records/stripes.toml",
        include_bytes!("../../../sample/holtburg/records/stripes.toml"),
    ),
    (
        "holtburg/edits/landblocks/A8B4.toml",
        include_bytes!("../../../sample/holtburg/edits/landblocks/A8B4.toml"),
    ),
    (
        "holtburg/edits/landblocks/A9B4.toml",
        include_bytes!("../../../sample/holtburg/edits/landblocks/A9B4.toml"),
    ),
    (
        "holtburg/server/sql/weenies/guide.sql",
        include_bytes!("../../../sample/holtburg/server/sql/weenies/guide.sql"),
    ),
    (
        "holtburg/server/sql/landblocks/A9B4 guide.sql",
        include_bytes!("../../../sample/holtburg/server/sql/landblocks/A9B4 guide.sql"),
    ),
];

/// The sample's one pack.
pub const PACK: &str = "holtburg";

/// The sample's file `rel` (one of [`FILES`]) as a world built on `era` holds it: the NPC's motion
/// table is the one that era's files have.
#[must_use]
pub fn file_for<'a>(era: EraId, rel: &str, bytes: &'a [u8]) -> Cow<'a, [u8]> {
    match era {
        EraId::Eor => Cow::Borrowed(bytes),
        EraId::Infiltration if rel.ends_with("weenies/guide.sql") => Cow::Owned(
            String::from_utf8_lossy(bytes)
                .replace("0x090001D0", "0x09000008")
                .into_bytes(),
        ),
        EraId::Infiltration => Cow::Borrowed(bytes),
    }
}

/// A new world's `world.toml`: its key, its era, its one pack, and its build in its own folder.
#[must_use]
pub fn world_toml(key: &str, pack: &str, sample: bool, era: EraId) -> String {
    let about = if sample {
        "# A world made from Lyceum's sample: statics west of Holtburg, a hill, a new texture and an\n\
         # NPC wearing it.\n"
    } else {
        "# A new world, with one pack to build it in.\n"
    };
    let era = match era {
        EraId::Eor => String::new(),
        other => format!("era = \"{}\"\n", other.name()),
    };
    format!(
        "{about}format_version = 1\nkey = \"{key}\"\n{era}packs = [\"{pack}\"]\nout = \"build\"\n"
    )
}

/// An empty pack's `pack.toml`.
#[must_use]
pub fn pack_toml(name: &str) -> String {
    format!("name = \"{name}\"\nformat_version = 1\nversion = \"0.1.0\"\n")
}

/// Why a new world was not made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotMade {
    /// The folder's name cannot name a world.
    BadName(String),
    /// The folder already holds files.
    Taken(PathBuf),
    /// A file would not write, or the folder would not read.
    Failed(String),
}

impl fmt::Display for NotMade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadName(n) => write!(
                f,
                "{n:?} cannot name a world: lowercase letters, digits and hyphens"
            ),
            Self::Taken(d) => write!(f, "{} already holds files", d.display()),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for NotMade {}

/// Make a new world in `dir`, which must not exist yet or be empty, built on the files of `era`:
/// from the sample, or with one empty pack named as the world. Answers its `world.toml`.
///
/// # Errors
/// A folder name that is no world key, a folder that already holds something, or a file that
/// will not write.
pub fn make(dir: &Path, empty: bool, era: EraId) -> Result<PathBuf, NotMade> {
    let key = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| NotMade::BadName(dir.display().to_string()))?;
    if !crate::symbol::is_name(key) {
        return Err(NotMade::BadName(key.to_owned()));
    }
    if dir.exists()
        && std::fs::read_dir(dir)
            .map_err(|e| NotMade::Failed(format!("{}: {e}", dir.display())))?
            .next()
            .is_some()
    {
        return Err(NotMade::Taken(dir.to_path_buf()));
    }
    let write = |rel: &str, bytes: &[u8]| -> Result<(), NotMade> {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| NotMade::Failed(format!("{}: {e}", parent.display())))?;
        }
        std::fs::write(&path, bytes)
            .map_err(|e| NotMade::Failed(format!("{}: {e}", path.display())))
    };
    if empty {
        write(&format!("{key}/pack.toml"), pack_toml(key).as_bytes())?;
        write("world.toml", world_toml(key, key, false, era).as_bytes())?;
    } else {
        for (rel, bytes) in FILES {
            write(rel, &file_for(era, rel, bytes))?;
        }
        write("world.toml", world_toml(key, PACK, true, era).as_bytes())?;
    }
    Ok(dir.join("world.toml"))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the sample world Lyceum carries, and a new world made from it or empty)
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lyceum-sample-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_sample_world_and_an_empty_one_both_read_as_worlds() {
        for (empty, era) in [
            (false, EraId::Eor),
            (true, EraId::Eor),
            (false, EraId::Infiltration),
        ] {
            let parent = scratch(&format!("{empty}-{}", era.name()));
            let world = make(&parent.join("my-world"), empty, era).unwrap();
            let read = crate::world::World::read(&world).unwrap();
            assert_eq!(read.key, "my-world");
            assert_eq!(read.era, era);
            assert_eq!(read.packs.len(), 1);
            let pack = crate::source::read_pack(&read.packs[0]).unwrap();
            assert_eq!(pack.name, if empty { "my-world" } else { PACK });
            assert_eq!(pack.edits.is_empty(), empty);
            assert_eq!(parent.join("my-world/world.lock").is_file(), !empty);
            if !empty {
                for (rel, bytes) in FILES {
                    let on_disk = std::fs::read(parent.join("my-world").join(rel)).unwrap();
                    assert_eq!(on_disk, *file_for(era, rel, bytes), "{rel}");
                    let changed = rel.ends_with("weenies/guide.sql") && era == EraId::Infiltration;
                    assert_eq!(on_disk == *bytes, !changed, "{rel}");
                }
            }
            std::fs::remove_dir_all(&parent).unwrap();
        }
    }

    #[test]
    fn a_folder_that_holds_files_or_is_no_world_key_is_refused() {
        let parent = scratch("refused");
        std::fs::create_dir_all(parent.join("taken")).unwrap();
        std::fs::write(parent.join("taken/notes.txt"), b"mine").unwrap();
        assert_eq!(
            make(&parent.join("taken"), false, EraId::Eor),
            Err(NotMade::Taken(parent.join("taken")))
        );
        assert_eq!(
            std::fs::read(parent.join("taken/notes.txt")).unwrap(),
            b"mine"
        );
        let bad = make(&parent.join("My World"), true, EraId::Eor).unwrap_err();
        assert_eq!(bad, NotMade::BadName("My World".into()));
        assert!(bad.to_string().contains("lowercase"), "{bad}");
        // An empty folder, as a file browser's New folder leaves one, is fine.
        std::fs::create_dir_all(parent.join("fresh")).unwrap();
        make(&parent.join("fresh"), true, EraId::Eor).unwrap();
        std::fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn the_sample_carried_is_the_sample_world_in_the_tree() {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample");
        let mut on_disk = Vec::new();
        let mut todo = vec![sample.clone()];
        while let Some(dir) = todo.pop() {
            for e in std::fs::read_dir(&dir).unwrap() {
                let p = e.unwrap().path();
                // What the editor writes into a world it opens (`.playtest`, `.cache` and the
                // like) is not the sample's.
                if p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with('.'))
                {
                    continue;
                }
                if p.is_dir() {
                    todo.push(p);
                } else {
                    let rel = p.strip_prefix(&sample).unwrap();
                    on_disk.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        on_disk.retain(|r| r != "world.toml");
        on_disk.sort();
        let mut carried: Vec<String> = FILES.iter().map(|(r, _)| (*r).to_owned()).collect();
        carried.sort();
        assert_eq!(carried, on_disk, "the sample carried is the tree's");
        let sample_toml = std::fs::read_to_string(sample.join("world.toml")).unwrap();
        assert!(
            sample_toml.contains(&format!("packs = [\"{PACK}\"]")),
            "{sample_toml}"
        );
    }
}
