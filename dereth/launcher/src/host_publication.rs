//! Local published world snapshots, selected by one atomic configuration replacement.
//!
//! Each publish of a Lyceum world is the revision after the last one ([`publish`]): the server
//! patches every Dereth player to it, and a player who has the last one is sent what changed. The
//! newest [`KEPT_SNAPSHOTS`] snapshots are kept, and the one the world plays always.

use crate::{components, host_config, host_runtime};
use dereth_launch::choices;
use dereth_launch::datset::SetKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Publication {
    pub source: PathBuf,
    pub key: String,
    pub packs: Vec<String>,
    pub build: String,
    pub files: BTreeMap<String, String>,
    /// Which publish of the Lyceum world this is, counted from 1 on this world: the one after the
    /// last when what it builds differs, the last's own when it does not. 0 in a publication made
    /// before publishes were counted.
    #[serde(default)]
    pub revision: u32,
    /// The digest of the Lyceum world's sources it was built from
    /// ([`crate::lyceum_world::digest`]); empty in one made before they were kept.
    #[serde(default)]
    pub sources: String,
    /// The file name of the world database dump it was built over, which names the ACE release.
    #[serde(default)]
    pub base: Option<String>,
}

/// The revision a publish of a Lyceum world is, after its last publish on the world `last`: the
/// first is 1; one that builds what the last built is the last's revision, and any other the next.
/// A last publish from before publishes were counted counts as the first.
#[must_use]
pub fn next_revision(last: Option<&Publication>, build: &str) -> u32 {
    match last {
        None => 1,
        Some(last) if last.build == build => last.revision.max(1),
        Some(last) => last.revision.max(1) + 1,
    }
}

fn digest(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut input = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn inventory(root: &Path, dir: &Path, files: &mut BTreeMap<String, String>) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("Published content must not contain links.".into());
        }
        if kind.is_dir() {
            inventory(root, &entry.path(), files)?;
        } else if kind.is_file() {
            let relative = entry
                .path()
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(relative, digest(&entry.path())?);
        } else {
            return Err("Published content must contain ordinary files.".into());
        }
    }
    Ok(())
}

fn resolved(dir: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.into()
    } else {
        dir.join(path)
    }
}

/// Why the world in `dir` may not change its world database now: it keeps a content overlay
/// (`world_overlay_path`), changes made in the world over the database it plays. `doing` is what
/// would change it.
fn content_overlay_refusal(doc: &DocumentMut, doing: &str) -> Option<String> {
    doc.get("server")
        .and_then(|s| s.get("world_overlay_path"))
        .and_then(|p| p.as_str())
        .filter(|p| !p.trim().is_empty())
        .map(|_| {
            format!(
                "{doing} would replace the world database this world's content overlay \
                 (world_overlay_path in empyrean.toml) was made over. Remove world_overlay_path, \
                 or build the overlay into the world database with empyrean-import --overlay, \
                 then try again."
            )
        })
}

/// How many published snapshots a world keeps: the newest, and the two before it.
pub const KEPT_SNAPSHOTS: usize = 3;

/// The published snapshots of the world in `dir`, newest first, by their numbers.
fn snapshots(dir: &Path) -> Vec<(u32, PathBuf)> {
    let mut found: Vec<(u32, PathBuf)> = std::fs::read_dir(dir.join("published"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let n = e
                .file_name()
                .to_str()?
                .strip_prefix("snapshot-")?
                .parse()
                .ok()?;
            Some((n, e.path()))
        })
        .collect();
    found.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    found
}

/// The newest snapshot the Lyceum world `key` was published to `dir` as, which its next publish
/// follows: its overlay is the one players were last patched to.
fn last_publication(dir: &Path, key: &str) -> Option<PathBuf> {
    snapshots(dir).into_iter().map(|(_, p)| p).find(|p| {
        std::fs::read(p.join("publication.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Publication>(&b).ok())
            .is_some_and(|publication| publication.key == key)
    })
}

/// Remove the published snapshots of the world in `dir` older than the newest
/// [`KEPT_SNAPSHOTS`], never the one it plays. A folder that will not go is left for the next
/// publish. Answers the folders removed.
fn prune(dir: &Path) -> Vec<PathBuf> {
    let in_use = selected(dir)
        .ok()
        .flatten()
        .and_then(|(folder, _)| folder.canonicalize().ok());
    let mut removed = Vec::new();
    for (_, folder) in snapshots(dir).into_iter().skip(KEPT_SNAPSHOTS) {
        if folder.canonicalize().ok() == in_use {
            continue;
        }
        if std::fs::remove_dir_all(&folder).is_ok() {
            removed.push(folder);
        }
    }
    removed
}

pub fn prepared(dir: &Path) -> bool {
    pack_path(dir).is_file()
}

/// The world database the world in `dir` plays: its configuration's `world_pack_path`, else
/// `world.pack` beside it.
#[must_use]
pub fn pack_path(dir: &Path) -> PathBuf {
    std::fs::read_to_string(dir.join("empyrean.toml"))
        .ok()
        .and_then(|s| s.parse::<DocumentMut>().ok())
        .and_then(|d| {
            d.get("server")?
                .get("world_pack_path")?
                .as_str()
                .map(str::to_owned)
        })
        .map_or_else(|| dir.join("world.pack"), |p| resolved(dir, &p))
}

pub fn selected(dir: &Path) -> Result<Option<(PathBuf, Publication)>, String> {
    let config = std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?;
    let doc = config.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    let Some(overlay) = doc
        .get("dat_overlay")
        .and_then(|d| d.get("path"))
        .and_then(|p| p.as_str())
    else {
        return Ok(None);
    };
    let overlay = resolved(dir, overlay);
    let folder = overlay.parent().ok_or("Invalid published overlay path")?;
    let root = dir
        .join("published")
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let folder = folder.canonicalize().map_err(|e| e.to_string())?;
    if !folder.starts_with(root) {
        return Err("The published Lyceum world is outside this world's folder.".into());
    }
    let publication = serde_json::from_slice(
        &std::fs::read(folder.join("publication.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(Some((folder, publication)))
}

/// Why the Lyceum world `world` cannot be published to the world in `dir`, when its world file is
/// gone: said plainly when it is the one the world plays was published from.
#[must_use]
pub fn gone(dir: &Path, world: &Path) -> Option<String> {
    if world.is_file() {
        return None;
    }
    let source = selected(dir).ok().flatten().map(|(_, p)| p.source);
    Some(if source.is_some_and(|s| s == world) {
        "The Lyceum world it was published from is gone.".into()
    } else {
        "That Lyceum world is gone.".into()
    })
}

fn run(program: &Path, args: &[&std::ffi::OsStr], dir: &Path) -> Result<(), String> {
    let out = components::command(program)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(host_config::redact(&String::from_utf8_lossy(&out.stderr)))
    }
}

/// Builds from saved sources into a new folder, leaving the selected world intact on failure.
/// The caller excludes source edits; this operation locks the stopped host.
pub fn publish(dir: &Path, world: &Path, builder: &Path) -> Result<Publication, String> {
    use std::ffi::OsStr;
    let guard = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("run.lock"))
        .map_err(|e| e.to_string())?;
    guard
        .try_lock()
        .map_err(|_| "Stop the world before publishing.")?;
    let definition = host_runtime::read_definition(dir)?;
    if definition
        .program
        .parent()
        .ok_or("Missing runtime folder")?
        .join(".empyrean-update/managed-pending.json")
        .exists()
    {
        return Err("Apply or cancel the waiting Empyrean update before publishing.".into());
    }
    let config = std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?;
    let mut doc = config.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    if let Some(why) = content_overlay_refusal(&doc, "Publishing") {
        return Err(why);
    }
    let settings = host_config::Settings::from_config(&config)?;
    let era = doc
        .get("era")
        .and_then(|e| e.get("profile"))
        .and_then(|p| p.as_str())
        .unwrap_or("eor")
        .to_owned();
    let base_world = match era.as_str() {
        "eor" => "patches",
        "infiltration" => "16py",
        _ => {
            return Err(
                "Lyceum worlds publish only to end-of-retail and Infiltration worlds.".into(),
            )
        }
    };
    let prior_sql = selected(dir)?.map(|(folder, _)| folder.join("sql"));
    let inherited: Vec<String> = doc
        .get("server")
        .and_then(|s| s.get("world_base_patches"))
        .and_then(|p| p.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();

    let world = world.canonicalize().map_err(|e| e.to_string())?;
    let source = std::fs::read_to_string(&world).map_err(|e| e.to_string())?;
    let source_doc = source.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    let key = source_doc
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or("The world has no key.")?
        .to_owned();
    let packs = source_doc
        .get("packs")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let root = dir.join("published");
    std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    // Players hold the overlay this world was last published with; the new one is its next
    // revision, so they are patched with what changed.
    let last_folder = last_publication(dir, &key);
    let last_published: Option<Publication> = last_folder
        .as_ref()
        .and_then(|p| std::fs::read(p.join("publication.json")).ok())
        .and_then(|b| serde_json::from_slice(&b).ok());
    let last = last_folder.map(|p| p.join("overlay"));
    let sources = crate::lyceum_world::digest(&world)?;
    // Each snapshot is numbered after the newest, so the numbers say which is newer.
    let first = snapshots(dir).first().map_or(0, |(n, _)| n + 1);
    let folder = (first..first.saturating_add(10_000))
        .find_map(|n| {
            let p = root.join(format!("snapshot-{n}"));
            match std::fs::create_dir(&p) {
                Ok(()) => Some(Ok(p)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(e) => Some(Err(e.to_string())),
            }
        })
        .ok_or("Too many retained snapshots.")??;
    // A build that fails part way leaves nothing behind: the snapshot folder was made above
    // for this build alone, and the selected world is untouched until the very end.
    let built = (|| -> Result<Publication, String> {
        let mut build = vec![
            OsStr::new("build"),
            world.as_os_str(),
            OsStr::new("--dats"),
            settings.dats.as_os_str(),
            OsStr::new("--out"),
            folder.as_os_str(),
        ];
        if let Some(last) = &last {
            build.extend([OsStr::new("--after"), last.as_os_str()]);
        }
        run(builder, &build, dir)?;
        if std::fs::read_to_string(&world).map_err(|e| e.to_string())? != source
            || crate::lyceum_world::digest(&world)? != sources
        {
            return Err(
                "The world changed during publication. Save and close the editor, then retry."
                    .into(),
            );
        }
        let lock = world
            .parent()
            .ok_or("Missing world folder")?
            .join("world.lock");
        std::fs::copy(lock, folder.join("world.lock")).map_err(|e| e.to_string())?;
        let importer = definition
            .program
            .with_file_name(format!("empyrean-import{}", std::env::consts::EXE_SUFFIX));
        let base = if let Some(path) = doc
            .get("server")
            .and_then(|s| s.get("world_base_sql"))
            .and_then(|s| s.as_str())
            .filter(|s| !s.is_empty())
        {
            resolved(dir, path)
        } else {
            let cache = dir.join("base-sql");
            run(
                &importer,
                &[
                    OsStr::new("fetch"),
                    OsStr::new("--world"),
                    OsStr::new(base_world),
                    OsStr::new("--dir"),
                    cache.as_os_str(),
                ],
                dir,
            )?;
            let mut sql = Vec::new();
            for entry in std::fs::read_dir(&cache).map_err(|e| e.to_string())? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.extension().is_some_and(|e| e == "sql") {
                    sql.push(path);
                }
            }
            if sql.len() != 1 {
                return Err("The base SQL cache must hold exactly one dump.".into());
            }
            sql.remove(0)
        };
        let retained = folder.join("base.sql");
        keep(&base, &retained)?;
        let mut retained_patches = Vec::new();
        for (n, input) in inherited.iter().enumerate() {
            let (json, path) = input
                .strip_prefix("json:")
                .map_or((false, input.as_str()), |p| (true, p));
            let path = resolved(dir, path);
            if superseded_patch(&path, prior_sql.as_deref())? {
                continue;
            }
            let mut destination = folder.join("base-patches").join(n.to_string());
            if path.is_dir() {
                crate::hosts::copy_tree(&path, &destination)?;
            } else {
                destination = destination.join(path.file_name().ok_or("Invalid patch filename")?);
                std::fs::create_dir_all(destination.parent().ok_or("Missing patch folder")?)
                    .map_err(|e| e.to_string())?;
                std::fs::copy(path, &destination).map_err(|e| e.to_string())?;
            }
            retained_patches.push((json, destination));
        }
        let pack = folder.join("world.pack");
        let sql = folder.join("sql");
        let mut arguments = vec![
            OsStr::new("--sql"),
            retained.as_os_str(),
            OsStr::new("--era"),
            OsStr::new(&era),
        ];
        for (json, path) in &retained_patches {
            arguments.extend([
                OsStr::new(if *json { "--json" } else { "--patches" }),
                path.as_os_str(),
            ]);
        }
        arguments.extend([
            OsStr::new("--patches"),
            sql.as_os_str(),
            OsStr::new("--out"),
            pack.as_os_str(),
        ]);
        run(&importer, &arguments, dir)?;
        if !pack.is_file() {
            return Err("The importer did not write a world database.".into());
        }
        let mut files = BTreeMap::new();
        inventory(&folder, &folder.join("overlay"), &mut files)?;
        inventory(&folder, &sql, &mut files)?;
        files.insert("world.lock".into(), digest(&folder.join("world.lock"))?);
        files.insert("base.sql".into(), digest(&retained)?);
        if folder.join("base-patches").is_dir() {
            inventory(&folder, &folder.join("base-patches"), &mut files)?;
        }
        let build = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&files).map_err(|e| e.to_string())?)
        );
        let publication = Publication {
            revision: next_revision(last_published.as_ref(), &build),
            source: world,
            key,
            packs,
            build,
            files,
            sources,
            base: base.file_name().map(|n| n.to_string_lossy().into_owned()),
        };
        host_runtime::write_json(&folder.join("publication.json"), &publication)?;
        doc["server"]["world_pack_path"] = value(pack.to_string_lossy().as_ref());
        doc["server"]["world_base_sql"] = value(retained.to_string_lossy().as_ref());
        let mut patches = toml_edit::Array::new();
        for (json, path) in &retained_patches {
            patches.push(format!(
                "{}{}",
                if *json { "json:" } else { "" },
                path.to_string_lossy()
            ));
        }
        patches.push(sql.to_string_lossy().as_ref());
        doc["server"]["world_base_patches"] = value(patches);
        if doc.get("dat_overlay").is_none() {
            doc["dat_overlay"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if !doc["dat_overlay"].is_table() {
            return Err("dat_overlay must be a table.".into());
        }
        for name in publication
            .files
            .keys()
            .map(String::as_str)
            .chain(["world.pack", "publication.json"])
        {
            std::fs::OpenOptions::new()
                .write(true)
                .open(folder.join(name))
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
        doc["dat_overlay"]["path"] = value(folder.join("overlay").to_string_lossy().as_ref());
        // Each publish is the revision after the last, so the server patches every Dereth
        // player to it, the local one too.
        doc["dat_overlay"]["patching"] = value(true);
        crate::hosts::validate_config(&definition.program, dir, &doc.to_string())?;
        Ok(publication)
    })();
    match &built {
        Ok(_) => {
            prune(dir);
        }
        Err(_) => {
            let _ = std::fs::remove_dir_all(&folder);
        }
    }
    built
}

/// Puts the stopped world in `dir` back on its era's world database with no data overlay, so it no
/// longer plays the Lyceum world it was published as and its era and game data may change again.
/// The published snapshots stay on disk. This locks the stopped host.
///
/// # Errors
/// A world that is running, or a configuration the server refuses.
pub fn unpublish(dir: &Path) -> Result<(), String> {
    let guard = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("run.lock"))
        .map_err(|e| e.to_string())?;
    guard
        .try_lock()
        .map_err(|_| "Stop the world before it stops playing the Lyceum world.")?;
    let definition = host_runtime::read_definition(dir)?;
    let config = std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?;
    let doc = config.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    if let Some(why) = content_overlay_refusal(&doc, "Going back to the standard world") {
        return Err(why);
    }
    crate::hosts::validate_config(&definition.program, dir, &unpublished(&config, dir)?)
}

/// The configuration `config` of the world in `dir` with its publication taken away: its era's
/// world database, no data overlay, and none of the published world database's own inputs.
fn unpublished(config: &str, dir: &Path) -> Result<String, String> {
    let mut doc = config.parse::<DocumentMut>().map_err(|e| e.to_string())?;
    let era = host_config::Settings::from_config(config)?.era;
    let published = dir.join("published");
    let inside = |path: &str| {
        let path = path.strip_prefix("json:").unwrap_or(path);
        resolved(dir, path).starts_with(&published)
    };
    if !doc.contains_key("server") {
        doc["server"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    let server = doc["server"]
        .as_table_like_mut()
        .ok_or("server must be a table.")?;
    server.insert("world_pack_path", value(host_config::era_pack(&era)));
    if server
        .get("world_base_sql")
        .and_then(toml_edit::Item::as_str)
        .is_some_and(inside)
    {
        server.remove("world_base_sql");
    }
    if let Some(patches) = server
        .get_mut("world_base_patches")
        .and_then(toml_edit::Item::as_array_mut)
    {
        patches.retain(|p| !p.as_str().is_some_and(inside));
        if patches.is_empty() {
            server.remove("world_base_patches");
        }
    }
    if let Some(overlay) = doc
        .get_mut("dat_overlay")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        overlay.remove("path");
        overlay.remove("patching");
        if overlay.is_empty() {
            doc.remove("dat_overlay");
        }
    }
    Ok(doc.to_string())
}

/// Keep `from` at `to` as a second name for the same file where the file system allows, else as
/// a copy: a snapshot holds its world database dump without another copy of its bytes.
fn keep(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::hard_link(from, to)
        .or_else(|_| std::fs::copy(from, to).map(|_| ()))
        .map_err(|e| e.to_string())
}

fn superseded_patch(path: &Path, previous: Option<&Path>) -> Result<bool, String> {
    let Some(previous) = previous else {
        return Ok(false);
    };
    Ok(path.canonicalize().map_err(|e| e.to_string())?
        == previous.canonicalize().map_err(|e| e.to_string())?)
}

fn player_worlds(root: &Path) -> Result<BTreeMap<String, String>, String> {
    match std::fs::read(root.join("host-worlds.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("The list of the worlds you host is damaged: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn register_player(root: &Path, id: &str, slug: &str) -> Result<(), String> {
    let mut worlds = player_worlds(root)?;
    worlds.insert(slug.into(), id.into());
    host_runtime::write_json(&root.join("host-worlds.json"), &worlds)
}

pub fn forget_player(root: &Path, slug: &str) -> Result<(), String> {
    let mut worlds = player_worlds(root)?;
    if worlds.remove(slug).is_some() {
        host_runtime::write_json(&root.join("host-worlds.json"), &worlds)?;
    }
    Ok(())
}

/// Forget every world in the list that is the server `id`, as the server is deleted.
pub fn forget_host(root: &Path, id: &str) -> Result<(), String> {
    let mut worlds = player_worlds(root)?;
    let before = worlds.len();
    worlds.retain(|_, host| host != id);
    if worlds.len() != before {
        host_runtime::write_json(&root.join("host-worlds.json"), &worlds)?;
    }
    Ok(())
}

fn verify_files(folder: &Path, publication: &Publication) -> Result<(), String> {
    let build = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&publication.files).map_err(|e| e.to_string())?)
    );
    if build != publication.build {
        return Err("The published manifest is damaged.".into());
    }
    for (name, expected) in &publication.files {
        let path = Path::new(name);
        if path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            return Err("Invalid published content path.".into());
        }
        let file = folder.join(path);
        if !file
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(folder.canonicalize().map_err(|e| e.to_string())?)
            || digest(&file)? != *expected
        {
            return Err("The published snapshot changed. Publish it again before playing.".into());
        }
    }
    Ok(())
}

/// The folder the local Dereth client keeps what the world in `dir` patches it with, for the
/// Lyceum world `key` over the data files `dats`: one for each, so another world's overlay, or
/// one over other files, is never in it.
fn patch_folder(dir: &Path, key: &str, dats: &Path) -> PathBuf {
    let name: String = key
        .chars()
        .take(40)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut hash = Sha256::new();
    hash.update(key.as_bytes());
    hash.update([0]);
    hash.update(dats.to_string_lossy().as_bytes());
    let digest = format!("{:x}", hash.finalize());
    dir.join("client-overlays")
        .join(format!("{name}-{}", &digest[..12]))
}

#[derive(Debug)]
pub struct PlayInputs {
    /// The data files the world plays, and its era.
    pub dats: PathBuf,
    pub era: String,
    pub args: Vec<String>,
    pub host: String,
    pub port: u16,
    /// The published Lyceum world's name, which its data overlay names, when the world plays one.
    pub world_key: Option<String>,
}

impl PlayInputs {
    /// The data folders the Dereth client plays the world with, by the client's own rule
    /// ([`choices::dat_dirs_for`]): the world's own files for the kind its era is drawn from, and
    /// the player's chosen set, `modern` or `classic`, for the other.
    ///
    /// # Errors
    /// A set the era needs that the player has not chosen, in words for a person.
    pub fn client_dats(
        &self,
        modern: Option<&Path>,
        classic: Option<&Path>,
    ) -> Result<choices::DatDirs, String> {
        let world = dereth_launch::World {
            era: Some(self.era.clone()),
            ..dereth_launch::World::default()
        };
        let (modern, classic) = match choices::required_set_for(&world) {
            SetKind::Modern => (Some(self.dats.as_path()), classic),
            SetKind::Classic => (modern, Some(self.dats.as_path())),
        };
        choices::dat_dirs_for(&world, modern, classic).map_err(|e| e.to_string())
    }
}

pub fn play_inputs(root: &Path, slug: &str) -> Result<Option<PlayInputs>, String> {
    let worlds = player_worlds(root)?;
    let Some(id) = worlds.get(slug) else {
        return Ok(None);
    };
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err("Invalid world identity.".into());
    }
    let dir = root.join("hosts").join(id);
    let definition = host_runtime::read_definition(&dir)?;
    if definition.id != *id {
        return Err("The world's identity does not match its folder.".into());
    }
    if !host_runtime::active(&dir) || host_runtime::status(&dir).phase != "running" {
        return Err("Start this world and wait until it is running before playing.".into());
    }
    let settings = host_config::Settings::from_config(
        &std::fs::read_to_string(dir.join("empyrean.toml")).map_err(|e| e.to_string())?,
    )?;
    let mut args = Vec::new();
    let mut world_key = None;
    if let Some((folder, publication)) = selected(&dir)? {
        verify_files(&folder, &publication)?;
        // The world patches the client to what it publishes; the client keeps what it is sent in
        // a folder of its own, so a later publish sends only what changed.
        let patches = patch_folder(&dir, &publication.key, &settings.dats);
        std::fs::create_dir_all(&patches).map_err(|e| e.to_string())?;
        // Copies of a published overlay that earlier launchers gave the client instead.
        let _ = std::fs::remove_dir_all(dir.join("player-overlays"));
        args.extend([
            "--overlay-dat-dir".into(),
            patches.to_string_lossy().into_owned(),
        ]);
        world_key = Some(publication.key);
    }
    let host = if settings.bind == "0.0.0.0" {
        "127.0.0.1".into()
    } else {
        settings.bind
    };
    Ok(Some(PlayInputs {
        dats: settings.dats,
        era: settings.era,
        args,
        host,
        port: settings.port,
        world_key,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn host() -> (tempfile::TempDir, PathBuf) {
        host_of("eor")
    }
    /// A stopped world of `era`, its game data the temporary folder itself.
    fn host_of(era: &str) -> (tempfile::TempDir, PathBuf) {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("hosts/host-test");
        std::fs::create_dir_all(&dir).unwrap();
        let settings = host_config::Settings {
            dats: root.path().to_path_buf(),
            era: era.into(),
            ..Default::default()
        };
        std::fs::write(
            dir.join("empyrean.toml"),
            settings.update_config("").unwrap(),
        )
        .unwrap();
        host_runtime::write_json(
            &dir.join("host.json"),
            &host_runtime::Definition {
                id: "host-test".into(),
                version: "1.0.0".into(),
                program: dir.join("runtime/server"),
                wrapper: dir.join("wrapper"),
                checkout: false,
            },
        )
        .unwrap();
        (root, dir)
    }
    fn snapshot(dir: &Path) -> (PathBuf, Publication) {
        let folder = dir.join("published/snapshot-0");
        std::fs::create_dir_all(folder.join("overlay")).unwrap();
        std::fs::write(folder.join("overlay/record"), b"published").unwrap();
        std::fs::write(folder.join("world.pack"), b"pack").unwrap();
        let mut files = BTreeMap::new();
        inventory(&folder, &folder.join("overlay"), &mut files).unwrap();
        let build = format!("{:x}", Sha256::digest(serde_json::to_vec(&files).unwrap()));
        let publication = Publication {
            source: dir.join("source.toml"),
            key: "test".into(),
            build,
            files,
            ..Publication::default()
        };
        host_runtime::write_json(&folder.join("publication.json"), &publication).unwrap();
        let config = std::fs::read_to_string(dir.join("empyrean.toml")).unwrap();
        let mut doc = config.parse::<DocumentMut>().unwrap();
        doc["server"]["world_pack_path"] =
            value(folder.join("world.pack").to_string_lossy().as_ref());
        doc["dat_overlay"] = toml_edit::Item::Table(toml_edit::Table::new());
        doc["dat_overlay"]["path"] = value(folder.join("overlay").to_string_lossy().as_ref());
        std::fs::write(dir.join("empyrean.toml"), doc.to_string()).unwrap();
        (folder, publication)
    }
    #[test]
    fn a_failed_build_keeps_the_selected_world_and_configuration_and_leaves_no_snapshot() {
        let (_root, dir) = host();
        let (_, old) = snapshot(&dir);
        let config = std::fs::read(dir.join("empyrean.toml")).unwrap();
        let world = dir.join("source.toml");
        std::fs::write(&world, "key = 'new'\npacks = []\n").unwrap();
        assert!(publish(&dir, &world, &dir.join("missing-builder")).is_err());
        assert_eq!(std::fs::read(dir.join("empyrean.toml")).unwrap(), config);
        assert_eq!(selected(&dir).unwrap().unwrap().1.build, old.build);
        assert!(!dir.join("published/snapshot-1").exists());
    }
    /// A Lyceum world whose world file is gone is not published, and the reason says so plainly;
    /// when it is the one the world plays was published from, it says that. The world keeps
    /// playing its published copy.
    #[test]
    fn publishing_a_world_that_is_gone_says_so() {
        let (_root, dir) = host();
        let (_, old) = snapshot(&dir);
        assert_eq!(
            gone(&dir, &old.source).as_deref(),
            Some("The Lyceum world it was published from is gone.")
        );
        assert_eq!(
            gone(&dir, &dir.join("other/world.toml")).as_deref(),
            Some("That Lyceum world is gone.")
        );
        std::fs::write(&old.source, "key = 'test'\n").unwrap();
        assert_eq!(gone(&dir, &old.source), None);
        assert_eq!(selected(&dir).unwrap().unwrap().1.build, old.build);
    }
    #[test]
    fn a_deleted_server_is_forgotten_by_the_worlds_that_played_it() {
        let (root, _dir) = host();
        register_player(root.path(), "host-test", "custom-1").unwrap();
        register_player(root.path(), "host-other", "custom-2").unwrap();
        forget_host(root.path(), "host-test").unwrap();
        assert!(play_inputs(root.path(), "custom-1").unwrap().is_none());
        assert_eq!(
            player_worlds(root.path())
                .unwrap()
                .get("custom-2")
                .map(String::as_str),
            Some("host-other")
        );
    }
    #[test]
    fn changed_content_and_manifest_paths_are_refused() {
        let (_root, dir) = host();
        let (folder, mut publication) = snapshot(&dir);
        verify_files(&folder, &publication).unwrap();
        std::fs::write(folder.join("overlay/record"), b"changed").unwrap();
        assert!(verify_files(&folder, &publication)
            .unwrap_err()
            .contains("snapshot changed"));
        publication.files.insert("../outside".into(), "hash".into());
        publication.build = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&publication.files).unwrap())
        );
        assert!(verify_files(&folder, &publication).is_err());
    }
    fn running(root: &Path, dir: &Path) -> std::fs::File {
        register_player(root, "host-test", "custom-1").unwrap();
        let guard = std::fs::File::create(dir.join("run.lock")).unwrap();
        guard.lock().unwrap();
        host_runtime::write_json(
            &dir.join("status.json"),
            &host_runtime::Status {
                phase: "running".into(),
                ..Default::default()
            },
        )
        .unwrap();
        guard
    }
    #[test]
    fn play_gives_the_client_a_folder_of_its_own_to_be_patched_in_and_no_copy_of_the_world() {
        let (root, dir) = host();
        let (folder, _) = snapshot(&dir);
        let _guard = running(root.path(), &dir);
        let old_copy = dir.join("player-overlays/0123");
        std::fs::create_dir_all(&old_copy).unwrap();
        let inputs = play_inputs(root.path(), "custom-1").unwrap().unwrap();
        assert_eq!(inputs.dats, root.path());
        let args = inputs.args;
        assert_eq!(args[0], "--overlay-dat-dir");
        let patches = PathBuf::from(&args[1]);
        assert!(patches.is_dir(), "made for the client");
        assert_eq!(
            std::fs::read_dir(&patches).unwrap().count(),
            0,
            "nothing copied into it"
        );
        assert!(patches.starts_with(dir.join("client-overlays")));
        assert!(!patches.starts_with(&folder), "not the published snapshot");
        assert!(!dir.join("player-overlays").exists(), "earlier copies go");
        // What the client was patched with is kept for the next play.
        std::fs::write(patches.join("overlay_portal.dat"), b"patched").unwrap();
        let again = play_inputs(root.path(), "custom-1").unwrap().unwrap();
        assert_eq!(again.args, ["--overlay-dat-dir", patches.to_str().unwrap()]);
        assert!(patches.join("overlay_portal.dat").is_file());
        // Another world, or the same world over other files, has a folder of its own.
        let other = patch_folder(&dir, "another-world", root.path());
        let elsewhere = patch_folder(&dir, "test", &root.path().join("other"));
        assert_ne!(other, patches);
        assert_ne!(elsewhere, patches);
        assert_eq!(patch_folder(&dir, "test", root.path()), patches);
    }
    /// An empty snapshot folder `n`, published as the world `key`.
    fn numbered(dir: &Path, n: u32, key: &str) -> PathBuf {
        let folder = dir.join(format!("published/snapshot-{n}"));
        std::fs::create_dir_all(folder.join("overlay")).unwrap();
        host_runtime::write_json(
            &folder.join("publication.json"),
            &Publication {
                source: dir.join("source.toml"),
                key: key.into(),
                ..Publication::default()
            },
        )
        .unwrap();
        folder
    }
    /// Point the world in `dir` at the snapshot `folder`.
    fn select(dir: &Path, folder: &Path) {
        let config = std::fs::read_to_string(dir.join("empyrean.toml")).unwrap();
        let mut doc = config.parse::<DocumentMut>().unwrap();
        doc["dat_overlay"] = toml_edit::Item::Table(toml_edit::Table::new());
        doc["dat_overlay"]["path"] = value(folder.join("overlay").to_string_lossy().as_ref());
        std::fs::write(dir.join("empyrean.toml"), doc.to_string()).unwrap();
    }
    #[test]
    fn publishing_keeps_the_three_newest_snapshots_and_never_the_one_the_world_plays() {
        let (_root, dir) = host();
        let folders: Vec<PathBuf> = (0..6).map(|n| numbered(&dir, n, "test")).collect();
        // An old snapshot is the one played: it stays with the newest three.
        select(&dir, &folders[1]);
        let removed = prune(&dir);
        assert_eq!(removed, [folders[2].clone(), folders[0].clone()]);
        let left: Vec<u32> = snapshots(&dir).into_iter().map(|(n, _)| n).collect();
        assert_eq!(left, [5, 4, 3, 1]);
        // The newest is played: the rest go.
        select(&dir, &folders[5]);
        assert_eq!(prune(&dir), [folders[1].clone()]);
        let left: Vec<u32> = snapshots(&dir).into_iter().map(|(n, _)| n).collect();
        assert_eq!(left, [5, 4, 3]);
        assert_eq!(prune(&dir), Vec::<PathBuf>::new(), "three are kept");
    }
    /// Each publish of a Lyceum world on a world is numbered: the first 1, and each after it the
    /// next when it builds something else, or the same number when it builds what the last did. A
    /// publish counted before publishes were counted is the first.
    #[test]
    fn each_publish_that_builds_something_new_is_the_next_revision() {
        assert_eq!(next_revision(None, "a"), 1);
        let last = Publication {
            build: "a".into(),
            revision: 4,
            ..Publication::default()
        };
        assert_eq!(next_revision(Some(&last), "b"), 5);
        assert_eq!(next_revision(Some(&last), "a"), 4, "nothing new");
        let uncounted = Publication {
            build: "a".into(),
            ..Publication::default()
        };
        assert_eq!(next_revision(Some(&uncounted), "b"), 2);
        assert_eq!(next_revision(Some(&uncounted), "a"), 1);
        // What a publication made before these were kept reads as such.
        let old: Publication = serde_json::from_str(
            r#"{"source":"w.toml","key":"k","packs":[],"build":"a","files":{}}"#,
        )
        .unwrap();
        assert_eq!(
            (old.revision, old.sources.as_str(), old.base),
            (0, "", None)
        );
    }
    #[test]
    fn a_world_published_again_follows_its_own_newest_snapshot() {
        let (_root, dir) = host();
        assert_eq!(last_publication(&dir, "test"), None, "never published");
        let older = numbered(&dir, 3, "test");
        numbered(&dir, 2, "test");
        assert_eq!(last_publication(&dir, "test"), Some(older.clone()));
        // Another Lyceum world published since is not followed.
        numbered(&dir, 4, "another-world");
        assert_eq!(last_publication(&dir, "test"), Some(older));
        let newest = numbered(&dir, 10, "test");
        assert_eq!(last_publication(&dir, "test"), Some(newest));
        assert_eq!(snapshots(&dir)[0].0, 10, "numbered newest first");
    }
    #[test]
    fn a_world_with_a_content_overlay_neither_publishes_nor_goes_back_and_says_why() {
        let (_root, dir) = host();
        let (folder, _) = snapshot(&dir);
        let config = std::fs::read_to_string(dir.join("empyrean.toml")).unwrap();
        let mut doc = config.parse::<DocumentMut>().unwrap();
        doc["server"]["world_overlay_path"] = value("content.overlay");
        std::fs::write(dir.join("empyrean.toml"), doc.to_string()).unwrap();
        let config = std::fs::read(dir.join("empyrean.toml")).unwrap();
        let world = dir.join("source.toml");
        std::fs::write(&world, "key = 'test'\npacks = []\n").unwrap();
        let e = publish(&dir, &world, &dir.join("missing-builder")).unwrap_err();
        assert!(
            e.starts_with("Publishing would replace the world database")
                && e.contains("world_overlay_path")
                && e.contains("empyrean-import --overlay"),
            "{e}"
        );
        let e = unpublish(&dir).unwrap_err();
        assert!(
            e.starts_with("Going back to the standard world would replace")
                && e.contains("Remove world_overlay_path"),
            "{e}"
        );
        // Nothing changed: the configuration, the world it plays, and no new snapshot.
        assert_eq!(std::fs::read(dir.join("empyrean.toml")).unwrap(), config);
        assert_eq!(
            selected(&dir).unwrap().unwrap().0,
            folder.canonicalize().unwrap()
        );
        assert_eq!(snapshots(&dir).len(), 1);
        // An empty path is no content overlay.
        let mut doc = String::from_utf8(config)
            .unwrap()
            .parse::<DocumentMut>()
            .unwrap();
        doc["server"]["world_overlay_path"] = value("");
        assert_eq!(content_overlay_refusal(&doc, "Publishing"), None);
    }
    #[test]
    fn an_infiltration_world_is_played_over_its_classic_files_and_patched_in_a_folder_of_its_own() {
        let (root, dir) = host_of("infiltration");
        snapshot(&dir);
        let _guard = running(root.path(), &dir);
        let inputs = play_inputs(root.path(), "custom-1").unwrap().unwrap();
        assert_eq!(inputs.era, "infiltration");
        assert_eq!(inputs.args.len(), 2);
        assert_eq!(inputs.args[0], "--overlay-dat-dir");
        assert_eq!(
            Path::new(&inputs.args[1]),
            patch_folder(&dir, "test", root.path())
        );
        // The world's own Classic files, under the player's Modern set.
        let dirs = inputs
            .client_dats(Some(Path::new("lib/eor")), None)
            .unwrap();
        assert_eq!(dirs.dat_dir, Path::new("lib/eor"));
        assert_eq!(dirs.classic_dat_dir.as_deref(), Some(root.path()));
    }
    #[test]
    fn a_stopped_host_is_not_launched_as_an_unmanaged_world() {
        let (root, _dir) = host();
        register_player(root.path(), "host-test", "custom-1").unwrap();
        assert!(play_inputs(root.path(), "custom-1")
            .unwrap_err()
            .contains("Start this world"));
        assert!(play_inputs(root.path(), "unrelated").unwrap().is_none());
    }
    #[test]
    fn a_queued_server_update_blocks_publication_before_building() {
        let (_root, dir) = host();
        let pending = dir.join("runtime/.empyrean-update");
        std::fs::create_dir_all(&pending).unwrap();
        std::fs::write(pending.join("managed-pending.json"), b"queued").unwrap();
        assert!(publish(
            &dir,
            &dir.join("missing-world"),
            &dir.join("missing-builder")
        )
        .unwrap_err()
        .contains("waiting Empyrean update"));
        assert!(!dir.join("published").exists());
    }
    #[test]
    fn a_selected_snapshot_is_prepared_without_a_stock_pack_at_the_host_root() {
        let (_root, dir) = host();
        assert!(!prepared(&dir));
        snapshot(&dir);
        assert!(prepared(&dir));
        assert!(!dir.join("world.pack").exists());
    }
    #[test]
    fn a_world_that_stops_playing_its_lyceum_world_plays_its_eras_database_without_an_overlay() {
        let (_root, dir) = host();
        snapshot(&dir);
        let config = std::fs::read_to_string(dir.join("empyrean.toml")).unwrap();
        let mut doc = config.parse::<DocumentMut>().unwrap();
        let published = dir.join("published/snapshot-0");
        let own = dir.join("mine.sql");
        doc["server"]["world_base_sql"] =
            value(published.join("base.sql").to_string_lossy().as_ref());
        let mut patches = toml_edit::Array::new();
        patches.push(format!(
            "json:{}",
            published.join("base-patches/0").display()
        ));
        patches.push(own.to_string_lossy().as_ref());
        patches.push(published.join("sql").to_string_lossy().as_ref());
        doc["server"]["world_base_patches"] = value(patches);
        doc["dat_overlay"]["patching"] = value(false);
        doc["dat_overlay"]["records_per_minute"] = value(100);
        let text = unpublished(&doc.to_string(), &dir).unwrap();
        std::fs::write(dir.join("empyrean.toml"), &text).unwrap();
        assert!(selected(&dir).unwrap().is_none());
        assert_eq!(pack_path(&dir), dir.join("world.pack"));
        let doc = text.parse::<DocumentMut>().unwrap();
        assert!(doc["server"].get("world_base_sql").is_none());
        let kept: Vec<&str> = doc["server"]["world_base_patches"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(kept, [own.to_string_lossy().as_ref()]);
        let overlay = doc["dat_overlay"].as_table().unwrap();
        assert!(overlay.get("path").is_none() && overlay.get("patching").is_none());
        assert_eq!(overlay["records_per_minute"].as_integer(), Some(100));
        assert!(
            published.join("publication.json").is_file(),
            "the snapshot stays"
        );
    }
    #[test]
    fn a_running_world_does_not_stop_playing_its_lyceum_world() {
        let (_root, dir) = host();
        snapshot(&dir);
        let config = std::fs::read(dir.join("empyrean.toml")).unwrap();
        let guard = std::fs::File::create(dir.join("run.lock")).unwrap();
        guard.lock().unwrap();
        assert!(unpublish(&dir).unwrap_err().contains("Stop the world"));
        assert_eq!(std::fs::read(dir.join("empyrean.toml")).unwrap(), config);
    }
    #[test]
    fn a_hosted_world_plays_its_own_files_beside_the_players_set_of_the_other_kind() {
        let inputs = |dats: &str, era: &str| PlayInputs {
            dats: PathBuf::from(dats),
            era: era.into(),
            args: Vec::new(),
            host: "127.0.0.1".into(),
            port: 9000,
            world_key: None,
        };
        let (modern, classic) = (Path::new("lib/eor"), Path::new("lib/feb2005"));
        // Before Throne of Destiny: the world's Classic files, with the player's Modern set.
        let old = inputs("host/classic", "infiltration");
        let dirs = old.client_dats(Some(modern), None).unwrap();
        assert_eq!(dirs.dat_dir, modern);
        assert_eq!(
            dirs.classic_dat_dir.as_deref(),
            Some(Path::new("host/classic"))
        );
        assert!(old
            .client_dats(None, Some(classic))
            .unwrap_err()
            .contains("Modern"));
        // The end of retail: the world's own Modern files, with the player's Classic set.
        let dirs = inputs("host/modern", "eor")
            .client_dats(Some(modern), Some(classic))
            .unwrap();
        assert_eq!(dirs.dat_dir, Path::new("host/modern"));
        assert_eq!(dirs.classic_dat_dir.as_deref(), Some(classic));
    }
    #[test]
    fn a_snapshot_keeps_its_world_database_dump_without_another_copy_of_its_bytes() {
        let (_root, dir) = host();
        let dump = dir.join("dump.sql");
        std::fs::write(&dump, b"-- the world database").unwrap();
        let kept = dir.join("base.sql");
        keep(&dump, &kept).unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"-- the world database");
        // One file under two names: a change made through one name shows through the other.
        std::fs::write(&dump, b"-- changed").unwrap();
        assert_eq!(std::fs::read(&kept).unwrap(), b"-- changed");
    }

    #[test]
    fn republishing_removes_the_previous_world_patch_even_when_paths_use_different_spellings() {
        let (_root, dir) = host();
        let old = dir.join("published/old/sql");
        std::fs::create_dir_all(old.join("nested")).unwrap();
        let stored = old.join("nested/..");
        assert!(superseded_patch(&stored, Some(&old.canonicalize().unwrap())).unwrap());
        let base = dir.join("base-patches");
        std::fs::create_dir(&base).unwrap();
        assert!(!superseded_patch(&base, Some(&old)).unwrap());
    }
}
