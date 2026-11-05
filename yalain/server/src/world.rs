//! `yalain world`: the world database's commands, and the compile `--world-db` runs at start.
//!
//! ```text
//! yalain world check <folder>
//!   reads a database and checks it without an install: the schema, keys, references, the
//!   merge of its packs and every row's provenance
//!
//! yalain world release <folder> --tag vX.Y.Z [--since vA.B.C] [--out <folder>]
//!   checks it and writes its release archive, yalain-world-<tag>.zip, the archive's SHA-256
//!   beside it (<archive>.sha256) and, from the folder's CHANGES.md, the release notes
//!   (yalain-world-<tag>.notes.md): the tag's section, or with --since every section after that
//!   release; the same files always make the same bytes
//!
//! yalain world fetch [--version vX.Y.Z [--sha256 HEX]] [--from <address>] [--dir <folder>]
//!   downloads a release (by default the one this build pins), checks its SHA-256 and unpacks
//!   it into the per-user cache (or --dir), one folder per tag; the address is a URL, a
//!   file:// URL, a file or a folder holding the archive, with {tag} for the tag (default: the
//!   YALAIN_WORLD_URL variable, else the database's release downloads)
//!
//! yalain world compile --dats <install> [--world-db <folder> | pinned] [--world-base <folder>]
//!                      --out <folder>
//!   compiles a database against the install into the tables the server reads, written as text
//!   for reading and comparing: content.tsv, spawns.tsv, loot.tsv, stores.tsv, quests.tsv,
//!   talk.tsv
//! ```
//!
//! A world that names a base database (`[base]`, as a world builder's worlds do) is compiled over
//! it: the release its `version` names (else the one this build pins) from the per-user cache, or
//! the folder `--world-base` gives. Its packs replace and remove the base's rows by key.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::data;
use auberean_dat::overlay::{WorldOverlay, WORLD_FOLDER};
use yalain_world_db::compile::{compile, Tables};
use yalain_world_db::install::DatInstall;
use yalain_world_db::release::{self, PINNED_TAG, PINNED_ZIP_SHA256};
use yalain_world_db::{check, layer, merge, report, rows, Files, Problem};

/// Run `yalain world <command> ...`.
///
/// # Errors
/// What went wrong, for a person to act on.
pub fn main(args: &[String]) -> Result<(), String> {
    let (cmd, rest) = args
        .split_first()
        .ok_or("world takes check, release, fetch or compile")?;
    match cmd.as_str() {
        "check" => check_cmd(rest),
        "release" => release_cmd(rest),
        "fetch" => fetch_cmd(rest),
        "compile" => compile_cmd(rest),
        other => Err(format!(
            "world {other}: not a command (check, release, fetch, compile)"
        )),
    }
}

/// A database's text files, read from its folder: the world file, the lockfile, the licence and
/// readme, and every TOML file of its packs (hidden folders, such as a repository's, left out).
///
/// # Errors
/// The folder or a file does not read, or a file is not UTF-8.
pub fn load(dir: &Path) -> Result<Files, String> {
    let mut out = Files::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let rd = std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let rel = p
                .strip_prefix(dir)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let wanted = rel.ends_with(".toml")
                || matches!(rel.as_str(), "world.lock" | "LICENSE" | "README.md");
            if wanted {
                let text =
                    std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                out.insert(rel, text);
            }
        }
    }
    Ok(out)
}

/// The database folder an argument names: a folder, or `pinned` for this build's release in the
/// cache.
///
/// # Errors
/// `pinned` with no cache folder, or the pinned release not fetched yet.
pub fn database_dir(arg: &str) -> Result<PathBuf, String> {
    if arg != "pinned" {
        return Ok(PathBuf::from(arg));
    }
    let dir = cache_dir()?.join(PINNED_TAG);
    if dir.join("world.toml").is_file() {
        Ok(dir)
    } else {
        Err(format!(
            "the world database {PINNED_TAG} is not in {}: run `yalain world fetch` to download it, or give --world-db <folder> to serve a checkout",
            dir.display()
        ))
    }
}

/// The overlay of the world a `--world-db` argument names: the folder `given` (`--dat-overlay`), else
/// `<folder>/overlay`, made for it by `athenaeum build`. A release of the database has none of its
/// own, and neither has a world that changes no record.
///
/// # Errors
/// The folder holds what isn't an overlay, or the overlay of another world.
pub fn overlay_of(world_db: &str, given: Option<&Path>) -> Result<Option<WorldOverlay>, String> {
    let dir = Path::new(world_db);
    let folder = || {
        given
            .map_or_else(|| dir.join(WORLD_FOLDER), Path::to_path_buf)
            .display()
            .to_string()
    };
    let overlay = match given {
        Some(g) => WorldOverlay::open(g).map(Some),
        None if world_db == "pinned" => return Ok(None),
        None => WorldOverlay::of_world(dir),
    }
    .map_err(|e| format!("{}: {e}", folder()))?;
    let Some(overlay) = overlay else {
        return Ok(None);
    };
    if world_db == "pinned" {
        return Ok(Some(overlay));
    }
    if let Some(db) = rows::read_partly(&load(dir)?).0 {
        if db.world.key != overlay.world() {
            return Err(format!(
                "{}: this overlay is world {}'s, and this is world {}: build it again",
                folder(),
                overlay.world(),
                db.world.key
            ));
        }
    }
    Ok(Some(overlay))
}

/// What a world's clearings come to with no overlay: what the land grows stays, and the person who
/// wrote them should know why.
#[must_use]
pub fn clearing_warning(merged: &merge::Merged, has_overlay: bool) -> Option<String> {
    let clearings = merged
        .rows
        .iter()
        .filter(|l| matches!(l.row, rows::Row::Clearing(_)))
        .count();
    (clearings > 0 && !has_overlay).then(|| {
        format!("{clearings} clearings and no overlay: `athenaeum build` makes the world's overlay")
    })
}

/// Does a world served from this argument use the release this build pins? `pinned` does; a folder
/// does when it lies over a base with no version of its own and no `--world-base` in its place. One
/// that does not read counts as using it: the server will refuse it anyway.
#[must_use]
pub fn uses_the_pin(world_db: &str, world_base: Option<&Path>) -> bool {
    if world_db == "pinned" {
        return true;
    }
    if world_base.is_some() {
        return false;
    }
    let Ok(files) = load(Path::new(world_db)) else {
        return true;
    };
    let Some(db) = rows::read_partly(&files).0 else {
        return true;
    };
    db.world.base.is_some_and(|b| b.version.is_none())
}

/// A release's folder in the cache.
///
/// # Errors
/// No cache folder, or the release not fetched yet.
pub fn release_dir(tag: &str) -> Result<PathBuf, String> {
    if release::parse_tag(tag).as_deref() != Some(tag) {
        return Err(format!("{tag} is not a release of the world database"));
    }
    let dir = cache_dir()?.join(tag);
    if dir.join("world.toml").is_file() {
        Ok(dir)
    } else {
        Err(format!(
            "the world database {tag} is not in {}: run `yalain world fetch{}` to download it, or give --world-base <folder>",
            dir.display(),
            if tag == PINNED_TAG {
                String::new()
            } else {
                format!(" --version {tag}")
            }
        ))
    }
}

fn cache_dir() -> Result<PathBuf, String> {
    release::cache_dir_for(release::Host::current(), |n| std::env::var_os(n)).ok_or_else(|| {
        "no per-user cache folder: set LOCALAPPDATA (Windows) or HOME, or pass --dir".to_string()
    })
}

fn check_cmd(args: &[String]) -> Result<(), String> {
    let [dir] = args else {
        return Err("world check takes the database's folder".into());
    };
    let files = load(Path::new(dir))?;
    let problems = check::check(&files);
    if !problems.is_empty() {
        return Err(format!(
            "{}\n{} problems",
            report(&problems),
            problems.len()
        ));
    }
    let db = rows::read(&files).map_err(|p| report(&p))?;
    let rows: usize = db.packs.iter().map(|p| p.rows.len()).sum();
    println!(
        "{}: {} packs, {rows} rows, {} files: no problems",
        db.world.key,
        db.packs.len(),
        files.len()
    );
    Ok(())
}

fn release_cmd(args: &[String]) -> Result<(), String> {
    let (mut dir, mut tag, mut since, mut out) = (None, None, None, PathBuf::from("."));
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--tag" => {
                let v = value()?;
                tag =
                    Some(release::parse_tag(&v).ok_or_else(|| {
                        format!("--tag wants a release such as v0.1.0, got {v:?}")
                    })?);
            }
            "--since" => {
                let v = value()?;
                since =
                    Some(release::parse_tag(&v).ok_or_else(|| {
                        format!("--since wants a release such as v0.1.0, got {v:?}")
                    })?);
            }
            "--out" => out = PathBuf::from(value()?),
            other if dir.is_none() && !other.starts_with("--") => dir = Some(PathBuf::from(other)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let (Some(dir), Some(tag)) = (dir, tag) else {
        return Err(
            "world release takes <folder> --tag vX.Y.Z [--since vA.B.C] [--out <folder>]".into(),
        );
    };
    let files = load(&dir)?;
    let problems = check::check(&files);
    if !problems.is_empty() {
        return Err(format!(
            "{}\nnot released: {} problems",
            report(&problems),
            problems.len()
        ));
    }
    let db = rows::read(&files).map_err(|p| report(&p))?;
    let carried = release::release_files(&files, &db);
    let zip = release::archive(&carried, &release::manifest(&tag, &db));
    let sha = release::sha256_hex(&zip);
    // The notes come from the change log; a release without its section is not made.
    let changes = dir.join("CHANGES.md");
    let notes = match std::fs::read_to_string(&changes) {
        Ok(text) => Some(release::notes(&text, &tag, since.as_deref(), &sha)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && since.is_none() => None,
        Err(e) => return Err(format!("{}: {e}", changes.display())),
    };
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join(release::zip_name(&tag));
    std::fs::write(&path, &zip).map_err(|e| format!("{}: {e}", path.display()))?;
    let side = out.join(format!("{}.sha256", release::zip_name(&tag)));
    std::fs::write(&side, format!("{sha}  {}\n", release::zip_name(&tag)))
        .map_err(|e| format!("{}: {e}", side.display()))?;
    println!(
        "{} ({} files, {} bytes)",
        path.display(),
        carried.len() + 1,
        zip.len()
    );
    println!("SHA-256 {sha}");
    match notes {
        Some(text) => {
            let p = out.join(release::notes_name(&tag));
            std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
            println!("{}", p.display());
        }
        None => println!("no CHANGES.md: no release notes written"),
    }
    Ok(())
}

/// Where a fetch reads the archive from.
fn source(template: &str, tag: &str) -> String {
    let s = release::source_url(template, tag);
    let p = Path::new(&s);
    if !s.contains("://") && p.is_dir() {
        return p.join(release::zip_name(tag)).to_string_lossy().to_string();
    }
    s
}

fn read_source(src: &str) -> Result<Vec<u8>, String> {
    if src.starts_with("http://") || src.starts_with("https://") {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(concat!("yalain/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(Duration::from_secs(600)))
            .build()
            .into();
        return agent
            .get(src)
            .call()
            .and_then(|mut r| r.body_mut().with_config().limit(1 << 30).read_to_vec())
            .map_err(|e| match e {
                ureq::Error::StatusCode(404) => {
                    format!("{src} was not found (HTTP 404): is there such a release?")
                }
                ureq::Error::StatusCode(c) => format!("{src} answered HTTP {c}"),
                other => format!("could not reach {src}: {other}"),
            });
    }
    let path = src.strip_prefix("file:///").map_or_else(
        || src.strip_prefix("file://").unwrap_or(src).to_string(),
        |p| {
            // file:///C:/x on Windows, file:///home/x elsewhere.
            if p.as_bytes().get(1) == Some(&b':') {
                p.to_string()
            } else {
                format!("/{p}")
            }
        },
    );
    std::fs::read(&path).map_err(|e| format!("{path}: {e}"))
}

/// An archive's files: one made by `world release` directly, any other zip through the zip
/// reader (a single top folder, as a repository's own archive has, is taken off).
fn unpack(zip: &[u8]) -> Result<Files, String> {
    if let Some(f) = release::read_stored(zip) {
        return Ok(f);
    }
    let mut a = zip::ZipArchive::new(std::io::Cursor::new(zip))
        .map_err(|e| format!("the download is not a zip: {e}"))?;
    let mut out = Files::new();
    for i in 0..a.len() {
        let mut e = a.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        if !e.is_file() {
            continue;
        }
        let name = e.name().replace('\\', "/");
        let mut s = String::new();
        if e.read_to_string(&mut s).is_ok() {
            out.insert(name, s);
        }
    }
    if !out.contains_key("world.toml") {
        let tops: std::collections::BTreeSet<&str> = out
            .keys()
            .filter_map(|k| k.split_once('/').map(|(t, _)| t))
            .collect();
        if let [top] = tops.into_iter().collect::<Vec<_>>()[..] {
            let prefix = format!("{top}/");
            out = out
                .into_iter()
                .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|r| (r.to_string(), v)))
                .collect();
        }
    }
    Ok(out)
}

fn fetch_cmd(args: &[String]) -> Result<(), String> {
    let (mut tag, mut sha, mut from, mut dir) = (None, None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--version" => {
                let v = value()?;
                tag = Some(release::parse_tag(&v).ok_or_else(|| {
                    format!("--version wants a release such as 0.1.0, got {v:?}")
                })?);
            }
            "--sha256" => sha = Some(value()?.to_ascii_lowercase()),
            "--from" => from = Some(value()?),
            "--dir" => dir = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let tag = tag.unwrap_or_else(|| PINNED_TAG.to_string());
    let template = from.unwrap_or_else(usual_source);
    let dir = match dir {
        Some(d) => d,
        None => cache_dir()?,
    };
    fetch_release(&tag, sha, &template, &dir)
}

/// Where a release is read from unless told otherwise: the `YALAIN_WORLD_URL` variable, else the
/// database's release downloads.
fn usual_source() -> String {
    std::env::var("YALAIN_WORLD_URL")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| release::DEFAULT_SOURCE.to_string())
}

/// The SHA-256 this build knows for the release `tag`: its pin's.
fn known_sha256(tag: &str) -> Option<String> {
    (tag == PINNED_TAG).then(|| PINNED_ZIP_SHA256.to_string())
}

/// Whether the folder `dest` holds a release: its world file, and when its SHA-256 is known, the
/// very files of the archive with that SHA-256. A release of any other version is taken on trust.
fn holds_release(dest: &Path, sha: Option<&str>) -> bool {
    dest.join("world.toml").is_file()
        && sha.is_none_or(|sha| {
            release::unpacked(dest)
                .and_then(|files| release::unpacked_sha256(&files))
                .is_some_and(|actual| actual == sha)
        })
}

/// Put the release `tag` in `dir`, one folder per tag, unless it is there: read from `template`,
/// checked against `sha` (or the pin's, or the `.sha256` beside the archive), checked as a
/// database, and moved into place whole. Nothing already in `dir` changes when any of it fails.
fn fetch_release(tag: &str, sha: Option<String>, template: &str, dir: &Path) -> Result<(), String> {
    let dest = dir.join(tag);
    let known = sha.or_else(|| known_sha256(tag));
    if holds_release(&dest, known.as_deref()) {
        println!("{tag} is already cached: {}", dest.display());
        return Ok(());
    }
    let src = source(template, tag);
    let expected = match &known {
        Some(s) => s.clone(),
        None => {
            let side = read_source(&format!("{src}.sha256")).map_err(|e| {
                format!("{e}\n  {tag} is not the pinned release: give its SHA-256 with --sha256")
            })?;
            String::from_utf8_lossy(&side)
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_lowercase()
        }
    };
    println!("fetching {src}");
    let zip = read_source(&src)?;
    let actual = release::sha256_hex(&zip);
    if actual != expected {
        return Err(format!(
            "{} does not match: expected SHA-256 {expected}, got {actual}. Nothing was cached.",
            release::zip_name(tag)
        ));
    }
    println!("SHA-256 {actual} matches");
    let files = unpack(&zip)?;
    let problems = check::check(&files);
    if !problems.is_empty() {
        return Err(format!(
            "{}\nthe release does not check; nothing was cached",
            report(&problems)
        ));
    }
    if let Some(bad) = files.keys().find(|k| !safe(k)) {
        return Err(format!(
            "the archive's entry {bad} leaves its folder; nothing was cached"
        ));
    }
    place(&files, tag, known.as_deref(), dir)
}

/// Move a release's files into `dir` whole. Worlds starting together may fetch the same release:
/// each writes a folder of its own, and the first to finish is the one kept. A folder under the
/// release's name that is not the release (`sha`, [`holds_release`]) is moved aside, not deleted:
/// `<tag>.replaced`.
fn place(files: &Files, tag: &str, sha: Option<&str>, dir: &Path) -> Result<(), String> {
    let dest = dir.join(tag);
    let partial = dir.join(format!("{tag}.partial-{}", std::process::id()));
    std::fs::remove_dir_all(&partial).ok();
    let written = (|| -> std::io::Result<()> {
        for (k, v) in files {
            let p = partial.join(k);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&p, v)?;
        }
        if !dest.exists() || holds_release(&dest, sha) {
            return std::fs::rename(&partial, &dest);
        }
        let aside = dir.join(format!("{tag}.replaced"));
        std::fs::remove_dir_all(&aside).ok();
        std::fs::rename(&dest, &aside)?;
        println!("the folder that was not {tag} is now {}", aside.display());
        std::fs::rename(&partial, &dest).inspect_err(|_| {
            std::fs::rename(&aside, &dest).ok();
        })
    })();
    if let Err(e) = written {
        std::fs::remove_dir_all(&partial).ok();
        if holds_release(&dest, sha) {
            println!("{tag} is already cached: {}", dest.display());
            return Ok(());
        }
        return Err(format!("unpacking into {}: {e}", dest.display()));
    }
    println!("cached {} files in {}", files.len(), dest.display());
    Ok(())
}

/// The release a world served from this argument reads from the cache: `pinned` reads this build's;
/// a folder reads the one its base names, else this build's, unless `--world-base` stands in for
/// it. A folder alone, or one that does not read, reads none.
fn release_it_reads(world_db: &str, world_base: Option<&Path>) -> Option<String> {
    if world_db == "pinned" {
        return Some(PINNED_TAG.to_string());
    }
    if world_base.is_some() {
        return None;
    }
    let files = load(Path::new(world_db)).ok()?;
    let base = rows::read_partly(&files).0?.world.base?;
    match base.version {
        // A version that is not a tag names no folder of the cache: the compile refuses it.
        Some(version) => release::parse_tag(&version).filter(|tag| *tag == version),
        None => Some(PINNED_TAG.to_string()),
    }
}

/// Before a world starts: fetch the release it reads from the cache, when that is not there, as
/// after a rebuild that moved this build's pin, or when a folder under this build's pin is not that
/// release. A world on a folder or a version of its own stays on it.
///
/// # Errors
/// The release could not be fetched, in words for the operator.
pub fn fetch_for_start(world_db: &str, world_base: Option<&Path>) -> Result<(), String> {
    let Some(tag) = release_it_reads(world_db, world_base) else {
        return Ok(());
    };
    fetch_missing(&tag, known_sha256(&tag), &usual_source(), &cache_dir()?)
}

/// Fetch the release `tag` into `dir` for a world about to start, unless it is there, held to `sha`
/// when known.
fn fetch_missing(tag: &str, sha: Option<String>, template: &str, dir: &Path) -> Result<(), String> {
    let dest = dir.join(tag);
    if holds_release(&dest, sha.as_deref()) {
        return Ok(());
    }
    if dest.exists() {
        println!(
            "The world database in {} is not {tag}: downloading it.",
            dest.display()
        );
    } else {
        println!("This world needs the world database {tag}: downloading it.");
    }
    fetch_release(tag, sha, template, dir).map_err(|why| {
        let version = if tag == PINNED_TAG {
            String::new()
        } else {
            format!(" --version {tag}")
        };
        format!(
            "could not download {tag}, which this world needs: {why}\n  Nothing was changed. Run `yalain world fetch{version}` when you can reach it, or give --world-db <folder>."
        )
    })
}

/// A relative path that stays inside the folder it is joined to.
fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(':')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}

/// Compile a database folder against an install.
///
/// # Errors
/// The database does not read, check or merge, the install does not open, or a row names what
/// the install lacks.
pub fn compile_dir(
    p: &data::Portal,
    db_dir: &Path,
    base_dir: Option<&Path>,
) -> Result<Tables, String> {
    let files = load(db_dir)?;
    let (read, read_problems) = rows::read_partly(&files);
    let named = read.as_ref().and_then(|db| db.world.base.clone());
    let (problems, base, checked) = match &named {
        None => match (&read, read_problems.is_empty()) {
            (Some(db), true) => {
                let (problems, merged) = check::check_read(db);
                (problems, None, merged)
            }
            _ => (read_problems.clone(), None, None),
        },
        Some(b) => {
            let dir = match base_dir {
                Some(d) => d.to_path_buf(),
                None => release_dir(b.version.as_deref().unwrap_or(PINNED_TAG))?,
            };
            let base = layer::Base::new(&load(&dir)?)
                .map_err(|p| format!("{}\nthe base {}", report(&p), dir.display()))?;
            (layer::check(&base, &files), Some(base), None)
        }
    };
    if !problems.is_empty() {
        return Err(format!(
            "{}\n{} problems in {}",
            report(&problems),
            problems.len(),
            db_dir.display()
        ));
    }
    let merged = match checked {
        Some(merged) => merged,
        None => {
            let db = match (read, read_problems) {
                (Some(db), p) if p.is_empty() => db,
                (_, p) => return Err(report(&p)),
            };
            match &base {
                None => merge::merge(&db),
                Some(base) => layer::merge_over(base, &db),
            }
            .map_err(|p| report(&p))?
        }
    };
    let mut install = DatInstall::new(
        Box::new(|id| p.dat.read(id).ok()),
        p.ctx.clone(),
        data::library(p)?,
        Vec::new(),
    )?
    .with_quest_states(Box::new(|| {
        data::saved_states(p).map_or_else(
            |_| Vec::new(),
            |states| {
                states
                    .iter()
                    .filter_map(|s| Some((s.id, s.quest?)))
                    .collect()
            },
        )
    }));
    if let Some(local) = p.english() {
        install = install.with_text(Box::new(move |id| local.read(id).ok()));
    }
    let mut tables = compile(&merged, &install).map_err(|p| {
        format!(
            "{}\n{} problems compiling {}",
            report(&p),
            p.len(),
            db_dir.display()
        )
    })?;
    if let Some(w) = clearing_warning(&merged, p.has_overlay()) {
        tables.warnings.push(Problem::new("world.toml", 0, w));
    }
    Ok(tables)
}

fn compile_cmd(args: &[String]) -> Result<(), String> {
    let (mut dats, mut db, mut out, mut base) = (None, None, None, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--dats" => dats = Some(PathBuf::from(value()?)),
            "--world-db" => db = Some(value()?),
            "--world-base" => base = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let (Some(dats), Some(out)) = (dats, out) else {
        return Err(
            "world compile takes --dats <install> [--world-db <folder> | pinned] [--world-base <folder>] --out <folder>"
                .into(),
        );
    };
    let db = db.unwrap_or_else(|| "pinned".to_owned());
    let dir = database_dir(&db)?;
    let portal = data::Portal::open(&dats, overlay_of(&db, None)?)?;
    let t = compile_dir(&portal, &dir, base.as_deref())?;
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (name, text) in [
        ("content.tsv", t.content.to_text()),
        ("spawns.tsv", t.spawns.to_text()),
        ("loot.tsv", t.loot.to_text()),
        ("stores.tsv", t.stores.to_text()),
        ("quests.tsv", t.quests.to_text()),
        ("talk.tsv", t.talk.to_text()),
    ] {
        let p = out.join(name);
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    for w in &t.warnings {
        println!("  {w}");
    }
    println!(
        "{} -> {}",
        t.counts()
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(", "),
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks the world database's fetch: its sources, archives and paths, without the network)
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("yalain-world-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&d).ok();
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn an_archive_entry_stays_inside_the_folder_it_is_unpacked_into() {
        for good in ["world.toml", "seed-2017/server/loot.toml", "a/b-c/d.toml"] {
            assert!(safe(good), "{good}");
        }
        for bad in [
            "",
            "/etc/x",
            "../x",
            "a/../../x",
            "a//b",
            "./x",
            "C:/x",
            "c:x",
            r"a\b",
        ] {
            assert!(!safe(bad), "{bad}");
        }
    }

    #[test]
    fn a_release_archive_unpacks_to_its_files() {
        let mut files = Files::new();
        files.insert("world.toml".into(), "key = \"w\"\n".into());
        files.insert("p/pack.toml".into(), "name = \"p\"\n".into());
        let zip = release::archive(&files, "{}\n");
        let out = unpack(&zip).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out["p/pack.toml"], files["p/pack.toml"]);
        assert!(unpack(b"not a zip").is_err());
    }

    #[test]
    fn a_source_is_a_file_a_file_url_or_a_folder_holding_the_archive() {
        let d = scratch("source");
        let zip = d.join(release::zip_name("v0.1.0"));
        std::fs::write(&zip, b"bytes").unwrap();
        let folder = d.to_string_lossy().replace('\\', "/");
        // A folder: the archive named for the tag inside it.
        assert_eq!(read_source(&source(&folder, "v0.1.0")).unwrap(), b"bytes");
        assert!(source(&folder, "v0.1.0").ends_with("yalain-world-v0.1.0.zip"));
        // A template with the tag put in, read as a path or a file URL.
        let template = format!("{folder}/yalain-world-{{tag}}.zip");
        assert_eq!(read_source(&source(&template, "v0.1.0")).unwrap(), b"bytes");
        let url = if folder.starts_with('/') {
            format!("file://{folder}/yalain-world-{{tag}}.zip")
        } else {
            format!("file:///{folder}/yalain-world-{{tag}}.zip")
        };
        assert_eq!(read_source(&source(&url, "v0.1.0")).unwrap(), b"bytes");
        assert!(read_source(&source(&template, "v9.9.9")).is_err());
        std::fs::remove_dir_all(&d).ok();
    }

    /// The test kit's world, copied into `dir` with a change log.
    fn fixture_world(dir: &Path) {
        let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../testkit/fixtures/world");
        for (rel, text) in load(&from).unwrap() {
            let p = dir.join(&rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        std::fs::write(
            dir.join("CHANGES.md"),
            "# Changes\n\n## v0.3.0\n\n- Doors open.\n\n## v0.2.0\n\n- Camps fill.\n\n## v0.1.0\n\n- The first release.\n",
        )
        .unwrap();
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn a_release_is_written_the_same_twice_with_its_notes_and_fetches_back_from_its_file() {
        let d = scratch("release");
        let world = d.join("world");
        fixture_world(&world);
        let w = world.to_string_lossy().to_string();
        for out in ["a", "b"] {
            let o = d.join(out).to_string_lossy().to_string();
            let args = [
                "release", &w, "--tag", "v0.3.0", "--since", "v0.1.0", "--out", &o,
            ];
            main(&strings(&args)).unwrap();
        }
        for name in [
            release::zip_name("v0.3.0"),
            format!("{}.sha256", release::zip_name("v0.3.0")),
            release::notes_name("v0.3.0"),
        ] {
            let a = std::fs::read(d.join("a").join(&name)).unwrap();
            assert_eq!(a, std::fs::read(d.join("b").join(&name)).unwrap(), "{name}");
        }
        let notes =
            std::fs::read_to_string(d.join("a").join(release::notes_name("v0.3.0"))).unwrap();
        assert!(notes.contains("- Doors open.") && notes.contains("- Camps fill."));
        assert!(!notes.contains("The first release"), "{notes}");
        // A tag the change log does not name is not released.
        let o = d.join("c").to_string_lossy().to_string();
        let e = main(&strings(&["release", &w, "--tag", "v0.4.0", "--out", &o])).unwrap_err();
        assert!(e.contains("no section for v0.4.0"), "{e}");
        assert!(!d.join("c").exists());
        // A tag other than the pinned one is checked against the .sha256 beside its archive.
        let zip = d.join("a").join(release::zip_name("v0.3.0"));
        let cache = d.join("cache").to_string_lossy().to_string();
        let args = [
            "fetch",
            "--version",
            "v0.3.0",
            "--from",
            &zip.to_string_lossy(),
            "--dir",
            &cache,
        ];
        main(&strings(&args)).unwrap();
        let cached = load(&d.join("cache").join("v0.3.0")).unwrap();
        assert_eq!(cached["world.toml"], load(&world).unwrap()["world.toml"]);
        std::fs::remove_dir_all(&d).ok();
    }

    /// A folder holding release `tag`'s archive and the `.sha256` beside it, as a source to fetch from.
    fn source_holding(d: &Path, tag: &str) -> String {
        let world = d.join("world");
        fixture_world(&world);
        let (w, o) = (world.to_string_lossy(), d.join("source"));
        let args = ["release", &w, "--tag", tag, "--out", &o.to_string_lossy()];
        main(&strings(&args)).unwrap();
        o.to_string_lossy().to_string()
    }

    /// Everything under a folder, for seeing that a failure left it as it was.
    fn tree(dir: &Path) -> std::collections::BTreeSet<PathBuf> {
        let mut out = std::collections::BTreeSet::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                if e.path().is_dir() {
                    stack.push(e.path());
                }
                out.insert(e.path());
            }
        }
        out
    }

    /// A world about to start on a release the cache lacks gets it from the source, beside the
    /// releases already there; one the cache has is not read from the source at all.
    /// Behaviour: server.world.a-missing-release-is-fetched-at-start
    #[test]
    fn a_world_starting_on_a_release_not_in_the_cache_fetches_it() {
        let d = scratch("start-fetch");
        let source = source_holding(&d, "v0.3.0");
        let cache = d.join("cache");
        std::fs::create_dir_all(cache.join("v0.2.0")).unwrap();
        std::fs::write(cache.join("v0.2.0").join("world.toml"), "old").unwrap();
        fetch_missing("v0.3.0", None, &source, &cache).unwrap();
        assert_eq!(
            load(&cache.join("v0.3.0")).unwrap()["world.toml"],
            load(&d.join("world")).unwrap()["world.toml"],
        );
        assert_eq!(
            std::fs::read_to_string(cache.join("v0.2.0").join("world.toml")).unwrap(),
            "old",
            "another release stays as it was"
        );
        let after = tree(&cache);
        assert!(
            after
                .iter()
                .all(|p| !p.to_string_lossy().contains("partial")),
            "{after:?}"
        );
        // Now in the cache, the source is not read: it could not be.
        fetch_missing("v0.3.0", None, &d.join("gone").to_string_lossy(), &cache).unwrap();
        assert_eq!(tree(&cache), after);
        std::fs::remove_dir_all(&d).ok();
    }

    /// With no source to reach, or an archive whose hash is wrong, a world about to start is told
    /// so in plain words, what to do next among them, and the cache stays exactly as it was. This
    /// build's pin is held to the hash built into it.
    /// Behaviour: server.world.a-release-not-fetched-at-start-changes-nothing
    #[test]
    fn a_fetch_at_start_that_fails_says_so_and_changes_nothing() {
        let d = scratch("start-fetch-fails");
        let cache = d.join("cache");
        std::fs::create_dir_all(cache.join("v0.2.0")).unwrap();
        std::fs::write(cache.join("v0.2.0").join("world.toml"), "old").unwrap();
        let before = tree(&cache);

        // Nothing at the source.
        let gone = d.join("gone");
        let e = fetch_missing("v0.3.0", None, &gone.to_string_lossy(), &cache).unwrap_err();
        assert!(
            e.contains("could not download v0.3.0")
                && e.contains("Nothing was changed")
                && e.contains("yalain world fetch --version v0.3.0"),
            "{e}"
        );
        assert_eq!(tree(&cache), before);

        // An archive at the source that is not the pin's.
        let mut files = Files::new();
        files.insert("world.toml".into(), "key = \"w\"\n".into());
        let source = d.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(
            source.join(release::zip_name(PINNED_TAG)),
            release::archive(&files, "{}\n"),
        )
        .unwrap();
        let pin = known_sha256(PINNED_TAG);
        let e = fetch_missing(PINNED_TAG, pin, &source.to_string_lossy(), &cache).unwrap_err();
        assert!(
            e.contains("does not match") && e.contains(PINNED_ZIP_SHA256),
            "{e}"
        );
        assert!(
            e.contains("Run `yalain world fetch` when you can reach it"),
            "{e}"
        );
        assert_eq!(tree(&cache), before);
        std::fs::remove_dir_all(&d).ok();
    }

    /// A folder under a release's name holding something else, a candidate of the same name or a
    /// release gone wrong on disk, is not trusted where the hash is known: it is fetched again, and
    /// the folder that was there stays beside it. With the fetch failing, it stays where it was
    /// and the world does not start on it. The real release is left alone, and read from no source.
    /// A world's base names a release: anything else, a way out of the cache among them, is none.
    /// Behaviour: server.world.a-folder-that-is-not-the-pin-is-fetched-again
    #[test]
    fn a_base_that_names_no_release_is_refused() {
        for tag in [
            "../v0.1.0",
            "v0.1.0/..",
            "..",
            "v1",
            "/tmp/x",
            "v0.1.0/../../x",
        ] {
            let why = release_dir(tag).expect_err(tag);
            assert!(why.contains("is not a release"), "{why}");
        }
    }

    /// Behaviour: server.world.a-folder-that-is-not-the-pin-is-fetched-again
    #[test]
    fn a_cached_folder_that_is_not_the_release_it_is_named_for_is_fetched_again() {
        let d = scratch("start-fetch-wrong");
        let source = source_holding(&d, "v0.3.0");
        let side = std::fs::read_to_string(
            Path::new(&source).join(format!("{}.sha256", release::zip_name("v0.3.0"))),
        )
        .unwrap();
        let sha = side.split_whitespace().next().unwrap().to_string();
        let cache = d.join("cache");
        std::fs::create_dir_all(cache.join("v0.3.0")).unwrap();
        std::fs::write(cache.join("v0.3.0").join("world.toml"), "a candidate").unwrap();
        let world = || std::fs::read_to_string(cache.join("v0.3.0").join("world.toml")).unwrap();

        // No source: refused, and what was there is still there.
        let gone = d.join("gone").to_string_lossy().to_string();
        let e = fetch_missing("v0.3.0", Some(sha.clone()), &gone, &cache).unwrap_err();
        assert!(e.contains("Nothing was changed"), "{e}");
        assert_eq!(world(), "a candidate");
        assert!(!cache.join("v0.3.0.replaced").exists());

        // With no hash known, the folder is taken as it is.
        fetch_missing("v0.3.0", None, &gone, &cache).unwrap();
        assert_eq!(world(), "a candidate");

        // The real release takes its place; the other is kept beside it.
        fetch_missing("v0.3.0", Some(sha.clone()), &source, &cache).unwrap();
        assert_eq!(world(), load(&d.join("world")).unwrap()["world.toml"]);
        assert_eq!(
            std::fs::read_to_string(cache.join("v0.3.0.replaced").join("world.toml")).unwrap(),
            "a candidate"
        );

        // The real release is taken as it is, its hash known, with no source to read.
        let after = tree(&cache);
        fetch_missing("v0.3.0", Some(sha.clone()), &gone, &cache).unwrap();
        assert_eq!(tree(&cache), after);

        // One file changed on disk is not the release.
        let pack = after
            .iter()
            .find(|p| {
                p.extension().is_some_and(|e| e == "toml")
                    && p.starts_with(cache.join("v0.3.0"))
                    && !p.ends_with("world.toml")
            })
            .unwrap();
        std::fs::write(pack, "changed").unwrap();
        fetch_missing("v0.3.0", Some(sha), &source, &cache).unwrap();
        assert_ne!(std::fs::read_to_string(pack).unwrap(), "changed");
        std::fs::remove_dir_all(&d).ok();
    }

    /// `pinned` follows the build's pin; a world with a version of its own keeps it, a folder of its
    /// own reads nothing from the cache, and `--world-base` stands in for the cache.
    /// Behaviour: server.world.a-world-keeps-the-release-it-names
    #[test]
    fn the_release_fetched_at_start_is_the_one_the_world_reads() {
        let d = scratch("start-fetch-which");
        let world = |base: &str| {
            std::fs::write(
                d.join("world.toml"),
                format!(
                    "format_version = 1\nkey = \"w\"\ngame = \"ac2\"\nera = \"eor\"\npacks = []\n{base}"
                ),
            )
            .unwrap();
        };
        let text = d.to_string_lossy().to_string();
        let pin = Some(PINNED_TAG.to_string());
        assert_eq!(release_it_reads("pinned", None), pin);
        assert_eq!(
            release_it_reads(&format!("{text}/nothing-here"), None),
            None
        );
        world("");
        assert_eq!(release_it_reads(&text, None), None, "a folder alone");
        world("[base]\ndb = \"db\"\n");
        assert_eq!(release_it_reads(&text, None), pin, "over no named version");
        assert_eq!(
            release_it_reads(&text, Some(Path::new("/some/base"))),
            None,
            "over a folder of its own"
        );
        world("[base]\ndb = \"db\"\nversion = \"v0.1.0\"\n");
        assert_eq!(
            release_it_reads(&text, None).as_deref(),
            Some("v0.1.0"),
            "over the release it names"
        );
        world("[base]\ndb = \"db\"\nversion = \"../elsewhere\"\n");
        assert_eq!(release_it_reads(&text, None), None, "not a release's name");
        std::fs::remove_dir_all(&d).ok();
    }

    /// Two worlds starting together may fetch one release: the one that finishes second takes the
    /// first's, rather than failing or replacing it, and neither sees the other's unfinished folder.
    /// Behaviour: server.world.a-release-fetched-twice-at-once-is-kept-whole
    #[test]
    fn a_release_that_appears_during_a_fetch_is_kept_and_the_fetch_succeeds() {
        let d = scratch("start-fetch-twice");
        let cache = d.join("cache");
        let mut files = Files::new();
        files.insert("world.toml".into(), "new".into());
        std::fs::create_dir_all(cache.join("v0.3.0")).unwrap();
        std::fs::write(cache.join("v0.3.0").join("world.toml"), "first").unwrap();
        std::fs::create_dir_all(cache.join("v0.3.0.partial-1")).unwrap();
        std::fs::write(cache.join("v0.3.0.partial-1").join("world.toml"), "other").unwrap();
        place(&files, "v0.3.0", None, &cache).unwrap();
        assert_eq!(
            std::fs::read_to_string(cache.join("v0.3.0").join("world.toml")).unwrap(),
            "first"
        );
        assert_eq!(
            std::fs::read_to_string(cache.join("v0.3.0.partial-1").join("world.toml")).unwrap(),
            "other",
            "another program's unfinished folder is left to it"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_fetch_whose_sha256_differs_caches_nothing() {
        let d = scratch("fetch");
        let mut files = Files::new();
        files.insert("world.toml".into(), "key = \"w\"\n".into());
        let zip = release::archive(&files, "{}\n");
        std::fs::write(d.join(release::zip_name("v0.9.0")), &zip).unwrap();
        let folder = d.to_string_lossy().to_string();
        let cache = d.join("cache");
        let args: Vec<String> = [
            "--version",
            "v0.9.0",
            "--sha256",
            &"0".repeat(64),
            "--from",
            &folder,
            "--dir",
            &cache.to_string_lossy(),
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        let e = fetch_cmd(&args).unwrap_err();
        assert!(
            e.contains("does not match") && e.contains("Nothing was cached"),
            "{e}"
        );
        assert!(!cache.join("v0.9.0").exists());
        std::fs::remove_dir_all(&d).ok();
    }
}
