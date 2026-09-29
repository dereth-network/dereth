//! Where a [`crate::DatDatabase`] gets its files: the retail dats ([`RealDats`]) or an in-memory
//! set built by a test ([`crate::FakeDats`]).
//!
//! Nothing here is ported from ACE; it is the seam that lets the unit tier run with no dats.

use std::fmt;
use std::path::{Path, PathBuf};

use dereth_dat::{DatError, DatFile, RetailDatStore};

use crate::database::{CachedObject, DatDatabaseType};

/// A set of dat files, by database.
pub trait DatSource: Send + Sync + fmt::Debug {
    /// Whether this source has the database at all (the high-res dat is optional).
    fn has_database(&self, db: DatDatabaseType) -> bool;

    /// Every file id in the database, ascending (ACE's `AllFiles.Keys`).
    fn file_ids(&self, db: DatDatabaseType) -> Vec<u32>;

    /// How many files the database has (ACE's `AllFiles.Count`).
    fn file_count(&self, db: DatDatabaseType) -> usize {
        self.file_ids(db).len()
    }

    /// Whether the database has `id`.
    fn contains(&self, db: DatDatabaseType, id: u32) -> bool;

    /// The raw payload of `id`, or `None` when the database has no such file.
    fn read(&self, db: DatDatabaseType, id: u32) -> Option<Vec<u8>>;

    /// The iteration that introduced `id` (the directory entry's `iter_`, ACE's
    /// `DatFile.Iteration`), or `None` when the database has no such file.
    fn file_iteration(&self, db: DatDatabaseType, id: u32) -> Option<u32>;

    /// An already-decoded object for `id`, which the cache takes in place of decoding bytes.
    /// Only a test source has these.
    fn decoded(&self, _db: DatDatabaseType, _id: u32) -> Option<CachedObject> {
        None
    }

    /// A name for log lines: the file path, or what the fake is.
    fn describe(&self, db: DatDatabaseType) -> String;
}

/// The retail dats from one directory.
///
/// The files are opened through `dereth-dat`'s process-wide table, so however many `RealDats` a
/// process makes, each file's directory is walked **once** (and a `DatManager` shared as an `Arc`
/// holds one `RealDats` anyway). Unlike `dereth-dat`'s store, there is no compiled-in directory: the
/// caller always names one.
#[derive(Debug)]
pub struct RealDats {
    dir: PathBuf,
    store: RetailDatStore,
}

impl RealDats {
    /// Open the three required files under `dir`, and `client_highres.dat` when it is there
    /// (ACE opens it whenever it exists).
    ///
    /// # Errors
    ///
    /// A required file is missing or does not open.
    pub fn open(dir: &Path) -> Result<Self, DatError> {
        let store = RetailDatStore::open_dir(dir)?;
        store.grant_highres()?;
        Ok(Self {
            dir: dir.to_path_buf(),
            store,
        })
    }

    /// The directory the files came from.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file(&self, db: DatDatabaseType) -> Option<&DatFile> {
        match db {
            DatDatabaseType::Portal => Some(self.store.portal()),
            DatDatabaseType::Cell => Some(self.store.cell()),
            DatDatabaseType::Language => Some(self.store.local()),
            DatDatabaseType::HighRes => self.store.highres(),
        }
    }
}

impl DatSource for RealDats {
    fn has_database(&self, db: DatDatabaseType) -> bool {
        self.file(db).is_some()
    }

    fn file_ids(&self, db: DatDatabaseType) -> Vec<u32> {
        let mut ids: Vec<u32> = self
            .file(db)
            .map(|f| f.iter_ids().map(|i| i.0).collect())
            .unwrap_or_default();
        ids.sort_unstable();
        ids
    }

    fn file_count(&self, db: DatDatabaseType) -> usize {
        self.file(db).map_or(0, DatFile::len)
    }

    fn contains(&self, db: DatDatabaseType, id: u32) -> bool {
        self.file(db)
            .is_some_and(|f| f.contains(dereth_primitives::DataId(id)))
    }

    fn read(&self, db: DatDatabaseType, id: u32) -> Option<Vec<u8>> {
        let file = self.file(db)?;
        match file.read(dereth_primitives::DataId(id)) {
            Ok(bytes) => Some(bytes),
            Err(DatError::NotFound(_)) => None,
            Err(e) => {
                log::error!("{:?} dat: reading 0x{id:08X} failed: {e}", db);
                None
            }
        }
    }

    fn file_iteration(&self, db: DatDatabaseType, id: u32) -> Option<u32> {
        self.file(db)?
            .entry(dereth_primitives::DataId(id))
            .map(|e| e.iteration)
    }

    fn describe(&self, db: DatDatabaseType) -> String {
        let file = match db {
            DatDatabaseType::Portal => dereth_dat::RetailDat::Portal,
            DatDatabaseType::Cell => dereth_dat::RetailDat::Cell,
            DatDatabaseType::Language => dereth_dat::RetailDat::Local,
            DatDatabaseType::HighRes => dereth_dat::RetailDat::HighRes,
        };
        file.in_dir(&self.dir).display().to_string()
    }
}
