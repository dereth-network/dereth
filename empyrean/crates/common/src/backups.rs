//! Not ACE: the backups the server keeps of a data file before it upgrades or replaces it.
//!
//! A backup is a sibling of the file it copies, named
//! `<file name>.backup-<label>-<UTC timestamp>` (`shard.db.backup-shard-v1-v2-20260928T141503Z`),
//! or `<file name>.backup-<UTC timestamp>` without a label (`world.pack.backup-20260928T141503Z`).
//! The timestamp is the UTC time the backup was made, to the second, so names sort by age; two
//! backups made in the same second get `.2`, `.3`, ... after the timestamp.
//!
//! Only the newest [`KEEP_BACKUPS`] backups of a file are kept: [`prune`] removes the older ones
//! once a new backup has done its job.

use std::path::{Path, PathBuf};

use crate::dotnet::datetime::DotNetDateTime;

/// How many backups of one file are kept (the newest ones).
pub const KEEP_BACKUPS: usize = 3;

/// What follows a file's name in its backups' names.
const MARKER: &str = ".backup-";

/// `t` (UTC) as a compact timestamp: `YYYYMMDDTHHMMSSZ`.
#[must_use]
pub fn utc_timestamp(t: DotNetDateTime) -> String {
    format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        t.year(),
        t.month(),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

fn file_name(file: &Path) -> String {
    file.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Where a new backup of `file` goes: in `dir` (the file's own folder when `None`), named for
/// `label` and `now` (UTC), and not the name of anything already there.
#[must_use]
pub fn backup_path(
    file: &Path,
    dir: Option<&Path>,
    label: Option<&str>,
    now: DotNetDateTime,
) -> PathBuf {
    let dir = dir.map_or_else(
        || file.parent().map(Path::to_path_buf).unwrap_or_default(),
        Path::to_path_buf,
    );
    let mut base = format!("{}{MARKER}", file_name(file));
    if let Some(label) = label.filter(|l| !l.is_empty()) {
        base.push_str(label);
        base.push('-');
    }
    base.push_str(&utc_timestamp(now));
    let mut candidate = dir.join(&base);
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base}.{n}"));
        n += 1;
    }
    candidate
}

/// The sort key of a backup's name: its timestamp (and same-second counter), or `None` when the
/// name is not a backup of `name`.
fn age_key(name: &str, backup: &str) -> Option<(String, u32)> {
    let rest = backup.strip_prefix(name)?.strip_prefix(MARKER)?;
    let last = rest.rsplit('-').next()?;
    let (stamp, counter) = match last.split_once('.') {
        Some((s, c)) => (s, c.parse().ok()?),
        None => (last, 1),
    };
    let ok = stamp.len() == 16
        && stamp.as_bytes()[8] == b'T'
        && stamp.ends_with('Z')
        && stamp[..8].bytes().all(|b| b.is_ascii_digit())
        && stamp[9..15].bytes().all(|b| b.is_ascii_digit());
    ok.then(|| (stamp.to_owned(), counter))
}

/// The backups of `file` in `dir` (the file's own folder when `None`), oldest first.
#[must_use]
pub fn list(file: &Path, dir: Option<&Path>) -> Vec<PathBuf> {
    let dir = dir.map_or_else(
        || file.parent().map(Path::to_path_buf).unwrap_or_default(),
        Path::to_path_buf,
    );
    let name = file_name(file);
    let read_dir = if dir.as_os_str().is_empty() {
        std::fs::read_dir(".")
    } else {
        std::fs::read_dir(&dir)
    };
    let Ok(entries) = read_dir else {
        return Vec::new();
    };
    let mut found: Vec<((String, u32), PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            age_key(&name, &n).map(|k| (k, dir.join(n)))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

/// The newest backup of `file` in `dir` (the file's own folder when `None`).
#[must_use]
pub fn newest(file: &Path, dir: Option<&Path>) -> Option<PathBuf> {
    list(file, dir).pop()
}

/// Removes all but the newest `keep` backups of `file` in `dir` (the file's own folder when
/// `None`). Returns the backups removed; one that cannot be removed is logged and left.
pub fn prune(file: &Path, dir: Option<&Path>, keep: usize) -> Vec<PathBuf> {
    let all = list(file, dir);
    let excess = all.len().saturating_sub(keep);
    let mut removed = Vec::new();
    for old in all.into_iter().take(excess) {
        match std::fs::remove_file(&old) {
            Ok(()) => removed.push(old),
            Err(e) => log::warn!("Could not remove the old backup {}: {e}", old.display()),
        }
    }
    removed
}
