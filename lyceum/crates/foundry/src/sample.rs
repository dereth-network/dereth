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
        EraId::Launch | EraId::Infiltration => Cow::Borrowed(bytes),
    }
}

/// A new world's `world.toml`: its key, its era, its one pack, its build in its own folder, and
/// its land's spacing in meters when it is finer than the files' 24.
#[must_use]
pub fn world_toml(key: &str, pack: &str, sample: bool, era: EraId, spacing: u32) -> String {
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
    let land = if spacing == crate::world::LAND_CELL {
        String::new()
    } else {
        format!("\n[land]\nspacing = {spacing}\n")
    };
    format!(
        "{about}format_version = 1\nkey = \"{key}\"\n{era}packs = [\"{pack}\"]\nout = \"build\"\n{land}"
    )
}

/// A new empty world started on the world an import made: its overlay folder `overlay`, laid under
/// the world's one pack (`imports`), whose land grid, records and rules the world takes as its own
/// ([`crate::land`], [`crate::compile::carry_world_files`]). The world's folder names the overlay
/// relative to itself where it can.
///
/// # Errors
/// As [`make`], and a folder that holds no overlay.
pub fn make_on(dir: &Path, era: EraId, overlay: &Path) -> Result<PathBuf, NotMade> {
    let holds = std::fs::read_dir(overlay).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.starts_with("overlay_") && name.ends_with(".dat")
        })
    });
    if !holds {
        return Err(NotMade::Failed(format!(
            "{} holds no world to start from",
            overlay.display()
        )));
    }
    let world = make(dir, true, era, crate::world::LAND_CELL)?;
    let key = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let named = relative(dir, overlay).replace('\\', "/");
    let text = format!(
        "# A new world on the world in {named}, with one pack to build it in.\nformat_version = 1\n\
         key = \"{key}\"\n{}imports = [\"{named}\"]\npacks = [\"{key}\"]\nout = \"build\"\n",
        match era {
            EraId::Eor => String::new(),
            other => format!("era = \"{}\"\n", other.name()),
        }
    );
    std::fs::write(&world, text)
        .map_err(|e| NotMade::Failed(format!("{}: {e}", world.display())))?;
    Ok(world)
}

/// `to` as a path from folder `from`: up from `from` to the folders they share, then down; `to`
/// itself, made whole, when they share none (another drive).
fn relative(from: &Path, to: &Path) -> String {
    let whole = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let (from, to) = (whole(from), whole(to));
    let a: Vec<_> = from.components().collect();
    let b: Vec<_> = to.components().collect();
    let shared = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if shared == 0 {
        return to.display().to_string();
    }
    let mut out: Vec<String> = vec!["..".to_owned(); a.len() - shared];
    out.extend(
        b[shared..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    if out.is_empty() {
        ".".to_owned()
    } else {
        out.join("/")
    }
}

/// A sample landblock file's `[[vertex]]` rows moved onto a land of `steps` vertex steps a land
/// cell: each row's corners at the same place in meters, so the sample's hill stands where it
/// does on the files' land.
fn on_land(text: &str, steps: usize) -> String {
    if steps <= 1 {
        return text.to_owned();
    }
    let Ok(mut doc) = text.parse::<toml_edit::DocumentMut>() else {
        return text.to_owned();
    };
    if let Some(rows) = doc
        .get_mut("vertex")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
    {
        let k = i64::try_from(steps).unwrap_or(1);
        for row in rows.iter_mut() {
            for key in ["at", "to"] {
                let Some(a) = row.get_mut(key).and_then(toml_edit::Item::as_array_mut) else {
                    continue;
                };
                for v in a.iter_mut() {
                    if let Some(n) = v.as_integer() {
                        let decor = v.decor().clone();
                        *v = toml_edit::Value::from(n * k);
                        *v.decor_mut() = decor;
                    }
                }
            }
        }
    }
    doc.to_string()
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
pub fn make(dir: &Path, empty: bool, era: EraId, spacing: u32) -> Result<PathBuf, NotMade> {
    let steps = crate::world::land_steps(spacing).ok_or_else(|| {
        NotMade::Failed(format!(
            "{spacing} m is not a land spacing; use 24, 12 or 8"
        ))
    })?;
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
        write(
            "world.toml",
            world_toml(key, key, false, era, spacing).as_bytes(),
        )?;
    } else {
        for (rel, bytes) in FILES {
            let bytes = file_for(era, rel, bytes);
            if rel.contains("/edits/landblocks/") && steps > 1 {
                let text = on_land(&String::from_utf8_lossy(&bytes), steps);
                write(rel, text.as_bytes())?;
            } else {
                write(rel, &bytes)?;
            }
        }
        write(
            "world.toml",
            world_toml(key, PACK, true, era, spacing).as_bytes(),
        )?;
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
            let world = make(&parent.join("my-world"), empty, era, 24).unwrap();
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
    fn a_world_made_on_a_finer_land_says_its_spacing_and_keeps_the_sample_s_hill_where_it_was() {
        let parent = scratch("fine");
        let world = make(&parent.join("fine-world"), false, EraId::Eor, 8).unwrap();
        let read = crate::world::World::read(&world).unwrap();
        assert_eq!(read.land_steps, Some(3));
        let pack = crate::source::read_pack(&read.packs[0]).unwrap();
        let coarse = make(&parent.join("coarse"), false, EraId::Eor, 24).unwrap();
        let coarse = crate::world::World::read(&coarse).unwrap();
        let coarse = crate::source::read_pack(&coarse.packs[0]).unwrap();
        let rows = |p: &crate::source::Pack| -> Vec<((u8, u8), (u8, u8))> {
            p.edits
                .iter()
                .flat_map(|e| e.vertices.iter().map(|v| (v.item.from, v.item.to)))
                .collect()
        };
        let tripled: Vec<_> = rows(&coarse)
            .into_iter()
            .map(|(a, b)| ((a.0 * 3, a.1 * 3), (b.0 * 3, b.1 * 3)))
            .collect();
        assert!(!tripled.is_empty());
        assert_eq!(rows(&pack), tripled);
        // The files' land is no setting; a spacing that does not cut a cell evenly is refused.
        let plain = make(&parent.join("plain"), true, EraId::Eor, 24).unwrap();
        assert_eq!(crate::world::World::read(&plain).unwrap().land_steps, None);
        assert!(make(&parent.join("odd"), true, EraId::Eor, 10).is_err());
        assert!(make(&parent.join("six"), true, EraId::Eor, 6).is_err());
        std::fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn a_world_started_on_an_imported_world_names_its_overlay_beside_it_and_has_no_land_setting() {
        let parent = scratch("on");
        let overlay = parent.join("converted/overlay");
        std::fs::create_dir_all(&overlay).unwrap();
        assert!(
            make_on(&parent.join("bare"), EraId::Eor, &overlay).is_err(),
            "no overlay yet"
        );
        std::fs::write(overlay.join("overlay_cell.dat"), b"").unwrap();
        let world = make_on(&parent.join("mine"), EraId::Eor, &overlay).unwrap();
        let read = crate::world::World::read(&world).unwrap();
        assert_eq!(read.imports.len(), 1);
        assert!(std::fs::read_to_string(&world)
            .unwrap()
            .contains("imports = [\"../converted/overlay\"]"));
        assert_eq!(read.land_steps, None, "the grid is the import's");
        let pack = crate::source::read_pack(&read.packs[0]).unwrap();
        assert!(
            pack.edits.is_empty(),
            "an empty pack, not the sample's Holtburg"
        );
        std::fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn a_folder_that_holds_files_or_is_no_world_key_is_refused() {
        let parent = scratch("refused");
        std::fs::create_dir_all(parent.join("taken")).unwrap();
        std::fs::write(parent.join("taken/notes.txt"), b"mine").unwrap();
        assert_eq!(
            make(&parent.join("taken"), false, EraId::Eor, 24),
            Err(NotMade::Taken(parent.join("taken")))
        );
        assert_eq!(
            std::fs::read(parent.join("taken/notes.txt")).unwrap(),
            b"mine"
        );
        let bad = make(&parent.join("My World"), true, EraId::Eor, 24).unwrap_err();
        assert_eq!(bad, NotMade::BadName("My World".into()));
        assert!(bad.to_string().contains("lowercase"), "{bad}");
        // An empty folder, as a file browser's New folder leaves one, is fine.
        std::fs::create_dir_all(parent.join("fresh")).unwrap();
        make(&parent.join("fresh"), true, EraId::Eor, 24).unwrap();
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
