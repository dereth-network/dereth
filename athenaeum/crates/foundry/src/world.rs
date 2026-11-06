//! A world's files and its base, read from disk.
//!
//! A world names its base in its world file (`[base] db = "yalain-world"`, with a `version` or
//! without one for the release this build pins). The base is read from the server's per-user
//! cache, where `yalain world fetch` puts each release, unless a folder is given for it (a
//! checkout of the database, say). A world that names no base is a database of its own and lies
//! over nothing.

use std::path::{Path, PathBuf};

use yalain_world_db::layer::Base;
use yalain_world_db::release::{cache_dir_for, Host, PINNED_TAG};
use yalain_world_db::rows::{read_partly, Database};
use yalain_world_db::{layer, report, Files, Problem};

/// The world database a new world lies over.
pub const DATABASE: &str = "yalain-world";

/// A world's (or a database's) text files, read from its folder: the world file, the lockfile and
/// every TOML file of its packs. Hidden folders (a repository's) and the build's output folder
/// are left out.
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
                if d == dir && name == "build" {
                    continue;
                }
                stack.push(p);
                continue;
            }
            let rel = p
                .strip_prefix(dir)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            if rel.ends_with(".toml") || rel == "world.lock" {
                let text =
                    std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                out.insert(rel, text);
            }
        }
    }
    Ok(out)
}

/// The cache folder of a release of the database.
#[must_use]
pub fn release_dir(tag: &str) -> Option<PathBuf> {
    cache_dir_for(Host::current(), |n| std::env::var_os(n)).map(|c| c.join(tag))
}

/// The folder of the base a world names: `given` when there is one, else the release it names
/// (or the one this build pins) in the cache.
///
/// # Errors
/// No cache folder, or the release not fetched yet.
pub fn base_dir(version: Option<&str>, given: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(d) = given {
        return Ok(d.to_path_buf());
    }
    let tag = version.unwrap_or(PINNED_TAG);
    if yalain_world_db::release::parse_tag(tag).as_deref() != Some(tag) {
        return Err(format!("{tag} is not a release of the world database"));
    }
    let dir =
        release_dir(tag).ok_or("no per-user cache folder: set LOCALAPPDATA (Windows) or HOME")?;
    if dir.join("world.toml").is_file() {
        Ok(dir)
    } else {
        Err(format!(
            "the world database {tag} is not in {}: fetch it with `yalain world fetch{}`, or give its folder with --base",
            dir.display(),
            if tag == PINNED_TAG {
                String::new()
            } else {
                format!(" --version {tag}")
            }
        ))
    }
}

/// A world read: its folder and files, and the base it lies over.
#[derive(Debug, Clone)]
pub struct Opened {
    pub dir: PathBuf,
    pub files: Files,
    /// The base's folder; `None` for a world that names none.
    pub base_dir: Option<PathBuf>,
    pub base: Base,
}

impl Opened {
    /// The world as far as it reads over its base, and every problem of its own.
    #[must_use]
    pub fn read(&self) -> (Option<Database>, Vec<Problem>) {
        layer::read(&self.base, &self.files)
    }

    /// Every problem the world has over its base: its schema, keys, references, provenance and
    /// merge.
    #[must_use]
    pub fn check(&self) -> Vec<Problem> {
        layer::check(&self.base, &self.files)
    }
}

/// Read the world in `dir` and its base (`base` in place of the one it names, when given).
///
/// # Errors
/// The world's folder or its base does not read, or the base has problems of its own.
pub fn open(dir: &Path, base: Option<&Path>) -> Result<Opened, String> {
    let files = load(dir)?;
    if !files.contains_key("world.toml") {
        return Err(format!("{}: no world.toml", dir.display()));
    }
    let named = read_partly(&files).0.and_then(|db| db.world.base);
    let (base_dir, base) = match &named {
        None => (None, Base::default()),
        Some(b) => {
            let d = base_dir(b.version.as_deref(), base)?;
            let base_files = load(&d)?;
            let base = Base::new(&base_files)
                .map_err(|p| format!("{}\nthe base {} has problems", report(&p), d.display()))?;
            (Some(d), base)
        }
    };
    Ok(Opened {
        dir: dir.to_path_buf(),
        files,
        base_dir,
        base,
    })
}
