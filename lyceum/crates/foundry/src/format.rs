//! Source format versions and explicit upgrades that preserve content and assigned ids.

use crate::{Error, Loc};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The current world and pack source format. Unversioned files are format zero.
pub const CURRENT: u32 = 1;

/// Checks the format before interpreting the rest of a source file.
///
/// # Errors
/// Invalid TOML, a malformed version, or a format newer than this program supports.
pub fn check(path: &Path, text: &str) -> Result<u32, Error> {
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|e| Error::source(&Loc::new(path, 0), e.to_string()))?;
    let version = match doc.get("format_version") {
        None => 0,
        Some(value) => value
            .as_integer()
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| {
                Error::source(
                    &Loc::new(path, 0),
                    "format_version must be a nonnegative integer",
                )
            })?,
    };
    if version > CURRENT {
        return Err(Error::source(
            &Loc::new(path, 0),
            format!(
            "source format {version} needs a newer Lyceum; this version supports up to {CURRENT}"),
        ));
    }
    Ok(version)
}

/// Upgrades a world's source headers, keeping backups and leaving world.lock untouched.
/// Format zero and one have identical content semantics; only the version declaration changes.
///
/// # Errors
/// Invalid sources, an existing conflicting backup, or a file that cannot be written.
pub fn upgrade(path: &Path, check_only: bool) -> Result<Vec<PathBuf>, Error> {
    let world = crate::world::World::read(path)?;
    let mut paths = vec![path.to_path_buf()];
    for pack in &world.packs {
        crate::source::read_pack(pack)?;
        paths.push(pack.join("pack.toml"));
    }
    paths.sort();
    paths.dedup();
    let mut changes = Vec::new();
    for path in paths {
        let original = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        if check(&path, &original)? == CURRENT {
            continue;
        }
        let mut doc = original
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| Error::source(&Loc::new(&path, 0), e.to_string()))?;
        doc["format_version"] = toml_edit::value(i64::from(CURRENT));
        let backup = path.with_extension("toml.before-upgrade");
        if backup.exists()
            && std::fs::read(&backup).map_err(|e| Error::io(&backup, e))? != original.as_bytes()
        {
            return Err(Error::source(
                &Loc::new(&backup, 0),
                "an earlier upgrade backup differs; keep it elsewhere before upgrading",
            ));
        }
        changes.push((path, original, doc.to_string(), backup));
    }
    if check_only || changes.is_empty() {
        return Ok(changes.into_iter().map(|(path, _, _, _)| path).collect());
    }
    // Every source is validated and every original backed up before the first replacement.
    let lock = world.lock_path();
    if lock.is_file() {
        let backup = lock.with_extension("lock.before-upgrade");
        let bytes = std::fs::read(&lock).map_err(|e| Error::io(&lock, e))?;
        if backup.exists() {
            if std::fs::read(&backup).map_err(|e| Error::io(&backup, e))? != bytes {
                return Err(Error::source(
                    &Loc::new(&backup, 0),
                    "an earlier lockfile backup differs; keep it elsewhere before upgrading",
                ));
            }
        } else {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)
                .map_err(|e| Error::io(&backup, e))?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| Error::io(&backup, e))?;
        }
    }
    for (_, original, _, backup) in &changes {
        if !backup.exists() {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(backup)
                .map_err(|e| Error::io(backup, e))?;
            file.write_all(original.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(|e| Error::io(backup, e))?;
        }
    }
    for (path, _, text, _) in &changes {
        let temp = path.with_extension("toml.upgrading");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| Error::io(&temp, e))?;
        file.write_all(text.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| Error::io(&temp, e))?;
        drop(file);
        std::fs::rename(&temp, path).map_err(|e| Error::io(path, e))?;
    }
    Ok(changes.into_iter().map(|(path, _, _, _)| path).collect())
}
