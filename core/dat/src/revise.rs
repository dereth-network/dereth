//! A world's overlay published again: the next revision of the overlay a server already serves.
//!
//! A server patches a client by revision: it sends the records of each revision the client lacks
//! and makes that revision's deletions. An overlay built afresh puts everything in one revision
//! past its base ([`crate::decompose::revision_over`]), so a client patched to an earlier build
//! holds as many revisions as the new one and is told it is current. [`revise`] numbers the new
//! overlay as the revision after the last one instead:
//!
//! - a record that reads as it did keeps its revision (one the last overlay held is held again,
//!   as it was);
//! - a record that reads otherwise, or is new, goes into the next revision;
//! - a record the last overlay read and the new one does not is deleted in the next revision, as
//!   a deletion of its id alone;
//! - the last overlay's deletions stay, in their revisions, so a client patched to an older
//!   revision still makes them.
//!
//! A client patched to the last overlay is then sent the next revision alone, and a client with
//! none every revision; either reads as the new overlay does. An overlay that carries its world's
//! own iteration list ([`crate::decompose::Iterations::World`]) has no revisions to follow and is
//! refused.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_primitives::DataId;

use crate::container::DatFile;
use crate::divine::ITERATION_LIST;
use crate::error::DatError;
use crate::locate::ModernDat;
use crate::overlay::{self, Layer, OverlayDir, OverlayError, OverlayWriter};

/// Why an overlay was not revised.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReviseError {
    #[error(transparent)]
    Overlay(#[from] OverlayError),
    #[error(transparent)]
    Dat(#[from] DatError),
    /// The overlay numbers its iterations as its world does, not as revisions over its base.
    #[error("{} carries its world's own iterations, not revisions over its base", .0.display())]
    OwnIterations(PathBuf),
}

/// What [`revise`] wrote of one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Revised {
    /// The revision the changes went into; `None` when the file reads as it did.
    pub revision: Option<u32>,
    /// The overlay's revisions, ascending; empty when no overlay was written (the file is its
    /// base, and was).
    pub revisions: Vec<u32>,
    /// Records written into the new revision.
    pub changed: usize,
    /// Records deleted in it.
    pub deleted: usize,
    /// Records kept in the revisions they had.
    pub kept: usize,
}

/// One record of the overlay being written.
struct Put {
    bytes: Vec<u8>,
    version: u16,
    iteration: u32,
    date: u32,
}

/// The version an entry records; a file from before Throne of Destiny records none, and the
/// writer needs one.
fn version_of(bits: u32) -> u16 {
    u16::try_from(bits >> 16).unwrap_or(1).max(1)
}

/// Record `id` as the overlay `layer` over `base` reads it: the overlay's own, none when one of
/// its deletions covers it, else the base's.
fn view(base: &DatFile, layer: Option<&Layer>, id: DataId) -> Result<Option<Vec<u8>>, DatError> {
    if let Some(l) = layer {
        if l.record(id).is_some() {
            return l.read(id).map(Some);
        }
        if l.hides(id) {
            return Ok(None);
        }
    }
    if base.base_entry(id).is_none() {
        return Ok(None);
    }
    base.read(id).map(Some)
}

/// Every id the overlays may read otherwise than `base`: the records they hold, the ids they delete
/// alone, and the base's records their family deletions cover.
fn touched(base: &DatFile, layers: &[&Layer]) -> BTreeSet<u32> {
    let mut ids = BTreeSet::new();
    for l in layers {
        ids.extend(l.records().map(|(id, _)| id.raw()));
        ids.extend(l.tombstones().iter().filter(|t| t.mask == 0).map(|t| t.id));
    }
    if layers
        .iter()
        .any(|l| l.tombstones().iter().any(|t| t.mask != 0))
    {
        for (id, _) in base.base_entries() {
            if layers.iter().any(|l| l.hides(id)) {
                ids.insert(id.raw());
            }
        }
    }
    ids.retain(|id| {
        let id = DataId(*id);
        id != ITERATION_LIST && !overlay::is_reserved(id)
    });
    ids
}

/// Write at `path` the overlay over `base` that reads as `fresh` does, numbered as the revision
/// after `last`, the world's last overlay over the same base (as the module says). `None` for
/// `last` is a world never published, whose overlay is one revision past the base; `None` for
/// `fresh` is a world that now changes nothing in this file. `base_name` is the base file's name
/// as the manifest records it. A file that is its base, and was, gets no overlay; any other gets
/// one, with every revision it has had, even when nothing in it changed.
///
/// # Errors
/// [`ReviseError::OwnIterations`] for an overlay carrying its world's own iterations, and the
/// reader's and the writer's errors.
pub fn revise(
    base: &DatFile,
    last: Option<&Layer>,
    fresh: Option<&Layer>,
    path: &Path,
    base_name: &str,
    world_key: &str,
) -> Result<Revised, ReviseError> {
    let base = base.base();
    for l in [last, fresh].into_iter().flatten() {
        if l.manifest().exact_iterations {
            return Err(ReviseError::OwnIterations(l.path().to_path_buf()));
        }
    }
    let mut revisions: Vec<u32> = last
        .map(|l| l.own_iterations().to_vec())
        .unwrap_or_default();
    let first = crate::decompose::revision_over(&base);
    let next = revisions.last().map_or(first, |r| (r + 1).max(first));
    let mut tombstones: BTreeMap<(u32, u32), u32> = last
        .map(|l| {
            l.tombstones()
                .iter()
                .map(|t| ((t.id, t.mask), t.iteration))
                .collect()
        })
        .unwrap_or_default();
    let mut puts: BTreeMap<u32, Put> = BTreeMap::new();
    let mut out = Revised::default();
    let layers: Vec<&Layer> = [last, fresh].into_iter().flatten().collect();
    for raw in touched(&base, &layers) {
        let id = DataId(raw);
        let was = view(&base, last, id)?;
        let now = view(&base, fresh, id)?;
        if was == now {
            // As it was: the last overlay's record of it, in its revision, or its deletion.
            if let (Some(e), Some(bytes)) = (last.and_then(|l| l.record(id)), was) {
                puts.insert(
                    raw,
                    Put {
                        bytes,
                        version: version_of(e.bits),
                        iteration: e.iteration,
                        date: e.date,
                    },
                );
                out.kept += 1;
            }
            continue;
        }
        match now {
            Some(bytes) => {
                let (version, date) = fresh
                    .and_then(|l| l.record(id))
                    .or_else(|| base.base_entry(id))
                    .map_or((1, 0), |e| (version_of(e.bits), e.date));
                puts.insert(
                    raw,
                    Put {
                        bytes,
                        version,
                        iteration: next,
                        date,
                    },
                );
                // A deletion of this id alone no longer stands; the record is read over any other.
                tombstones.remove(&(raw, 0));
                out.changed += 1;
            }
            None => {
                tombstones.insert((raw, 0), next);
                out.deleted += 1;
            }
        }
    }
    if out.changed + out.deleted > 0 {
        revisions.push(next);
        out.revision = Some(next);
    }
    if path.is_file() {
        std::fs::remove_file(path).map_err(DatError::from)?;
    }
    if revisions.is_empty() {
        return Ok(out);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(DatError::from)?;
    }
    let mut w = OverlayWriter::open_or_create(path, &base, base_name, world_key, 0)?;
    // The deletions first: a family deletion takes the overlay's own records of the family with it,
    // and those it keeps are put over it.
    for ((raw, mask), iteration) in &tombstones {
        w.tombstone(DataId(*raw), *mask, *iteration)?;
    }
    for (raw, p) in &puts {
        w.put(DataId(*raw), &p.bytes, p.version, p.iteration, p.date)?;
    }
    for r in &revisions {
        w.add_iteration(*r, 0)?;
    }
    w.flush(0)?;
    out.revisions = revisions;
    Ok(out)
}

/// [`revise`] for each file of `bases` (each base file with the file it is): the overlay folder
/// `fresh` written into `out` as the revision after the folder `last`. Each overlay must belong to
/// the world `world_key` and lie over its base. A file's base name is the one its overlays record,
/// else the file's own name.
///
/// # Errors
/// As [`revise`], and an overlay of another world or made against other files
/// ([`OverlayError::OtherWorld`], [`OverlayError::BaseMismatch`]).
pub fn revise_folder(
    bases: &[(ModernDat, DatFile)],
    last: Option<&OverlayDir>,
    fresh: &OverlayDir,
    out: &OverlayDir,
    world_key: &str,
) -> Result<Vec<(ModernDat, Revised)>, ReviseError> {
    let mut done = Vec::new();
    for (t, base) in bases {
        let last = match last {
            Some(dir) => dir.layer_over(*t, base, Some(world_key))?,
            None => None,
        };
        let fresh = fresh.layer_over(*t, base, Some(world_key))?;
        let base_name = fresh.as_ref().or(last.as_ref()).map_or_else(
            || t.file_name().to_owned(),
            |l| l.manifest().base_name.clone(),
        );
        let revised = revise(
            base,
            last.as_ref(),
            fresh.as_ref(),
            &out.container(*t),
            &base_name,
            world_key,
        )?;
        done.push((*t, revised));
    }
    Ok(done)
}
