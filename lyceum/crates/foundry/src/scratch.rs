//! The overlay an editor draws, kept in step with an edit session one edit at a time.
//!
//! Two folders take turns. Each edit is written into the folder the drawn store is not reading,
//! and only what the edits since that folder was last written reached goes into it: each reached
//! landblock's terrain and statics records (with the spawns' previews, [`Session::scratch_records`])
//! are put in, but those the folder holds as they are, and one that is the base's again is taken
//! out. Then the store is laid over that
//! folder, and the next edit goes into the other. So the folder a store reads is never written
//! while it reads it, and an edit writes a few records, not the whole overlay.
//!
//! A folder written this way holds what a whole write of the scratch overlay holds, record for
//! record. A folder is written whole the first time, and again whenever the overlay's deletions
//! change, which no edit of the session makes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_dat::overlay::{container_name, record_hash, OverlayWriter};
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
    /// Records put in, and records taken out, when it was not; and records it already held as
    /// they are, left as they were.
    pub put: usize,
    pub reverted: usize,
    pub kept: usize,
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
        let writing = me.begin(session, &BTreeSet::new())?;
        let written = me.finish(writing.run())?;
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
        let writing = self.begin(session, touched)?;
        self.finish(writing.run())
    }

    /// [`Self::update`] in two halves, so that the records are written on another thread: what
    /// to write, taken from `session` now, and its writing ([`Writing::run`]), which
    /// [`Self::finish`] takes once done. The folder the store reads is not written meanwhile; the
    /// next update begins after this one has finished.
    ///
    /// # Errors
    /// The writer's errors.
    pub fn begin(&mut self, session: &Session, touched: &BTreeSet<u16>) -> Result<Writing, Error> {
        for f in &mut self.folders {
            f.pending.extend(touched.iter().copied());
        }
        let which = 1 - self.current;
        let tombstones = &session.compiled().tombstones;
        let whole = !self.folders[which].written || *tombstones != self.tombstones;
        let dir = self.folders[which].dir.clone();
        let pending = std::mem::take(&mut self.folders[which].pending);
        let mut written = Written {
            dir: dir.clone(),
            whole,
            put: 0,
            reverted: 0,
            kept: 0,
        };
        if whole {
            // Rare (a folder's first write): written whole now.
            session.write_scratch(&dir)?;
            self.folders[which].written = true;
            self.tombstones.clone_from(tombstones);
            return Ok(Writing {
                which,
                work: Work::Done(written),
            });
        }
        if pending.is_empty() {
            return Ok(Writing {
                which,
                work: Work::Done(written),
            });
        }
        let mut records = session.scratch_records_in(Some(&pending));
        let cells = records.remove(&ModernDat::Cell).unwrap_or_default();
        let base = session.base();
        let Some(file) = base.file(ModernDat::Cell) else {
            return Err(Error::Dat("the files have no cell file".into()));
        };
        let revision = decompose::revision_over(file);
        let path = dir.join(container_name(ModernDat::Cell));
        let w = OverlayWriter::open_or_create(
            &path,
            file,
            ModernDat::Cell.file_name(),
            &session.world().key,
            0,
        )?;
        written.dir = dir;
        Ok(Writing {
            which,
            work: Work::Puts(Box::new(Puts {
                w,
                path,
                pending,
                cells,
                revision,
                written,
            })),
        })
    }

    /// Take a write [`Self::begin`] began, done: its folder is the one to read now.
    ///
    /// # Errors
    /// The write's own.
    pub fn finish(&mut self, done: Result<(usize, Written), Error>) -> Result<Written, Error> {
        let (which, written) = done?;
        self.current = which;
        Ok(written)
    }
}

/// A write of a scratch folder [`Scratch::begin`] began.
#[must_use]
pub struct Writing {
    which: usize,
    work: Work,
}

impl std::fmt::Debug for Writing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Writing")
            .field("which", &self.which)
            .finish_non_exhaustive()
    }
}

enum Work {
    Done(Written),
    Puts(Box<Puts>),
}

struct Puts {
    w: OverlayWriter,
    path: PathBuf,
    pending: BTreeSet<u16>,
    cells: BTreeMap<u32, crate::compile::Staged>,
    revision: u32,
    written: Written,
}

impl Writing {
    /// Write the records, on whatever thread calls it; what [`Scratch::finish`] takes.
    ///
    /// # Errors
    /// The writer's errors.
    pub fn run(self) -> Result<(usize, Written), Error> {
        let which = self.which;
        let p = match self.work {
            Work::Done(w) => return Ok((which, w)),
            Work::Puts(p) => *p,
        };
        let Puts {
            mut w,
            path,
            pending,
            cells,
            revision,
            mut written,
        } = p;
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
            ids.extend(cells.range(first..=first | 0xFFFF).map(|(id, _)| *id));
            for id in ids {
                match cells.get(&id) {
                    // A record the folder holds as it is now: a dungeon's thousands of rooms are
                    // not written again for the one that changed.
                    Some(s) if w.manifest().hashes.get(&id) == Some(&record_hash(&s.bytes)) => {
                        written.kept += 1;
                    }
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
        // A container left holding nothing of the world's is one a whole write would not make.
        let empty = w.manifest().hashes.is_empty() && w.tombstones().is_empty();
        drop(w);
        if empty {
            std::fs::remove_file(&path).map_err(|e| Error::io(&path, e))?;
        }
        Ok((which, written))
    }
}
