//! The overlay an editor draws, kept in step with an edit session one edit at a time.
//!
//! Two folders take turns. Each edit is written into the folder the drawn store is not reading,
//! and only what the edits since that folder was last written reached goes into it: each reached
//! landblock's terrain and statics records (with the spawns' previews, [`Session::scratch_records`])
//! are put in, and one that is the base's again is taken out. Then the store is laid over that
//! folder, and the next edit goes into the other. So the folder a store reads is never written
//! while it reads it, and an edit writes a few records, not the whole overlay.
//!
//! A folder written this way holds what a whole write of the scratch overlay holds, record for
//! record. A folder is written whole the first time, and again whenever the overlay's deletions
//! change, which no edit of the session makes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_dat::overlay::{container_name, OverlayWriter};
use dereth_dat::{decompose, ModernDat};
use dereth_primitives::DataId;

use crate::edit::Session;
use crate::error::Error;

/// What one write did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    /// The folder written, which the store is laid over next.
    pub dir: PathBuf,
    /// Whether it was written whole.
    pub whole: bool,
    /// Records put in, and records taken out, when it was not.
    pub put: usize,
    pub reverted: usize,
}

#[derive(Clone, Debug)]
struct Folder {
    dir: PathBuf,
    /// Whether the folder holds a whole write yet.
    written: bool,
    /// The landblocks edited since it was last written.
    pending: BTreeSet<u16>,
}

/// The two folders of an editor's scratch overlay.
#[derive(Clone, Debug)]
pub struct Scratch {
    folders: [Folder; 2],
    /// The folder written last: the one the drawn store reads.
    current: usize,
    /// The deletions the folders were written with.
    tombstones: BTreeMap<ModernDat, BTreeSet<(u32, u32)>>,
}

impl Scratch {
    /// The scratch overlay of `session` in `root`, written whole into its first folder.
    ///
    /// # Errors
    /// The writer's errors.
    pub fn create(root: &Path, session: &Session) -> Result<(Self, Written), Error> {
        let folder = |name: &str| Folder {
            dir: root.join(name),
            written: false,
            pending: BTreeSet::new(),
        };
        let mut me = Self {
            folders: [folder("overlay-a"), folder("overlay-b")],
            current: 1,
            tombstones: session.compiled().tombstones.clone(),
        };
        let written = me.write(session, 0)?;
        Ok((me, written))
    }

    /// The folder the store is to read: the one written last.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.folders[self.current].dir
    }

    /// Bring the other folder up to `session` after an edit that reached the landblocks
    /// `touched`, and make it the one to read.
    ///
    /// # Errors
    /// The writer's errors.
    pub fn update(&mut self, session: &Session, touched: &BTreeSet<u16>) -> Result<Written, Error> {
        for f in &mut self.folders {
            f.pending.extend(touched.iter().copied());
        }
        self.write(session, 1 - self.current)
    }

    fn write(&mut self, session: &Session, which: usize) -> Result<Written, Error> {
        let tombstones = &session.compiled().tombstones;
        let whole = !self.folders[which].written || *tombstones != self.tombstones;
        let dir = self.folders[which].dir.clone();
        let pending = std::mem::take(&mut self.folders[which].pending);
        let mut written = Written {
            dir: dir.clone(),
            whole,
            put: 0,
            reverted: 0,
        };
        if whole {
            session.write_scratch(&dir)?;
            self.folders[which].written = true;
            self.tombstones.clone_from(tombstones);
        } else if !pending.is_empty() {
            let records = session.scratch_records_in(Some(&pending));
            let cells = records.get(&ModernDat::Cell);
            let base = session.base();
            let Some(file) = base.file(ModernDat::Cell) else {
                return Err(Error::Dat("the files have no cell file".into()));
            };
            let revision = decompose::revision_over(file);
            let path = dir.join(container_name(ModernDat::Cell));
            let mut w = OverlayWriter::open_or_create(
                &path,
                file,
                ModernDat::Cell.file_name(),
                &session.world().key,
                0,
            )?;
            for lb in &pending {
                // The landblock's two records, its cells the overlay holds, and those it has now.
                let first = u32::from(*lb) << 16;
                let mut ids: BTreeSet<u32> = [first | 0xFFFF, first | 0xFFFE].into();
                ids.extend(
                    w.manifest()
                        .hashes
                        .keys()
                        .filter(|id| *id >> 16 == u32::from(*lb)),
                );
                ids.extend(
                    cells
                        .into_iter()
                        .flat_map(|c| c.range(first..=first | 0xFFFF).map(|(id, _)| *id)),
                );
                for id in ids {
                    match cells.and_then(|c| c.get(&id)) {
                        Some(s) => {
                            w.put(DataId(id), &s.bytes, s.version, revision, 0)?;
                            written.put += 1;
                        }
                        None => {
                            if w.revert(DataId(id))? {
                                written.reverted += 1;
                            }
                        }
                    }
                }
            }
            w.add_iteration(revision, 0)?;
            w.flush(0)?;
            // A container left holding nothing of the world's is one a whole write would not
            // make.
            let empty = w.manifest().hashes.is_empty() && w.tombstones().is_empty();
            drop(w);
            if empty {
                std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
            }
        }
        self.current = which;
        Ok(written)
    }
}
