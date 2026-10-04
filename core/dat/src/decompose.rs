//! A world's data files taken apart against the base they were made from: the overlay that, laid
//! over the base, reads as the world.
//!
//! Bytes decide what changed. A record is **added** when the world has an id the base lacks,
//! **changed** when both have it and its bytes differ, and **removed** when the base has it and
//! the world does not; a record whose bytes are the same is left to the base whatever its
//! iteration says (an edited file's iterations do not mark change reliably). The iteration list
//! and an overlay's own records are never compared.
//!
//! The overlay's iterations are its own by default: every change goes into one revision, the
//! base's iteration count plus one, so a client holding the base reports the base's count and is
//! sent exactly that revision. A world made outside Dereth names iterations of its own (its own
//! numbering, which a server compares a client's against): with [`Iterations::World`] the overlay
//! carries the world's whole iteration list and each record its world iteration, and a client
//! with it reports exactly the world's list.
//!
//! Removed records are tombstones. In the cell file a landblock the world keeps none of the base's
//! records of as they are is one family tombstone, as the retail purge deletes a landblock, with
//! the overlay's records of it read over it; anything less is a tombstone per record.

use std::path::Path;

use dereth_primitives::DataId;

use crate::container::DatFile;
use crate::divine::ITERATION_LIST;
use crate::error::DatError;
use crate::overlay::{self, OverlayError, OverlayWriter};

/// Which iterations the overlay carries.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Iterations {
    /// One revision of its own, one past the base's.
    #[default]
    Revision,
    /// The world's own list, and each record's world iteration.
    World,
}

/// What one world file holds that its base does not, by id, ascending.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    /// Ids the world has and the base lacks.
    pub added: Vec<u32>,
    /// Ids both have, with different bytes.
    pub changed: Vec<u32>,
    /// Ids the base has and the world lacks.
    pub removed: Vec<u32>,
    /// Ids both have with the same bytes and another iteration: left to the base.
    pub restamped: usize,
    /// Of `changed`, how many kept their iteration.
    pub changed_same_iteration: usize,
    /// The bytes of every added and changed record.
    pub bytes: u64,
}

impl Diff {
    /// Whether the world is its base.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

/// Whether a record is one of the comparison's: not the iteration list, not an overlay's own.
fn compared(id: DataId) -> bool {
    id != ITERATION_LIST && !overlay::is_reserved(id)
}

/// Compare `world` with `base`, record by record: their entries first, then the bytes of every
/// record both hold with the same size.
///
/// # Errors
/// A record of either file that will not read.
pub fn diff(base: &DatFile, world: &DatFile) -> Result<Diff, DatError> {
    let base = base.base();
    let world = world.base();
    let mut d = Diff::default();
    for (id, w) in world.base_entries() {
        if !compared(id) {
            continue;
        }
        match base.base_entry(id) {
            None => {
                d.added.push(id.raw());
                d.bytes += u64::from(w.size);
            }
            Some(b) => {
                let differ = b.size != w.size || base.read(id)? != world.read(id)?;
                if differ {
                    d.changed.push(id.raw());
                    d.bytes += u64::from(w.size);
                    if b.iteration == w.iteration {
                        d.changed_same_iteration += 1;
                    }
                } else if b.iteration != w.iteration {
                    d.restamped += 1;
                }
            }
        }
    }
    d.removed = base
        .base_entries()
        .filter(|(id, _)| compared(*id) && world.base_entry(*id).is_none())
        .map(|(id, _)| id.raw())
        .collect();
    Ok(d)
}

/// The revision an overlay over `base` puts its changes in: one past the base's iterations.
#[must_use]
pub fn revision_over(base: &DatFile) -> u32 {
    let set = overlay::base_iterations(&base.base());
    set.iter()
        .copied()
        .max()
        .unwrap_or(0)
        .max(u32::try_from(set.len()).unwrap_or(0))
        + 1
}

/// The deletions `diff` makes of `base`'s records. In the cell file, a landblock none of whose base
/// records the world keeps as they are (each is removed or changed) is one family tombstone
/// (`mask` `0xFFFF0000`): the overlay's own copies of its changed and added records are read over
/// it. Every other removed id is its own tombstone.
#[must_use]
pub fn deletions(base: &DatFile, diff: &Diff, cell: bool) -> Vec<(u32, u32)> {
    const FAMILY: u32 = 0xFFFF_0000;
    if !cell {
        return diff.removed.iter().map(|id| (*id, 0)).collect();
    }
    let base = base.base();
    let mut by_block: std::collections::BTreeMap<u32, Vec<u32>> = std::collections::BTreeMap::new();
    for id in &diff.removed {
        by_block.entry(id & FAMILY).or_default().push(*id);
    }
    let changed: std::collections::HashSet<u32> = diff.changed.iter().copied().collect();
    // Of the blocks with a removal, how many base records each keeps as they are.
    let mut kept: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for (id, _) in base.base_entries() {
        let block = id.raw() & FAMILY;
        if let Some(gone) = by_block.get(&block) {
            if !changed.contains(&id.raw()) && gone.binary_search(&id.raw()).is_err() {
                *kept.entry(block).or_default() += 1;
            }
        }
    }
    let mut out = Vec::new();
    for (block, ids) in by_block {
        if kept.get(&block).copied().unwrap_or(0) == 0 {
            out.push((block, FAMILY));
        } else {
            out.extend(ids.into_iter().map(|id| (id, 0)));
        }
    }
    out
}

/// Write `diff` of `world` over `base` as a fresh overlay container at `path`, for the world
/// `world_key`: the added and changed records, and the removed ones as tombstones ([`deletions`]).
/// With [`Iterations::Revision`] they all go into revision [`revision_over`]; with
/// [`Iterations::World`] each record keeps its world iteration, the deletions take the world's
/// last, and the overlay carries the world's iteration list. `base_name` is the base file's name
/// as the manifest records it; `cell` says the file is the cell file. Answers the overlay's
/// highest iteration.
///
/// # Errors
/// The writer's and the reader's errors.
#[allow(clippy::too_many_arguments)]
pub fn write(
    base: &DatFile,
    world: &DatFile,
    diff: &Diff,
    path: &Path,
    base_name: &str,
    world_key: &str,
    cell: bool,
    iterations: Iterations,
) -> Result<u32, OverlayError> {
    let base = base.base();
    let world = world.base();
    if path.is_file() {
        std::fs::remove_file(path).map_err(DatError::from)?;
    }
    let world_list = overlay::base_iterations(&world);
    let revision = match iterations {
        Iterations::Revision => revision_over(&base),
        Iterations::World => world_list.iter().copied().max().unwrap_or(0),
    };
    let mut w = OverlayWriter::open_or_create(path, &base, base_name, world_key, 0)?;
    // The deletions first: a family tombstone takes the overlay's own records of the family with
    // it, and the world's records of it are then put over it.
    for (id, mask) in deletions(&base, diff, cell) {
        w.tombstone(DataId(id), mask, revision)?;
    }
    for id in diff.added.iter().chain(&diff.changed) {
        let id = DataId(*id);
        let e = *world.base_entry(id).ok_or(DatError::NotFound(id))?;
        let bytes = world.read(id)?;
        // A file from before Throne of Destiny records no version; the writer needs one.
        let version = u16::try_from(e.bits >> 16).unwrap_or(1).max(1);
        let iteration = match iterations {
            Iterations::Revision => revision,
            Iterations::World => e.iteration,
        };
        w.put(id, &bytes, version, iteration, e.date)?;
    }
    match iterations {
        Iterations::Revision => w.add_iteration(revision, 0)?,
        Iterations::World => w.set_exact_iterations(&world_list, 0)?,
    }
    w.flush(0)?;
    Ok(revision)
}
