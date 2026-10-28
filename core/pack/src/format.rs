//! Source format versions and explicit upgrades that preserve content and assigned ids.

use std::path::{Path, PathBuf};

use crate::dialect::Dialect;
use crate::error::{Error, Loc};
use crate::store::SourceStore;

/// The current world and pack source format. Unversioned files are format zero.
pub const CURRENT: u32 = 1;

/// Checks the format before interpreting the rest of a source file.
///
/// # Errors
/// Invalid TOML, a malformed version, or a format newer than this program supports.
pub fn check<D: Dialect>(path: &Path, text: &str) -> Result<u32, Error> {
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
                "source format {version} needs a newer {}; this version supports up to {CURRENT}",
                D::PRODUCT
            ),
        ));
    }
    Ok(version)
}

/// Upgrades the source headers `files` (a world file and its packs' headers, each read and
/// checked by the caller first), keeping backups, and backs up the world's lockfile `lock`
/// untouched. Format zero and one have identical content semantics; only the version
/// declaration changes. With `check_only`, nothing is written. Answers the files upgraded (or,
/// with `check_only`, those that would be).
///
/// # Errors
/// Invalid sources, an existing conflicting backup, or a file that cannot be written.
pub fn upgrade_files<D: Dialect>(
    store: &dyn SourceStore,
    files: &[PathBuf],
    lock: &Path,
    check_only: bool,
) -> Result<Vec<PathBuf>, Error> {
    let mut paths = files.to_vec();
    paths.sort();
    paths.dedup();
    let mut changes = Vec::new();
    for path in paths {
        let original = store
            .read_to_string(&path)
            .map_err(|e| Error::io(&path, e))?;
        if check::<D>(&path, &original)? == CURRENT {
            continue;
        }
        let mut doc = original
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| Error::source(&Loc::new(&path, 0), e.to_string()))?;
        doc["format_version"] = toml_edit::value(i64::from(CURRENT));
        let backup = path.with_extension("toml.before-upgrade");
        if store.is_file(&backup)
            && store.read(&backup).map_err(|e| Error::io(&backup, e))? != original.as_bytes()
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
    if store.is_file(lock) {
        let backup = lock.with_extension("lock.before-upgrade");
        let bytes = store.read(lock).map_err(|e| Error::io(lock, e))?;
        if store.is_file(&backup) {
            if store.read(&backup).map_err(|e| Error::io(&backup, e))? != bytes {
                return Err(Error::source(
                    &Loc::new(&backup, 0),
                    "an earlier lockfile backup differs; keep it elsewhere before upgrading",
                ));
            }
        } else {
            store
                .create_new(&backup, &bytes)
                .map_err(|e| Error::io(&backup, e))?;
        }
    }
    for (_, original, _, backup) in &changes {
        if !store.is_file(backup) {
            store
                .create_new(backup, original.as_bytes())
                .map_err(|e| Error::io(backup, e))?;
        }
    }
    for (path, _, text, _) in &changes {
        let temp = path.with_extension("toml.upgrading");
        store
            .replace(path, &temp, text.as_bytes())
            .map_err(|e| Error::io(path, e))?;
    }
    Ok(changes.into_iter().map(|(path, _, _, _)| path).collect())
}
