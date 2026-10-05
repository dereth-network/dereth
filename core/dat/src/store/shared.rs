//! One open container per file per process.
//!
//! [`DatFile::open`] walks the whole B-tree into a `BTreeMap` — 887,455 entries across the four
//! retail files, read out of 1.4 GB — so opening a directory is tens of milliseconds of disk and
//! tens of megabytes of map. Nothing in this rebuild wants that per *reader*: it wants it per
//! *file*. The distinction is expensive because the `dat` test tiers open a store per test (about
//! forty in one `dereth-testkit` batch, one per `App` everywhere else): walked per reader, one
//! tier run measured 73 GB of reads, and four concurrent copies of a run that takes half an hour
//! alone took seven and a half hours of wall clock.
//!
//! So [`open`] keeps a table of the containers this process has already walked and hands out
//! [`Arc`] clones of them. It is a cache of *work*, not of *contents*, and the two rules below are
//! what keep it from becoming the second kind.
//!
//! ## What may be reused
//!
//! An entry is reused only while the file on disk still has the length and mtime it had when the
//! walk ran, checked on both sides of the walk so that a file edited *during* it is never
//! entered. That is the rule `dereth_client_shell::App::invalidate_after_ddd` depends on: it reopens the
//! store precisely because a patch landed, and a patch changes at least one of the two.
//! [`forget`] closes the remaining gap — a writer that rewrote a record in place inside one
//! filesystem timestamp tick — by dropping the entry as soon as [`crate::write::DatWriter`] opens
//! the file at all, so the answer never depends on a clock's resolution.
//!
//! ## What is kept alive
//!
//! The table holds a [`Weak`] to every file and a strong [`Arc`] only for files inside a
//! directory declared read-only with [`crate::protect_install`]. A pristine install is the one
//! directory worth pinning — it is what a test run reopens, `DatWriter::refuse_owner_dat` refuses
//! to write to it, and nothing deletes it — and
//! pinning nothing else means a temporary directory a test built is released with the last store
//! over it, file handle and all. (On Windows an open handle would
//! otherwise block the test's own cleanup.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::SystemTime;

use crate::container::DatFile;
use crate::error::DatError;

/// What the file looked like when the walk ran. Two fields because either alone is weak: a patch
/// that replaces one record with another of the same size leaves the length alone, and a copy
/// restored from a backup can carry an older mtime at a new length.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

fn stamp(path: &Path) -> Option<Stamp> {
    let md = std::fs::metadata(path).ok()?;
    Some(Stamp {
        len: md.len(),
        modified: md.modified().ok(),
    })
}

struct Entry {
    stamp: Stamp,
    file: Weak<DatFile>,
    /// The keep-alive: set only for a file in a read-only install, never read, and named with the
    /// underscore because holding it *is* what it does. See the module note.
    _pin: Option<Arc<DatFile>>,
}

type Table = HashMap<PathBuf, Entry>;

fn table() -> MutexGuard<'static, Table> {
    static TABLE: OnceLock<Mutex<Table>> = OnceLock::new();
    TABLE
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// The table's key. Canonical so that `<install>/client_portal.dat` and the same file reached
/// through a different spelling are one entry; the caller's own spelling is never replaced by this
/// one, because `RetailDatStore::client_dir` and every error message must keep the words the
/// caller used.
fn key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Whether `key` is inside a read-only install. The same question, asked the same way, as
/// [`crate::write::DatWriter`]'s `refuse_owner_dat` — which is what makes "a protected install is
/// read-only in this process" a checked fact rather than an assumption.
fn is_retail_install(key: &Path) -> bool {
    crate::locate::is_protected(key)
}

/// The container at `path`, walked once per process for as long as the file does not move.
///
/// # Errors
///
/// Whatever [`DatFile::open`] returns, unchanged: a miss here is an ordinary open.
pub(crate) fn open(path: &Path) -> Result<Arc<DatFile>, DatError> {
    let key = key(path);
    let before = stamp(&key);
    if let Some(before) = before {
        if let Some(hit) = table()
            .get(&key)
            .and_then(|e| (e.stamp == before).then(|| e.file.upgrade()).flatten())
        {
            return Ok(hit);
        }
    }

    // Walked outside the lock: this is tens of milliseconds of disk, and a second directory must
    // not queue behind it. Two openers racing on one path both walk it and the loser's container
    // simply lives until its store is dropped.
    let file = Arc::new(DatFile::open(path)?);

    // Only a file that did not move across its own walk goes in the table.
    if let Some(before) = before {
        if stamp(&key) == Some(before) {
            let _pin = is_retail_install(&key).then(|| Arc::clone(&file));
            table().insert(
                key,
                Entry {
                    stamp: before,
                    file: Arc::downgrade(&file),
                    _pin,
                },
            );
        }
    }
    Ok(file)
}

/// Make `file` what the next [`open`] of `path` answers with. [`super::RetailDatStore::reload`]'s
/// half of the invalidation: the store that reloaded has the patched file, and a store opened
/// afterwards must not be handed the pre-patch walk.
pub(crate) fn replace(path: &Path, file: &Arc<DatFile>) {
    let key = key(path);
    match stamp(&key) {
        Some(now) => {
            let _pin = is_retail_install(&key).then(|| Arc::clone(file));
            table().insert(
                key,
                Entry {
                    stamp: now,
                    file: Arc::downgrade(file),
                    _pin,
                },
            );
        }
        // The file is gone; there is nothing an entry could promise about it.
        None => drop(table().remove(&key)),
    }
}

/// Drop whatever `path` maps to, so the next [`open`] walks it again.
///
/// [`crate::write::DatWriter`] calls this when it opens a container for writing, which makes the
/// staleness question independent of the filesystem's timestamp resolution: a writer that rewrote
/// a record in place, in the same tick and at the same length, would otherwise be invisible to
/// [`open`]'s stamp check.
pub(crate) fn forget(path: &Path) {
    drop(table().remove(&key(path)));
}
