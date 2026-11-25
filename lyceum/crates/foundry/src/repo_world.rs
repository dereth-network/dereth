//! A world repository opened as the world builder's world: the packs of its layers, up to the one
//! being edited, in order (the base's, then each patch's), laid over its land.
//!
//! The land is the converter's data overlay, which the repository names only by what it was built
//! from and by the SHA-256 of each of its four files (`[land]` in its `world.toml`); the overlay
//! itself is the server owner's own, never in the repository. The folder given for it must be
//! that land, file for file ([`check_land`]): one built otherwise is refused, with the command that
//! builds it from the repository's pins. The land lies over AC1's files, so the player's own must be
//! the ones it was built over, file for file too ([`check_inputs`]).
//!
//! The world builder's own edits of the land (`records/`, `edits/`) are in those packs beside their
//! rows, and its ids in `world.lock` at the repository's root, beside `world.toml`. What it builds
//! goes elsewhere: never into the repository.

use std::path::{Path, PathBuf};

use dereth_dat::ModernDat;
use sha2::{Digest, Sha256};

use crate::repo::Repo;
use crate::world::{EraId, World};

/// The land overlay's files, each with the `[land]` key that pins it.
pub const LAND_FILES: [(&str, &str); 4] = [
    ("overlay_cell.dat", "cell_overlay_sha256"),
    ("overlay_portal.dat", "portal_overlay_sha256"),
    ("places.tsv", "places_sha256"),
    ("world.toml", "rules_sha256"),
];

/// Whether `world_toml` is a world repository's world file: one that names its layers
/// (`lineage`).
#[must_use]
pub fn is_repository(world_toml: &Path) -> bool {
    std::fs::read_to_string(world_toml)
        .ok()
        .and_then(|t| toml::from_str::<toml::Table>(&t).ok())
        .is_some_and(|t| t.contains_key("lineage"))
}

/// The repository's world file, read as a table.
fn world_table(repo: &Repo) -> Result<toml::Table, String> {
    let text =
        std::fs::read_to_string(&repo.path).map_err(|e| format!("{}: {e}", repo.path.display()))?;
    toml::from_str(&text).map_err(|e| format!("{}: {e}", repo.path.display()))
}

/// The AC1 files a converted land is built over, each with the `[land]` key that pins it: the
/// player's own files must be these, or the land isn't the repository's.
pub const INPUT_FILES: [(ModernDat, &str); 4] = [
    (ModernDat::Cell, "ac1_cell_sha256"),
    (ModernDat::Portal, "ac1_portal_sha256"),
    (ModernDat::Local, "ac1_local_sha256"),
    (ModernDat::HighRes, "ac1_highres_sha256"),
];

/// Where the SHA-256 of files already hashed is kept, by each file's path, length and time of
/// change, so a land and its inputs (more than a gigabyte) are hashed again only when they change.
fn hash_cache() -> PathBuf {
    std::env::temp_dir().join("lyceum-file-hashes.tsv")
}

/// A file's SHA-256, read through: `None` when it doesn't read.
fn sha256_of(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let when = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    let whole = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let key = format!("{}\t{}\t{when}\t", whole.display(), meta.len());
    let cache = hash_cache();
    let whole_hash = |h: &str| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit());
    if let Some(found) = std::fs::read_to_string(&cache).ok().and_then(|t| {
        t.lines().rev().find_map(|l| {
            l.strip_prefix(key.as_str())
                .filter(|h| whole_hash(h))
                .map(str::to_owned)
        })
    }) {
        return Some(found);
    }
    let mut h = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path).ok()?, &mut h).ok()?;
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    // One write a line, so lines written at once by several builders don't run into each other.
    use std::io::Write as _;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&cache)
    {
        let _ = f.write_all(format!("{key}{hex}\n").as_bytes());
    }
    Some(hex)
}

/// Refused unless the player's own AC1 files in `dats` are those the repository's land is built
/// over (`[land]`'s `ac1_*_sha256`), naming each that isn't, with its expected and found SHA-256.
/// A repository that pins none of them (or no land) checks nothing.
///
/// # Errors
/// Any pinned file that isn't the one named.
pub fn check_inputs(repo: &Repo, dats: &Path) -> Result<(), String> {
    let world = world_table(repo)?;
    let Some(land) = world.get("land").and_then(toml::Value::as_table) else {
        return Ok(());
    };
    let mut wrong = Vec::new();
    for (role, key) in INPUT_FILES {
        let Some(expected) = land.get(key).and_then(toml::Value::as_str) else {
            continue;
        };
        let found = sha256_of(&role.in_dir(dats));
        if found.as_deref() != Some(expected) {
            wrong.push(format!(
                "  {}: expected {expected}, found {}",
                role.file_name(),
                found.as_deref().unwrap_or("no file")
            ));
        }
    }
    if wrong.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} aren't the files {}'s land is built over, so its land wouldn't be the one the server sends:\n{}\nUse acclient 00.00.11.6096's own files.",
        dats.display(),
        repo.path.display(),
        wrong.join("\n")
    ))
}

/// Refused unless `overlay` is the land the repository pins (`[land]`): each of its four files
/// as the pins say, the expected and found SHA-256 of each that isn't named, with the command that
/// builds the land from the pins. A repository that pins no land takes none (`overlay` `None`).
///
/// # Errors
/// A land pinned and none given, one given and none pinned, or one that isn't the pinned land.
pub fn check_land(repo: &Repo, overlay: Option<&Path>) -> Result<(), String> {
    let world = world_table(repo)?;
    // A land is pinned by its files' hashes: a `[land]` that pins none (a ledger alone) takes none.
    let land = world
        .get("land")
        .and_then(toml::Value::as_table)
        .filter(|l| LAND_FILES.iter().any(|(_, key)| l.contains_key(*key)));
    let (land, overlay) = match (land, overlay) {
        (None, None) => return Ok(()),
        (None, Some(o)) => {
            return Err(format!(
                "{} pins no land, so it takes no overlay ({})",
                repo.path.display(),
                o.display()
            ))
        }
        (Some(_), None) => {
            return Err(format!(
                "{} is built on a converted land: give its overlay folder (--on <folder>)",
                repo.path.display()
            ))
        }
        (Some(l), Some(o)) => (l, o),
    };
    let mut wrong = Vec::new();
    for (file, key) in LAND_FILES {
        let expected = land.get(key).and_then(toml::Value::as_str).unwrap_or("");
        let found = sha256_of(&overlay.join(file));
        if found.as_deref() != Some(expected) {
            wrong.push(format!(
                "  {file}: expected {expected}, found {}",
                found.as_deref().unwrap_or("no file")
            ));
        }
    }
    if wrong.is_empty() {
        return Ok(());
    }
    let s = |k: &str| land.get(k).and_then(toml::Value::as_str).unwrap_or("");
    let args: Vec<&str> = land
        .get("converter_args")
        .and_then(toml::Value::as_array)
        .map(|a| a.iter().filter_map(toml::Value::as_str).collect())
        .unwrap_or_default();
    let ledger = s("ledger");
    Err(format!(
        "{} isn't the land {} pins:\n{}\nBuild it from the pins, with the converter at dereth commit {}, in the repository's folder:\n  {} --ac1 <AC1 install> --ac2 <AC2 install> --out <folder>{}{}",
        overlay.display(),
        repo.path.display(),
        wrong.join("\n"),
        s("converter_commit"),
        s("converter"),
        if ledger.is_empty() {
            String::new()
        } else {
            format!(" --ledger {ledger}")
        },
        args.iter().map(|a| format!(" {a}")).collect::<String>(),
    ))
}

/// Refused when the repository's `world.lock` holds a merge's conflict markers: it is regenerated
/// (`lyceum lock`, [`crate::lock_merge::rebuild`]), never put right by hand; `land` is the land
/// the command is to name.
///
/// # Errors
/// A lockfile in conflict, with the command that regenerates it.
pub fn check_lock(repo: &Repo, land: Option<&Path>) -> Result<(), String> {
    let lock = repo.path.with_file_name("world.lock");
    if std::fs::read_to_string(&lock).is_ok_and(|t| t.lines().any(|l| l.starts_with("<<<<<<<"))) {
        return Err(format!(
            "{} holds a merge's conflict: regenerate it, never by hand: lyceum lock {} --dats <client folder>{}",
            lock.display(),
            repo.path.display(),
            land.map_or_else(String::new, |l| format!(" --on {}", l.display()))
        ));
    }
    Ok(())
}

/// The moment the repository's world's calendar starts, in UTC (`2026-12-01T00:00:00Z`): its
/// `[calendar] starts_at` once it has launched, else the first moment of the month its base opens
/// (`[base] month`); `None` when it names neither.
///
/// # Errors
/// A world file that doesn't read.
pub fn launched_at(repo: &Repo) -> Result<Option<String>, String> {
    let world = world_table(repo)?;
    let s = |table: &str, key: &str| {
        world
            .get(table)
            .and_then(|t| t.get(key))
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    };
    Ok(s("calendar", "starts_at")
        .or_else(|| s("base", "month").map(|m| format!("{m}-01T00:00:00Z"))))
}

/// The landblock of the repository's first starting area, where new characters appear: the
/// destination of the weenie its first `Start: <name>` point of interest names, as the newest layer
/// that sets each has it. `None` when it has none.
#[must_use]
pub fn start_landblock(repo: &Repo) -> Option<u16> {
    let mut starts: Vec<(String, i64)> = Vec::new();
    let mut cells: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
    for pack in repo.layers.iter().flat_map(|l| &l.packs) {
        for text in pack.files.values() {
            let Ok(t) = toml::from_str::<toml::Table>(text) else {
                continue;
            };
            let rows = |kind: &str| {
                t.get(kind)
                    .and_then(toml::Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(toml::Value::as_table)
            };
            for poi in rows("poi") {
                let (Some(name), Some(class)) = (
                    poi.get("name").and_then(toml::Value::as_str),
                    poi.get("weenie_class_id").and_then(toml::Value::as_integer),
                ) else {
                    continue;
                };
                if name.starts_with("Start:") {
                    starts.retain(|(n, _)| n != name);
                    starts.push((name.to_owned(), class));
                }
            }
            for weenie in rows("weenie") {
                let (Some(class), Some(cell)) = (
                    weenie.get("class").and_then(toml::Value::as_integer),
                    weenie
                        .get("position")
                        .and_then(|p| p.get("Destination"))
                        .and_then(|d| d.get("cell"))
                        .and_then(toml::Value::as_integer),
                ) else {
                    continue;
                };
                cells.insert(class, cell);
            }
        }
    }
    starts.sort();
    starts
        .iter()
        .find_map(|(_, class)| cells.get(class))
        .and_then(|cell| u16::try_from(cell >> 16).ok())
}

/// The repository's key: its world file's, else its folder's name.
///
/// # Errors
/// A world file that doesn't read.
pub fn key_of(repo: &Repo) -> Result<String, String> {
    let world = world_table(repo)?;
    Ok(world
        .get("key")
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            repo.path
                .parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_default())
}

/// The world builder's world of `repo` at layer `layer`: every layer's packs up to it, in order,
/// over the land `land` (checked: [`check_land`]), its build going to `out`. Its files are the end
/// of retail's, which the land is built over; the rules the repository plays are its own
/// ([`Repo::era`]).
///
/// # Errors
/// A layer that can't be shown, two packs of one name (the world builder names what it makes by
/// pack), or a land that isn't the one pinned.
pub fn world_of(
    repo: &Repo,
    layer: usize,
    land: Option<&Path>,
    out: &Path,
) -> Result<World, String> {
    if layer >= repo.shown_layers() {
        return Err(format!(
            "{}: the world after a checkpoint comes from its pinned release, which Lyceum doesn't edit",
            repo.layers.get(layer).map_or("?", |l| l.id.as_str())
        ));
    }
    check_land(repo, land)?;
    let mut packs: Vec<PathBuf> = Vec::new();
    let mut names: Vec<&str> = Vec::new();
    for l in repo.layers.iter().take(layer + 1) {
        for p in &l.packs {
            if names.contains(&p.name.as_str()) {
                return Err(format!(
                    "two packs are named {}: the world builder names what it makes by its pack, so a repository's pack names are its own (in {})",
                    p.name, l.id
                ));
            }
            names.push(&p.name);
            packs.push(p.dir.clone());
        }
    }
    Ok(World {
        path: repo.path.clone(),
        key: key_of(repo)?,
        era: EraId::Eor,
        packs,
        imports: land.map(Path::to_path_buf).into_iter().collect(),
        out: out.to_path_buf(),
        ids: std::collections::BTreeMap::new(),
        overrides: Vec::new(),
        base: None,
        lineage: Vec::new(),
        land_steps: None,
    })
}

/// World repository `world_toml` opened to edit as the editor opens it: its world at its newest
/// layer over land `land` ([`world_of`], building into `out`), the player's files `dats` checked
/// against its pins ([`check_inputs`]), with the server's `schema`, and its newest layer the one
/// edited ([`crate::edit::Session::open_repository`]), so that its spawns are written as that
/// layer's placement rows. `pack` names the pack edits go into, as for any world.
///
/// # Errors
/// A lockfile in conflict, a land or files that aren't the repository's, a world that doesn't
/// open, or a layer that can't be edited.
pub fn open_to_edit(
    world_toml: &Path,
    dats: &Path,
    land: Option<&Path>,
    out: &Path,
    pack: Option<&str>,
    schema: crate::content::Schema,
) -> Result<crate::edit::Session, String> {
    let repo = Repo::open(world_toml)?;
    check_lock(&repo, land)?;
    let layer = repo
        .shown_layers()
        .checked_sub(1)
        .ok_or_else(|| format!("{}: no layer can be edited", repo.path.display()))?;
    let world = world_of(&repo, layer, land, out)?;
    check_inputs(&repo, dats)?;
    let mut s =
        crate::edit::Session::open_world(world, dats, pack, None).map_err(|e| e.to_string())?;
    s.set_schema(Some(schema));
    s.open_repository(&repo, layer).map_err(|e| e.to_string())?;
    Ok(s)
}
