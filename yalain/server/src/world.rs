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
use yalain_world_db::compile::{compile, Tables};
use yalain_world_db::install::DatInstall;
use yalain_world_db::release::{self, PINNED_TAG, PINNED_ZIP_SHA256};
use yalain_world_db::{check, layer, merge, report, rows, Files};

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
    let template = from
        .or_else(|| {
            std::env::var("YALAIN_WORLD_URL")
                .ok()
                .filter(|v| !v.is_empty())
        })
        .unwrap_or_else(|| release::DEFAULT_SOURCE.to_string());
    let dir = match dir {
        Some(d) => d,
        None => cache_dir()?,
    };
    let dest = dir.join(&tag);
    if dest.join("world.toml").is_file() {
        println!("{tag} is already cached: {}", dest.display());
        return Ok(());
    }
    let src = source(&template, &tag);
    let expected = match (sha, tag == PINNED_TAG) {
        (Some(s), _) => s,
        (None, true) => PINNED_ZIP_SHA256.to_string(),
        (None, false) => {
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
            release::zip_name(&tag)
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
    let partial = dir.join(format!("{tag}.partial"));
    std::fs::remove_dir_all(&partial).ok();
    let written = (|| -> std::io::Result<()> {
        for (k, v) in &files {
            let p = partial.join(k);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&p, v)?;
        }
        std::fs::rename(&partial, &dest)
    })();
    if let Err(e) = written {
        std::fs::remove_dir_all(&partial).ok();
        return Err(format!("unpacking into {}: {e}", dest.display()));
    }
    println!("cached {} files in {}", files.len(), dest.display());
    Ok(())
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
    let tables = compile(&merged, &install).map_err(|p| {
        format!(
            "{}\n{} problems compiling {}",
            report(&p),
            p.len(),
            db_dir.display()
        )
    })?;
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
    let dir = database_dir(db.as_deref().unwrap_or("pinned"))?;
    let portal = data::Portal::open(&dats)?;
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
